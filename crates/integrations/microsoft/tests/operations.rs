//! Every Microsoft operation, called by name against a local server that answers as Microsoft Graph does.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{Effect, ErrorKind, Integration};
use socketkit_microsoft::models::{
    Attendee, CreateEvent, DateTimeTimeZone, EventResponse, Paging, RespondToEvent, UpdateEvent,
};
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    AS_TEXT, Case, IN_UTC, answer, answering, body_of, contains, graph_error, invoke, message, message_returned,
    microsoft, only_request, prefer, query_of, to,
};

const JOIN_URL: &str = "https://teams.microsoft.com/l/meetup-join/19%3ameeting_abc%40thread.v2/0";

fn at(date_time: &str) -> Value {
    json!({ "dateTime": date_time, "timeZone": "UTC" })
}

fn calendar() -> Value {
    json!({
        "id": "cal-1", "name": "Calendar", "color": "auto", "hexColor": "", "isDefaultCalendar": true,
        "canEdit": true, "canShare": true, "canViewPrivateItems": true,
        "owner": { "name": "Ada Lovelace", "address": "ada@contoso.example" },
        "defaultOnlineMeetingProvider": "teamsForBusiness", "allowedOnlineMeetingProviders": ["teamsForBusiness"]
    })
}

/// A Teams meeting as Graph returns it when asked for UTC.
fn event() -> Value {
    json!({
        "id": "evt-1",
        "subject": "Design review",
        "bodyPreview": "Agenda attached",
        "body": { "contentType": "html", "content": "<p>Agenda attached</p>" },
        "start": { "dateTime": "2026-10-12T16:00:00.0000000", "timeZone": "UTC" },
        "end": { "dateTime": "2026-10-12T17:00:00.0000000", "timeZone": "UTC" },
        "originalStartTimeZone": "Pacific Standard Time",
        "originalEndTimeZone": "Pacific Standard Time",
        "isAllDay": false, "isCancelled": false, "isOrganizer": true,
        "organizer": { "emailAddress": { "name": "Ada Lovelace", "address": "ada@contoso.example" } },
        "attendees": [{
            "type": "required",
            "status": { "response": "accepted", "time": "2026-10-10T09:00:00Z" },
            "emailAddress": { "name": "Grace Hopper", "address": "grace@contoso.example" }
        }],
        "responseStatus": { "response": "organizer", "time": "0001-01-01T00:00:00Z" },
        "location": { "displayName": "Room 1", "locationType": "conferenceRoom" },
        "isOnlineMeeting": true,
        "onlineMeetingProvider": "teamsForBusiness",
        "onlineMeeting": { "joinUrl": JOIN_URL, "conferenceId": "177513992", "tollNumber": "+1 425 555 0100" },
        "onlineMeetingUrl": null,
        "seriesMasterId": null,
        "type": "singleInstance",
        "showAs": "busy",
        "webLink": "https://outlook.office365.com/owa/?itemid=evt-1",
        "iCalUId": "040000008200E00074C5B7101A82E008"
    })
}

/// What an operation that returns `event()` must pass on.
fn event_returned() -> Value {
    json!({
        "id": "evt-1",
        "subject": "Design review",
        "start": { "dateTime": "2026-10-12T16:00:00.0000000", "timeZone": "UTC" },
        "end": { "dateTime": "2026-10-12T17:00:00.0000000", "timeZone": "UTC" },
        "originalStartTimeZone": "Pacific Standard Time",
        "originalEndTimeZone": "Pacific Standard Time",
        "organizer": { "emailAddress": { "address": "ada@contoso.example" } },
        "attendees": [{ "type": "required", "status": { "response": "accepted" }, "emailAddress": { "address": "grace@contoso.example" } }],
        "location": { "displayName": "Room 1" },
        "body": { "contentType": "html", "content": "<p>Agenda attached</p>" },
        "isOnlineMeeting": true,
        "onlineMeeting": { "joinUrl": JOIN_URL },
        "iCalUId": "040000008200E00074C5B7101A82E008"
    })
}

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, path, query, body, in_utc: bool, status, response, returns| Case { name, input, verb, path, query, body, prefer: in_utc.then_some(IN_UTC), status, response, returns };
    let week = json!({ "startDateTime": "2026-10-12T00:00:00Z", "endDateTime": "2026-10-19T00:00:00Z" });
    let attendee = json!({ "type": "required", "emailAddress": { "address": "grace@contoso.example" } });
    vec![
        // calendars
        case("calendars.list", json!({ "limit": 2 }), "GET", "/me/calendars", json!({ "$top": "2" }), json!(null), false, 200,
            json!({ "value": [calendar()] }),
            json!({ "items": [{ "id": "cal-1", "name": "Calendar", "isDefaultCalendar": true, "canEdit": true, "owner": { "address": "ada@contoso.example" } }], "next_cursor": null })),
        case("calendars.get", json!({ "calendar": "cal-1" }), "GET", "/me/calendars/cal-1", json!({}), json!(null), false, 200,
            calendar(), json!({ "id": "cal-1", "name": "Calendar", "defaultOnlineMeetingProvider": "teamsForBusiness" })),

        // events: reading
        case("events.list_between", json!({ "start": "2026-10-12T00:00:00Z", "end": "2026-10-19T00:00:00Z" }), "GET", "/me/calendarView", week.clone(), json!(null), true, 200,
            json!({ "value": [event()] }), json!({ "items": [event_returned()], "next_cursor": null })),
        case("events.get", json!({ "event": "evt-1" }), "GET", "/me/events/evt-1", json!({}), json!(null), true, 200,
            event(), event_returned()),
        case("events.instances", json!({ "event": "evt-1", "start": "2026-10-12T00:00:00Z", "end": "2026-10-19T00:00:00Z", "limit": 5 }), "GET", "/me/events/evt-1/instances",
            json!({ "startDateTime": "2026-10-12T00:00:00Z", "endDateTime": "2026-10-19T00:00:00Z", "$top": "5" }), json!(null), true, 200,
            json!({ "value": [event()] }), json!({ "items": [{ "id": "evt-1" }], "next_cursor": null })),

        // events: availability. Graph offers these only as POST; they change nothing.
        case("events.find_meeting_times",
            json!({ "attendees": [attendee.clone()], "timeConstraint": { "timeSlots": [{ "start": at("2026-10-12T09:00:00"), "end": at("2026-10-12T17:00:00") }] }, "meetingDuration": "PT1H", "maxCandidates": 3 }),
            "POST", "/me/findMeetingTimes", json!({}),
            json!({ "attendees": [attendee.clone()], "timeConstraint": { "timeSlots": [{ "start": at("2026-10-12T09:00:00"), "end": at("2026-10-12T17:00:00") }] }, "meetingDuration": "PT1H", "maxCandidates": 3 }),
            true, 200,
            json!({ "emptySuggestionsReason": "", "meetingTimeSuggestions": [{
                "confidence": 100.0, "order": 1, "organizerAvailability": "free", "suggestionReason": "Suggested because it is one of the nearest times when all attendees are available.",
                "attendeeAvailability": [{ "availability": "free", "attendee": attendee.clone() }],
                "meetingTimeSlot": { "start": at("2026-10-12T10:00:00.0000000"), "end": at("2026-10-12T11:00:00.0000000") }, "locations": []
            }] }),
            json!({ "emptySuggestionsReason": "", "meetingTimeSuggestions": [{
                "confidence": 100.0, "order": 1, "organizerAvailability": "free",
                "attendeeAvailability": [{ "availability": "free", "attendee": { "emailAddress": { "address": "grace@contoso.example" } } }],
                "meetingTimeSlot": { "start": at("2026-10-12T10:00:00.0000000"), "end": at("2026-10-12T11:00:00.0000000") }
            }] })),
        case("events.schedule",
            json!({ "schedules": ["grace@contoso.example", "room1@contoso.example"], "startTime": at("2026-10-12T09:00:00"), "endTime": at("2026-10-12T11:00:00"), "availabilityViewInterval": 30 }),
            "POST", "/me/calendar/getSchedule", json!({}),
            json!({ "schedules": ["grace@contoso.example", "room1@contoso.example"], "startTime": at("2026-10-12T09:00:00"), "endTime": at("2026-10-12T11:00:00"), "availabilityViewInterval": 30 }),
            true, 200,
            json!({ "value": [
                { "scheduleId": "grace@contoso.example", "availabilityView": "0220",
                  "scheduleItems": [{ "status": "busy", "start": at("2026-10-12T09:30:00.0000000"), "end": at("2026-10-12T10:30:00.0000000"), "subject": "1:1", "location": "Room 1", "isPrivate": false }],
                  "workingHours": { "daysOfWeek": ["monday"], "startTime": "08:00:00.0000000", "endTime": "17:00:00.0000000", "timeZone": { "name": "Pacific Standard Time" } } },
                { "scheduleId": "room1@contoso.example", "availabilityView": "", "error": { "message": "Unable to resolve the mailbox.", "responseCode": "ErrorMailRecipientNotFound" } }
            ] }),
            json!([
                { "scheduleId": "grace@contoso.example", "availabilityView": "0220", "scheduleItems": [{ "status": "busy", "subject": "1:1", "location": "Room 1", "start": at("2026-10-12T09:30:00.0000000") }] },
                { "scheduleId": "room1@contoso.example", "scheduleItems": [], "error": { "responseCode": "ErrorMailRecipientNotFound" } }
            ])),

        // events: writing
        case("events.create",
            json!({ "subject": "Design review", "body": { "contentType": "html", "content": "<p>Agenda attached</p>" }, "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00"),
                "location": { "displayName": "Room 1" }, "attendees": [attendee.clone()], "isOnlineMeeting": true }),
            "POST", "/me/events", json!({}),
            json!({ "subject": "Design review", "body": { "contentType": "html", "content": "<p>Agenda attached</p>" }, "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00"),
                "location": { "displayName": "Room 1" }, "attendees": [attendee.clone()], "isOnlineMeeting": true, "onlineMeetingProvider": "teamsForBusiness" }),
            true, 201, event(), event_returned()),
        case("events.update", json!({ "event": "evt-1", "subject": "Design review, part two", "showAs": "tentative" }), "PATCH", "/me/events/evt-1", json!({}),
            json!({ "subject": "Design review, part two", "showAs": "tentative" }), true, 200, event(), json!({ "id": "evt-1" })),
        case("events.respond", json!({ "event": "evt-1", "response": "tentatively_accept", "comment": "Might be late", "sendResponse": true }), "POST", "/me/events/evt-1/tentativelyAccept", json!({}),
            json!({ "comment": "Might be late", "sendResponse": true }), false, 202, json!(null), json!(null)),
        case("events.cancel", json!({ "event": "evt-1", "comment": "Moved to next week" }), "POST", "/me/events/evt-1/cancel", json!({}),
            json!({ "comment": "Moved to next week" }), false, 202, json!(null), json!(null)),
        case("events.delete", json!({ "event": "evt-1" }), "DELETE", "/me/events/evt-1", json!({}), json!(null), false, 204, json!(null), json!(null)),
    ]
}

/// The mail operations: what each sends and what it returns.
#[rustfmt::skip]
fn mail_cases() -> Vec<Case> {
    let case = |name, input, verb, path, query, body, prefer, status, response, returns| Case { name, input, verb, path, query, body, prefer, status, response, returns };
    let text = json!({ "contentType": "text", "content": "See you Monday." });
    let draft = json!({ "id": "msg-9", "isDraft": true, "subject": "Re: Q3 plan", "toRecipients": to("grace@contoso.example") });
    let folder = json!({ "id": "folder-inbox", "displayName": "Inbox", "parentFolderId": "folder-root", "childFolderCount": 2, "unreadItemCount": 5, "totalItemCount": 120, "isHidden": false });
    let attachment = json!({ "@odata.type": "#microsoft.graph.fileAttachment", "id": "att-1", "name": "plan.pdf", "contentType": "application/pdf", "size": 2048, "isInline": false, "lastModifiedDateTime": "2026-10-09T08:14:00Z", "contentId": null });
    vec![
        // mail: reading
        case("mail.list", json!({ "folder": "inbox", "filter": "receivedDateTime ge 2026-10-01T00:00:00Z", "orderBy": "receivedDateTime desc", "limit": 5 }), "GET", "/me/mailFolders/inbox/messages",
            json!({ "$filter": "receivedDateTime ge 2026-10-01T00:00:00Z", "$orderby": "receivedDateTime desc", "$top": "5" }), json!(null), Some(AS_TEXT), 200,
            json!({ "value": [message()] }), json!({ "items": [message_returned()], "next_cursor": null })),
        case("mail.get", json!({ "message": "msg-1" }), "GET", "/me/messages/msg-1", json!({}), json!(null), Some(AS_TEXT), 200,
            message(), message_returned()),
        // Oldest first, in the only form Graph's rules for a filter with a sort allow.
        case("mail.conversation", json!({ "conversation": "conv-1" }), "GET", "/me/messages",
            json!({ "$filter": "receivedDateTime ge 1900-01-01T00:00:00Z and conversationId eq 'conv-1'", "$orderby": "receivedDateTime asc" }), json!(null), Some(AS_TEXT), 200,
            json!({ "value": [message()] }), json!({ "items": [{ "id": "msg-1", "conversationId": "conv-1" }], "next_cursor": null })),
        // The list asks for what describes an attachment and not for its content.
        case("mail.attachments_list", json!({ "message": "msg-1" }), "GET", "/me/messages/msg-1/attachments",
            json!({ "$select": "id,name,contentType,size,isInline,lastModifiedDateTime" }), json!(null), None, 200,
            json!({ "value": [attachment.clone()] }), json!({ "items": [{ "@odata.type": "#microsoft.graph.fileAttachment", "id": "att-1", "name": "plan.pdf", "contentType": "application/pdf", "size": 2048, "isInline": false, "contentBytes": null }], "next_cursor": null })),
        case("mail.attachment_get", json!({ "message": "msg-1", "attachment": "att-1" }), "GET", "/me/messages/msg-1/attachments/att-1", json!({}), json!(null), None, 200,
            json!({ "@odata.type": "#microsoft.graph.fileAttachment", "id": "att-1", "name": "plan.pdf", "contentType": "application/pdf", "size": 5, "isInline": false, "contentBytes": "aGVsbG8=" }),
            json!({ "id": "att-1", "name": "plan.pdf", "size": 5, "contentBytes": "aGVsbG8=" })),

        // mail folders
        case("mail_folders.list", json!({ "limit": 50 }), "GET", "/me/mailFolders", json!({ "$top": "50" }), json!(null), None, 200,
            json!({ "value": [folder.clone()] }), json!({ "items": [{ "id": "folder-inbox", "displayName": "Inbox", "unreadItemCount": 5, "totalItemCount": 120, "childFolderCount": 2 }], "next_cursor": null })),
        case("mail_folders.get", json!({ "folder": "inbox" }), "GET", "/me/mailFolders/inbox", json!({}), json!(null), None, 200,
            folder.clone(), json!({ "id": "folder-inbox", "displayName": "Inbox", "parentFolderId": "folder-root" })),

        // mail: drafts
        case("mail.create_draft", json!({ "subject": "Monday", "body": text.clone(), "toRecipients": to("grace@contoso.example") }), "POST", "/me/messages", json!({}),
            json!({ "subject": "Monday", "body": text.clone(), "toRecipients": to("grace@contoso.example") }), None, 201, draft.clone(), json!({ "id": "msg-9", "isDraft": true })),
        case("mail.update_draft", json!({ "message": "msg-9", "subject": "Monday, 10:00" }), "PATCH", "/me/messages/msg-9", json!({}),
            json!({ "subject": "Monday, 10:00" }), None, 200, draft.clone(), json!({ "id": "msg-9" })),
        case("mail.create_reply", json!({ "message": "msg-1", "comment": "Thanks, will read." }), "POST", "/me/messages/msg-1/createReply", json!({}),
            json!({ "comment": "Thanks, will read." }), None, 201, draft.clone(), json!({ "id": "msg-9", "isDraft": true })),
        case("mail.create_reply_all", json!({ "message": "msg-1", "body": text.clone() }), "POST", "/me/messages/msg-1/createReplyAll", json!({}),
            json!({ "message": { "body": text.clone() } }), None, 200, draft.clone(), json!({ "id": "msg-9" })),
        case("mail.create_forward", json!({ "message": "msg-1", "toRecipients": to("alan@contoso.example"), "comment": "FYI" }), "POST", "/me/messages/msg-1/createForward", json!({}),
            json!({ "comment": "FYI", "message": { "toRecipients": to("alan@contoso.example") } }), None, 201, draft.clone(), json!({ "id": "msg-9" })),

        // mail: sending
        case("mail.send", json!({ "subject": "Monday", "body": text.clone(), "toRecipients": to("grace@contoso.example"), "saveToSentItems": false }), "POST", "/me/sendMail", json!({}),
            json!({ "message": { "subject": "Monday", "body": text.clone(), "toRecipients": to("grace@contoso.example") }, "saveToSentItems": false }), None, 202, json!(null), json!(null)),
        case("mail.send_draft", json!({ "message": "msg-9" }), "POST", "/me/messages/msg-9/send", json!({}), json!({}), None, 202, json!(null), json!(null)),
        case("mail.reply", json!({ "message": "msg-1", "comment": "Agreed." }), "POST", "/me/messages/msg-1/reply", json!({}),
            json!({ "comment": "Agreed." }), None, 202, json!(null), json!(null)),

        // mail: changing
        case("mail.update", json!({ "message": "msg-1", "isRead": true, "categories": ["Customer", "Q3"], "flag": { "flagStatus": "flagged" } }), "PATCH", "/me/messages/msg-1", json!({}),
            json!({ "isRead": true, "categories": ["Customer", "Q3"], "flag": { "flagStatus": "flagged" } }), None, 200, message(), json!({ "id": "msg-1" })),
        // A moved message comes back under a new id.
        case("mail.move_to", json!({ "message": "msg-1", "folder": "archive" }), "POST", "/me/messages/msg-1/move", json!({}),
            json!({ "destinationId": "archive" }), None, 201, json!({ "id": "msg-2", "parentFolderId": "folder-archive", "subject": "Q3 plan" }), json!({ "id": "msg-2", "parentFolderId": "folder-archive" })),
        case("mail.delete", json!({ "message": "msg-1" }), "DELETE", "/me/messages/msg-1", json!({}), json!(null), None, 204, json!(null), json!(null)),
    ]
}

/// Every operation, whichever product it belongs to.
fn every_case() -> Vec<Case> {
    let mut all = cases();
    all.extend(mail_cases());
    all
}

#[tokio::test]
async fn the_table_below_covers_every_operation_microsoft_offers() {
    let listed: Vec<String> = Microsoft::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = every_case().iter().map(|c| format!("microsoft.{}", c.name)).collect();
    tested.extend([
        "microsoft.identity.get".to_owned(),
        "microsoft.resource.resolve".to_owned(),
    ]);
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist"
    );
    assert_eq!(listed.len(), 32);
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_graph_sent() {
    for case in every_case() {
        let (server, socket, key) = microsoft().await;
        Mock::given(method(case.verb))
            .and(path(format!("/v1.0{}", case.path)))
            .respond_with(answer(case.status, &case.response))
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

        let request = only_request(&server).await;
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            "Bearer eyJ.good",
            "{}",
            case.name
        );
        assert_eq!(
            query_of(&request),
            case.query,
            "{}: exactly these parameters reach Graph",
            case.name
        );
        assert_eq!(
            body_of(&request),
            case.body,
            "{}: exactly this body reaches Graph",
            case.name
        );
        assert_eq!(
            prefer(&request),
            case.prefer,
            "{}: exactly this preference reaches Graph",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_describes_its_input_and_marks_what_it_changes() {
    let operations = Microsoft::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    for operation in &operations {
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert!(!operation.description.is_empty(), "{}", operation.name);
    }

    // A host lets a read run freely and asks a person before anything else,
    // so each effect is stated here and not derived from the code under test.
    let expected = [
        ("calendars.list", Effect::Read, "Calendars.Read"),
        ("calendars.get", Effect::Read, "Calendars.Read"),
        ("events.list_between", Effect::Read, "Calendars.Read"),
        ("events.get", Effect::Read, "Calendars.Read"),
        ("events.instances", Effect::Read, "Calendars.Read"),
        ("events.find_meeting_times", Effect::Read, "Calendars.Read.Shared"),
        ("events.schedule", Effect::Read, "Calendars.Read"),
        ("events.create", Effect::Write, "Calendars.ReadWrite"),
        // Overwrites what was there, and drops any attendee left out of the list.
        ("events.update", Effect::Destructive, "Calendars.ReadWrite"),
        // An answer reaches the organiser and cannot be taken back.
        ("events.respond", Effect::Destructive, "Calendars.ReadWrite"),
        ("events.cancel", Effect::Destructive, "Calendars.ReadWrite"),
        ("events.delete", Effect::Destructive, "Calendars.ReadWrite"),
        ("mail.list", Effect::Read, "Mail.Read"),
        ("mail.get", Effect::Read, "Mail.Read"),
        ("mail.conversation", Effect::Read, "Mail.Read"),
        ("mail.attachments_list", Effect::Read, "Mail.Read"),
        ("mail.attachment_get", Effect::Read, "Mail.Read"),
        ("mail_folders.list", Effect::Read, "Mail.Read"),
        ("mail_folders.get", Effect::Read, "Mail.Read"),
        // A draft is the person's own until it is sent, and can be thrown away.
        ("mail.create_draft", Effect::Write, "Mail.ReadWrite"),
        ("mail.create_reply", Effect::Write, "Mail.ReadWrite"),
        ("mail.create_reply_all", Effect::Write, "Mail.ReadWrite"),
        ("mail.create_forward", Effect::Write, "Mail.ReadWrite"),
        // Overwrites the draft's text and recipients.
        ("mail.update_draft", Effect::Destructive, "Mail.ReadWrite"),
        // Sent mail cannot be taken back.
        ("mail.send", Effect::Destructive, "Mail.Send"),
        ("mail.send_draft", Effect::Destructive, "Mail.Send"),
        ("mail.reply", Effect::Destructive, "Mail.Send"),
        // Marks on the person's own copy, each of which can be set back.
        ("mail.update", Effect::Write, "Mail.ReadWrite"),
        // Takes the message from where it was, to Deleted Items if asked, and its id stops working.
        ("mail.move_to", Effect::Destructive, "Mail.ReadWrite"),
        ("mail.delete", Effect::Destructive, "Mail.ReadWrite"),
    ];
    assert_eq!(expected.len(), every_case().len());
    for (name, effect, scope) in expected {
        let operation = find(&format!("microsoft.{name}"));
        assert_eq!(operation.effect, effect, "{name}");
        assert_eq!(operation.required_scopes, [scope], "{name}");
    }

    // Nothing that changes a calendar is sent as a GET, which the transport
    // always repeats after a server error. The two reads Graph offers only as
    // POST are the only reads that are not a GET.
    for case in every_case() {
        let effect = find(&format!("microsoft.{}", case.name)).effect;
        let posted_read = matches!(case.name, "events.find_meeting_times" | "events.schedule");
        match effect {
            Effect::Read if posted_read => assert_eq!(case.verb, "POST", "{}", case.name),
            Effect::Read => assert_eq!(case.verb, "GET", "{}", case.name),
            _ => assert_ne!(case.verb, "GET", "{}", case.name),
        }
    }

    let create = find("microsoft.events.create");
    let required: Vec<&str> = create.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(required, ["start", "end"], "an event needs only its times");
    for field in [
        "calendar",
        "subject",
        "body",
        "attendees",
        "location",
        "isOnlineMeeting",
    ] {
        assert!(
            create.input_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    for field in ["id", "start", "onlineMeeting", "originalStartTimeZone"] {
        assert!(
            create.output_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent_at_any_depth() {
    let (server, socket, key) = answering(201, event()).await;
    let input = json!({
        "start": { "dateTime": "2026-10-12T16:00:00", "timeZone": "UTC" },
        "end": { "dateTime": "2026-10-12T17:00:00", "timeZone": "Pacific Standard Time" },
        "attendees": [{ "emailAddress": { "address": "grace@contoso.example" } }]
    });
    invoke(&socket, &key, "events.create", input.clone()).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        input,
        "Graph applies its own defaults to everything else"
    );
}

#[tokio::test]
async fn a_time_must_say_which_zone_it_is_in() {
    // A time with no zone would have to be guessed at, and a wrong guess puts
    // a meeting hours away from where it was meant, with the invitations
    // already sent. A zone under a misspelt key is no zone.
    let (server, socket, key) = answering(201, event()).await;
    let good = at("2026-10-12T17:00:00");
    for start in [
        json!({ "dateTime": "2026-10-12T16:00:00" }),
        json!({ "dateTime": "2026-10-12T16:00:00", "timezone": "Pacific Standard Time" }),
        json!({ "dateTime": "2026-10-12T16:00:00", "time_zone": "Pacific Standard Time" }),
        json!({ "dateTime": "2026-10-12T16:00:00", "timeZone": null }),
        json!({ "dateTime": "2026-10-12T16:00:00", "timeZone": " " }),
        json!({ "datetime": "2026-10-12T16:00:00", "timeZone": "UTC" }),
    ] {
        for (name, input) in [
            ("events.create", json!({ "start": start.clone(), "end": good.clone() })),
            ("events.create", json!({ "start": good.clone(), "end": start.clone() })),
            ("events.update", json!({ "event": "evt-1", "start": start.clone() })),
            (
                "events.respond",
                json!({ "event": "evt-1", "response": "decline", "proposedNewTime": { "start": start.clone(), "end": good.clone() } }),
            ),
            (
                "events.schedule",
                json!({ "schedules": ["grace@contoso.example"], "startTime": good.clone(), "endTime": start.clone() }),
            ),
            (
                "events.find_meeting_times",
                json!({ "timeConstraint": { "timeSlots": [{ "start": start.clone(), "end": good.clone() }] } }),
            ),
        ] {
            let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        }
    }
    let missing = json!({ "start": { "dateTime": "2026-10-12T16:00:00" }, "end": good });
    let err = invoke(&socket, &key, "events.create", missing).await.unwrap_err();
    assert!(err.message().contains("timeZone"), "{}", err.message());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_attendees_answer_is_read_from_graph_and_never_sent_back() {
    // Adding one attendee means sending the whole list, and the natural list
    // to send is the one that was just read, answers and all.
    let (server, socket, key) = answering(200, event()).await;
    let read = invoke(&socket, &key, "events.get", json!({ "event": "evt-1" }))
        .await
        .unwrap();
    let mut attendees = read["attendees"].as_array().unwrap().clone();
    assert_eq!(attendees[0]["status"]["response"], "accepted");
    attendees.push(json!({ "type": "optional", "emailAddress": { "address": "alan@contoso.example" } }));
    server.reset().await;
    Mock::given(any())
        .respond_with(answer(200, &event()))
        .mount(&server)
        .await;

    let sent = json!([
        { "type": "required", "emailAddress": { "name": "Grace Hopper", "address": "grace@contoso.example" } },
        { "type": "optional", "emailAddress": { "address": "alan@contoso.example" } }
    ]);
    let times = json!({ "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00") });
    for (name, mut input) in [
        ("events.update", json!({ "event": "evt-1" })),
        ("events.create", times.clone()),
        ("events.find_meeting_times", json!({})),
    ] {
        input["attendees"] = json!(attendees);
        invoke(&socket, &key, name, input).await.unwrap();
        let received = server.received_requests().await.unwrap();
        assert_eq!(body_of(received.last().unwrap())["attendees"], sent, "{name}");
    }
}

#[tokio::test]
async fn an_online_meeting_is_a_teams_meeting_unless_another_provider_is_named() {
    let times = json!({ "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00") });
    for (extra, sent) in [
        (
            json!({ "isOnlineMeeting": true }),
            json!({ "isOnlineMeeting": true, "onlineMeetingProvider": "teamsForBusiness" }),
        ),
        (
            json!({ "isOnlineMeeting": true, "onlineMeetingProvider": "skypeForConsumer" }),
            json!({ "isOnlineMeeting": true, "onlineMeetingProvider": "skypeForConsumer" }),
        ),
        (json!({ "isOnlineMeeting": false }), json!({ "isOnlineMeeting": false })),
        (json!({}), json!({})),
    ] {
        let (server, socket, key) = answering(201, event()).await;
        let mut input = times.clone();
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        let created = invoke(&socket, &key, "events.create", input).await.unwrap();
        let mut expected = times.clone();
        expected
            .as_object_mut()
            .unwrap()
            .extend(sent.as_object().unwrap().clone());
        assert_eq!(body_of(&only_request(&server).await), expected, "{extra}");
        // The join link is how the meeting's transcript is found later.
        assert_eq!(created["onlineMeeting"]["joinUrl"], JOIN_URL);
    }
}

#[tokio::test]
async fn an_event_goes_into_the_calendar_that_is_named() {
    let (server, socket, key) = answering(201, event()).await;
    let input = json!({ "calendar": "cal-1", "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00") });
    invoke(&socket, &key, "events.create", input).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/v1.0/me/calendars/cal-1/events");
    assert_eq!(
        body_of(&request),
        json!({ "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00") }),
        "the calendar is in the address, not in the event"
    );

    let (server, socket, key) = answering(200, json!({ "value": [] })).await;
    let input = json!({ "calendar": "cal-1", "start": "2026-10-12T00:00:00Z", "end": "2026-10-19T00:00:00+05:30" });
    invoke(&socket, &key, "events.list_between", input).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/v1.0/me/calendars/cal-1/calendarView");
    assert_eq!(
        query_of(&request),
        json!({ "startDateTime": "2026-10-12T00:00:00Z", "endDateTime": "2026-10-19T00:00:00+05:30" }),
        "an offset reaches Graph as it was written"
    );
}

#[tokio::test]
async fn each_response_to_an_invitation_has_its_own_action() {
    for (response, action) in [
        ("accept", "accept"),
        ("tentatively_accept", "tentativelyAccept"),
        ("decline", "decline"),
    ] {
        let (server, socket, key) = answering(202, json!(null)).await;
        let input = json!({ "event": "evt-1", "response": response });
        assert_eq!(
            invoke(&socket, &key, "events.respond", input).await.unwrap(),
            json!(null)
        );
        let request = only_request(&server).await;
        assert_eq!(request.method.as_str(), "POST");
        assert_eq!(request.url.path(), format!("/v1.0/me/events/evt-1/{action}"));
        assert_eq!(body_of(&request), json!({}));
    }

    // A new time can be proposed when declining or accepting tentatively.
    let (server, socket, key) = answering(202, json!(null)).await;
    let slot = json!({ "start": at("2026-10-13T16:00:00"), "end": at("2026-10-13T17:00:00") });
    let input = json!({ "event": "evt-1", "response": "decline", "proposedNewTime": slot.clone() });
    invoke(&socket, &key, "events.respond", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "proposedNewTime": slot })
    );
}

#[tokio::test]
async fn what_graph_leaves_empty_does_not_stop_an_event_from_being_read() {
    // A private appointment on a shared calendar, and an event with nothing but its times.
    for sparse in [
        json!({
            "id": "evt-2", "subject": null, "bodyPreview": null, "body": null, "location": null,
            "organizer": null, "attendees": null, "onlineMeeting": null, "onlineMeetingProvider": null,
            "isOnlineMeeting": null, "isAllDay": null, "isCancelled": null, "responseStatus": null,
            "originalStartTimeZone": null, "seriesMasterId": null, "type": null, "showAs": null,
            "start": { "dateTime": "2026-10-12T00:00:00.0000000", "timeZone": "UTC" },
            "end": { "dateTime": "2026-10-13T00:00:00.0000000", "timeZone": "UTC" }
        }),
        json!({ "id": "evt-2" }),
        json!({ "id": "evt-2", "attendees": [{ "emailAddress": { "name": null, "address": null }, "status": null, "type": null }], "location": { "displayName": null } }),
        json!({
            "id": "evt-2", "organizer": { "emailAddress": null }, "attendees": [{ "emailAddress": null }],
            "body": { "contentType": null, "content": null },
            "start": { "dateTime": "2026-10-12T00:00:00.0000000", "timeZone": null },
            "end": { "dateTime": null, "timeZone": null }
        }),
    ] {
        let (_server, socket, key) = answering(200, sparse.clone()).await;
        let read = invoke(&socket, &key, "events.get", json!({ "event": "evt-2" }))
            .await
            .unwrap_or_else(|e| panic!("{sparse}: {e}"));
        assert_eq!(read["id"], "evt-2");
        assert_eq!(read["subject"], "");
        assert_eq!(read["isOnlineMeeting"], false);
        assert_eq!(read["onlineMeeting"], json!(null), "no link is invented");
    }
}

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error_not_an_empty_result() {
    for (name, input, response) in [
        ("events.get", json!({ "event": "evt-1" }), json!({})),
        ("events.get", json!({ "event": "evt-1" }), json!({ "id": "" })),
        ("events.get", json!({ "event": "evt-1" }), json!({ "id": 7 })),
        (
            "calendars.get",
            json!({ "calendar": "cal-1" }),
            json!({ "name": "Calendar" }),
        ),
        ("calendars.list", json!({}), json!({})),
        ("calendars.list", json!({}), json!({ "value": "none" })),
        (
            "events.list_between",
            json!({ "start": "2026-10-12", "end": "2026-10-13" }),
            json!({ "value": [{ "id": "evt-1", "start": "today" }] }),
        ),
        (
            "events.schedule",
            json!({ "schedules": ["a@contoso.example"], "startTime": at("2026-10-12T09:00:00"), "endTime": at("2026-10-12T10:00:00") }),
            json!({}),
        ),
        ("events.find_meeting_times", json!({}), json!(null)),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
    }
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_field_and_never_repeats_what_graph_sent() {
    // What Graph sends is the content of someone's calendar. An error is
    // logged and shown, so none of that content may travel in it, not in the
    // message and not in the cause behind it.
    let secret = "CONFIDENTIAL merger call with legal";
    for (name, input, response, field) in [
        (
            "events.get",
            json!({ "event": "evt-1" }),
            json!({ "id": "evt-1", "subject": "x", "start": secret }),
            "start",
        ),
        (
            "events.get",
            json!({ "event": "evt-1" }),
            json!({ "id": "evt-1", "attendees": [{ "emailAddress": secret }] }),
            "attendees[0].emailAddress",
        ),
        (
            "events.list_between",
            json!({ "start": "2026-10-12", "end": "2026-10-13" }),
            json!({ "value": [event(), { "id": "evt-2", "isOnlineMeeting": secret }] }),
            "[1].isOnlineMeeting",
        ),
        (
            "events.schedule",
            json!({ "schedules": ["a@contoso.example"], "startTime": at("2026-10-12T09:00:00"), "endTime": at("2026-10-12T10:00:00") }),
            json!({ "value": [{ "scheduleId": "a@contoso.example", "scheduleItems": [{ "subject": [secret] }] }] }),
            "[0].scheduleItems[0].subject",
        ),
        (
            "calendars.get",
            json!({ "calendar": "cal-1" }),
            json!({ "id": "cal-1", "owner": secret }),
            "owner",
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name}");
        assert!(
            err.message().contains(&format!("`{field}`")),
            "{name}: the field is named: {}",
            err.message()
        );
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
        assert!(!everything.contains("merger"), "{name}: {everything}");
    }
}

// ── Paging ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_address_of_the_next_page_is_the_cursor_and_is_requested_as_it_is() {
    let (server, socket, key) = microsoft().await;
    let next = format!(
        "{}/v1.0/me/calendarView?startDateTime=2026-10-12T00%3a00%3a00Z&endDateTime=2026-10-19T00%3a00%3a00Z&%24top=1&%24skip=1",
        server.uri()
    );
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [event()], "@odata.nextLink": next })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let first = invoke(
        &socket,
        &key,
        "events.list_between",
        json!({ "start": "2026-10-12T00:00:00Z", "end": "2026-10-19T00:00:00Z", "limit": 1 }),
    )
    .await
    .unwrap();
    assert_eq!(first["next_cursor"], next);

    server.reset().await;
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [event()] })))
        .mount(&server)
        .await;
    // The range and the limit are given again, as a caller in a loop would, and are not sent twice.
    let second = invoke(
        &socket,
        &key,
        "events.list_between",
        json!({ "start": "2026-10-12T00:00:00Z", "end": "2026-10-19T00:00:00Z", "limit": 1, "cursor": next }),
    )
    .await
    .unwrap();
    assert_eq!(second["next_cursor"], json!(null), "the last page has no cursor");
    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "GET");
    // The mock server records the address without its own host and port.
    let asked = format!("{}?{}", request.url.path(), request.url.query().unwrap());
    assert_eq!(
        asked,
        next.strip_prefix(&server.uri()).unwrap(),
        "Graph's address is used whole: nothing added, nothing written another way"
    );
    assert_eq!(prefer(&request), Some(IN_UTC), "the next page is asked for in UTC too");
    assert_eq!(request.headers.get("authorization").unwrap(), "Bearer eyJ.good");
}

#[tokio::test]
async fn a_cursor_from_another_host_or_with_no_place_in_the_list_is_refused() {
    // Microsoft has two hosts: Graph, and the one that signs people in. Here
    // `elsewhere` stands for the second: the provider may send it credentials,
    // but it is not the API.
    let server = MockServer::start().await;
    let elsewhere = MockServer::start().await;
    let mut spec = point_at(provider(), &server);
    spec.allowed_hosts
        .push(elsewhere.uri().trim_start_matches("http://").to_owned());
    let (socket, key) = connect(Arc::new(Microsoft::with_spec(spec)), "eyJ.good").await;
    for cursor in [
        format!("{}/v1.0/me/calendars?%24skip=10", elsewhere.uri()),
        "https://graph.microsoft.com/v1.0/me/calendars?$skip=10".to_owned(),
        "https://evil.example/v1.0/me/calendars?$skip=10".to_owned(),
        // No place in the list: that is the first page, not a next one.
        format!("{}/v1.0/me/calendars", server.uri()),
        format!("{}/v1.0/me/calendars?", server.uri()),
        format!("{}/v1.0/me/calendars#%24skip=10", server.uri()),
        // Nothing that is written before the host is taken on trust either.
        format!("{}/v1.0/me/calendars?%24skip=10", server.uri()).replace("http://", "http://user:pw@"),
        // Not an address at all.
        "me/calendars?$skip=10".to_owned(),
        "%24skip=10".to_owned(),
        "page-2".to_owned(),
    ] {
        let err = invoke(&socket, &key, "calendars.list", json!({ "cursor": cursor }))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{cursor:?}: {err}");
        assert!(
            !err.message().contains(&cursor),
            "the cursor is not repeated: {}",
            err.message()
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
    assert!(elsewhere.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn whatever_address_a_cursor_names_the_request_goes_to_the_lists_own() {
    // A cursor comes back from the caller. Only its query is used: the page
    // is requested at the address built from the operation's own arguments.
    // So a cursor that names mail, another person, another version of the
    // API or the sign-in endpoint reads none of them.
    for path in [
        "/v1.0/me/messages",
        "/v1.0/users/someone-else/calendars",
        "/v1.0/me/calendars/cal-1/events",
        "/v1.0/me",
        "/beta/me/calendars",
        "/v1.0/../beta/me/calendars",
        "/common/oauth2/v2.0/token",
        "/",
    ] {
        let (server, socket, key) = answering(200, json!({ "value": [calendar()] })).await;
        let cursor = format!("{}{path}?%24skip=10", server.uri());
        let page = invoke(&socket, &key, "calendars.list", json!({ "cursor": cursor }))
            .await
            .unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(page["items"][0]["id"], "cal-1");
        let request = only_request(&server).await;
        assert_eq!(request.method.as_str(), "GET", "{path}");
        assert_eq!(request.url.path(), "/v1.0/me/calendars", "{path}");
        assert_eq!(request.url.query(), Some("%24skip=10"), "{path}");
    }
}

#[tokio::test]
async fn a_next_page_is_found_however_graph_writes_its_address() {
    // Graph may write the address of the next page in another case, with a
    // key in brackets, or leaving unencoded what Socket encodes. The query,
    // where Graph keeps its place, is passed on whole and unchanged.
    for (name, input, cursor_path, cursor_query, asked_path) in [
        (
            "events.list_between",
            json!({ "start": "2026-10-12", "end": "2026-10-19" }),
            "/v1.0/me/calendarview",
            "startdatetime=2026-10-12&enddatetime=2026-10-19&%24skip=10",
            "/v1.0/me/calendarView",
        ),
        (
            "events.instances",
            json!({ "event": "AAMk=", "start": "2026-10-12", "end": "2026-10-19" }),
            "/v1.0/me/events('AAMk=')/instances",
            "%24skiptoken=abc%2Fdef%3d%3d",
            "/v1.0/me/events/AAMk%3D/instances",
        ),
        (
            "events.list_between",
            json!({ "start": "2026-10-12", "end": "2026-10-19", "calendar": "cal 1" }),
            "/v1.0/users('48d31887-5fad-4d73-a9f5-3c356e68a038')/calendars('cal%201')/calendarView",
            "%24skip=10",
            "/v1.0/me/calendars/cal%201/calendarView",
        ),
    ] {
        let (server, socket, key) = answering(200, json!({ "value": [event()] })).await;
        let mut input = input;
        input["cursor"] = json!(format!("{}{cursor_path}?{cursor_query}#ignored", server.uri()));
        let page = invoke(&socket, &key, name, input)
            .await
            .unwrap_or_else(|e| panic!("{cursor_path}: {e}"));
        assert_eq!(page["items"][0]["id"], "evt-1");
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), asked_path, "the address is Socket's own");
        assert_eq!(request.url.query(), Some(cursor_query), "the query is Graph's, whole");
        assert_eq!(request.url.fragment(), None);
        assert_eq!(prefer(&request), Some(IN_UTC));
    }
}

#[tokio::test]
async fn a_blank_cursor_is_the_first_page() {
    let (server, socket, key) = answering(200, json!({ "value": [calendar()] })).await;
    let page = invoke(&socket, &key, "calendars.list", json!({ "cursor": " ", "limit": 10 }))
        .await
        .unwrap();
    assert_eq!(page["items"][0]["id"], "cal-1");
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/v1.0/me/calendars");
    assert_eq!(query_of(&request), json!({ "$top": "10" }));
}

// ── What is refused before Graph is called ───────────────────────────────────

#[tokio::test]
async fn an_id_is_one_path_segment_whatever_it_contains() {
    for (id, sent) in [
        ("AAMkAGI2=", "/v1.0/me/events/AAMkAGI2%3D"),
        ("AAMk/AGI2+x", "/v1.0/me/events/AAMk%2FAGI2%2Bx"),
        ("evt-1/cancel", "/v1.0/me/events/evt-1%2Fcancel"),
        ("evt 1?$expand=x#y", "/v1.0/me/events/evt%201%3F%24expand%3Dx%23y"),
        (
            "../../users/someone-else/events/evt-1",
            "/v1.0/me/events/..%2F..%2Fusers%2Fsomeone-else%2Fevents%2Fevt-1",
        ),
        ("AAMk_AGI2-x.y~z", "/v1.0/me/events/AAMk_AGI2-x.y~z"),
    ] {
        let (server, socket, key) = answering(200, event()).await;
        invoke(&socket, &key, "events.get", json!({ "event": id }))
            .await
            .unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), sent, "{id:?}");
        assert_eq!(request.url.query(), None, "{id:?}");
    }

    // A segment that is only dots would be resolved away, and address something else.
    let (server, socket, key) = answering(200, event()).await;
    for id in [".", ".."] {
        for (name, input) in [
            ("events.get", json!({ "event": id })),
            ("events.delete", json!({ "event": id })),
            ("calendars.get", json!({ "calendar": id })),
            (
                "events.create",
                json!({ "calendar": id, "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00") }),
            ),
        ] {
            let err = invoke(&socket, &key, name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {id:?}");
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn input_of_the_wrong_shape_is_refused_by_name_without_calling_graph() {
    let (server, socket, key) = answering(200, event()).await;
    let bad = [
        ("events.get", json!({}), "event"),
        ("events.get", json!({ "event": 7 }), "event"),
        ("events.list_between", json!({ "start": "2026-10-12" }), "end"),
        ("events.create", json!({ "end": at("2026-10-12T17:00:00") }), "start"),
        ("events.respond", json!({ "event": "evt-1" }), "response"),
        (
            "events.schedule",
            json!({ "startTime": at("2026-10-12T09:00:00"), "endTime": at("2026-10-12T10:00:00") }),
            "schedules",
        ),
        ("calendars.get", json!({ "calendar": ["cal-1"] }), "calendar"),
    ];
    for (name, input, field) in bad {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(
            err.message().contains(field),
            "{name}: the message names the field: {}",
            err.message()
        );
    }
    // A response that is not one of the three is refused, never guessed at.
    for input in [
        json!({ "event": "evt-1", "response": "maybe" }),
        json!({ "event": "evt-1", "response": "cancel" }),
        json!({ "event": "evt-1", "response": "tentativelyAccept/../cancel" }),
    ] {
        let err = invoke(&socket, &key, "events.respond", input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_field_the_operation_does_not_know_is_refused_and_named() {
    // A key that is not known would be dropped without a word, and what it
    // said with it: a subject, a zone, the people in copy, "keep no copy".
    // For mail and invitations that is the difference between what was meant
    // and what was sent.
    let (server, socket, key) = answering(202, json!(null)).await;
    let times = (at("2026-10-12T16:00:00"), at("2026-10-12T17:00:00"));
    let grace = to("grace@contoso.example");
    for (name, input, field) in [
        // Another way of writing a known field.
        (
            "mail.reply",
            json!({ "message": "msg-1", "Comment": "hello" }),
            "Comment",
        ),
        (
            "mail.reply",
            json!({ "message": "msg-1", "comment": "hi", "cc": grace.clone() }),
            "cc",
        ),
        (
            "mail.send",
            json!({ "subject": "x", "toRecipients": grace.clone(), "save_to_sent_items": false }),
            "save_to_sent_items",
        ),
        (
            "mail.send",
            json!({ "Subject": "x", "body": { "content": "y" }, "toRecipients": grace.clone() }),
            "Subject",
        ),
        // Graph's own shape for sending, which is not this operation's.
        (
            "mail.send",
            json!({ "message": { "subject": "x", "toRecipients": grace.clone() }, "saveToSentItems": false }),
            "message",
        ),
        (
            "events.list_between",
            json!({ "start": "2026-10-12", "end": "2026-10-19", "calendarId": "cal-1" }),
            "calendarId",
        ),
        ("mail.list", json!({ "top": 5 }), "top"),
        ("calendars.list", json!({ "$top": 5 }), "$top"),
        ("events.get", json!({ "event": "evt-1", "select": "subject" }), "select"),
        // The same below the top, where it is said in which field.
        (
            "mail.send",
            json!({ "subject": "x", "toRecipients": grace.clone(), "body": { "content": "<b>x</b>", "content_type": "html" } }),
            "body.content_type",
        ),
        (
            "mail.create_draft",
            json!({ "toRecipients": [{ "emailAddress": { "address": "grace@contoso.example" } }, { "emailAddres": {} }] }),
            "toRecipients[1].emailAddres",
        ),
        (
            "events.create",
            json!({ "start": times.0.clone(), "end": times.1.clone(), "attendees": [{ "emailAddress": { "address": "a@contoso.example", "displayName": "A" } }] }),
            "attendees[0].emailAddress.displayName",
        ),
        (
            "events.update",
            json!({ "event": "evt-1", "start": { "dateTime": "2026-10-12T16:00:00", "timeZone": "UTC", "offset": "-08:00" } }),
            "start.offset",
        ),
        (
            "events.find_meeting_times",
            json!({ "timeConstraint": { "timeslots": [{ "start": times.0.clone(), "end": times.1.clone() }] } }),
            "timeConstraint.timeslots",
        ),
        (
            "mail.update",
            json!({ "message": "msg-1", "flag": { "flag_status": "flagged" } }),
            "flag.flag_status",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert!(
            err.message().contains(&format!("`{field}`")),
            "{name}: the field is named: {}",
            err.message()
        );
    }

    // The name of a field is the caller's own text. One that does not look
    // like a name is not repeated, and neither is any value.
    for input in [
        json!({ "event": "evt-1", "my password is hunter2": true }),
        json!({ "event": "evt-1", "x": "hunter2", "a-very-long-key-that-goes-on-and-on-and-on-well-past-forty-characters": 1 }),
    ] {
        let err = invoke(&socket, &key, "events.get", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(!err.message().contains("hunter2"), "{}", err.message());
        assert!(!err.message().contains("past-forty"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // What the schema says is what is enforced.
    for operation in Microsoft::new().operations() {
        if operation.name.ends_with("identity.get") || operation.name.ends_with("resource.resolve") {
            continue;
        }
        assert_eq!(
            operation.input_schema["additionalProperties"], false,
            "{}",
            operation.name
        );
    }
}

#[tokio::test]
async fn values_that_cannot_work_are_refused_before_graph_is_called() {
    let (server, socket, key) = answering(200, event()).await;
    let times = (at("2026-10-12T16:00:00"), at("2026-10-12T17:00:00"));
    let bad = [
        ("events.get", json!({ "event": "" })),
        ("events.get", json!({ "event": "  " })),
        ("events.delete", json!({ "event": "" })),
        ("events.cancel", json!({ "event": " " })),
        ("calendars.get", json!({ "calendar": "" })),
        ("calendars.list", json!({ "limit": 0 })),
        ("events.list_between", json!({ "start": "", "end": "2026-10-19" })),
        ("events.list_between", json!({ "start": "2026-10-12", "end": " " })),
        (
            "events.list_between",
            json!({ "start": "2026-10-12", "end": "2026-10-19", "calendar": "" }),
        ),
        ("events.instances", json!({ "event": "evt-1", "start": "", "end": "" })),
        // A time with nothing in it.
        (
            "events.create",
            json!({ "start": { "dateTime": "" }, "end": times.1.clone() }),
        ),
        ("events.create", json!({ "start": times.0.clone(), "end": {} })),
        // An attendee with no address, and a body with no content, which would blank the event's text.
        (
            "events.create",
            json!({ "start": times.0.clone(), "end": times.1.clone(), "attendees": [{}] }),
        ),
        (
            "events.update",
            json!({ "event": "evt-1", "attendees": [{ "type": "required", "emailAddress": { "name": "Grace" } }] }),
        ),
        (
            "events.update",
            json!({ "event": "evt-1", "attendees": [{ "emailAddress": { "address": " " } }] }),
        ),
        (
            "events.find_meeting_times",
            json!({ "attendees": [{ "emailAddress": {} }] }),
        ),
        ("events.update", json!({ "event": "evt-1", "body": {} })),
        (
            "events.update",
            json!({ "event": "evt-1", "body": { "contentType": "html" } }),
        ),
        (
            "events.create",
            json!({ "start": times.0.clone(), "end": times.1.clone(), "body": { "contnet": "Agenda" } }),
        ),
        // An update that changes nothing.
        ("events.update", json!({ "event": "evt-1" })),
        ("events.update", json!({ "event": "evt-1", "subject": null })),
        (
            "events.update",
            json!({ "event": "evt-1", "start": { "dateTime": " " } }),
        ),
        // Graph takes a proposed time only with a decline or a tentative acceptance.
        (
            "events.respond",
            json!({ "event": "evt-1", "response": "accept", "proposedNewTime": { "start": times.0.clone(), "end": times.1.clone() } }),
        ),
        (
            "events.schedule",
            json!({ "schedules": [], "startTime": times.0.clone(), "endTime": times.1.clone() }),
        ),
        (
            "events.schedule",
            json!({ "schedules": ["grace@contoso.example", " "], "startTime": times.0.clone(), "endTime": times.1.clone() }),
        ),
        (
            "events.schedule",
            json!({ "schedules": ["grace@contoso.example"], "startTime": {}, "endTime": times.1.clone() }),
        ),
        (
            "events.schedule",
            json!({ "schedules": ["grace@contoso.example"], "startTime": times.0.clone(), "endTime": times.1.clone(), "availabilityViewInterval": 4 }),
        ),
        (
            "events.schedule",
            json!({ "schedules": ["grace@contoso.example"], "startTime": times.0.clone(), "endTime": times.1.clone(), "availabilityViewInterval": 1441 }),
        ),
    ];
    for (name, input) in bad {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Graph's answers ──────────────────────────────────────────────────────────

#[tokio::test]
async fn graphs_refusals_reach_the_caller_as_errors_they_can_act_on() {
    for (response, kind, reason) in [
        (
            graph_error(
                404,
                "ErrorItemNotFound",
                "The specified object was not found in the store.",
            ),
            ErrorKind::NotFound,
            "has no such resource",
        ),
        (
            graph_error(
                403,
                "ErrorAccessDenied",
                "Access is denied. Check credentials and try again.",
            ),
            ErrorKind::AccessDenied,
            "Access is denied. Check credentials and try again.",
        ),
        // An attendee cannot cancel a meeting; Graph says so in a 400.
        (
            graph_error(
                400,
                "ErrorAccessDenied",
                "Your request can't be completed. You need to be an organizer to cancel a meeting.",
            ),
            ErrorKind::InvalidInput,
            "You need to be an organizer to cancel a meeting.",
        ),
        (
            graph_error(401, "InvalidAuthenticationToken", "Access token has expired."),
            ErrorKind::ReconnectRequired,
            "rejected the stored authorization",
        ),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let err = invoke(&socket, &key, "events.cancel", json!({ "event": "evt-1" }))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().ends_with(reason), "{}", err.message());
    }
}

#[tokio::test]
async fn a_write_is_sent_once_when_graph_fails_and_a_read_is_tried_again() {
    let unavailable = || graph_error(503, "serviceNotAvailable", "The service is temporarily unavailable.");
    let times = json!({ "start": at("2026-10-12T16:00:00"), "end": at("2026-10-12T17:00:00") });
    for (name, input) in [
        ("events.create", times.clone()),
        ("events.update", json!({ "event": "evt-1", "subject": "x" })),
        ("events.respond", json!({ "event": "evt-1", "response": "accept" })),
        ("events.cancel", json!({ "event": "evt-1" })),
        // Reads by effect, but Graph takes them as POST, so they are not repeated either.
        ("events.find_meeting_times", json!({})),
        // `events.delete` is not in this list. On this branch the transport
        // still repeats a DELETE after a server error; see the guide.
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any()).respond_with(unavailable()).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have happened, so it is not sent again"
        );
    }

    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(unavailable())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(answer(200, &event()))
        .mount(&server)
        .await;
    let read = invoke(&socket, &key, "events.get", json!({ "event": "evt-1" }))
        .await
        .unwrap();
    assert_eq!(read["id"], "evt-1");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let microsoft = Microsoft::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(microsoft.clone()), "eyJ.good").await;
    let connection = socket.connection(key).await.unwrap();
    Mock::given(method("POST"))
        .and(path("/v1.0/me/events"))
        .respond_with(answer(201, &event()))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path("/v1.0/me/events/evt-1"))
        .respond_with(answer(200, &event()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/v1.0/me/events/evt-1/accept"))
        .respond_with(ResponseTemplate::new(202))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1.0/me/calendarView"))
        .respond_with(answer(200, &json!({ "value": [event()] })))
        .mount(&server)
        .await;

    let events = microsoft.events(&connection);
    let draft = CreateEvent {
        subject: Some("Design review".into()),
        attendees: Some(vec![
            Attendee::required("grace@contoso.example"),
            Attendee::optional("alan@contoso.example"),
        ]),
        is_online_meeting: Some(true),
        ..CreateEvent::between(
            DateTimeTimeZone::utc("2026-10-12T16:00:00"),
            DateTimeTimeZone::utc("2026-10-12T17:00:00"),
        )
    };
    let created = events.create(None, draft).await.unwrap();
    assert_eq!(created.id, "evt-1");
    assert_eq!(
        created.online_meeting.and_then(|meeting| meeting.join_url).as_deref(),
        Some(JOIN_URL)
    );
    let start = created.start.unwrap();
    assert_eq!(
        (start.date_time.as_str(), start.time_zone.as_str()),
        ("2026-10-12T16:00:00.0000000", "UTC")
    );
    assert_eq!(
        created.original_start_time_zone.as_deref(),
        Some("Pacific Standard Time")
    );
    assert_eq!(created.attendees[0].email_address.address, "grace@contoso.example");
    assert_eq!(
        created.attendees[0].status.as_ref().and_then(|s| s.response.as_deref()),
        Some("accepted")
    );

    let changes = UpdateEvent {
        subject: Some("Design review, part two".into()),
        ..UpdateEvent::default()
    };
    assert_eq!(events.update("evt-1", changes).await.unwrap().id, "evt-1");
    events
        .respond("evt-1", EventResponse::Accept, RespondToEvent::default())
        .await
        .unwrap();
    let week = events
        .list_between("2026-10-12T00:00:00Z", "2026-10-19T00:00:00Z", None, Paging::default())
        .await
        .unwrap();
    assert_eq!(week.items.len(), 1);
    assert_eq!(week.next_cursor, None);

    let sent: Vec<Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method.as_str() == "POST" && request.url.path() == "/v1.0/me/events")
        .map(body_of)
        .collect();
    assert_eq!(
        sent,
        [json!({
            "subject": "Design review",
            "start": at("2026-10-12T16:00:00"),
            "end": at("2026-10-12T17:00:00"),
            "attendees": [
                { "type": "required", "emailAddress": { "address": "grace@contoso.example" } },
                { "type": "optional", "emailAddress": { "address": "alan@contoso.example" } }
            ],
            "isOnlineMeeting": true,
            "onlineMeetingProvider": "teamsForBusiness"
        })]
    );
}
