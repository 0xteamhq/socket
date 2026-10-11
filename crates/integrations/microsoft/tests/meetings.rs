//! Teams meetings against a local server that answers as Microsoft Graph does.
//!
//! What every operation sends and returns is in the table in `operations.rs`,
//! and reading WebVTT is in `webvtt.rs`. This file holds the rest of what is
//! particular to meetings.

use std::sync::Arc;

use serde_json::json;
use socketkit_core::ErrorKind;
use socketkit_microsoft::models::Paging;
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    MEETING, MEETING_LINK, MEETING_PATH, TRANSCRIPT, TRANSCRIPT_PATH, VTT, answer, answering, graph_error, invoke,
    meeting, microsoft, only_request, query_of, transcript,
};

fn vtt(text: &'static str) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_raw(text, "text/vtt")
}

#[tokio::test]
async fn a_join_link_is_sent_inside_the_filter_exactly_as_the_calendar_gave_it() {
    let (server, socket, key) = answering(200, json!({ "value": [meeting()] })).await;
    // A quote would end the filter's string early; OData doubles it.
    let quoted = "https://teams.microsoft.com/l/meetup-join/it's";
    for join_url in [MEETING_LINK, quoted] {
        invoke(
            &socket,
            &key,
            "online_meetings.find_by_join_url",
            json!({ "join_url": join_url }),
        )
        .await
        .unwrap();
    }
    let received = server.received_requests().await.unwrap();
    assert_eq!(received[0].url.path(), "/v1.0/me/onlineMeetings");
    let raw = received[0].url.query().unwrap();
    // Graph documents the filter with `%20` for its spaces, and the link's own
    // percent signs encoded once more.
    assert!(raw.starts_with("$filter=JoinWebUrl%20eq%20%27https%3A%2F%2F"), "{raw}");
    assert!(raw.contains("19%253ameeting_MGQ4%2540thread.v2"), "{raw}");
    assert!(!raw.contains('+'), "{raw}");
    assert_eq!(
        query_of(&received[1])["$filter"],
        "JoinWebUrl eq 'https://teams.microsoft.com/l/meetup-join/it''s'"
    );
}

#[tokio::test]
async fn a_join_link_that_matches_no_meeting_is_not_found_and_is_not_repeated() {
    let (_server, socket, key) = answering(200, json!({ "value": [] })).await;
    let err = invoke(
        &socket,
        &key,
        "online_meetings.find_by_join_url",
        json!({ "join_url": MEETING_LINK }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(err.message().contains("join URL"), "{}", err.message());
    // Whoever holds the link can join the meeting.
    assert!(!format!("{err} {err:?}").contains("meetup-join"), "{err:?}");

    let (server, socket, key) = answering(200, json!({ "value": [meeting()] })).await;
    for blank in ["", "  "] {
        let err = invoke(
            &socket,
            &key,
            "online_meetings.find_by_join_url",
            json!({ "join_url": blank }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_identifier_stays_one_path_segment_and_one_that_cannot_is_refused() {
    let (server, socket, key) = answering(200, transcript()).await;
    invoke(
        &socket,
        &key,
        "transcripts.get",
        json!({ "meeting": "a/b?c#d", "transcript": "../x" }),
    )
    .await
    .unwrap();
    let request = only_request(&server).await;
    assert_eq!(
        request.url.path(),
        "/v1.0/me/onlineMeetings/a%2Fb%3Fc%23d/transcripts/..%2Fx"
    );
    assert_eq!(request.url.query(), None);

    let (server, socket, key) = answering(200, transcript()).await;
    for (name, input) in [
        ("transcripts.get", json!({ "meeting": "", "transcript": "t" })),
        ("transcripts.get", json!({ "meeting": "..", "transcript": "t" })),
        ("transcripts.get", json!({ "meeting": "m", "transcript": "" })),
        ("transcripts.content", json!({ "meeting": "m", "transcript": ".." })),
        ("transcripts.list", json!({ "meeting": "." })),
        ("online_meetings.get", json!({ "meeting": " " })),
        ("recordings.get", json!({ "meeting": "m", "recording": ".." })),
        ("recordings.list", json!({ "meeting": "" })),
        ("attendance.reports", json!({ "meeting": ".." })),
        ("attendance.records", json!({ "meeting": "m", "report": "" })),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_success_that_does_not_carry_what_was_asked_for_is_an_error() {
    let both = json!({ "meeting": MEETING, "transcript": TRANSCRIPT });
    for (name, input, response) in [
        // An object without an id, and a list without `value`.
        (
            "online_meetings.get",
            json!({ "meeting": MEETING }),
            json!({ "subject": "Launch review" }),
        ),
        (
            "online_meetings.find_by_join_url",
            json!({ "join_url": MEETING_LINK }),
            json!({ "value": [{ "subject": "Launch review" }] }),
        ),
        (
            "online_meetings.find_by_join_url",
            json!({ "join_url": MEETING_LINK }),
            json!({}),
        ),
        ("transcripts.get", both.clone(), json!({ "meetingId": MEETING })),
        (
            "recordings.list",
            json!({ "meeting": MEETING }),
            json!({ "error": null }),
        ),
        (
            "recordings.get",
            json!({ "meeting": MEETING, "recording": "rec-1" }),
            json!({}),
        ),
        (
            "attendance.records",
            json!({ "meeting": MEETING, "report": "rep-1" }),
            json!({ "value": "none" }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
    }

    // A transcript with nothing in it.
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "transcripts.content", both).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn a_transcript_that_cannot_be_read_is_an_error_that_does_not_repeat_what_was_said() {
    // A cue with a broken timing, and a page that is not a transcript at all.
    for body in [
        "WEBVTT\n\n00:00 --> soon\n<v Ada>The password is hunter2.</v>\n",
        "<html><body>hunter2: sign in to continue</body></html>",
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(method("GET")).respond_with(vtt(body)).mount(&server).await;
        let err = invoke(
            &socket,
            &key,
            "transcripts.content",
            json!({ "meeting": MEETING, "transcript": TRANSCRIPT }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{body}");
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("hunter2"), "{everything}");
        assert_eq!(err.provider().map(|p| p.as_str()), Some("microsoft"));
    }
}

#[tokio::test]
async fn graphs_refusals_of_a_transcript_reach_the_caller() {
    for (response, kind, reason) in [
        // Transcription was never switched on, or the meeting is not this person's to read.
        (
            graph_error(404, "NotFound", "The transcript was not found."),
            ErrorKind::NotFound,
            "has no such resource",
        ),
        // The permission needs an administrator's approval and has none.
        (
            graph_error(
                403,
                "Forbidden",
                "Application is not allowed to perform operations on the user.",
            ),
            ErrorKind::AccessDenied,
            "Application is not allowed to perform operations on the user.",
        ),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let err = invoke(
            &socket,
            &key,
            "transcripts.content",
            json!({ "meeting": MEETING, "transcript": TRANSCRIPT }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().ends_with(reason), "{}", err.message());
    }
}

#[tokio::test]
async fn the_typed_methods_return_the_same_data_as_the_named_operations() {
    let server = MockServer::start().await;
    let microsoft = Microsoft::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(microsoft.clone()), "eyJ.good").await;
    let connection = socket.connection(key).await.unwrap();
    Mock::given(path(format!(
        "/v1.0/me/onlineMeetings/{MEETING_PATH}/transcripts/{TRANSCRIPT_PATH}/content"
    )))
    .respond_with(vtt(VTT))
    .mount(&server)
    .await;
    let next = format!(
        "{}/v1.0/users('u-1')/onlineMeetings('m')/attendanceReports?$skiptoken=page-2",
        server.uri()
    );
    Mock::given(path(format!(
        "/v1.0/me/onlineMeetings/{MEETING_PATH}/attendanceReports"
    )))
    .respond_with(answer(
        200,
        &json!({ "value": [{ "id": "rep-1", "totalParticipantCount": 2 }], "@odata.nextLink": next }),
    ))
    .mount(&server)
    .await;
    Mock::given(path("/v1.0/me/onlineMeetings"))
        .respond_with(answer(200, &json!({ "value": [meeting()] })))
        .mount(&server)
        .await;

    // From the join link on a calendar event to what was said.
    let found = microsoft
        .online_meetings(&connection)
        .find_by_join_url(MEETING_LINK)
        .await
        .unwrap();
    assert_eq!(found.id, MEETING);
    assert_eq!(found.join_web_url.as_deref(), Some(MEETING_LINK));
    let content = microsoft
        .transcripts(&connection)
        .content(&found.id, TRANSCRIPT)
        .await
        .unwrap();
    assert_eq!(content.text, VTT);
    assert_eq!(content.entries[0].speaker.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        (content.entries[0].start_ms, content.entries[0].end_ms),
        (16_246, 17_726)
    );

    let first = Paging {
        limit: Some(1),
        ..Paging::default()
    };
    let reports = microsoft.attendance(&connection).reports(MEETING, first).await.unwrap();
    assert_eq!(reports.items[0].id, "rep-1");
    assert_eq!(reports.items[0].total_participant_count, Some(2));
    assert_eq!(reports.next_cursor.as_deref(), Some(next.as_str()));

    // The next page is asked for at the list's own address, with Graph's place in it.
    server.reset().await;
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [] })))
        .mount(&server)
        .await;
    let rest = Paging {
        cursor: reports.next_cursor,
        ..Paging::default()
    };
    let last = microsoft.attendance(&connection).reports(MEETING, rest).await.unwrap();
    assert!(last.items.is_empty() && last.next_cursor.is_none());
    let request = only_request(&server).await;
    assert_eq!(
        request.url.path(),
        format!("/v1.0/me/onlineMeetings/{MEETING_PATH}/attendanceReports")
    );
    assert_eq!(request.url.query(), Some("$skiptoken=page-2"));
}
