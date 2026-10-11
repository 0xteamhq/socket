//! Every Google operation, called by name against a local server that answers as Google does.

use socketkit_core::{Effect, Integration};
use socketkit_google::{Google, scopes};
use socketkit_testkit::wiremock::matchers::{method, path};
use socketkit_testkit::wiremock::{Mock, ResponseTemplate};

mod support;
use support::{Case, TOKEN, answer, body_of, contains, google, invoke, query_of};

// One table of cases for each product. A case is one operation: the request
// that must reach Google, and what the operation returns for Google's answer.

// ── gmail: cases ──
#[rustfmt::skip]
fn gmail_cases() -> Vec<Case> {
    vec![]
}

// ── calendar: cases ──
use support::calendar::{
    AT_NINE, AT_TEN, CALENDAR_EVENT_PATH, CALENDAR_EVENTS_PATH, CALENDAR_LIST_PATH, FREEBUSY_PATH, MEET_LINK,
    calendar_entry, calendar_event, calendar_events_page, calendar_invitation, freebusy,
};
#[rustfmt::skip]
fn calendar_cases() -> Vec<Case> {
    use serde_json::json;
    let one = json!({ "calendar": "primary", "event": "evt1" });
    // The invitation Grace reads, and what it is once she has answered it.
    let mut answered = calendar_invitation();
    answered["attendees"][1]["responseStatus"] = json!("accepted");
    answered["attendees"][1]["comment"] = json!("See you there");
    vec![
        Case::new("calendar_list.list", json!({ "minAccessRole": "writer", "showHidden": true, "limit": 50, "cursor": "page-1" }), "GET", CALENDAR_LIST_PATH)
            .query(json!({ "minAccessRole": "writer", "showHidden": "true", "maxResults": "50", "pageToken": "page-1" }))
            .answers(200, json!({ "kind": "calendar#calendarList", "etag": "\"p33g\"", "items": [calendar_entry()], "nextPageToken": "page-2" }))
            .returns(json!({ "items": [{ "id": "ada@example.test", "summary": "Ada Lovelace", "accessRole": "owner", "primary": true }], "next_cursor": "page-2" })),
        Case::new("calendar_list.get", json!({ "calendar": "primary" }), "GET", format!("{CALENDAR_LIST_PATH}/primary"))
            .answers(200, calendar_entry())
            .returns(json!({ "id": "ada@example.test", "timeZone": "America/Los_Angeles", "primary": true, "hidden": false })),
        Case::new("calendar_events.list", json!({ "calendar": "primary", "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-19T00:00:00Z", "q": "design", "singleEvents": true, "orderBy": "startTime", "limit": 10 }), "GET", CALENDAR_EVENTS_PATH)
            .query(json!({ "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-19T00:00:00Z", "q": "design", "singleEvents": "true", "orderBy": "startTime", "maxResults": "10" }))
            .answers(200, { let mut page = calendar_events_page(json!([calendar_event()])); page["nextPageToken"] = json!("page-2"); page })
            .returns(json!({ "items": [{ "id": "evt1", "summary": "Design review", "start": { "dateTime": AT_NINE }, "hangoutLink": MEET_LINK }], "next_cursor": "page-2" })),
        Case::new("calendar_events.get", one.clone(), "GET", CALENDAR_EVENT_PATH)
            .answers(200, calendar_event())
            .returns(json!({ "id": "evt1", "organizer": { "email": "ada@example.test", "self": true }, "attendees": [{ "email": "ada@example.test", "responseStatus": "accepted" }, { "email": "grace@example.test", "optional": true }],
                "conferenceData": { "conferenceId": "abc-defg-hij" }, "attachments": [{ "fileId": "1AbC" }] })),
        Case::new("calendar_events.instances", json!({ "calendar": "primary", "event": "evt1", "timeMin": "2026-10-01T00:00:00Z", "limit": 5 }), "GET", format!("{CALENDAR_EVENT_PATH}/instances"))
            .query(json!({ "timeMin": "2026-10-01T00:00:00Z", "maxResults": "5" }))
            .answers(200, calendar_events_page(json!([{ "kind": "calendar#event", "id": "evt1_20261012T160000Z", "recurringEventId": "evt1", "originalStartTime": { "dateTime": AT_NINE }, "start": { "dateTime": AT_NINE }, "end": { "dateTime": AT_TEN } }])))
            .returns(json!({ "items": [{ "id": "evt1_20261012T160000Z", "recurringEventId": "evt1", "originalStartTime": { "dateTime": AT_NINE } }], "next_cursor": null })),
        Case::new("calendar_events.insert", json!({ "calendar": "primary", "summary": "Design review", "location": "Room 4", "start": { "dateTime": AT_NINE }, "end": { "dateTime": AT_TEN }, "attendees": [{ "email": "grace@example.test", "optional": true }], "sendUpdates": "all" }), "POST", CALENDAR_EVENTS_PATH)
            .query(json!({ "sendUpdates": "all" }))
            .body(json!({ "summary": "Design review", "location": "Room 4", "start": { "dateTime": AT_NINE }, "end": { "dateTime": AT_TEN }, "attendees": [{ "email": "grace@example.test", "optional": true }] }))
            .answers(200, calendar_event())
            .returns(json!({ "id": "evt1", "htmlLink": "https://www.google.com/calendar/event?eid=ZXZ0MQ" })),
        Case::new("calendar_events.patch", json!({ "calendar": "primary", "event": "evt1", "summary": "Design review (moved)", "location": "Room 5" }), "PATCH", CALENDAR_EVENT_PATH)
            .body(json!({ "summary": "Design review (moved)", "location": "Room 5" }))
            .answers(200, calendar_event())
            .returns(json!({ "id": "evt1" })),
        // An answer reads the event first, and sends the whole guest list back.
        Case::new("calendar_events.respond", json!({ "calendar": "primary", "event": "evt1", "responseStatus": "accepted", "comment": "See you there", "sendUpdates": "all" }), "PATCH", CALENDAR_EVENT_PATH)
            .also("GET", CALENDAR_EVENT_PATH, calendar_invitation())
            .query(json!({ "sendUpdates": "all" }))
            .body(json!({ "attendees": answered["attendees"] }))
            .answers(200, answered.clone())
            .returns(json!({ "id": "evt1", "attendees": [{ "email": "ada@example.test" }, { "email": "grace@example.test", "self": true, "responseStatus": "accepted", "comment": "See you there" }] })),
        Case::new("calendar_events.delete", json!({ "calendar": "primary", "event": "evt1", "sendUpdates": "all" }), "DELETE", CALENDAR_EVENT_PATH)
            .query(json!({ "sendUpdates": "all" }))
            .answers(204, json!(null))
            .returns(json!(null)),
        Case::new("calendar_freebusy.query", json!({ "calendars": ["primary", "grace@example.test"], "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z", "timeZone": "Europe/Zurich" }), "POST", FREEBUSY_PATH)
            .body(json!({ "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z", "timeZone": "Europe/Zurich", "items": [{ "id": "primary" }, { "id": "grace@example.test" }] }))
            .answers(200, freebusy())
            .returns(json!({ "calendars": { "primary": { "busy": [{ "start": "2026-10-12T18:00:00+02:00", "end": "2026-10-12T19:00:00+02:00" }], "errors": [] },
                "grace@example.test": { "busy": [], "errors": [{ "domain": "global", "reason": "notFound" }] } } })),
    ]
}

// ── meet: cases ──
#[rustfmt::skip]
fn meet_cases() -> Vec<Case> {
    // Imported here and not at the top, so that a name as plain as `FILE`
    // or `SPACE` cannot meet another product's fixture of the same name.
    use serde_json::json;
    use support::meet::{
        ADA, ADA_ID, DOCUMENT, FILE, GRACE, MEETING_CODE, MEETING_LINK, RECORD, RECORD_ID, RECORD_PATH, RECORDING,
        SPACE, TRANSCRIPT, TRANSCRIPT_ID, TRANSCRIPT_PATH, ada, conference_record, entry, grace, recording, session,
        space, transcript,
    };
    // A record, and each thing in one, is given by its name or by its id.
    let both = json!({ "record": RECORD, "transcript": TRANSCRIPT });
    let said = entry("e-1", ADA, "2026-10-12T16:00:34.250Z", "2026-10-12T16:00:36Z", "Shall we begin?");
    vec![
        Case::new("meet_conference_records.list", json!({ "meetingCode": MEETING_CODE, "startTimeMin": "2026-10-01T00:00:00Z", "limit": 10 }), "GET", "/v2/conferenceRecords")
            .query(json!({ "filter": "space.meeting_code = \"abc-mnop-xyz\" AND start_time>=\"2026-10-01T00:00:00Z\"", "pageSize": "10" }))
            .answers(200, json!({ "conferenceRecords": [conference_record()], "nextPageToken": "page-2" }))
            .returns(json!({ "items": [{ "name": RECORD, "startTime": "2026-10-12T16:00:00.123456Z", "endTime": "2026-10-12T16:45:10.500Z", "expireTime": "2026-11-11T16:45:10.500Z", "space": SPACE }], "next_cursor": "page-2" })),
        Case::new("meet_conference_records.get", json!({ "record": RECORD_ID }), "GET", RECORD_PATH)
            .answers(200, conference_record())
            .returns(json!({ "name": RECORD, "space": SPACE })),
        Case::new("meet_participants.list", json!({ "record": RECORD, "limit": 250 }), "GET", format!("{RECORD_PATH}/participants"))
            .query(json!({ "pageSize": "250" }))
            .answers(200, json!({ "participants": [ada(), grace()] }))
            .returns(json!({ "items": [
                { "name": ADA, "signedinUser": { "user": "users/118203456789", "displayName": "Ada Lovelace" }, "earliestStartTime": "2026-10-12T16:00:02Z" },
                { "name": GRACE, "anonymousUser": { "displayName": "Grace (guest)" } }
            ], "next_cursor": null })),
        Case::new("meet_participants.get", json!({ "record": RECORD, "participant": ADA }), "GET", format!("{RECORD_PATH}/participants/{ADA_ID}"))
            .answers(200, ada())
            .returns(json!({ "name": ADA, "signedinUser": { "displayName": "Ada Lovelace" }, "latestEndTime": "2026-10-12T16:45:10Z" })),
        Case::new("meet_participants.sessions", json!({ "record": RECORD_ID, "participant": ADA_ID }), "GET", format!("{RECORD_PATH}/participants/{ADA_ID}/participantSessions"))
            .answers(200, json!({ "participantSessions": [
                session("s-2", "2026-10-12T16:20:00Z", "2026-10-12T16:45:10Z"),
                session("s-1", "2026-10-12T16:00:02Z", "2026-10-12T16:15:00Z")
            ] }))
            .returns(json!({ "items": [
                { "name": format!("{ADA}/participantSessions/s-2"), "startTime": "2026-10-12T16:20:00Z" },
                { "name": format!("{ADA}/participantSessions/s-1"), "endTime": "2026-10-12T16:15:00Z" }
            ] })),
        Case::new("meet_transcripts.list", json!({ "record": RECORD }), "GET", format!("{RECORD_PATH}/transcripts"))
            .answers(200, json!({ "transcripts": [transcript()] }))
            .returns(json!({ "items": [{ "name": TRANSCRIPT, "state": "FILE_GENERATED", "docsDestination": { "document": DOCUMENT } }] })),
        Case::new("meet_transcripts.get", json!({ "record": RECORD_ID, "transcript": TRANSCRIPT_ID }), "GET", TRANSCRIPT_PATH)
            .answers(200, transcript())
            .returns(json!({ "name": TRANSCRIPT, "startTime": "2026-10-12T16:00:30Z", "docsDestination": { "document": DOCUMENT, "exportUri": "https://docs.google.com/document/d/1kuceFZohVoCh6FulBHxwy6I15Ogpc4hP/view" } })),
        Case::new("meet_transcripts.entries", json!({ "record": RECORD, "transcript": TRANSCRIPT, "limit": 100, "cursor": "page-2" }), "GET", format!("{TRANSCRIPT_PATH}/entries"))
            .query(json!({ "pageSize": "100", "pageToken": "page-2" }))
            .answers(200, json!({ "transcriptEntries": [said.clone()], "nextPageToken": "page-3" }))
            .returns(json!({ "items": [{ "name": format!("{TRANSCRIPT}/entries/e-1"), "participant": ADA, "text": "Shall we begin?", "languageCode": "en-US", "startTime": "2026-10-12T16:00:34.250Z", "endTime": "2026-10-12T16:00:36Z" }], "next_cursor": "page-3" })),
        Case::new("meet_transcripts.read", both, "GET", TRANSCRIPT_PATH)
            .answers(200, transcript())
            .also("GET", format!("{TRANSCRIPT_PATH}/entries"), json!({ "transcriptEntries": [said] }))
            .also("GET", format!("{RECORD_PATH}/participants"), json!({ "participants": [grace(), ada()] }))
            .returns(json!({
                "text": "Ada Lovelace: Shall we begin?",
                "entries": [{ "speaker": "Ada Lovelace", "startMs": 4250, "endMs": 6000, "text": "Shall we begin?", "startTime": "2026-10-12T16:00:34.250Z", "endTime": "2026-10-12T16:00:36Z", "languageCode": "en-US", "participant": ADA }],
                "truncated": false,
                "transcript": { "name": TRANSCRIPT, "docsDestination": { "document": DOCUMENT } }
            })),
        Case::new("meet_recordings.list", json!({ "record": RECORD }), "GET", format!("{RECORD_PATH}/recordings"))
            .answers(200, json!({ "recordings": [recording()] }))
            .returns(json!({ "items": [{ "name": RECORDING, "state": "FILE_GENERATED", "driveDestination": { "file": FILE } }] })),
        Case::new("meet_recordings.get", json!({ "record": RECORD, "recording": RECORDING }), "GET", format!("{RECORD_PATH}/recordings/rec-01"))
            .answers(200, recording())
            .returns(json!({ "name": RECORDING, "driveDestination": { "file": FILE, "exportUri": "https://drive.google.com/file/d/1mZq9Xc0wT3vUu7rLhYp2sNdEaKbJ4gQf/view" } })),
        Case::new("meet_spaces.get", json!({ "space": MEETING_LINK }), "GET", "/v2/spaces/abc-mnop-xyz")
            .answers(200, space())
            .returns(json!({ "name": SPACE, "meetingUri": MEETING_LINK, "meetingCode": MEETING_CODE, "config": { "accessType": "TRUSTED", "artifactConfig": { "transcriptionConfig": { "autoTranscriptionGeneration": "ON" } } }, "activeConference": { "conferenceRecord": RECORD } })),
    ]
}

// ── drive: cases ──
#[rustfmt::skip]
fn drive_cases() -> Vec<Case> {
    // Named here and not at the top of the file, where five products' fixtures meet.
    use serde_json::json;
    use support::drive::{
        ARCHIVE, DOC, DRIVES_FIELDS, FILES_FIELDS, PERMISSIONS_FIELDS, PLANS, doc, doc_with, domain_reader, drives,
        folder, of_file, permissions, shared_drive, shared_sheet, writer,
    };
    let budget = "name contains 'budget' and trashed = false";
    let markdown = "# Q4 plan\n\nShip the **importer** by November.\n";
    vec![
        Case::new("drive_files.list", json!({ "q": budget, "orderBy": "modifiedTime desc", "limit": 50 }), "GET", "/drive/v3/files")
            .query(json!({ "fields": FILES_FIELDS, "supportsAllDrives": "true", "includeItemsFromAllDrives": "true", "q": budget, "orderBy": "modifiedTime desc", "pageSize": "50" }))
            .answers(200, json!({ "kind": "drive#fileList", "nextPageToken": "~!!~AI9FV7Q", "files": [shared_sheet(), doc()] }))
            .returns(json!({ "items": [shared_sheet(), doc()], "next_cursor": "~!!~AI9FV7Q" })),
        Case::new("drive_files.get", json!({ "file": DOC }), "GET", format!("/drive/v3/files/{DOC}"))
            .query(of_file())
            .answers(200, doc())
            .returns(doc()),
        Case::new("drive_files.export", json!({ "file": DOC, "mimeType": "text/markdown" }), "GET", format!("/drive/v3/files/{DOC}/export"))
            .query(json!({ "mimeType": "text/markdown" }))
            .answers_text("text/markdown", markdown)
            .returns(json!({ "mimeType": "text/markdown", "text": markdown })),
        Case::new("drive_files.permissions", json!({ "file": DOC }), "GET", format!("/drive/v3/files/{DOC}/permissions"))
            .query(json!({ "fields": PERMISSIONS_FIELDS, "supportsAllDrives": "true" }))
            .answers(200, permissions(json!([writer(), domain_reader()])))
            .returns(json!({ "items": [writer(), domain_reader()], "next_cursor": null })),
        Case::new("drive_files.create_folder", json!({ "name": "Plans", "parents": ["0AMyDriveRootId9PVA"] }), "POST", "/drive/v3/files")
            .query(of_file())
            .body(json!({ "name": "Plans", "mimeType": "application/vnd.google-apps.folder", "parents": ["0AMyDriveRootId9PVA"] }))
            .answers(200, folder())
            .returns(folder()),
        Case::new("drive_files.copy", json!({ "file": DOC, "name": "Q1 plan", "parents": [ARCHIVE] }), "POST", format!("/drive/v3/files/{DOC}/copy"))
            .query(of_file())
            .body(json!({ "name": "Q1 plan", "parents": [ARCHIVE] }))
            .answers(200, doc_with(json!({ "id": "1TheCopy_aBcDeFgHiJkLmNoPqRsTuVw", "name": "Q1 plan", "parents": [ARCHIVE] })))
            .returns(json!({ "id": "1TheCopy_aBcDeFgHiJkLmNoPqRsTuVw", "name": "Q1 plan", "parents": [ARCHIVE] })),
        // Google moves a file by adding one parent and taking another away,
        // so the file is read first for the parent it has.
        Case::new("drive_files.move_to", json!({ "file": DOC, "folder": ARCHIVE }), "PATCH", format!("/drive/v3/files/{DOC}"))
            .also("GET", format!("/drive/v3/files/{DOC}"), doc())
            .query(json!({ "addParents": ARCHIVE, "removeParents": PLANS, "fields": of_file()["fields"], "supportsAllDrives": "true" }))
            .body(json!({}))
            .answers(200, doc_with(json!({ "parents": [ARCHIVE] })))
            .returns(json!({ "id": DOC, "parents": [ARCHIVE] })),
        Case::new("drive_files.rename", json!({ "file": DOC, "name": "Q4 plan (final)" }), "PATCH", format!("/drive/v3/files/{DOC}"))
            .query(of_file())
            .body(json!({ "name": "Q4 plan (final)" }))
            .answers(200, doc_with(json!({ "name": "Q4 plan (final)" })))
            .returns(json!({ "id": DOC, "name": "Q4 plan (final)" })),
        Case::new("drive_files.trash", json!({ "file": DOC }), "PATCH", format!("/drive/v3/files/{DOC}"))
            .query(of_file())
            .body(json!({ "trashed": true }))
            .answers(200, doc_with(json!({ "trashed": true })))
            .returns(json!({ "id": DOC, "trashed": true })),
        Case::new("drive_shared_drives.list", json!({ "limit": 25 }), "GET", "/drive/v3/drives")
            .query(json!({ "fields": DRIVES_FIELDS, "pageSize": "25" }))
            .answers(200, drives(json!([shared_drive()])))
            .returns(json!({ "items": [shared_drive()], "next_cursor": null })),
    ]
}

// ── docs and sheets: cases ──
#[rustfmt::skip]
fn docs_cases() -> Vec<Case> {
    vec![]
}

/// Every operation, whichever product it belongs to.
fn every_case() -> Vec<Case> {
    let mut all = gmail_cases();
    all.extend(calendar_cases());
    all.extend(meet_cases());
    all.extend(drive_cases());
    all.extend(docs_cases());
    all
}

/// What each operation does to Google's data, and the scopes it needs.
///
/// A host lets a read run freely and asks a person before anything else, so
/// each effect is stated here and not derived from the code under test.
#[rustfmt::skip]
fn expected() -> Vec<(&'static str, Effect, &'static [&'static str])> {
    vec![
        // ── gmail: effects ──

        // ── calendar: effects ──
        ("calendar_list.list", Effect::Read, &[scopes::CALENDAR_READONLY]),
        ("calendar_list.get", Effect::Read, &[scopes::CALENDAR_READONLY]),
        ("calendar_events.list", Effect::Read, &[scopes::CALENDAR_READONLY]),
        ("calendar_events.get", Effect::Read, &[scopes::CALENDAR_READONLY]),
        ("calendar_events.instances", Effect::Read, &[scopes::CALENDAR_READONLY]),
        ("calendar_freebusy.query", Effect::Read, &[scopes::CALENDAR_READONLY]),
        ("calendar_events.insert", Effect::Write, &[scopes::CALENDAR_EVENTS]),
        ("calendar_events.patch", Effect::Destructive, &[scopes::CALENDAR_EVENTS]),
        // The organiser sees an answer at once, and a notice that was sent cannot be taken back.
        ("calendar_events.respond", Effect::Destructive, &[scopes::CALENDAR_EVENTS]),
        ("calendar_events.delete", Effect::Destructive, &[scopes::CALENDAR_EVENTS]),

        // ── meet: effects ──
        ("meet_conference_records.list", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_conference_records.get", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_participants.list", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_participants.get", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_participants.sessions", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_transcripts.list", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_transcripts.get", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_transcripts.entries", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_transcripts.read", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_recordings.list", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_recordings.get", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),
        ("meet_spaces.get", Effect::Read, &[scopes::MEETINGS_SPACE_READONLY]),

        // ── drive: effects ──
        ("drive_files.list", Effect::Read, &[scopes::DRIVE_READONLY]),
        ("drive_files.get", Effect::Read, &[scopes::DRIVE_READONLY]),
        ("drive_files.export", Effect::Read, &[scopes::DRIVE_READONLY]),
        ("drive_files.permissions", Effect::Read, &[scopes::DRIVE_READONLY]),
        ("drive_files.create_folder", Effect::Write, &[scopes::DRIVE_FILE]),
        ("drive_files.copy", Effect::Write, &[scopes::DRIVE_FILE]),
        ("drive_files.move_to", Effect::Write, &[scopes::DRIVE_FILE]),
        ("drive_files.rename", Effect::Write, &[scopes::DRIVE_FILE]),
        ("drive_files.trash", Effect::Write, &[scopes::DRIVE_FILE]),
        ("drive_shared_drives.list", Effect::Read, &[scopes::DRIVE_READONLY]),

        // ── docs and sheets: effects ──
    ]
}

/// The reads Google offers only as POST. They change nothing, and are the
/// only reads that are not a GET.
const POSTED_READS: [&str; 1] = ["calendar_freebusy.query"];

#[tokio::test]
async fn the_tables_above_cover_every_operation_google_offers() {
    let listed: Vec<String> = Google::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = every_case().iter().map(|c| format!("google.{}", c.name)).collect();
    tested.extend(["google.identity.get".to_owned(), "google.resource.resolve".to_owned()]);
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist, or one is tested twice"
    );
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_google_sent() {
    for case in every_case() {
        let (server, socket, key) = google().await;
        for (verb, also, response) in &case.also {
            assert!(
                (*verb, also.as_str()) != (case.verb, case.path.as_str()),
                "{}: another request has to go to another address",
                case.name
            );
            Mock::given(method(*verb))
                .and(path(also.as_str()))
                .respond_with(answer(200, response))
                .mount(&server)
                .await;
        }
        // An answer that is not JSON is sent as the text it is.
        let answered = match (case.text, case.response.as_str()) {
            (Some(content_type), Some(text)) => ResponseTemplate::new(case.status).set_body_raw(text, content_type),
            _ => answer(case.status, &case.response),
        };
        Mock::given(method(case.verb))
            .and(path(case.path.as_str()))
            .respond_with(answered)
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
        for request in &received {
            assert_eq!(
                request.headers.get("authorization").unwrap(),
                &format!("Bearer {TOKEN}"),
                "{}",
                case.name
            );
        }
        assert_eq!(
            received.len(),
            1 + case.also.len(),
            "{}: one request, and each of the others it is said to make",
            case.name
        );
        // The other requests go to other addresses, so the one this case
        // describes is told from them by where it went.
        let mut described = received
            .iter()
            .filter(|request| request.method.as_str() == case.verb && request.url.path() == case.path);
        let request = described
            .next()
            .unwrap_or_else(|| panic!("{}: nothing reached {} {}", case.name, case.verb, case.path));
        assert!(described.next().is_none(), "{}: sent more than once", case.name);
        assert_eq!(
            query_of(request),
            case.query,
            "{}: exactly these parameters reach Google",
            case.name
        );
        assert_eq!(
            body_of(request),
            case.body,
            "{}: exactly this body reaches Google",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_describes_its_input_and_marks_what_it_changes() {
    let operations = Google::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    for operation in &operations {
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert!(!operation.description.is_empty(), "{}", operation.name);
        assert!(
            operation.name.len() <= 64,
            "{}: a tool's name is at most 64 characters where agents are given them",
            operation.name
        );
    }

    let expected = expected();
    assert_eq!(expected.len(), every_case().len(), "every operation's effect is stated");
    for (name, effect, scopes) in &expected {
        let operation = find(&format!("google.{name}"));
        assert_eq!(operation.effect, *effect, "{name}");
        assert_eq!(operation.required_scopes, *scopes, "{name}");
        assert!(!scopes.is_empty(), "{name}: says what it needs");
    }

    // Nothing that changes anything is sent as a GET, which the transport
    // always repeats after a server error.
    for case in every_case() {
        let effect = find(&format!("google.{}", case.name)).effect;
        match effect {
            Effect::Read if POSTED_READS.contains(&case.name) => assert_eq!(case.verb, "POST", "{}", case.name),
            Effect::Read => assert_eq!(case.verb, "GET", "{}", case.name),
            _ => assert_ne!(case.verb, "GET", "{}", case.name),
        }
    }

    // Only the two Drive and Docs read scopes are asked for by default. An
    // operation that needs another says so, and the application asks for it.
    let socketkit_core::AuthScheme::OAuth2(oauth) = socketkit_google::provider().auth else {
        panic!("google uses OAuth")
    };
    assert_eq!(
        oauth.default_scopes,
        [scopes::DRIVE_READONLY, scopes::DOCUMENTS_READONLY]
    );
}
