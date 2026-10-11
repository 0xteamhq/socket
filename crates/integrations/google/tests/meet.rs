//! Meet against a local server that answers as Google does.
//!
//! What every operation sends and returns is in the table in `operations.rs`,
//! and reading a timestamp is tested beside its parser. This file holds the
//! rest of what is particular to Meet: how a thing is named, the filter on
//! conference records, paging, and putting a whole transcript together.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, ErrorKind, Socket};
use socketkit_google::models::{MeetListConferenceRecords, MeetReadTranscript, Paging};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::{method, path, query_param, query_param_is_missing};
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::meet::{
    ADA, CALLER, DOCUMENT, FILE, GRACE, MEETING_CODE, MEETING_LINK, RECORD, RECORD_ID, RECORD_PATH, RECORDING, SPACE,
    TRANSCRIPT, TRANSCRIPT_ID, TRANSCRIPT_PATH, ada, caller, conference_record, entry, grace, recording, space,
    transcript,
};
use support::{TOKEN, answer, answering, google, google_error, invoke, only_request, query_of};

/// Answers a GET of `at` with `body`, whatever its query.
async fn serve(server: &MockServer, at: &str, body: Value) {
    Mock::given(method("GET"))
        .and(path(at))
        .respond_with(answer(200, &body))
        .mount(server)
        .await;
}

/// Answers one page of the list at `at`: the first when `token` is `None`,
/// or the one asked for with that page token.
async fn serve_page(server: &MockServer, at: &str, token: Option<&str>, body: Value) {
    let page = Mock::given(method("GET")).and(path(at));
    let page = match token {
        Some(token) => page.and(query_param("pageToken", token)),
        None => page.and(query_param_is_missing("pageToken")),
    };
    page.respond_with(answer(200, &body)).mount(server).await;
}

/// The requests the server received for `at`, as their queries.
async fn queries(server: &MockServer, at: &str) -> Vec<Value> {
    let received = server.received_requests().await.unwrap();
    received
        .iter()
        .filter(|request| request.url.path() == at)
        .map(query_of)
        .collect()
}

async fn read(socket: &Socket, key: &ConnectionKey, options: Value) -> socketkit_core::Result<Value> {
    let mut input = json!({ "record": RECORD, "transcript": TRANSCRIPT });
    for (name, value) in options.as_object().unwrap() {
        input[name] = value.clone();
    }
    invoke(socket, key, "meet_transcripts.read", input).await
}

fn entries_path() -> String {
    format!("{TRANSCRIPT_PATH}/entries")
}

fn participants_path() -> String {
    format!("{RECORD_PATH}/participants")
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent() {
    let (server, socket, key) = answering(200, json!({ "conferenceRecords": [conference_record()] })).await;
    // Nothing given, and options given as blank text, ask for the same thing.
    for input in [
        json!({}),
        json!({ "meetingCode": " ", "space": "", "startTimeMin": "", "startTimeMax": "  ", "cursor": " " }),
    ] {
        let listed = invoke(&socket, &key, "meet_conference_records.list", input)
            .await
            .unwrap();
        assert_eq!(listed["items"][0]["name"], RECORD);
    }
    for request in server.received_requests().await.unwrap() {
        assert_eq!(request.url.path(), "/v2/conferenceRecords");
        assert_eq!(request.url.query(), None, "no filter, no page size, no page token");
    }

    for (name, input) in [
        ("meet_participants.list", json!({ "record": RECORD })),
        ("meet_transcripts.list", json!({ "record": RECORD })),
        ("meet_recordings.list", json!({ "record": RECORD })),
        (
            "meet_transcripts.entries",
            json!({ "record": RECORD, "transcript": TRANSCRIPT }),
        ),
    ] {
        let (server, socket, key) = answering(200, json!({})).await;
        invoke(&socket, &key, name, input).await.unwrap();
        assert_eq!(only_request(&server).await.url.query(), None, "{name}");
    }
}

#[tokio::test]
async fn each_filter_option_becomes_the_clause_google_documents() {
    let (server, socket, key) = answering(200, json!({})).await;
    let cases = [
        (
            json!({ "meetingCode": MEETING_CODE }),
            "space.meeting_code = \"abc-mnop-xyz\"",
        ),
        // A code is typed in any case, and is often at hand as the link.
        (
            json!({ "meetingCode": "ABC-Mnop-XYZ" }),
            "space.meeting_code = \"abc-mnop-xyz\"",
        ),
        (
            json!({ "meetingCode": format!("{MEETING_LINK}?authuser=1&hs=179") }),
            "space.meeting_code = \"abc-mnop-xyz\"",
        ),
        // A space by the name Google returned, or by its id.
        (json!({ "space": SPACE }), "space.name = \"spaces/jQCFfuBOdN5z\""),
        (
            json!({ "space": "jQCFfuBOdN5z" }),
            "space.name = \"spaces/jQCFfuBOdN5z\"",
        ),
        // The example in Google's reference, to the letter.
        (
            json!({ "startTimeMin": "2024-01-01T00:00:00.000Z", "startTimeMax": "2024-01-02T00:00:00.000Z" }),
            "start_time>=\"2024-01-01T00:00:00.000Z\" AND start_time<=\"2024-01-02T00:00:00.000Z\"",
        ),
        (
            json!({ "startTimeMax": "2026-10-02T00:00:00Z" }),
            "start_time<=\"2026-10-02T00:00:00Z\"",
        ),
        // Everything at once. The `+` of an offset has to arrive as a plus,
        // not as the space a bare `+` means in a query.
        (
            json!({ "space": SPACE, "startTimeMin": "2026-10-01T05:30:00+05:30", "startTimeMax": "2026-10-02T00:00:00Z" }),
            "space.name = \"spaces/jQCFfuBOdN5z\" AND start_time>=\"2026-10-01T05:30:00+05:30\" AND start_time<=\"2026-10-02T00:00:00Z\"",
        ),
    ];
    for (input, _) in &cases {
        invoke(&socket, &key, "meet_conference_records.list", input.clone())
            .await
            .unwrap_or_else(|e| panic!("{input}: {e}"));
    }
    let sent = queries(&server, "/v2/conferenceRecords").await;
    assert_eq!(sent.len(), cases.len());
    for ((input, filter), query) in cases.iter().zip(sent) {
        assert_eq!(query, json!({ "filter": filter }), "{input}");
    }
}

#[tokio::test]
async fn a_filter_value_cannot_leave_its_quotes_or_add_a_clause() {
    let (server, socket, key) = answering(200, json!({ "conferenceRecords": [conference_record()] })).await;
    let long_code = "a".repeat(129);
    for (input, field) in [
        // A quote would end the value early, and what follows would be read as a clause.
        (
            json!({ "meetingCode": "abc\" OR end_time IS NULL OR space.meeting_code = \"x" }),
            "meetingCode",
        ),
        (json!({ "meetingCode": "abc-mnop-xyz\\" }), "meetingCode"),
        (json!({ "meetingCode": "abc mnop xyz" }), "meetingCode"),
        (json!({ "meetingCode": long_code }), "meetingCode"),
        (json!({ "meetingCode": "https://meet.google.com/" }), "meetingCode"),
        (
            json!({ "meetingCode": "https://meet.google.com/lookup/abc" }),
            "meetingCode",
        ),
        (json!({ "space": "jQCF\" OR space.name = \"spaces/other" }), "space"),
        (
            json!({ "space": "jQCF\" OR end_time IS NULL OR space.name = \"x" }),
            "space",
        ),
        (json!({ "space": "spaces/jQCF\" OR end_time IS NULL" }), "space"),
        (json!({ "space": "jQCFfuBOdN5z\\" }), "space"),
        (json!({ "space": "jQCF\u{7}fuBOdN5z" }), "space"),
        // A name of something that is not a space.
        (json!({ "space": RECORD }), "space"),
        (json!({ "space": "spaces/a/b" }), "space"),
        (
            json!({ "startTimeMin": "2026-10-01T00:00:00Z\" OR start_time>=\"1970-01-01T00:00:00Z" }),
            "startTimeMin",
        ),
        (json!({ "startTimeMin": "yesterday" }), "startTimeMin"),
        (json!({ "startTimeMax": "2026-10-01" }), "startTimeMax"),
        // Nothing began after it had to have begun.
        (
            json!({ "startTimeMin": "2026-10-02T00:00:00Z", "startTimeMax": "2026-10-01T23:59:59+00:00" }),
            "startTimeMin",
        ),
        // One space has one code: both together say the same thing twice, or nothing.
        (json!({ "meetingCode": MEETING_CODE, "space": SPACE }), "meetingCode"),
        (json!({ "limit": 0 }), "limit"),
        (json!({ "limit": 101 }), "limit"),
    ] {
        let err = invoke(&socket, &key, "meet_conference_records.list", input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}: {err}");
        assert!(err.message().contains(field), "{input}: {}", err.message());
        // What the caller wrote is not repeated back.
        for written in ["end_time", "other", "1970", "yesterday", "lookup"] {
            assert!(!format!("{err} {err:?}").contains(written), "{input}: {err:?}");
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_name_google_returned_and_a_bare_id_reach_the_same_address() {
    let ada_id = ADA.rsplit('/').next().unwrap();
    let cases = [
        (
            "meet_conference_records.get",
            json!({ "record": RECORD }),
            json!({ "record": RECORD_ID }),
            RECORD_PATH.to_owned(),
        ),
        (
            "meet_participants.get",
            json!({ "record": RECORD, "participant": ADA }),
            json!({ "record": RECORD_ID, "participant": ada_id }),
            format!("{RECORD_PATH}/participants/{ada_id}"),
        ),
        // The two forms can be mixed: the record by id, the participant by name.
        (
            "meet_participants.sessions",
            json!({ "record": RECORD_ID, "participant": ADA }),
            json!({ "record": RECORD, "participant": ada_id }),
            format!("{RECORD_PATH}/participants/{ada_id}/participantSessions"),
        ),
        (
            "meet_transcripts.get",
            json!({ "record": RECORD, "transcript": TRANSCRIPT }),
            json!({ "record": RECORD_ID, "transcript": TRANSCRIPT_ID }),
            TRANSCRIPT_PATH.to_owned(),
        ),
        (
            "meet_transcripts.entries",
            json!({ "record": format!("  {RECORD} "), "transcript": format!(" {TRANSCRIPT}") }),
            json!({ "record": RECORD_ID, "transcript": TRANSCRIPT_ID }),
            entries_path(),
        ),
        (
            "meet_recordings.get",
            json!({ "record": RECORD, "recording": RECORDING }),
            json!({ "record": RECORD_ID, "recording": "rec-01" }),
            format!("{RECORD_PATH}/recordings/rec-01"),
        ),
        (
            "meet_spaces.get",
            json!({ "space": SPACE }),
            json!({ "space": "jQCFfuBOdN5z" }),
            "/v2/spaces/jQCFfuBOdN5z".to_owned(),
        ),
    ];
    for (name, by_name, by_id, expected) in cases {
        // An answer that reads as any one thing, and as an empty list.
        let (server, socket, key) = answering(200, json!({ "name": "as-asked" })).await;
        invoke(&socket, &key, name, by_name).await.unwrap();
        invoke(&socket, &key, name, by_id).await.unwrap();
        let received = server.received_requests().await.unwrap();
        let reached: Vec<&str> = received.iter().map(|request| request.url.path()).collect();
        assert_eq!(reached, [expected.as_str(), expected.as_str()], "{name}");
    }
}

#[tokio::test]
async fn a_meeting_code_and_the_link_people_join_by_ask_for_the_same_space() {
    let (server, socket, key) = answering(200, space()).await;
    for given in [
        MEETING_CODE.to_owned(),
        MEETING_LINK.to_owned(),
        format!("{MEETING_LINK}?authuser=0&hs=179#top"),
        format!("spaces/{MEETING_CODE}"),
    ] {
        let found = invoke(&socket, &key, "meet_spaces.get", json!({ "space": given }))
            .await
            .unwrap();
        // The name is what to keep: a code can come to mean another space.
        assert_eq!(found["name"], SPACE);
        assert_eq!(found["activeConference"]["conferenceRecord"], RECORD);
    }
    for request in server.received_requests().await.unwrap() {
        assert_eq!(request.url.path(), "/v2/spaces/abc-mnop-xyz");
        assert_eq!(request.url.query(), None);
    }
}

#[tokio::test]
async fn an_identifier_stays_one_path_segment() {
    let (server, socket, key) = answering(200, transcript()).await;
    for (record, transcript) in [
        ("a?b#c d%", "x?y=1"),
        // The same characters inside a name Google is supposed to have returned.
        ("a?b#c d%", "conferenceRecords/a?b#c d%/transcripts/x?y=1"),
        ("conferenceRecords/a?b#c d%", "x?y=1"),
    ] {
        invoke(
            &socket,
            &key,
            "meet_transcripts.get",
            json!({ "record": record, "transcript": transcript }),
        )
        .await
        .unwrap();
    }
    for request in server.received_requests().await.unwrap() {
        assert_eq!(
            request.url.path(),
            "/v2/conferenceRecords/a%3Fb%23c%20d%25/transcripts/x%3Fy%3D1"
        );
        assert_eq!(request.url.query(), None);
        assert_eq!(request.url.fragment(), None);
    }

    let (server, socket, key) = answering(200, space()).await;
    invoke(
        &socket,
        &key,
        "meet_spaces.get",
        json!({ "space": "spaces/..%2Fv2%2Fx?alt=media" }),
    )
    .await
    .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/v2/spaces/..%252Fv2%252Fx%3Falt%3Dmedia");
    assert_eq!(request.url.query(), None);
}

#[tokio::test]
async fn what_is_neither_an_id_nor_the_right_name_is_refused_before_google_is_called() {
    let (server, socket, key) = answering(200, json!({ "name": "as-asked" })).await;
    let other_record = "conferenceRecords/another-meeting";
    let in_record =
        |name: &str, field: &str, value: String| (name.to_owned(), json!({ "record": RECORD, field: value }));
    let mut refused: Vec<(String, Value)> = Vec::new();
    // A conference record that is blank, only dots, or the name of something else.
    for record in [
        "",
        "  ",
        ".",
        "..",
        "conferenceRecords/",
        "conferenceRecords/..",
        "conferenceRecords/a/b",
        "conferencerecords/abc",
        "/abc",
        "abc/",
        SPACE,
        TRANSCRIPT,
        "https://meet.googleapis.com/v2/conferenceRecords/abc",
    ] {
        for name in [
            "meet_conference_records.get",
            "meet_participants.list",
            "meet_transcripts.list",
            "meet_recordings.list",
        ] {
            refused.push((name.to_owned(), json!({ "record": record })));
        }
    }
    // A thing in a record: blank, of another collection, cut short, or of another record.
    for name in [
        "meet_transcripts.get",
        "meet_transcripts.entries",
        "meet_transcripts.read",
    ] {
        for transcript in [
            String::new(),
            "..".to_owned(),
            format!("{RECORD}/transcripts/.."),
            "transcripts/tr-01".to_owned(),
            RECORDING.to_owned(),
            format!("{TRANSCRIPT}/entries/e-1"),
            format!("{other_record}/transcripts/tr-01"),
        ] {
            refused.push(in_record(name, "transcript", transcript));
        }
    }
    for name in ["meet_participants.get", "meet_participants.sessions"] {
        for participant in [
            " ".to_owned(),
            TRANSCRIPT.to_owned(),
            format!("{other_record}/participants/118203456789"),
        ] {
            refused.push(in_record(name, "participant", participant));
        }
    }
    for recording in [
        String::new(),
        TRANSCRIPT.to_owned(),
        format!("{other_record}/recordings/rec-01"),
    ] {
        refused.push(in_record("meet_recordings.get", "recording", recording));
    }
    // A space: the name of something else, or a link that is not a meeting's.
    for space in [
        "",
        "..",
        "spaces/",
        RECORD,
        "spaces/abc/members/1",
        "https://meet.google.com/lookup/abc",
        "https://meet.google.com/",
        "https://example.test/abc-mnop-xyz",
    ] {
        refused.push(("meet_spaces.get".to_owned(), json!({ "space": space })));
    }

    for (name, input) in refused {
        let err = invoke(&socket, &key, &name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert!(!err.message().contains("another-meeting"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn every_list_pages_under_googles_names_and_refuses_a_limit_out_of_range() {
    let in_transcript = json!({ "record": RECORD, "transcript": TRANSCRIPT });
    let cases = [
        ("meet_conference_records.list", json!({}), "conferenceRecords", 100),
        (
            "meet_participants.list",
            json!({ "record": RECORD }),
            "participants",
            250,
        ),
        (
            "meet_participants.sessions",
            json!({ "record": RECORD, "participant": ADA }),
            "participantSessions",
            250,
        ),
        ("meet_transcripts.list", json!({ "record": RECORD }), "transcripts", 100),
        ("meet_transcripts.entries", in_transcript, "transcriptEntries", 100),
        ("meet_recordings.list", json!({ "record": RECORD }), "recordings", 100),
    ];
    for (name, input, field, most) in cases {
        let with = |cursor: Option<&str>, limit: u32| {
            let mut input = input.clone();
            input["limit"] = json!(limit);
            if let Some(cursor) = cursor {
                input["cursor"] = json!(cursor);
            }
            input
        };
        let (server, socket, key) =
            answering(200, json!({ field: [{ "name": "one" }], "nextPageToken": "page-3" })).await;
        let page = invoke(&socket, &key, name, with(Some("page-2"), most)).await.unwrap();
        assert_eq!(page["items"][0]["name"], "one", "{name}");
        assert_eq!(page["next_cursor"], "page-3", "{name}");
        assert_eq!(
            query_of(&only_request(&server).await),
            json!({ "pageSize": most.to_string(), "pageToken": "page-2" }),
            "{name}"
        );

        // The last page has no token, and an empty list has no list at all.
        let (_server, socket, key) = answering(200, json!({})).await;
        let page = invoke(&socket, &key, name, with(None, 1)).await.unwrap();
        assert_eq!(page, json!({ "items": [], "next_cursor": null }), "{name}");

        let (server, socket, key) = answering(200, json!({})).await;
        for limit in [0, most + 1] {
            let err = invoke(&socket, &key, name, with(None, limit)).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {limit}");
            assert!(
                err.message().contains(&format!("from 1 to {most}")),
                "{}",
                err.message()
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_field_and_never_repeats_what_google_sent() {
    let said = entry(
        "e-1",
        ADA,
        "2026-10-12T16:00:34Z",
        "2026-10-12T16:00:36Z",
        "Shall we begin?",
    );
    let in_transcript = json!({ "record": RECORD, "transcript": TRANSCRIPT });
    for (name, input, response, place) in [
        (
            "meet_transcripts.entries",
            in_transcript.clone(),
            json!({ "transcriptEntries": [said, { "name": "e-2", "text": { "said": "the password is hunter2" } }] }),
            "[1].text",
        ),
        (
            "meet_participants.get",
            json!({ "record": RECORD, "participant": ADA }),
            json!({ "name": ADA, "signedinUser": { "displayName": ["hunter2"] } }),
            "signedinUser.displayName",
        ),
        (
            "meet_transcripts.get",
            in_transcript.clone(),
            json!({ "name": TRANSCRIPT, "docsDestination": "hunter2" }),
            "docsDestination",
        ),
        (
            "meet_transcripts.read",
            in_transcript,
            json!({ "name": TRANSCRIPT, "startTime": { "seconds": "hunter2" } }),
            "startTime",
        ),
        (
            "meet_recordings.list",
            json!({ "record": RECORD }),
            json!({ "recordings": [{ "name": RECORDING, "driveDestination": { "file": { "id": "hunter2" } } }] }),
            "[0].driveDestination.file",
        ),
        (
            "meet_spaces.get",
            json!({ "space": SPACE }),
            json!({ "name": SPACE, "config": { "accessType": ["hunter2"] } }),
            "config.accessType",
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name}: {err}");
        assert!(
            err.message().contains(&format!("`{place}`")),
            "{name}: {}",
            err.message()
        );
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("hunter2"), "{everything}");
        assert_eq!(err.provider().map(|p| p.as_str()), Some("google"), "{name}");
    }
}

#[tokio::test]
async fn a_success_that_does_not_carry_what_was_asked_for_is_an_error() {
    let in_transcript = json!({ "record": RECORD, "transcript": TRANSCRIPT });
    for (name, input, response) in [
        // One thing, answered without its name.
        ("meet_conference_records.get", json!({ "record": RECORD }), json!({})),
        (
            "meet_participants.get",
            json!({ "record": RECORD, "participant": ADA }),
            json!({ "signedinUser": { "displayName": "Ada Lovelace" } }),
        ),
        (
            "meet_transcripts.get",
            in_transcript.clone(),
            json!({ "state": "ENDED" }),
        ),
        (
            "meet_transcripts.read",
            in_transcript.clone(),
            json!({ "startTime": "2026-10-12T16:00:30Z" }),
        ),
        (
            "meet_recordings.get",
            json!({ "record": RECORD, "recording": RECORDING }),
            json!({ "driveDestination": { "file": FILE } }),
        ),
        (
            "meet_spaces.get",
            json!({ "space": SPACE }),
            json!({ "meetingCode": MEETING_CODE }),
        ),
        // A list that is not a list.
        ("meet_conference_records.list", json!({}), json!("none")),
        ("meet_participants.list", json!({ "record": RECORD }), json!([])),
        (
            "meet_participants.sessions",
            json!({ "record": RECORD, "participant": ADA }),
            json!({ "participantSessions": "none" }),
        ),
        (
            "meet_transcripts.list",
            json!({ "record": RECORD }),
            json!({ "transcripts": {} }),
        ),
        ("meet_transcripts.entries", in_transcript, json!(42)),
        (
            "meet_recordings.list",
            json!({ "record": RECORD }),
            json!({ "recordings": 3 }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
    }
}

#[tokio::test]
async fn a_refusal_from_google_keeps_its_kind() {
    for (status, reason, kind) in [
        // A record older than 30 days, or a meeting the account was not in.
        (404, "notFound", ErrorKind::NotFound),
        // The scope was not granted, or the Meet API is not enabled.
        (403, "forbidden", ErrorKind::AccessDenied),
    ] {
        let (server, socket, key) = google().await;
        Mock::given(method("GET"))
            .respond_with(google_error(status, reason, "Requested entity was not found."))
            .mount(&server)
            .await;
        let err = read(&socket, &key, json!({})).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{status}");
        // The transcript was refused, so nothing else was asked for.
        assert_eq!(queries(&server, &entries_path()).await.len(), 0);
    }
}

/// A meeting with one of each kind of participant, a speaker Meet no longer
/// lists, and a transcript and a participant list of two pages each.
async fn a_whole_meeting() -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    let gone = format!("{RECORD}/participants/gone");
    let first = json!({
        "transcriptEntries": [
            entry("e-1", ADA, "2026-10-12T16:00:34.250Z", "2026-10-12T16:00:36Z", "Shall we begin?"),
            entry("e-2", GRACE, "2026-10-12T16:00:37Z", "2026-10-12T16:00:41.5Z", "Yes.\nI have the numbers.")
        ],
        "nextPageToken": "entries-2"
    });
    let mut unnamed = entry(
        "e-6",
        ADA,
        "2026-10-12T16:30:04Z",
        "2026-10-12T16:30:05Z",
        "Thank you all.",
    );
    unnamed.as_object_mut().unwrap().remove("participant");
    let second = json!({
        "transcriptEntries": [
            entry("e-3", CALLER, "2026-10-12T16:00:42Z", "2026-10-12T16:00:44Z", "Can you hear me?"),
            entry("e-4", &gone, "2026-10-12T16:00:45Z", "2026-10-12T16:00:46Z", "Loud and clear."),
            // Ada left at 16:15 and came back at 16:20: still one participant.
            entry("e-5", ADA, "2026-10-12T16:30:00.000001Z", "2026-10-12T16:30:02.999999999Z", "I am back."),
            unnamed
        ]
    });
    serve_page(&server, &entries_path(), None, first).await;
    serve_page(&server, &entries_path(), Some("entries-2"), second).await;
    serve_page(
        &server,
        &participants_path(),
        None,
        json!({ "participants": [caller()], "nextPageToken": "people-2" }),
    )
    .await;
    serve_page(
        &server,
        &participants_path(),
        Some("people-2"),
        json!({ "participants": [grace(), ada()] }),
    )
    .await;
    (server, socket, key)
}

#[tokio::test]
async fn a_whole_transcript_is_read_with_each_speaker_named_from_the_participants() {
    let (server, socket, key) = a_whole_meeting().await;
    let content = read(&socket, &key, json!({})).await.unwrap();

    assert_eq!(
        content["text"],
        "Ada Lovelace: Shall we begin?\n\
         Grace (guest): Yes. I have the numbers.\n\
         +1 ***-***-0093: Can you hear me?\n\
         Loud and clear.\n\
         Ada Lovelace: I am back.\n\
         Thank you all."
    );
    let entries = content["entries"].as_array().unwrap();
    let column = |field: &str| -> Vec<Value> { entries.iter().map(|entry| entry[field].clone()).collect() };
    // A signed-in person, a guest and a caller each have a name of their own
    // kind. A speaker Meet does not list, and an entry with no speaker, have none.
    assert_eq!(
        column("speaker"),
        [
            json!("Ada Lovelace"),
            json!("Grace (guest)"),
            json!("+1 ***-***-0093"),
            Value::Null,
            json!("Ada Lovelace"),
            Value::Null
        ]
    );
    // From the transcript's own start, 16:00:30, whatever the fraction is written with.
    assert_eq!(
        column("startMs"),
        [4250, 7000, 12000, 15000, 1_770_000, 1_774_000].map(Value::from)
    );
    assert_eq!(
        column("endMs"),
        [6000, 11500, 14000, 16000, 1_772_999, 1_775_000].map(Value::from)
    );
    // Meet's own fields are kept as Meet wrote them.
    assert_eq!(entries[1]["text"], "Yes.\nI have the numbers.");
    assert_eq!(entries[1]["startTime"], "2026-10-12T16:00:37Z");
    assert_eq!(entries[1]["endTime"], "2026-10-12T16:00:41.5Z");
    assert_eq!(entries[1]["languageCode"], "en-US");
    assert_eq!(entries[1]["participant"], GRACE);
    assert_eq!(entries[3]["participant"], format!("{RECORD}/participants/gone"));
    assert_eq!(entries[5]["participant"], Value::Null);
    assert_eq!(content["truncated"], false);
    // The Doc the transcript was saved to comes with it, for Drive and Docs.
    assert_eq!(content["transcript"]["name"], TRANSCRIPT);
    assert_eq!(content["transcript"]["startTime"], "2026-10-12T16:00:30Z");
    assert_eq!(content["transcript"]["docsDestination"]["document"], DOCUMENT);

    // The transcript, every page of its entries, every page of the participants.
    assert_eq!(queries(&server, TRANSCRIPT_PATH).await, [json!({})]);
    assert_eq!(
        queries(&server, &entries_path()).await,
        [
            json!({ "pageSize": "100" }),
            json!({ "pageSize": "100", "pageToken": "entries-2" })
        ]
    );
    assert_eq!(
        queries(&server, &participants_path()).await,
        [
            json!({ "pageSize": "250" }),
            json!({ "pageSize": "250", "pageToken": "people-2" })
        ]
    );
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 5, "nothing else is asked for, sessions included");
    for request in &received {
        assert_eq!(request.method.as_str(), "GET");
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            &format!("Bearer {TOKEN}")
        );
    }
}

#[tokio::test]
async fn reading_stops_asking_for_participants_once_every_speaker_is_found() {
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    let said = entry(
        "e-1",
        ADA,
        "2026-10-12T16:00:34Z",
        "2026-10-12T16:00:36Z",
        "Shall we begin?",
    );
    serve(&server, &entries_path(), json!({ "transcriptEntries": [said] })).await;
    // A large meeting: Ada is on the first page of many.
    serve(
        &server,
        &participants_path(),
        json!({ "participants": [grace(), ada()], "nextPageToken": "people-2" }),
    )
    .await;
    let content = read(&socket, &key, json!({})).await.unwrap();
    assert_eq!(content["entries"][0]["speaker"], "Ada Lovelace");
    assert_eq!(queries(&server, &participants_path()).await.len(), 1);

    // Nobody spoke: there is nobody to look for.
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    serve(&server, &entries_path(), json!({})).await;
    let content = read(&socket, &key, json!({})).await.unwrap();
    assert_eq!(content["text"], "");
    assert_eq!(content["entries"], json!([]));
    assert_eq!(content["truncated"], false);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_long_transcript_is_cut_at_the_number_asked_for_and_says_so() {
    let said = |id: &str| entry(id, ADA, "2026-10-12T16:00:34Z", "2026-10-12T16:00:36Z", id);
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    serve(&server, &participants_path(), json!({ "participants": [ada()] })).await;
    // Google may fill a page with fewer entries than it was asked for.
    serve_page(
        &server,
        &entries_path(),
        None,
        json!({ "transcriptEntries": [said("one"), said("two")], "nextPageToken": "entries-2" }),
    )
    .await;
    serve_page(
        &server,
        &entries_path(),
        Some("entries-2"),
        json!({ "transcriptEntries": [said("three")], "nextPageToken": "entries-3" }),
    )
    .await;
    serve_page(
        &server,
        &entries_path(),
        Some("entries-3"),
        json!({ "transcriptEntries": [said("four")] }),
    )
    .await;

    let content = read(&socket, &key, json!({ "maxEntries": 3 })).await.unwrap();
    assert_eq!(
        content["text"],
        "Ada Lovelace: one\nAda Lovelace: two\nAda Lovelace: three"
    );
    assert_eq!(content["entries"].as_array().unwrap().len(), 3);
    assert_eq!(content["truncated"], true);
    // Only as many entries are asked for as there is still room for.
    assert_eq!(
        queries(&server, &entries_path()).await,
        [
            json!({ "pageSize": "3" }),
            json!({ "pageSize": "1", "pageToken": "entries-2" })
        ]
    );

    // With room for all four, the transcript is whole and says so.
    let content = read(&socket, &key, json!({ "maxEntries": 4 })).await.unwrap();
    assert_eq!(content["entries"].as_array().unwrap().len(), 4);
    assert_eq!(content["truncated"], false);

    // A page with more in it than was asked for is cut too.
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    serve(&server, &participants_path(), json!({ "participants": [ada()] })).await;
    serve(
        &server,
        &entries_path(),
        json!({ "transcriptEntries": [said("one"), said("two")] }),
    )
    .await;
    let content = read(&socket, &key, json!({ "maxEntries": 1 })).await.unwrap();
    assert_eq!(content["text"], "Ada Lovelace: one");
    assert_eq!(content["truncated"], true);

    let (server, socket, key) = answering(200, transcript()).await;
    for most in [0, 10_001] {
        let err = read(&socket, &key, json!({ "maxEntries": most })).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{most}");
        assert!(err.message().contains("maxEntries"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn reading_ends_even_when_google_never_stops_offering_another_page() {
    // Entries: a page with nothing in it and a token for the next, for ever.
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    serve(&server, &entries_path(), json!({ "nextPageToken": "again" })).await;
    let content = read(&socket, &key, json!({ "maxEntries": 1 })).await.unwrap();
    assert_eq!(content["entries"], json!([]));
    assert_eq!(content["truncated"], true, "it did not reach the end, and says so");
    assert_eq!(queries(&server, &entries_path()).await.len(), 4);

    // Participants: the speaker is never among them, and there is always another page.
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    let said = entry(
        "e-1",
        ADA,
        "2026-10-12T16:00:34Z",
        "2026-10-12T16:00:36Z",
        "Shall we begin?",
    );
    serve(&server, &entries_path(), json!({ "transcriptEntries": [said] })).await;
    serve(
        &server,
        &participants_path(),
        json!({ "participants": [grace()], "nextPageToken": "again" }),
    )
    .await;
    let content = read(&socket, &key, json!({})).await.unwrap();
    assert_eq!(content["entries"][0]["speaker"], Value::Null);
    assert_eq!(content["text"], "Shall we begin?");
    assert_eq!(queries(&server, &participants_path()).await.len(), 40);
}

#[tokio::test]
async fn something_still_being_said_ends_where_it_began_and_a_finer_time_is_read() {
    // An entry of a transcript that is still being written has no end yet,
    // and a time may be written finer than a thousandth of a second.
    let mut unfinished = entry("e-1", ADA, "2026-10-12T16:00:37.2501234567Z", "", "Shall we begin?");
    unfinished.as_object_mut().unwrap().remove("endTime");
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    serve(&server, &entries_path(), json!({ "transcriptEntries": [unfinished] })).await;
    serve(&server, &participants_path(), json!({ "participants": [ada()] })).await;
    let content = read(&socket, &key, json!({})).await.unwrap();
    let said = &content["entries"][0];
    assert_eq!(said["startMs"], said["endMs"]);
    assert_eq!(said["startMs"].as_i64().unwrap() % 1000, 250);
    assert_eq!(said["endTime"], Value::Null);
}

#[tokio::test]
async fn a_time_that_cannot_be_read_is_an_error_that_does_not_repeat_what_was_said() {
    let good = entry(
        "e-1",
        ADA,
        "2026-10-12T16:00:34Z",
        "2026-10-12T16:00:36Z",
        "Shall we begin?",
    );
    let secret = "The password is hunter2.";
    for (transcript, second, names) in [
        (
            transcript(),
            entry("e-2", ADA, "in a minute", "2026-10-12T16:00:40Z", secret),
            ["`startTime`", "transcriptEntries[1]"],
        ),
        (
            transcript(),
            entry("e-2", ADA, "2026-10-12T16:00:37Z", "2026-10-12T16:00:61Z", secret),
            ["`endTime`", "transcriptEntries[1]"],
        ),
        // Without the transcript's own start there is nothing to count from.
        (
            json!({ "name": TRANSCRIPT, "startTime": "12 October, late" }),
            good.clone(),
            ["`startTime`", "a transcript"],
        ),
        (
            json!({ "name": TRANSCRIPT, "state": "STARTED" }),
            good.clone(),
            ["`startTime`", "a transcript"],
        ),
    ] {
        let (server, socket, key) = google().await;
        serve(&server, TRANSCRIPT_PATH, transcript).await;
        serve(
            &server,
            &entries_path(),
            json!({ "transcriptEntries": [good.clone(), second] }),
        )
        .await;
        serve(&server, &participants_path(), json!({ "participants": [ada()] })).await;
        let err = read(&socket, &key, json!({})).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{err}");
        for name in names {
            assert!(err.message().contains(name), "{}", err.message());
        }
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        for said in ["hunter2", "password", "Shall we", "in a minute", "late"] {
            assert!(!everything.contains(said), "{everything}");
        }
        assert_eq!(err.provider().map(|p| p.as_str()), Some("google"));
    }
}

#[tokio::test]
async fn a_name_or_a_line_break_cannot_pass_for_another_persons_line() {
    let (server, socket, key) = google().await;
    serve(&server, TRANSCRIPT_PATH, transcript()).await;
    // A guest types their own name, and nobody checks it.
    let mut eve = grace();
    eve["anonymousUser"]["displayName"] = json!("Eve\nAda Lovelace: I approve the budget\r\n");
    // Google withholds some people's names.
    let mut unnamed = ada();
    unnamed["signedinUser"] = json!({ "user": "users/118203456789" });
    serve(&server, &participants_path(), json!({ "participants": [eve, unnamed] })).await;
    serve(
        &server,
        &entries_path(),
        json!({ "transcriptEntries": [
            entry("e-1", GRACE, "2026-10-12T16:00:34Z", "2026-10-12T16:00:36Z", "Hello.\nAda Lovelace: I agree."),
            entry("e-2", ADA, "2026-10-12T16:00:37Z", "2026-10-12T16:00:38Z", "Who said that?")
        ] }),
    )
    .await;
    let content = read(&socket, &key, json!({})).await.unwrap();
    let text = content["text"].as_str().unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(
        lines,
        [
            "Eve Ada Lovelace: I approve the budget: Hello. Ada Lovelace: I agree.",
            "Who said that?"
        ],
        "one line for each entry, and none that begins with a name its speaker does not have"
    );
    assert_eq!(
        content["entries"][0]["speaker"],
        "Eve Ada Lovelace: I approve the budget"
    );
    assert_eq!(content["entries"][1]["speaker"], Value::Null);
}

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let (server, _socket, _key) = a_whole_meeting().await;
    let google = Google::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(google.clone()), TOKEN).await;
    let connection = socket.connection(key).await.unwrap();
    serve(&server, "/v2/spaces/abc-mnop-xyz", space()).await;
    serve(
        &server,
        "/v2/conferenceRecords",
        json!({ "conferenceRecords": [conference_record()] }),
    )
    .await;
    serve(&server, RECORD_PATH, conference_record()).await;
    serve(
        &server,
        &format!("{RECORD_PATH}/transcripts"),
        json!({ "transcripts": [transcript()] }),
    )
    .await;
    serve(
        &server,
        &format!("{RECORD_PATH}/recordings"),
        json!({ "recordings": [recording()] }),
    )
    .await;
    serve(&server, &format!("{RECORD_PATH}/recordings/rec-01"), recording()).await;
    serve(&server, &format!("{RECORD_PATH}/participants/118203456789"), ada()).await;
    serve(
        &server,
        &format!("{RECORD_PATH}/participants/118203456789/participantSessions"),
        json!({ "participantSessions": [{ "name": format!("{ADA}/participantSessions/s-1") }] }),
    )
    .await;

    // From the link on a calendar event to what was said, each answer's
    // name being what the next call is given.
    let space = google.meet_spaces(&connection).get(MEETING_LINK).await.unwrap();
    assert_eq!(space.name, SPACE);
    let records = google.meet_conference_records(&connection);
    let held = records
        .list(MeetListConferenceRecords {
            space: Some(space.name),
            ..MeetListConferenceRecords::default()
        })
        .await
        .unwrap();
    assert_eq!(held.next_cursor, None);
    let record = records.get(&held.items[0].name).await.unwrap();
    assert_eq!(record.space.as_deref(), Some(SPACE));
    assert_eq!(record.expire_time.as_deref(), Some("2026-11-11T16:45:10.500Z"));

    let transcripts = google.meet_transcripts(&connection);
    let made = transcripts.list(&record.name, Paging::default()).await.unwrap();
    let transcript = transcripts.get(&record.name, &made.items[0].name).await.unwrap();
    let saved = transcript.docs_destination.unwrap();
    assert_eq!(saved.document.as_deref(), Some(DOCUMENT));
    assert!(
        saved
            .export_uri
            .unwrap()
            .starts_with("https://docs.google.com/document/d/")
    );

    let page = transcripts
        .entries(&record.name, &transcript.name, Paging::default())
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.next_cursor.as_deref(), Some("entries-2"));
    let content = transcripts
        .read(
            &record.name,
            &transcript.name,
            MeetReadTranscript { max_entries: Some(5) },
        )
        .await
        .unwrap();
    assert_eq!(content.entries.len(), 5);
    assert!(content.truncated);
    assert_eq!(content.entries[0].speaker.as_deref(), Some("Ada Lovelace"));
    assert_eq!((content.entries[0].start_ms, content.entries[0].end_ms), (4250, 6000));
    assert!(
        content
            .text
            .starts_with("Ada Lovelace: Shall we begin?\nGrace (guest): Yes.")
    );

    // An entry names its speaker by the participant's name, which is asked for as it is.
    let participants = google.meet_participants(&connection);
    let speaker = content.entries[0].participant.clone().unwrap();
    let ada = participants.get(&record.name, &speaker).await.unwrap();
    assert_eq!(ada.display_name(), Some("Ada Lovelace"));
    assert_eq!(ada.signedin_user.unwrap().user.as_deref(), Some("users/118203456789"));
    let everyone = participants.list(&record.name, Paging::default()).await.unwrap();
    assert_eq!(everyone.items[0].display_name(), Some("+1 ***-***-0093"));
    assert_eq!(everyone.next_cursor.as_deref(), Some("people-2"));
    let sessions = participants
        .sessions(&record.name, &speaker, Paging::default())
        .await
        .unwrap();
    assert_eq!(sessions.items.len(), 1);

    let recordings = google.meet_recordings(&connection);
    let recorded = recordings.list(&record.name, Paging::default()).await.unwrap();
    let recording = recordings.get(&record.name, &recorded.items[0].name).await.unwrap();
    let file = recording.drive_destination.unwrap();
    assert_eq!(file.file.as_deref(), Some(FILE));
    assert!(file.export_uri.unwrap().starts_with("https://drive.google.com/file/d/"));

    // The filter the typed options were turned into.
    assert_eq!(
        queries(&server, "/v2/conferenceRecords").await,
        [json!({ "filter": "space.name = \"spaces/jQCFfuBOdN5z\"" })]
    );
}
