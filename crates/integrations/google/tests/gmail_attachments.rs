//! What is attached to a Gmail message, against a local server that answers
//! as Google does.
//!
//! What every operation sends and returns is in the table in `operations.rs`.
//! This file holds what is particular to a file: bytes, limits, and the rule
//! that an operation called by name returns text or nothing.

use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE;
use serde_json::json;
use socketkit_core::{Connection, ErrorKind};
use socketkit_google::models::{Download, TextLimit};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

mod support;
use support::gmail::{GMAIL_ATTACHMENT, GMAIL_MESSAGE, GMAIL_MESSAGES};
use support::{answer, answering, google, google_error, invoke, only_request};

/// Bytes that are not text: reading them as UTF-8 would change them. In
/// base64 they are `-_8AJVBERi0xLjf_`, with both of the characters the two
/// alphabets differ in.
const FILE: &[u8] = &[0xfb, 0xff, 0x00, b'%', b'P', b'D', b'F', b'-', b'1', b'.', b'7', 0xff];

/// Gmail on `server`, and a connection for the typed methods.
async fn typed(server: &MockServer) -> (Google, Connection) {
    let google = Google::with_spec(point_at(provider(), server));
    let (socket, key) = connect(Arc::new(google.clone()), support::TOKEN).await;
    let connection = socket.connection(key).await.unwrap();
    (google, connection)
}

/// A server that answers every request with `file` as Gmail hands one over.
async fn attaching(file: &[u8]) -> MockServer {
    let server = MockServer::start().await;
    let body = json!({ "attachmentId": GMAIL_ATTACHMENT, "size": file.len(), "data": URL_SAFE.encode(file) });
    Mock::given(any()).respond_with(answer(200, &body)).mount(&server).await;
    server
}

fn at_most(bytes: usize) -> Download {
    Download {
        max_bytes: Some(bytes),
        ..Download::default()
    }
}

#[tokio::test]
async fn an_attachment_comes_back_as_the_bytes_of_the_file_however_gmail_wrote_them() {
    // Padded and not, and in the alphabet Gmail documents and the other one.
    for data in [
        "-_8AJVBERi0xLjf_",
        "-_8AJVBERi0xLjf_AA==",
        "-_8AJVBERi0xLjf_AA",
        "+/8AJVBERi0xLjf/AA==",
    ] {
        let file = if data.len() == 16 {
            FILE.to_vec()
        } else {
            [FILE, &[0]].concat()
        };
        let server = MockServer::start().await;
        let body = json!({ "size": file.len(), "data": data });
        Mock::given(any()).respond_with(answer(200, &body)).mount(&server).await;
        let (google, connection) = typed(&server).await;
        let content = google
            .gmail_messages(&connection)
            .attachment_content(GMAIL_MESSAGE, GMAIL_ATTACHMENT, Download::default())
            .await
            .unwrap();
        assert_eq!(content.bytes, file, "{data}: nothing was read as text on the way");
        assert_eq!(content.content_type, None, "Gmail does not say what a file is");

        let request = only_request(&server).await;
        assert_eq!(request.method.as_str(), "GET");
        assert_eq!(
            request.url.path(),
            format!("{GMAIL_MESSAGES}/{GMAIL_MESSAGE}/attachments/{GMAIL_ATTACHMENT}")
        );
        assert_eq!(request.url.query(), None);
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            &format!("Bearer {}", support::TOKEN)
        );
    }

    // An empty file is a file: Gmail leaves out what is empty.
    for empty in [json!({ "size": 0 }), json!({ "size": 0, "data": "" })] {
        let server = MockServer::start().await;
        Mock::given(any())
            .respond_with(answer(200, &empty))
            .mount(&server)
            .await;
        let (google, connection) = typed(&server).await;
        let messages = google.gmail_messages(&connection);
        let content = messages.attachment_content("m1", "a1", at_most(0)).await.unwrap();
        assert!(content.is_empty(), "{empty}");
    }
}

#[tokio::test]
async fn an_attachment_over_what_the_caller_allows_is_refused_whole() {
    // Every length around a group of three bytes, where base64 pads.
    for length in [1_usize, 2, 3, 4, 5, 6, 1000, 3001] {
        let file: Vec<u8> = (0..length).map(|at| (at % 251) as u8).collect();
        let server = attaching(&file).await;
        let (google, connection) = typed(&server).await;
        let messages = google.gmail_messages(&connection);

        // Exactly the limit is accepted, whole.
        let content = messages.attachment_content("m1", "a1", at_most(length)).await.unwrap();
        assert_eq!(content.bytes, file, "{length}");

        // One byte more than the limit is refused, and no part of it comes
        // back. The error names the limit that was asked for.
        let err = messages
            .attachment_content("m1", "a1", at_most(length - 1))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::TooLarge, "{length}: {err}");
        assert_eq!(err.kind().code(), "too_large");
        assert!(
            err.message()
                .contains(&format!("larger than the limit of {} bytes", length - 1)),
            "{err}"
        );
    }

    // Far over the limit: the fetch itself stops, with the same error and
    // the caller's own number in it, not the one the fetch was given.
    let server = attaching(&vec![b'x'; 60_000]).await;
    let (google, connection) = typed(&server).await;
    let messages = google.gmail_messages(&connection);
    let err = messages
        .attachment_content("m1", "a1", at_most(1000))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge, "{err}");
    assert!(err.message().contains("larger than the limit of 1000 bytes"), "{err}");
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "a file that is too large is not asked for again"
    );

    // What Gmail writes around the file does not count against the limit:
    // a long id, and an answer set out over several lines.
    let file = vec![0xa7_u8; 3000];
    let around = json!({ "attachmentId": "A".repeat(2000), "size": 3000, "data": URL_SAFE.encode(&file) });
    let server = MockServer::start().await;
    let pretty = serde_json::to_vec_pretty(&around).unwrap();
    let served = ResponseTemplate::new(200).set_body_raw(pretty, "application/json; charset=UTF-8");
    Mock::given(any()).respond_with(served).mount(&server).await;
    let (google, connection) = typed(&server).await;
    let messages = google.gmail_messages(&connection);
    let content = messages.attachment_content("m1", "a1", at_most(3000)).await.unwrap();
    assert_eq!(content.bytes, file);

    // With no limit given, a file is accepted up to ten megabytes: more
    // than the megabyte an operation called by name is given.
    let file = vec![0x5a_u8; 2 * 1024 * 1024];
    let server = attaching(&file).await;
    let (google, connection) = typed(&server).await;
    let messages = google.gmail_messages(&connection);
    let content = messages
        .attachment_content("m1", "a1", Download::default())
        .await
        .unwrap();
    assert_eq!(content.len(), file.len());
}

#[tokio::test]
async fn an_attachment_that_is_text_is_read_as_text_by_name() {
    let csv = "date,total\r\n2026-10-09,42\r\nGrüße,☕\r\n";
    let (server, socket, key) = google().await;
    let body = json!({ "size": csv.len(), "data": URL_SAFE.encode(csv) });
    Mock::given(any()).respond_with(answer(200, &body)).mount(&server).await;
    let input = json!({ "message": GMAIL_MESSAGE, "attachment": GMAIL_ATTACHMENT });
    let read = invoke(&socket, &key, "gmail_messages.attachment_text", input)
        .await
        .unwrap();
    assert_eq!(read, json!({ "size": csv.len(), "text": csv }));
    let request = only_request(&server).await;
    assert_eq!(
        (request.method.as_str(), request.url.path()),
        (
            "GET",
            format!("{GMAIL_MESSAGES}/{GMAIL_MESSAGE}/attachments/{GMAIL_ATTACHMENT}").as_str()
        )
    );
    assert_eq!(request.url.query(), None, "the limit is Socket's own, and is not sent");

    // A byte order mark is left out of the text; the size is the file's.
    for (file, text) in [
        (&b"\xef\xbb\xbfid,name\n1,Ada\n"[..], "id,name\n1,Ada\n"),
        (
            &b"BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n"[..],
            "BEGIN:VCALENDAR\r\nEND:VCALENDAR\r\n",
        ),
        (&b""[..], ""),
    ] {
        let (_server, socket, key) = answering(200, json!({ "size": file.len(), "data": URL_SAFE.encode(file) })).await;
        let input = json!({ "message": "m1", "attachment": "a1", "maxBytes": 4096 });
        let read = invoke(&socket, &key, "gmail_messages.attachment_text", input)
            .await
            .unwrap();
        assert_eq!(read, json!({ "size": file.len(), "text": text }));
    }
}

#[tokio::test]
async fn a_file_that_is_not_text_is_refused_by_name_and_nothing_of_it_is_returned() {
    let pdf = [&b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\nCONFIDENTIAL offer letter\n"[..], FILE].concat();
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR CONFIDENTIAL".to_vec();
    let latin = b"CONFIDENTIAL caf\xe9".to_vec();
    // UTF-8 as far as its bytes go, and a binary file all the same: a NUL,
    // and an escape that would clear the screen it is shown on.
    let zip = b"PK\x03\x04CONFIDENTIAL\0\0".to_vec();
    let escape = b"CONFIDENTIAL \x1b[2J\x1b[H".to_vec();
    // The same escape as one character, which a terminal reads as well.
    let one_character = "CONFIDENTIAL \u{9b}2J".as_bytes().to_vec();
    for file in [pdf, png, latin, zip, escape, one_character] {
        let (server, socket, key) = google().await;
        let body = json!({ "size": file.len(), "data": URL_SAFE.encode(&file) });
        Mock::given(any()).respond_with(answer(200, &body)).mount(&server).await;
        let input = json!({ "message": GMAIL_MESSAGE, "attachment": GMAIL_ATTACHMENT });
        let err = invoke(&socket, &key, "gmail_messages.attachment_text", input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unsupported, "{err}");
        assert_eq!(
            err.message(),
            format!(
                "google returned an attachment of {} bytes, which is not text; an operation called by name returns text only",
                file.len()
            )
        );
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(
            !everything.contains("CONFIDENTIAL") && !everything.contains("PDF") && !everything.contains("JVBER"),
            "{everything}"
        );

        // The typed method returns the same file as it is.
        let (google, connection) = typed(&server).await;
        let content = google
            .gmail_messages(&connection)
            .attachment_content(GMAIL_MESSAGE, GMAIL_ATTACHMENT, Download::default())
            .await
            .unwrap();
        assert_eq!(content.bytes, file);
    }
}

#[tokio::test]
async fn text_by_name_is_a_megabyte_unless_more_is_asked_for_and_never_over_ten() {
    const MEGABYTE: usize = 1024 * 1024;
    let lines = |bytes: usize| "0123456789abcde\n".repeat(bytes / 16) + &"x".repeat(bytes % 16);
    let input = |most: Option<usize>| {
        let mut input = json!({ "message": "m1", "attachment": "a1" });
        if let Some(most) = most {
            input["maxBytes"] = json!(most);
        }
        input
    };

    let text = "gmail_messages.attachment_text";
    // A connection to a server that hands over `file`, for an operation called by name.
    async fn serving(file: &str) -> (MockServer, socketkit_core::Socket, socketkit_core::ConnectionKey) {
        let server = attaching(file.as_bytes()).await;
        let google = Google::with_spec(point_at(provider(), &server));
        let (socket, key) = connect(Arc::new(google), support::TOKEN).await;
        (server, socket, key)
    }

    // Exactly a megabyte, with no limit given.
    let exactly = lines(MEGABYTE);
    let (_server, socket, key) = serving(&exactly).await;
    let read = invoke(&socket, &key, text, input(None)).await.unwrap();
    assert_eq!(read["size"], MEGABYTE);
    assert_eq!(
        read["text"].as_str().unwrap().len(),
        MEGABYTE,
        "all of it, not cut short"
    );

    // One byte more is refused, and is read when the caller allows it.
    let over = lines(MEGABYTE + 1);
    let (_server, socket, key) = serving(&over).await;
    let err = invoke(&socket, &key, text, input(None)).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge, "{err}");
    assert!(
        err.message().contains("larger than the limit of 1048576 bytes"),
        "{err}"
    );
    assert!(err.message().len() < 300, "nothing of the file is in the error");
    let read = invoke(&socket, &key, text, input(Some(MEGABYTE + 1))).await.unwrap();
    assert_eq!(read["text"].as_str().unwrap(), over);
    let err = invoke(&socket, &key, text, input(Some(MEGABYTE))).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::TooLarge, "{err}");

    // Ten megabytes is the most that can be asked for as text, and a call
    // that asks for more is refused before Google is called.
    let (server, socket, key) = answering(200, json!({ "size": 1, "data": "eA" })).await;
    for (asked, says) in [
        (json!(10 * MEGABYTE + 1), "`maxBytes` can be at most 10485760 for text"),
        (json!(usize::MAX), "`maxBytes` can be at most 10485760 for text"),
        (json!(-1), "a field of the wrong type"),
        (json!("1000"), "a field of the wrong type"),
    ] {
        let mut asking = input(None);
        asking["maxBytes"] = asked;
        let err = invoke(&socket, &key, "gmail_messages.attachment_text", asking.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{asking}: {err}");
        assert!(err.message().contains(says), "{asking}: {err}");
    }
    let unknown = json!({ "message": "m1", "attachment": "a1", "timeoutSecs": 5 });
    let err = invoke(&socket, &key, "gmail_messages.attachment_text", unknown)
        .await
        .unwrap_err();
    assert!(err.message().contains("`timeoutSecs` is not a field"), "{err}");
    assert!(server.received_requests().await.unwrap().is_empty());
    // Ten megabytes itself is asked for without complaint.
    let read = invoke(
        &socket,
        &key,
        "gmail_messages.attachment_text",
        input(Some(10 * MEGABYTE)),
    )
    .await
    .unwrap();
    assert_eq!(read, json!({ "size": 1, "text": "x" }));
}

#[tokio::test]
async fn an_answer_that_is_not_an_attachment_is_an_error_that_repeats_none_of_it() {
    let secret = "CONFIDENTIAL offer letter for Grace";
    let page = format!("<html><body>{secret}</body></html>");
    for (served, says) in [
        // Not JSON at all, and JSON that is not an object.
        (
            ResponseTemplate::new(200).set_body_raw(page, "text/html"),
            "google sent an attachment that could not be read",
        ),
        (
            ResponseTemplate::new(200).set_body_raw(format!("{{\"data\": \"{secret}"), "application/json"),
            "google sent an attachment that could not be read",
        ),
        (
            answer(200, &json!([secret])),
            "google sent an attachment that could not be read",
        ),
        // Data that is not base64, named by where it sits.
        (
            answer(200, &json!({ "size": 35, "data": secret })),
            "google sent an attachment that could not be read, at `data`",
        ),
        (
            answer(200, &json!({ "size": [secret], "data": "eA" })),
            "google sent an attachment that could not be read, at `size`",
        ),
        // No data, in an answer that is otherwise well made.
        (answer(200, &json!({})), "google answered without an attachment"),
        (
            answer(200, &json!({ "attachmentId": secret })),
            "google answered without an attachment",
        ),
        (
            answer(200, &json!({ "size": 41230 })),
            "google answered without an attachment",
        ),
        (
            answer(200, &json!({ "size": 41230, "data": "" })),
            "google answered without an attachment",
        ),
    ] {
        let server = MockServer::start().await;
        Mock::given(any()).respond_with(served).mount(&server).await;
        let (google, connection) = typed(&server).await;
        let messages = google.gmail_messages(&connection);
        let content = messages.attachment_content("m1", "a1", Download::default()).await;
        let text = messages.attachment_text("m1", "a1", TextLimit::default()).await;
        for err in [content.unwrap_err(), text.unwrap_err()] {
            assert_eq!(err.kind(), ErrorKind::Decode, "{says}: {err}");
            assert!(err.message().ends_with(says), "{says}: {err}");
            let everything = format!("{err} {err:?} {:?}", err.to_wire());
            assert!(!everything.contains("CONFIDENTIAL"), "{everything}");
        }
    }

    // Gmail's own refusals are what they are for any other request.
    let server = MockServer::start().await;
    let gone = google_error(404, "notFound", "Requested entity was not found.");
    Mock::given(any()).respond_with(gone).mount(&server).await;
    let (google, connection) = typed(&server).await;
    let messages = google.gmail_messages(&connection);
    let err = messages
        .attachment_content("m1", "a1", Download::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound, "{err}");
}
