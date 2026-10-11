//! Every Google operation, called by name against a local server that answers as Google does.

use socketkit_core::{Effect, ErrorKind, Integration};
use socketkit_google::{Google, scopes};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, ResponseTemplate};

mod support;
use support::{Case, TOKEN, answer, body_of, contains, google, google_error, invoke, query_of};

// One table of cases for each product. A case is one operation: the request
// that must reach Google, and what the operation returns for Google's answer.

// ── gmail: cases ──
use support::gmail::{
    GMAIL_ATTACHMENT, GMAIL_DRAFT, GMAIL_DRAFTS, GMAIL_LABELS, GMAIL_MESSAGE, GMAIL_MESSAGES, GMAIL_PROFILE,
    GMAIL_THREAD, GMAIL_THREADS, gmail_data, gmail_draft_ref, gmail_label, gmail_message, gmail_message_returned,
    gmail_metadata, gmail_part, gmail_ref,
};
#[rustfmt::skip]
fn gmail_cases() -> Vec<Case> {
    use serde_json::json;
    let message = format!("{GMAIL_MESSAGES}/{GMAIL_MESSAGE}");
    let draft = format!("{GMAIL_DRAFTS}/{GMAIL_DRAFT}");
    let grace = json!([{ "email": "grace@example.test", "name": "Grace Hopper" }]);
    // What reaches Gmail in `raw` is the whole message, written as mail travels.
    let monday = gmail_data(&format!("To: Grace Hopper <grace@example.test>\r\nSubject: Monday\r\nMIME-Version: 1.0\r\n{}", gmail_part("text/plain", "See you Monday.")));
    let answer = gmail_data(&format!(
        "To: Grace Hopper <grace@example.test>\r\nSubject: Re: Q3 plan\r\nIn-Reply-To: <CAF1plan@mail.example.test>\r\nReferences: <CAF0kickoff@mail.example.test> <CAF1plan@mail.example.test>\r\nMIME-Version: 1.0\r\n{}",
        gmail_part("text/plain", "Monday works.")));
    let pdf = json!({ "size": 9, "data": gmail_data("%PDF-1.7\n") });
    let inbox = json!({ "id": "INBOX", "name": "INBOX", "type": "system", "messageListVisibility": "hide", "labelListVisibility": "labelShow" });
    let projects = json!({ "id": "Label_12", "name": "Projects/Q3", "type": "user", "messageListVisibility": "show", "labelListVisibility": "labelShow" });
    let profile = json!({ "emailAddress": "ada@example.test", "messagesTotal": 20481, "threadsTotal": 9150, "historyId": "987654" });
    vec![
        // messages: reading
        Case::new("gmail_messages.list", json!({ "q": "from:grace is:unread", "labelIds": ["INBOX", "UNREAD"], "includeSpamTrash": false, "limit": 2, "cursor": "09876543210" }), "GET", GMAIL_MESSAGES)
            .query(json!({ "q": "from:grace is:unread", "labelIds": ["INBOX", "UNREAD"], "includeSpamTrash": "false", "maxResults": "2", "pageToken": "09876543210" }))
            .answers(200, json!({ "messages": [{ "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD }, { "id": "18c1a2b3c4d5e6f8", "threadId": GMAIL_THREAD }], "nextPageToken": "12345678901", "resultSizeEstimate": 7 }))
            .returns(json!({ "items": [{ "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD, "labelIds": [] }, { "id": "18c1a2b3c4d5e6f8", "threadId": GMAIL_THREAD }], "next_cursor": "12345678901" })),
        Case::new("gmail_messages.get", json!({ "message": GMAIL_MESSAGE }), "GET", message.clone())
            .answers(200, gmail_message()).returns(gmail_message_returned()),
        Case::new("gmail_messages.attachment_get", json!({ "message": GMAIL_MESSAGE, "attachment": GMAIL_ATTACHMENT }), "GET", format!("{message}/attachments/{GMAIL_ATTACHMENT}"))
            .answers(200, pdf.clone()).returns(pdf),

        // messages: sending
        Case::new("gmail_messages.send", json!({ "to": grace.clone(), "subject": "Monday", "text": "See you Monday." }), "POST", format!("{GMAIL_MESSAGES}/send"))
            .body(json!({ "raw": monday.clone() }))
            .answers(200, gmail_ref(&["SENT"])).returns(gmail_ref(&["SENT"])),
        Case::new("gmail_messages.reply", json!({ "message": GMAIL_MESSAGE, "to": grace.clone(), "text": "Monday works." }), "POST", format!("{GMAIL_MESSAGES}/send"))
            .body(json!({ "raw": answer, "threadId": GMAIL_THREAD }))
            .also("GET", message.clone(), gmail_metadata())
            .answers(200, gmail_ref(&["SENT"])).returns(gmail_ref(&["SENT"])),
        Case::new("gmail_messages.send_draft", json!({ "draft": GMAIL_DRAFT }), "POST", format!("{GMAIL_DRAFTS}/send"))
            .body(json!({ "id": GMAIL_DRAFT }))
            .answers(200, gmail_ref(&["SENT"])).returns(gmail_ref(&["SENT"])),

        // messages: labels and the bin
        Case::new("gmail_messages.modify", json!({ "message": GMAIL_MESSAGE, "addLabelIds": ["STARRED"], "removeLabelIds": ["UNREAD", "INBOX"] }), "POST", format!("{message}/modify"))
            .body(json!({ "addLabelIds": ["STARRED"], "removeLabelIds": ["UNREAD", "INBOX"] }))
            .answers(200, gmail_ref(&["IMPORTANT", "STARRED"])).returns(gmail_ref(&["IMPORTANT", "STARRED"])),
        Case::new("gmail_messages.trash", json!({ "message": GMAIL_MESSAGE }), "POST", format!("{message}/trash"))
            .body(json!({}))
            .answers(200, gmail_ref(&["TRASH"])).returns(gmail_ref(&["TRASH"])),
        Case::new("gmail_messages.untrash", json!({ "message": GMAIL_MESSAGE }), "POST", format!("{message}/untrash"))
            .body(json!({}))
            .answers(200, gmail_ref(&["INBOX"])).returns(gmail_ref(&["INBOX"])),

        // threads
        Case::new("gmail_threads.list", json!({ "q": "subject:plan", "limit": 10 }), "GET", GMAIL_THREADS)
            .query(json!({ "q": "subject:plan", "maxResults": "10" }))
            .answers(200, json!({ "threads": [{ "id": GMAIL_THREAD, "snippet": "Attached is the plan for Q3.", "historyId": "987654" }], "resultSizeEstimate": 1 }))
            .returns(json!({ "items": [{ "id": GMAIL_THREAD, "snippet": "Attached is the plan for Q3.", "historyId": "987654", "messages": [] }], "next_cursor": null })),
        Case::new("gmail_threads.get", json!({ "thread": GMAIL_THREAD }), "GET", format!("{GMAIL_THREADS}/{GMAIL_THREAD}"))
            .answers(200, json!({ "id": GMAIL_THREAD, "historyId": "987654", "messages": [gmail_message()] }))
            .returns(json!({ "id": GMAIL_THREAD, "historyId": "987654", "messages": [gmail_message_returned()] })),

        // labels
        Case::new("gmail_labels.list", json!({}), "GET", GMAIL_LABELS)
            .answers(200, json!({ "labels": [inbox.clone(), projects.clone()] }))
            .returns(json!([inbox, projects])),
        Case::new("gmail_labels.get", json!({ "label": "Label_12" }), "GET", format!("{GMAIL_LABELS}/Label_12"))
            .answers(200, gmail_label()).returns(gmail_label()),

        // profile
        Case::new("gmail_profile.get", json!({}), "GET", GMAIL_PROFILE)
            .answers(200, profile.clone()).returns(profile),

        // drafts
        Case::new("gmail_drafts.list", json!({ "q": "to:grace", "includeSpamTrash": true, "limit": 5 }), "GET", GMAIL_DRAFTS)
            .query(json!({ "q": "to:grace", "includeSpamTrash": "true", "maxResults": "5" }))
            .answers(200, json!({ "drafts": [{ "id": GMAIL_DRAFT, "message": { "id": "18c9f0e1d2c3b4a6", "threadId": "18c9f0e1d2c3b4a6" } }], "nextPageToken": "55443322110", "resultSizeEstimate": 9 }))
            .returns(json!({ "items": [{ "id": GMAIL_DRAFT, "message": { "id": "18c9f0e1d2c3b4a6", "threadId": "18c9f0e1d2c3b4a6" } }], "next_cursor": "55443322110" })),
        Case::new("gmail_drafts.get", json!({ "draft": GMAIL_DRAFT }), "GET", draft.clone())
            .answers(200, json!({ "id": GMAIL_DRAFT, "message": gmail_message() }))
            .returns(json!({ "id": GMAIL_DRAFT, "message": gmail_message_returned() })),
        Case::new("gmail_drafts.create", json!({ "to": grace.clone(), "subject": "Monday", "text": "See you Monday." }), "POST", GMAIL_DRAFTS)
            .body(json!({ "message": { "raw": monday.clone() } }))
            .answers(200, gmail_draft_ref()).returns(gmail_draft_ref()),
        Case::new("gmail_drafts.update", json!({ "draft": GMAIL_DRAFT, "to": grace, "subject": "Monday", "text": "See you Monday." }), "PUT", draft.clone())
            .body(json!({ "message": { "raw": monday } }))
            .answers(200, gmail_draft_ref()).returns(gmail_draft_ref()),
        Case::new("gmail_drafts.delete", json!({ "draft": GMAIL_DRAFT }), "DELETE", draft)
            .answers(204, json!(null)).returns(json!(null)),
    ]
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
use support::docs::{
    DOCUMENT, DOCUMENT_FIELDS, PLAN_TEXT, REVISION, SPREADSHEET, SPREADSHEET_FIELDS, created_document, document,
    document_updated, document_with_content, spreadsheet, unformatted_values, value_ranges, values_appended,
    values_updated,
};
#[rustfmt::skip]
fn docs_cases() -> Vec<Case> {
    use serde_json::json;
    let document_path = format!("/v1/documents/{DOCUMENT}");
    let spreadsheet_path = format!("/v4/spreadsheets/{SPREADSHEET}");
    vec![
        // Asks for the names of the document and of its tabs, and nothing written in them.
        Case::new("docs_documents.get", json!({ "document": DOCUMENT }), "GET", document_path.clone())
            .query(json!({ "fields": DOCUMENT_FIELDS }))
            .answers(200, document())
            .returns(json!({ "documentId": DOCUMENT, "title": "Q3 plan", "revisionId": REVISION, "tabs": [
                { "tabId": "t.0", "title": "Plan", "index": 0, "nestingLevel": 0, "parentTabId": null },
                { "tabId": "t.8f2k", "title": "Budget", "index": 0, "nestingLevel": 1, "parentTabId": "t.0" },
                { "tabId": "t.x1n4", "title": "Notes", "index": 1, "nestingLevel": 0 }
            ] })),
        // The whole document, as it stands without what is only suggested.
        Case::new("docs_documents.read", json!({ "document": DOCUMENT }), "GET", document_path.clone())
            .query(json!({ "includeTabsContent": "true", "suggestionsViewMode": "PREVIEW_WITHOUT_SUGGESTIONS" }))
            .answers(200, document_with_content())
            .returns(json!({ "documentId": DOCUMENT, "title": "Q3 plan", "revisionId": REVISION, "tabs": [
                { "tabId": "t.0", "title": "Plan", "text": PLAN_TEXT },
                { "tabId": "t.8f2k", "title": "Budget", "parentTabId": "t.0", "nestingLevel": 1, "text": "Rent is the largest cost." },
                { "tabId": "t.x1n4", "title": "Notes", "text": "" }
            ] })),
        Case::new("docs_documents.create", json!({ "title": "Q3 plan" }), "POST", "/v1/documents")
            .query(json!({ "fields": DOCUMENT_FIELDS }))
            .body(json!({ "title": "Q3 plan" }))
            .answers(200, created_document())
            .returns(json!({ "documentId": DOCUMENT, "title": "Q3 plan", "tabs": [{ "tabId": "t.0", "title": "Tab 1" }] })),
        Case::new("docs_documents.append_text", json!({ "document": DOCUMENT, "text": "\nDecision: open in Paris first.", "tabId": "t.8f2k" }), "POST", format!("{document_path}:batchUpdate"))
            .body(json!({ "requests": [{ "insertText": {
                "text": "\nDecision: open in Paris first.",
                "endOfSegmentLocation": { "tabId": "t.8f2k" }
            } }] }))
            .answers(200, document_updated())
            .returns(json!({ "documentId": DOCUMENT, "writeControl": { "requiredRevisionId": REVISION } })),
        // Naming the fields is what keeps the cells out of the answer.
        Case::new("sheets_spreadsheets.get", json!({ "spreadsheet": SPREADSHEET }), "GET", spreadsheet_path.clone())
            .query(json!({ "fields": SPREADSHEET_FIELDS }))
            .answers(200, spreadsheet())
            .returns(json!({ "spreadsheetId": SPREADSHEET, "properties": { "title": "Stock", "locale": "en_GB", "timeZone": "Europe/London" }, "sheets": [
                { "properties": { "sheetId": 0, "title": "Sheet1", "index": 0, "sheetType": "GRID", "hidden": false, "gridProperties": { "rowCount": 1000, "columnCount": 26, "frozenRowCount": 1, "frozenColumnCount": 0 } } },
                { "properties": { "sheetId": 1837264519, "title": "Q3 plan/final", "index": 1, "hidden": true, "gridProperties": { "rowCount": 200, "columnCount": 8 } } },
                { "properties": { "sheetId": 771203, "title": "Chart1", "sheetType": "OBJECT", "gridProperties": null } }
            ] })),
        // The range is one segment of the path, with its `!` and `:` written out.
        Case::new("sheets_spreadsheets.values_get", json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1:C4", "valueRenderOption": "UNFORMATTED_VALUE", "dateTimeRenderOption": "FORMATTED_STRING" }), "GET", format!("{spreadsheet_path}/values/Sheet1%21A1%3AC4"))
            .query(json!({ "valueRenderOption": "UNFORMATTED_VALUE", "dateTimeRenderOption": "FORMATTED_STRING" }))
            .answers(200, unformatted_values())
            .returns(json!({ "range": "Sheet1!A1:C4", "majorDimension": "ROWS", "values": [["Item", "Qty", "In stock"], ["Bolts", 40, true], ["Nuts"], ["", 12.5]] })),
        Case::new("sheets_spreadsheets.values_batch_get", json!({ "spreadsheet": SPREADSHEET, "ranges": ["Sheet1!A1:A4", "'Q3 plan/final'!B2"], "majorDimension": "COLUMNS" }), "GET", format!("{spreadsheet_path}/values:batchGet"))
            .query(json!({ "ranges": ["Sheet1!A1:A4", "'Q3 plan/final'!B2"], "majorDimension": "COLUMNS" }))
            .answers(200, value_ranges())
            .returns(json!({ "spreadsheetId": SPREADSHEET, "valueRanges": [
                { "range": "Sheet1!A1:A4", "majorDimension": "COLUMNS", "values": [["Item", "Bolts", "Nuts"]] },
                { "range": "'Q3 plan/final'!B2", "values": [] }
            ] })),
        Case::new("sheets_spreadsheets.values_update", json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A2:C2", "values": [["Bolts", 38, true]], "valueInputOption": "RAW" }), "PUT", format!("{spreadsheet_path}/values/Sheet1%21A2%3AC2"))
            .query(json!({ "valueInputOption": "RAW" }))
            .body(json!({ "values": [["Bolts", 38, true]] }))
            .answers(200, values_updated())
            .returns(json!({ "spreadsheetId": SPREADSHEET, "updatedRange": "Sheet1!A2:C2", "updatedRows": 1, "updatedColumns": 3, "updatedCells": 3 })),
        Case::new("sheets_spreadsheets.values_append", json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A:C", "values": [["Washers", "=6*2", false]], "valueInputOption": "USER_ENTERED" }), "POST", format!("{spreadsheet_path}/values/Sheet1%21A%3AC:append"))
            .query(json!({ "valueInputOption": "USER_ENTERED", "insertDataOption": "INSERT_ROWS" }))
            .body(json!({ "values": [["Washers", "=6*2", false]] }))
            .answers(200, values_appended())
            .returns(json!({ "spreadsheetId": SPREADSHEET, "tableRange": "Sheet1!A1:C4", "updates": { "updatedRange": "Sheet1!A5:C5", "updatedRows": 1, "updatedColumns": 3, "updatedCells": 3 } })),
    ]
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
        ("gmail_messages.list", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_messages.get", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_messages.attachment_get", Effect::Read, &[scopes::GMAIL_READONLY]),
        // Mail that was sent cannot be taken back.
        ("gmail_messages.send", Effect::Destructive, &[scopes::GMAIL_SEND]),
        // A reply reads the message it answers before it sends.
        ("gmail_messages.reply", Effect::Destructive, &[scopes::GMAIL_READONLY, scopes::GMAIL_SEND]),
        // Google sends a draft under the scope for drafts, not the one for sending.
        ("gmail_messages.send_draft", Effect::Destructive, &[scopes::GMAIL_COMPOSE]),
        ("gmail_messages.modify", Effect::Write, &[scopes::GMAIL_MODIFY]),
        // Takes the message out of the mailbox, and Gmail deletes it for good when it empties the bin.
        ("gmail_messages.trash", Effect::Destructive, &[scopes::GMAIL_MODIFY]),
        ("gmail_messages.untrash", Effect::Write, &[scopes::GMAIL_MODIFY]),
        ("gmail_threads.list", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_threads.get", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_labels.list", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_labels.get", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_profile.get", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_drafts.list", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_drafts.get", Effect::Read, &[scopes::GMAIL_READONLY]),
        ("gmail_drafts.create", Effect::Write, &[scopes::GMAIL_COMPOSE]),
        // An update replaces the whole of what the draft said.
        ("gmail_drafts.update", Effect::Destructive, &[scopes::GMAIL_COMPOSE]),
        ("gmail_drafts.delete", Effect::Destructive, &[scopes::GMAIL_COMPOSE]),

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
        // Who can see a file follows its folder, and a shared drive keeps what is moved into it.
        ("drive_files.move_to", Effect::Destructive, &[scopes::DRIVE_FILE]),
        ("drive_files.rename", Effect::Write, &[scopes::DRIVE_FILE]),
        // Takes the file from everyone who could see it, and Google deletes it for good after 30 days.
        ("drive_files.trash", Effect::Destructive, &[scopes::DRIVE_FILE]),
        ("drive_shared_drives.list", Effect::Read, &[scopes::DRIVE_READONLY]),

        // ── docs and sheets: effects ──
        ("docs_documents.get", Effect::Read, &[scopes::DOCUMENTS_READONLY]),
        ("docs_documents.read", Effect::Read, &[scopes::DOCUMENTS_READONLY]),
        ("docs_documents.create", Effect::Write, &[scopes::DOCUMENTS]),
        // Adds to the end of a document; nothing that was there is changed.
        ("docs_documents.append_text", Effect::Write, &[scopes::DOCUMENTS]),
        ("sheets_spreadsheets.get", Effect::Read, &[scopes::SPREADSHEETS_READONLY]),
        ("sheets_spreadsheets.values_get", Effect::Read, &[scopes::SPREADSHEETS_READONLY]),
        ("sheets_spreadsheets.values_batch_get", Effect::Read, &[scopes::SPREADSHEETS_READONLY]),
        // What was in the cells of the range is written over.
        ("sheets_spreadsheets.values_update", Effect::Destructive, &[scopes::SPREADSHEETS]),
        ("sheets_spreadsheets.values_append", Effect::Write, &[scopes::SPREADSHEETS]),
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
    // Gmail 19, Calendar 10, Meet 12, Drive 10, Docs and Sheets 9, and the two every integration has.
    assert_eq!(listed.len(), 62);
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

#[tokio::test]
async fn a_field_an_operation_does_not_know_is_refused_by_every_one_of_them_before_google_is_called() {
    // A field that is not known would be dropped in silence, with what it
    // said. That holds for an operation that takes nothing as well.
    let (server, socket, key) = google().await;
    for case in every_case() {
        let mut input = case.input.clone();
        input["notAField"] = serde_json::json!("x");
        let err = invoke(&socket, &key, case.name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{}: {err}", case.name);
        assert!(
            err.message().contains("`notAField`"),
            "{}: {}",
            case.name,
            err.message()
        );
    }
    // The two every integration has take nothing else either.
    for (name, input) in [
        ("identity.get", serde_json::json!({ "notAField": "x" })),
        (
            "resource.resolve",
            serde_json::json!({ "input": "1AbC_dEf-GhIjKlMnOpQrStUvWxYz012345", "notAField": "x" }),
        ),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}: {err}");
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing reached Google"
    );
}

#[tokio::test]
async fn input_that_is_not_an_object_is_refused_and_never_read_by_position() {
    // A list would otherwise be taken as the arguments in their order: a
    // file and a folder, with nothing to say which was which.
    let (server, socket, key) = google().await;
    for case in every_case() {
        let values: Vec<serde_json::Value> = case
            .input
            .as_object()
            .map(|fields| fields.values().cloned().collect())
            .unwrap_or_default();
        for input in [
            serde_json::Value::Array(values),
            serde_json::json!("x"),
            serde_json::json!(7),
        ] {
            let err = invoke(&socket, &key, case.name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{}: {err}", case.name);
        }
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing reached Google"
    );
}

#[tokio::test]
async fn a_change_is_sent_once_when_google_fails_and_a_read_is_tried_again() {
    for case in every_case() {
        let (server, socket, key) = google().await;
        Mock::given(any())
            .respond_with(google_error(503, "backendError", "Backend Error"))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, case.name, case.input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{}: {err}", case.name);
        // The first request an operation makes is the one that failed: the
        // read it begins with, when it makes several, or else its own.
        let first = case.also.first().map_or(case.verb, |(verb, _, _)| *verb);
        let sent = server.received_requests().await.unwrap().len();
        match first {
            // The test connection tries twice.
            "GET" => assert_eq!(sent, 2, "{}: a read is tried again", case.name),
            // It may have happened, so it is not sent again.
            "POST" | "PATCH" => assert_eq!(sent, 1, "{}: sent once", case.name),
            // The transport still repeats these two after a server error; the
            // guide says what that means for each operation that uses them.
            "PUT" | "DELETE" => assert_eq!(sent, 2, "{}", case.name),
            other => panic!("{}: {other} is not a verb Google is sent", case.name),
        }
    }
}
