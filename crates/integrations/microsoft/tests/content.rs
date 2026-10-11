//! Files against a local server that answers as Microsoft Graph does: a
//! recording's video, and what is attached to a message.
//!
//! What every operation sends and returns is in the table in `operations.rs`.
//! This file holds what is particular to content: bytes, limits, and the
//! rule that an operation called by name returns text or nothing.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::{Connection, ErrorKind, Retry};
use socketkit_microsoft::models::{Download, TextLimit};
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{connect, point_at, with_content_host};

mod support;
use support::{MEETING, MEETING_PATH, graph_error, invoke, microsoft, only_request};

/// Bytes that are not text: reading them as UTF-8 would change them.
const VIDEO: &[u8] = &[
    0x00, 0x00, 0x00, 0x18, b'f', b't', b'y', b'p', 0xff, 0xfe, 0x80, 0x0d, 0x0a, 0x00,
];

fn video() -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(VIDEO, "video/mp4")
}

fn served(body: impl Into<Vec<u8>>, content_type: &str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(body.into(), content_type)
}

/// Graph on `server`, and a connection for the typed methods.
async fn typed(server: &MockServer) -> (Microsoft, Connection) {
    let microsoft = Microsoft::with_spec(point_at(provider(), server));
    let (socket, key) = connect(Arc::new(microsoft.clone()), "eyJ.good").await;
    let connection = socket.connection(key).await.unwrap();
    (microsoft, connection)
}

fn recording_path() -> String {
    format!("/v1.0/me/onlineMeetings/{MEETING_PATH}/recordings/rec-1/content")
}

// ── A recording ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_recording_comes_back_as_the_bytes_graph_served() {
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(video()).mount(&server).await;
    let (microsoft, connection) = typed(&server).await;
    let content = microsoft
        .recordings(&connection)
        .content(MEETING, "rec-1", Download::default())
        .await
        .unwrap();
    assert_eq!(content.bytes, VIDEO, "nothing was read as text on the way");
    assert_eq!(content.content_type.as_deref(), Some("video/mp4"));

    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "GET");
    assert_eq!(request.url.path(), recording_path());
    assert_eq!(request.url.query(), None);
    assert_eq!(request.headers.get("authorization").unwrap(), "Bearer eyJ.good");
    assert_eq!(request.headers.get("accept").unwrap(), "*/*");
}

#[tokio::test]
async fn a_recording_over_what_the_caller_allows_is_refused_whole() {
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(video()).mount(&server).await;
    let (microsoft, connection) = typed(&server).await;
    let recordings = microsoft.recordings(&connection);
    let most = |bytes: usize| Download {
        max_bytes: Some(bytes),
        ..Download::default()
    };
    let err = recordings
        .content(MEETING, "rec-1", most(VIDEO.len() - 1))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge, "{err}");
    assert_eq!(err.to_wire().code, "too_large");
    assert_eq!(err.retry(), Retry::Never);
    let whole = recordings.content(MEETING, "rec-1", most(VIDEO.len())).await.unwrap();
    assert_eq!(whole.bytes, VIDEO);

    // Unless told otherwise a fetch stops at ten megabytes, and a recording is larger.
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(served(vec![1u8; 10 * 1024 * 1024 + 1], "video/mp4"))
        .mount(&server)
        .await;
    let (microsoft, connection) = typed(&server).await;
    let recordings = microsoft.recordings(&connection);
    let err = recordings
        .content(MEETING, "rec-1", Download::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge);
    let content = recordings
        .content(MEETING, "rec-1", most(32 * 1024 * 1024))
        .await
        .unwrap();
    assert_eq!(content.len(), 10 * 1024 * 1024 + 1);
}

#[tokio::test]
async fn a_recording_kept_on_another_host_is_fetched_only_when_that_host_is_declared_and_without_the_token() {
    let redirect = |elsewhere: &MockServer| {
        ResponseTemplate::new(302).insert_header("location", format!("{}/video?sig=abc", elsewhere.uri()).as_str())
    };

    // Graph's own definition names no other host, so the token and the request stay on Graph.
    let graph = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    Mock::given(path(recording_path()))
        .respond_with(redirect(&elsewhere))
        .mount(&graph)
        .await;
    Mock::given(any()).respond_with(video()).mount(&elsewhere).await;
    let (microsoft, connection) = typed(&graph).await;
    let err = microsoft
        .recordings(&connection)
        .content(MEETING, "rec-1", Download::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected, "{err}");
    assert!(err.message().contains("redirected"), "{}", err.message());
    assert!(elsewhere.received_requests().await.unwrap().is_empty());

    // An application that declares the host gets the video, and the host never sees the token.
    let graph = MockServer::start().await;
    let signed = MockServer::start().await;
    Mock::given(path(recording_path()))
        .respond_with(redirect(&signed))
        .mount(&graph)
        .await;
    Mock::given(method("GET"))
        .and(path("/video"))
        .respond_with(video())
        .mount(&signed)
        .await;
    let spec = with_content_host(point_at(provider(), &graph), &signed, false);
    let microsoft = Microsoft::with_spec(spec);
    let (socket, key) = connect(Arc::new(microsoft.clone()), "eyJ.good").await;
    let connection = socket.connection(key).await.unwrap();
    let content = microsoft
        .recordings(&connection)
        .content(MEETING, "rec-1", Download::default())
        .await
        .unwrap();
    assert_eq!(content.bytes, VIDEO);
    let at_host = only_request(&signed).await;
    assert!(at_host.headers.get("authorization").is_none());
    assert_eq!(at_host.url.query(), Some("sig=abc"));
}

#[tokio::test]
async fn graphs_refusal_of_a_recording_is_an_error_and_never_a_file() {
    for (response, kind) in [
        (
            graph_error(403, "Forbidden", "Only the organizer can download"),
            ErrorKind::AccessDenied,
        ),
        (graph_error(404, "NotFound", "No such recording"), ErrorKind::NotFound),
        (
            graph_error(401, "InvalidAuthenticationToken", "Expired"),
            ErrorKind::ReconnectRequired,
        ),
        (
            ResponseTemplate::new(429).insert_header("retry-after", "600"),
            ErrorKind::RateLimited,
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let (microsoft, connection) = typed(&server).await;
        let err = microsoft
            .recordings(&connection)
            .content(MEETING, "rec-1", Download::default())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
    }
}

#[tokio::test]
async fn an_id_in_the_address_of_a_file_is_one_segment_whatever_it_holds() {
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(video()).mount(&server).await;
    let (microsoft, connection) = typed(&server).await;
    microsoft
        .recordings(&connection)
        .content("a/b?c", "../x#y", Download::default())
        .await
        .unwrap();
    microsoft
        .mail(&connection)
        .attachment_content("AAMk/a+b=", "att/1?x", Download::default())
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    assert_eq!(
        received[0].url.path(),
        "/v1.0/me/onlineMeetings/a%2Fb%3Fc/recordings/..%2Fx%23y/content"
    );
    assert_eq!(
        received[1].url.path(),
        "/v1.0/me/messages/AAMk%2Fa%2Bb%3D/attachments/att%2F1%3Fx/$value"
    );
    assert!(received.iter().all(|request| request.url.query().is_none()));

    for (meeting, recording) in [("", "rec-1"), ("..", "rec-1"), (MEETING, ""), (MEETING, ".")] {
        let err = microsoft
            .recordings(&connection)
            .content(meeting, recording, Download::default())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{meeting:?} {recording:?}");
    }
    for (message, attachment) in [("", "att-1"), ("msg-1", ".."), ("msg-1", " ")] {
        let err = microsoft
            .mail(&connection)
            .attachment_content(message, attachment, Download::default())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{message:?} {attachment:?}");
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

// ── An attachment ────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_attachment_comes_back_as_the_file_it_is() {
    let pdf: &[u8] = b"%PDF-1.7\n\xff\xfe\x00binary";
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(served(pdf, "application/pdf"))
        .mount(&server)
        .await;
    let (microsoft, connection) = typed(&server).await;
    let content = microsoft
        .mail(&connection)
        .attachment_content("msg-1", "att-1", Download::default())
        .await
        .unwrap();
    assert_eq!(content.bytes, pdf);
    assert_eq!(content.content_type.as_deref(), Some("application/pdf"));
    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "GET");
    assert_eq!(request.url.path(), "/v1.0/me/messages/msg-1/attachments/att-1/$value");
    assert_eq!(request.headers.get("authorization").unwrap(), "Bearer eyJ.good");

    let err = microsoft
        .mail(&connection)
        .attachment_content(
            "msg-1",
            "att-1",
            Download {
                max_bytes: Some(pdf.len() - 1),
                timeout_secs: Some(5),
            },
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge);
}

#[tokio::test]
async fn describing_an_attachment_never_brings_the_file_with_it() {
    // Graph sends the content of a file unless it is told which fields are wanted.
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "@odata.type": "#microsoft.graph.fileAttachment", "id": "att-1", "name": "plan.pdf",
            "contentType": "application/pdf", "size": 5, "isInline": false, "contentBytes": "aGVsbG8="
        })))
        .mount(&server)
        .await;
    let described = invoke(
        &socket,
        &key,
        "mail.attachment_get",
        json!({ "message": "msg-1", "attachment": "att-1" }),
    )
    .await
    .unwrap();
    let request = only_request(&server).await;
    assert_eq!(
        request.url.query_pairs().find(|(name, _)| name == "$select").unwrap().1,
        "id,name,contentType,size,isInline,lastModifiedDateTime"
    );
    assert_eq!(described["name"], "plan.pdf");
    // And should Graph send it all the same, it goes no further.
    assert!(!described.to_string().contains("aGVsbG8="), "{described}");
}

// ── By name: text, or nothing ────────────────────────────────────────────────

#[tokio::test]
async fn an_operation_called_by_name_returns_an_attachment_only_when_it_is_text() {
    let one = json!({ "message": "msg-1", "attachment": "att-1" });
    for (body, content_type) in [
        ("name,owner\nQ3 plan,Grace\n", "text/csv"),
        ("caf\u{e9} \u{2014} notes", "text/plain; charset=utf-8"),
        ("BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n", "text/calendar"),
        ("{\"plan\":\"Q3\"}", "application/json"),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any())
            .respond_with(served(body, content_type))
            .mount(&server)
            .await;
        let read = invoke(&socket, &key, "mail.attachment_text", one.clone())
            .await
            .unwrap_or_else(|e| panic!("{content_type}: {e}"));
        assert_eq!(read, json!({ "contentType": content_type, "text": body }));
    }

    // Bytes are not handed over by name, in any form.
    let secret = "SALARY TABLE";
    for content_type in ["application/pdf", "application/octet-stream", "image/png", "video/mp4"] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any())
            .respond_with(served(format!("%PDF {secret}"), content_type))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, "mail.attachment_text", one.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported, "{content_type}: {err}");
        assert_eq!(err.to_wire().code, "unsupported");
        assert!(err.message().contains("17 bytes"), "{}", err.message());
        assert!(!format!("{err} {err:?}").contains(secret), "{err:?}");
    }

    // Something sent as text that is not text is refused, not mended.
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(served(VIDEO, "text/plain"))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "mail.attachment_text", one).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn text_by_name_is_a_megabyte_unless_more_is_asked_for_and_never_more_than_ten() {
    const MEGABYTE: usize = 1024 * 1024;
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(served(vec![b'a'; MEGABYTE + 1], "text/plain"))
        .mount(&server)
        .await;
    let one = json!({ "message": "msg-1", "attachment": "att-1" });
    let err = invoke(&socket, &key, "mail.attachment_text", one.clone())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge, "{err}");
    assert!(err.message().contains("1048576"), "{}", err.message());

    // The caller who asks for more gets it.
    let mut more = one.clone();
    more["maxBytes"] = json!(2 * MEGABYTE);
    let read = invoke(&socket, &key, "mail.attachment_text", more).await.unwrap();
    assert_eq!(read["text"].as_str().unwrap().len(), MEGABYTE + 1);

    // And the caller who asks for less gets less, or nothing: never a part.
    let mut less = one.clone();
    less["maxBytes"] = json!(10);
    let err = invoke(&socket, &key, "mail.attachment_text", less).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge);

    let asked = server.received_requests().await.unwrap().len();
    for too_much in [json!(10 * MEGABYTE + 1), json!(u64::MAX)] {
        let mut input = one.clone();
        input["maxBytes"] = too_much.clone();
        let err = invoke(&socket, &key, "mail.attachment_text", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{too_much}: {err}");
    }
    for bad in [json!(-1), json!("many"), json!(1.5)] {
        let mut input = one.clone();
        input["maxBytes"] = bad.clone();
        let err = invoke(&socket, &key, "mail.attachment_text", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad}: {err}");
    }
    // How long to wait is not the caller's to set by name.
    let mut input = one.clone();
    input["timeoutSecs"] = json!(600);
    let err = invoke(&socket, &key, "mail.attachment_text", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        asked,
        "none of these reached Graph"
    );

    // The typed method keeps the same rule.
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(served(vec![b'a'; MEGABYTE + 1], "text/plain"))
        .mount(&server)
        .await;
    let (microsoft, connection) = typed(&server).await;
    let err = microsoft
        .mail(&connection)
        .attachment_text("msg-1", "att-1", TextLimit::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge);
}

#[tokio::test]
async fn no_operation_called_by_name_returns_bytes() {
    let (server, socket, key) = microsoft().await;
    Mock::given(any()).respond_with(video()).mount(&server).await;
    for (name, input) in [
        (
            "recordings.content",
            json!({ "meeting": MEETING, "recording": "rec-1" }),
        ),
        (
            "mail.attachment_content",
            json!({ "message": "msg-1", "attachment": "att-1" }),
        ),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported, "{name}: {err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // No operation describes its answer as bytes or as base64.
    for operation in socket.operations() {
        let described = operation.output_schema.to_string();
        for word in ["contentBytes", "base64", "\"bytes\""] {
            assert!(!described.contains(word), "{}: {word}", operation.name);
        }
    }
}
