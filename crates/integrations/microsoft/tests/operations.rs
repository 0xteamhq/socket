//! Every Microsoft operation, called by name against a local server that answers as Microsoft Graph does.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Effect, ErrorKind, Integration, Socket};
use socketkit_microsoft::models::Paging;
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::{method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// An online meeting's id as Graph writes it, with characters that must be encoded in a path.
const MEETING: &str = "MSpkYzE3Njc0Yy04MWQ5*MCoqMTk6bWVldGluZ18@thread.v2";
const MEETING_PATH: &str = "MSpkYzE3Njc0Yy04MWQ5%2AMCoqMTk6bWVldGluZ18%40thread.v2";
/// A transcript's id ends in base64 padding.
const TRANSCRIPT: &str = "MSMjMCMjNzU3ODc2ZDY=";
const TRANSCRIPT_PATH: &str = "MSMjMCMjNzU3ODc2ZDY%3D";
const RECORDING: &str = "7e31db25-bc6e-4fd8-96c7-e01264e9b6fc";
const REPORT: &str = "c9b6db1c-d5eb-427d-a5c0-20088d9b22d7";
/// A join URL as it appears on a calendar event: already percent-encoded once.
const JOIN_URL: &str =
    "https://teams.microsoft.com/l/meetup-join/19%3ameeting_MGQ4%40thread.v2/0?context=%7b%22Tid%22%3a%22909c%22%7d";

const VTT: &str = "WEBVTT\n\n00:00:16.246 --> 00:00:17.726\n<v Ada Lovelace>We ship on Friday.</v>\n";

/// What Graph answers with.
enum Reply {
    Json(Value),
    Text(&'static str, &'static str),
}

/// One operation's expected behaviour.
struct Case {
    name: &'static str,
    input: Value,
    /// The path Graph is asked for, after `/v1.0/`, exactly as it is sent.
    path: String,
    /// Exactly the query that reaches Graph, decoded.
    query: Value,
    /// The `Accept` header that reaches Graph.
    accept: &'static str,
    reply: Reply,
    /// What the operation returns. Checked as a subset, so models may carry more fields.
    returns: Value,
}

fn organizer() -> Value {
    json!({ "application": null, "device": null, "user": { "id": "u-1", "displayName": null, "tenantId": "t-1" } })
}

fn meeting() -> Value {
    json!({
        "id": MEETING,
        "subject": "Launch review",
        "startDateTime": "2026-09-29T22:35:31.389759Z",
        "endDateTime": "2026-09-29T23:35:31.389759Z",
        "joinWebUrl": JOIN_URL,
        "allowTranscription": true,
        "chatInfo": { "threadId": "19:meeting_MGQ4@thread.v2", "messageId": "0", "replyChainMessageId": null },
        "participants": {
            "organizer": { "upn": "ada@example.test", "role": "presenter", "identity": organizer() },
            "attendees": null
        }
    })
}

fn transcript() -> Value {
    json!({
        "id": TRANSCRIPT,
        "meetingId": MEETING,
        "callId": "af630fe0",
        "contentCorrelationId": "bc842d7a-0",
        "createdDateTime": "2026-09-17T06:09:24.8968037Z",
        "endDateTime": "2026-09-17T06:27:25.2346000Z",
        "transcriptContentUrl": "https://graph.microsoft.com/v1.0/me/onlineMeetings/m/transcripts/t/content",
        "meetingOrganizer": organizer()
    })
}

fn recording() -> Value {
    json!({
        "id": RECORDING,
        "meetingId": MEETING,
        "callId": "af630fe0",
        "contentCorrelationId": "bc842d7a-0",
        "createdDateTime": "2026-09-17T06:09:24.8968037Z",
        "endDateTime": "2026-09-17T06:27:25.2346000Z",
        "recordingContentUrl": "https://graph.microsoft.com/v1.0/me/onlineMeetings/m/recordings/r/content",
        "meetingOrganizer": organizer()
    })
}

fn record() -> Value {
    json!({
        "emailAddress": "ada@example.test",
        "totalAttendanceInSeconds": 322,
        "role": "Organizer",
        "identity": { "id": "u-1", "displayName": "Ada Lovelace", "tenantId": null },
        "attendanceIntervals": [
            { "joinDateTime": "2026-10-05T04:38:27.6027225Z", "leaveDateTime": "2026-10-05T04:43:49.7702391Z", "durationInSeconds": 322 }
        ]
    })
}

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, path: String, query, accept, reply, returns| Case { name, input, path, query, accept, reply, returns };
    let meetings = format!("me/onlineMeetings/{MEETING_PATH}");
    let next = "https://graph.microsoft.com/v1.0/me/onlineMeetings/m/transcripts?$skiptoken=abc";
    vec![
        // online meetings
        case("online_meetings.get", json!({ "meeting": MEETING }), meetings.clone(), json!({}), "application/json",
            Reply::Json(meeting()),
            json!({ "id": MEETING, "subject": "Launch review", "joinWebUrl": JOIN_URL, "allowTranscription": true,
                    "chatInfo": { "threadId": "19:meeting_MGQ4@thread.v2" },
                    "participants": { "organizer": { "upn": "ada@example.test", "identity": { "user": { "id": "u-1" } } }, "attendees": [] } })),
        case("online_meetings.find_by_join_url", json!({ "join_url": JOIN_URL }), "me/onlineMeetings".into(),
            json!({ "$filter": format!("JoinWebUrl eq '{JOIN_URL}'") }), "application/json",
            Reply::Json(json!({ "value": [meeting()] })), json!({ "id": MEETING, "joinWebUrl": JOIN_URL })),

        // transcripts
        case("transcripts.list", json!({ "meeting": MEETING, "top": 5 }), format!("{meetings}/transcripts"), json!({ "$top": "5" }), "application/json",
            Reply::Json(json!({ "@odata.count": 1, "value": [transcript()], "@odata.nextLink": next })),
            json!({ "items": [{ "id": TRANSCRIPT, "meetingId": MEETING, "meetingOrganizer": { "user": { "id": "u-1" } } }], "next_cursor": next })),
        case("transcripts.get", json!({ "meeting": MEETING, "transcript": TRANSCRIPT }), format!("{meetings}/transcripts/{TRANSCRIPT_PATH}"), json!({}), "application/json",
            Reply::Json(transcript()), json!({ "id": TRANSCRIPT, "contentCorrelationId": "bc842d7a-0", "endDateTime": "2026-09-17T06:27:25.2346000Z" })),
        case("transcripts.content", json!({ "meeting": MEETING, "transcript": TRANSCRIPT }), format!("{meetings}/transcripts/{TRANSCRIPT_PATH}/content"), json!({}), "text/vtt",
            Reply::Text(VTT, "text/vtt"),
            json!({ "text": VTT, "entries": [{ "speaker": "Ada Lovelace", "startMs": 16_246, "endMs": 17_726, "text": "We ship on Friday." }] })),

        // recordings
        case("recordings.list", json!({ "meeting": MEETING }), format!("{meetings}/recordings"), json!({}), "application/json",
            Reply::Json(json!({ "value": [recording()] })), json!({ "items": [{ "id": RECORDING, "meetingId": MEETING }], "next_cursor": null })),
        case("recordings.get", json!({ "meeting": MEETING, "recording": RECORDING }), format!("{meetings}/recordings/{RECORDING}"), json!({}), "application/json",
            Reply::Json(recording()), json!({ "id": RECORDING, "recordingContentUrl": "https://graph.microsoft.com/v1.0/me/onlineMeetings/m/recordings/r/content" })),

        // attendance
        case("attendance.reports", json!({ "meeting": MEETING }), format!("{meetings}/attendanceReports"), json!({}), "application/json",
            Reply::Json(json!({ "value": [{ "id": REPORT, "totalParticipantCount": 2, "meetingStartDateTime": "2026-10-05T04:38:23.945Z", "meetingEndDateTime": "2026-10-05T04:43:49.77Z", "attendanceRecords": [] }] })),
            json!({ "items": [{ "id": REPORT, "totalParticipantCount": 2, "meetingEndDateTime": "2026-10-05T04:43:49.77Z" }], "next_cursor": null })),
        case("attendance.records", json!({ "meeting": MEETING, "report": REPORT, "top": 50 }), format!("{meetings}/attendanceReports/{REPORT}/attendanceRecords"), json!({ "$top": "50" }), "application/json",
            Reply::Json(json!({ "value": [record(), { "emailAddress": null, "identity": null, "attendanceIntervals": null }] })),
            json!({ "items": [
                { "emailAddress": "ada@example.test", "role": "Organizer", "totalAttendanceInSeconds": 322, "identity": { "displayName": "Ada Lovelace" },
                  "attendanceIntervals": [{ "joinDateTime": "2026-10-05T04:38:27.6027225Z", "durationInSeconds": 322 }] },
                { "emailAddress": null, "attendanceIntervals": [] }
            ], "next_cursor": null })),
    ]
}

/// True when every part of `expected` is present in `actual`.
fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => e.iter().all(|(k, v)| a.get(k).is_some_and(|av| contains(av, v))),
        (Value::Array(a), Value::Array(e)) => a.len() == e.len() && a.iter().zip(e).all(|(av, ev)| contains(av, ev)),
        _ => actual == expected,
    }
}

async fn microsoft() -> (MockServer, Microsoft, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let microsoft = Microsoft::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(microsoft.clone()), "eyJ-good").await;
    (server, microsoft, socket, key)
}

fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
}

fn query_of(request: &socketkit_testkit::wiremock::Request) -> Value {
    Value::Object(
        request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
            .collect(),
    )
}

async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("microsoft.{name}"), input).await
}

#[tokio::test]
async fn the_table_above_covers_every_operation_microsoft_offers() {
    let listed: Vec<String> = Microsoft::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = cases().iter().map(|c| format!("microsoft.{}", c.name)).collect();
    tested.push("microsoft.identity.get".to_owned());
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist"
    );
    assert_eq!(listed.len(), 10);
}

#[tokio::test]
async fn every_operation_asks_graph_for_the_right_thing_and_returns_what_graph_sent() {
    for case in cases() {
        let (server, _, socket, key) = microsoft().await;
        let reply = match &case.reply {
            Reply::Json(body) => ok(body.clone()),
            Reply::Text(text, kind) => ResponseTemplate::new(200).set_body_raw(*text, kind),
        };
        // Only a GET to exactly this path is answered: a read sent any other way fails the case.
        Mock::given(method("GET"))
            .and(path(format!("/v1.0/{}", case.path)))
            .respond_with(reply)
            .mount(&server)
            .await;

        let output = invoke(&socket, &key, case.name, case.input.clone())
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(
            contains(&output, &case.returns),
            "{}: returned {output}, expected {}",
            case.name,
            case.returns
        );

        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1, "{}: one call to Graph", case.name);
        let request = &received[0];
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            "Bearer eyJ-good",
            "{}",
            case.name
        );
        assert_eq!(request.headers.get("accept").unwrap(), case.accept, "{}", case.name);
        assert!(request.body.is_empty(), "{}: a read sends no body", case.name);
        assert_eq!(
            query_of(request),
            case.query,
            "{}: exactly this query reaches Graph",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_is_a_read_and_names_the_permission_it_needs() {
    let operations = Microsoft::new().operations();
    for operation in &operations {
        assert_eq!(operation.effect, Effect::Read, "{}", operation.name);
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert!(!operation.description.is_empty(), "{}", operation.name);
    }
    let scopes = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == format!("microsoft.{name}"))
            .unwrap_or_else(|| panic!("{name}"))
            .required_scopes
            .clone()
    };
    for name in ["online_meetings.get", "online_meetings.find_by_join_url"] {
        assert_eq!(scopes(name), ["OnlineMeetings.Read"], "{name}");
    }
    for name in ["transcripts.list", "transcripts.get", "transcripts.content"] {
        assert_eq!(scopes(name), ["OnlineMeetingTranscript.Read.All"], "{name}");
    }
    for name in ["recordings.list", "recordings.get"] {
        assert_eq!(scopes(name), ["OnlineMeetingRecording.Read.All"], "{name}");
    }
    for name in ["attendance.reports", "attendance.records"] {
        assert_eq!(scopes(name), ["OnlineMeetingArtifact.Read.All"], "{name}");
    }
}

#[tokio::test]
async fn the_input_schema_requires_the_identifiers_and_describes_paging_as_optional() {
    let operations = Microsoft::new().operations();
    let records = operations
        .iter()
        .find(|o| o.name == "microsoft.attendance.records")
        .unwrap();
    let mut required: Vec<&str> = records.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["meeting", "report"]);
    for field in ["meeting", "report", "cursor", "top"] {
        assert!(
            records.input_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    let content = operations
        .iter()
        .find(|o| o.name == "microsoft.transcripts.content")
        .unwrap();
    for field in ["text", "entries"] {
        assert!(
            content.output_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
}

#[tokio::test]
async fn a_join_url_is_sent_inside_the_filter_exactly_as_the_calendar_gave_it() {
    let (server, _, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me/onlineMeetings"))
        .respond_with(ok(json!({ "value": [meeting()] })))
        .mount(&server)
        .await;
    // A quote would end the filter's string early; OData doubles it.
    let quoted = "https://teams.microsoft.com/l/meetup-join/it's";
    for join_url in [JOIN_URL, quoted] {
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
    let raw = received[0].url.query().unwrap();
    // Graph documents the filter with `%20` for its spaces, and the URL's own
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
async fn a_join_url_that_matches_no_meeting_is_not_found() {
    let (server, _, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me/onlineMeetings"))
        .respond_with(ok(json!({ "value": [] })))
        .mount(&server)
        .await;
    let err = invoke(
        &socket,
        &key,
        "online_meetings.find_by_join_url",
        json!({ "join_url": JOIN_URL }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(err.message().contains("join URL"), "{}", err.message());
    assert!(!err.message().contains("meetup-join"), "{}", err.message());
}

#[tokio::test]
async fn the_next_page_is_asked_for_with_the_link_graph_gave_and_nothing_added() {
    let (server, _, socket, key) = microsoft().await;
    // Graph's link names the user where the first request said `me`.
    let next = format!(
        "{}/v1.0/users('u-1')/onlineMeetings('m')/transcripts?$skiptoken=page-2&$top=1",
        server.uri()
    );
    Mock::given(path("/v1.0/users('u-1')/onlineMeetings('m')/transcripts"))
        .respond_with(ok(json!({ "value": [transcript()] })))
        .mount(&server)
        .await;
    let page = invoke(
        &socket,
        &key,
        "transcripts.list",
        json!({ "meeting": MEETING, "cursor": next, "top": 9 }),
    )
    .await
    .unwrap();
    assert_eq!(page["items"][0]["id"], TRANSCRIPT);
    assert_eq!(page["next_cursor"], Value::Null);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1);
    assert_eq!(
        query_of(&received[0]),
        json!({ "$skiptoken": "page-2", "$top": "1" }),
        "the link already carries the page size"
    );
}

#[tokio::test]
async fn a_cursor_that_points_outside_the_graph_api_is_refused_without_a_request() {
    let (server, _, socket, key) = microsoft().await;
    let elsewhere = [
        "https://evil.example/v1.0/me/onlineMeetings/m/transcripts".to_owned(),
        // The same host, but not the API this connection is for.
        format!("{}/beta/me/onlineMeetings/m/transcripts", server.uri()),
        format!("{}/v1.0/../common/oauth2/v2.0/token", server.uri()),
        format!("{}/v1.0beta/me", server.uri()),
        "not a link".to_owned(),
    ];
    for cursor in elsewhere {
        let err = invoke(
            &socket,
            &key,
            "transcripts.list",
            json!({ "meeting": MEETING, "cursor": cursor }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{cursor}");
        assert!(err.message().contains("cursor"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_identifier_stays_one_path_segment_and_one_that_cannot_is_refused() {
    let (server, _, socket, key) = microsoft().await;
    Mock::given(path("/v1.0/me/onlineMeetings/a%2Fb%3Fc%23d/transcripts/..%2Fx"))
        .respond_with(ok(transcript()))
        .expect(1)
        .mount(&server)
        .await;
    invoke(
        &socket,
        &key,
        "transcripts.get",
        json!({ "meeting": "a/b?c#d", "transcript": "../x" }),
    )
    .await
    .unwrap();

    for (meeting, transcript) in [("", "t"), (" ", "t"), (".", "t"), ("..", "t"), ("m", ""), ("m", "..")] {
        let err = invoke(
            &socket,
            &key,
            "transcripts.get",
            json!({ "meeting": meeting, "transcript": transcript }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{meeting:?} {transcript:?}");
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn a_success_that_does_not_carry_what_was_asked_for_is_an_error() {
    let (server, _, socket, key) = microsoft().await;
    // An object without an id, a list without `value`, and a transcript with nothing in it.
    Mock::given(path(format!("/v1.0/me/onlineMeetings/{MEETING_PATH}")))
        .respond_with(ok(json!({ "subject": "Launch review" })))
        .mount(&server)
        .await;
    Mock::given(path(format!("/v1.0/me/onlineMeetings/{MEETING_PATH}/recordings")))
        .respond_with(ok(json!({ "error": null })))
        .mount(&server)
        .await;
    Mock::given(path(format!(
        "/v1.0/me/onlineMeetings/{MEETING_PATH}/transcripts/{TRANSCRIPT_PATH}/content"
    )))
    .respond_with(ResponseTemplate::new(200))
    .mount(&server)
    .await;
    let both = json!({ "meeting": MEETING, "transcript": TRANSCRIPT });
    for (name, input) in [
        ("online_meetings.get", json!({ "meeting": MEETING })),
        ("recordings.list", json!({ "meeting": MEETING })),
        ("transcripts.content", both),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name}: {err}");
    }
}

#[tokio::test]
async fn a_transcript_that_cannot_be_read_is_an_error_that_does_not_repeat_what_was_said() {
    // A cue with a broken timing, and a page that is not a transcript at all.
    for body in [
        "WEBVTT\n\n00:00 --> soon\n<v Ada>The password is hunter2.</v>\n",
        "<html><body>hunter2: sign in to continue</body></html>",
    ] {
        let (server, _, socket, key) = microsoft().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/vtt"))
            .mount(&server)
            .await;
        let err = invoke(
            &socket,
            &key,
            "transcripts.content",
            json!({ "meeting": MEETING, "transcript": TRANSCRIPT }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{body}");
        let everything = format!("{err} {err:?}");
        assert!(!everything.contains("hunter2"), "{everything}");
        assert_eq!(err.provider().map(|p| p.as_str()), Some("microsoft"));
    }
}

#[tokio::test]
async fn the_typed_methods_return_the_same_data_as_the_named_operations() {
    let (server, microsoft, socket, key) = microsoft().await;
    Mock::given(path(format!(
        "/v1.0/me/onlineMeetings/{MEETING_PATH}/transcripts/{TRANSCRIPT_PATH}/content"
    )))
    .respond_with(ResponseTemplate::new(200).set_body_raw(VTT, "text/vtt"))
    .mount(&server)
    .await;
    Mock::given(path(format!("/v1.0/me/onlineMeetings/{MEETING_PATH}/attendanceReports")))
        .respond_with(ok(
            json!({ "value": [{ "id": REPORT, "totalParticipantCount": 2 }], "@odata.nextLink": format!("{}/v1.0/next", server.uri()) }),
        ))
        .mount(&server)
        .await;
    let connection = socket.connection(key).await.unwrap();

    let content = microsoft
        .transcripts(&connection)
        .content(MEETING, TRANSCRIPT)
        .await
        .unwrap();
    assert_eq!(content.text, VTT);
    assert_eq!(content.entries[0].speaker.as_deref(), Some("Ada Lovelace"));
    assert_eq!(
        (content.entries[0].start_ms, content.entries[0].end_ms),
        (16_246, 17_726)
    );

    let paging = Paging {
        top: Some(1),
        ..Paging::default()
    };
    let reports = microsoft
        .attendance(&connection)
        .reports(MEETING, paging)
        .await
        .unwrap();
    assert_eq!(reports.items[0].id, REPORT);
    assert_eq!(reports.items[0].total_participant_count, Some(2));
    assert_eq!(
        reports.next_cursor.as_deref(),
        Some(format!("{}/v1.0/next", server.uri()).as_str())
    );
}

#[tokio::test]
async fn an_input_error_names_the_field_but_never_repeats_the_callers_values() {
    let (server, _, socket, key) = microsoft().await;
    let secret = "eyJ-PRIVATE-do-not-log";
    let bad = [
        // Paging sits beside the identifiers, so serde cannot say which of its fields was wrong.
        (
            "transcripts.list",
            json!({ "meeting": MEETING, "top": secret }),
            "input",
        ),
        (
            "transcripts.get",
            json!({ "meeting": { "nested": secret }, "transcript": "t" }),
            "meeting",
        ),
        ("recordings.get", json!({ "meeting": MEETING }), "recording"),
    ];
    for (name, input, names) in bad {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
        let everything = format!("{err} {err:?} {}", serde_json::to_string(&err.to_wire()).unwrap());
        assert!(!everything.contains("PRIVATE"), "{name}: {everything}");
        assert!(err.message().contains(names), "{name}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn microsoft_under_another_provider_id_is_refused_when_the_socket_is_built() {
    let mut spec = provider();
    spec.id = socketkit_core::ProviderId::new("graph").unwrap();
    let integration: Arc<dyn Integration> = Arc::new(Microsoft::with_spec(spec));
    let err = Socket::in_memory().integration(integration).build().unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Config);
    assert!(err.message().contains("microsoft"), "{}", err.message());
}
