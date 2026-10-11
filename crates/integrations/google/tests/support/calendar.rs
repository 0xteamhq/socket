//! What Google answers for Calendar, as the tests need it: fixtures and constants.

use serde_json::{Value, json};

/// Where the events of the signed-in person's own calendar are, and one of them.
pub const CALENDAR_EVENTS_PATH: &str = "/calendar/v3/calendars/primary/events";
pub const CALENDAR_EVENT_PATH: &str = "/calendar/v3/calendars/primary/events/evt1";
pub const CALENDAR_LIST_PATH: &str = "/calendar/v3/users/me/calendarList";
pub const FREEBUSY_PATH: &str = "/calendar/v3/freeBusy";

/// Nine and ten in the morning in California, as Google writes them.
pub const AT_NINE: &str = "2026-10-12T09:00:00-07:00";
pub const AT_TEN: &str = "2026-10-12T10:00:00-07:00";

/// The Meet link of [`calendar_event`].
pub const MEET_LINK: &str = "https://meet.google.com/abc-defg-hij";

/// Ada's own calendar, as it is on her calendar list.
pub fn calendar_entry() -> Value {
    json!({ "kind": "calendar#calendarListEntry", "etag": "\"1700000000000000\"", "id": "ada@example.test", "summary": "Ada Lovelace",
        "timeZone": "America/Los_Angeles", "accessRole": "owner", "primary": true, "selected": true,
        "colorId": "14", "backgroundColor": "#9fe1e7", "foregroundColor": "#000000" })
}

/// A meeting Ada organises, with a Meet link and a recording attached.
pub fn calendar_event() -> Value {
    json!({
        "kind": "calendar#event", "etag": "\"3181161784712000\"", "id": "evt1", "status": "confirmed",
        "htmlLink": "https://www.google.com/calendar/event?eid=ZXZ0MQ",
        "created": "2026-10-01T08:00:00.000Z", "updated": "2026-10-02T08:00:00.000Z",
        "summary": "Design review", "description": "Walk through the plan.", "location": "Room 4",
        "start": { "dateTime": AT_NINE, "timeZone": "America/Los_Angeles" },
        "end": { "dateTime": AT_TEN, "timeZone": "America/Los_Angeles" },
        "creator": { "email": "ada@example.test", "self": true },
        "organizer": { "email": "ada@example.test", "displayName": "Ada Lovelace", "self": true },
        "attendees": [
            { "email": "ada@example.test", "organizer": true, "self": true, "responseStatus": "accepted" },
            { "email": "grace@example.test", "displayName": "Grace Hopper", "responseStatus": "needsAction", "optional": true }
        ],
        "hangoutLink": MEET_LINK,
        "conferenceData": {
            "conferenceId": "abc-defg-hij",
            "conferenceSolution": { "key": { "type": "hangoutsMeet" }, "name": "Google Meet", "iconUri": "https://fonts.gstatic.com/meet.png" },
            "entryPoints": [
                { "entryPointType": "video", "uri": MEET_LINK, "label": "meet.google.com/abc-defg-hij" },
                { "entryPointType": "phone", "uri": "tel:+1-555-0100", "pin": "123456789" }
            ]
        },
        "attachments": [{ "fileUrl": "https://drive.google.com/open?id=1AbC", "title": "Design review - Recording", "mimeType": "video/mp4", "fileId": "1AbC" }],
        "iCalUID": "evt1@google.com", "eventType": "default"
    })
}

/// The same meeting as Grace sees it: she is invited and has not answered.
/// Her entry carries a field these types do not describe.
pub fn calendar_invitation() -> Value {
    json!({
        "kind": "calendar#event", "etag": "\"111\"", "id": "evt1", "status": "confirmed", "summary": "Design review",
        "start": { "dateTime": AT_NINE }, "end": { "dateTime": AT_TEN },
        "organizer": { "email": "ada@example.test" },
        "attendees": [
            { "email": "ada@example.test", "organizer": true, "responseStatus": "accepted" },
            { "email": "grace@example.test", "self": true, "responseStatus": "needsAction", "additionalGuests": 1, "futureField": "kept" }
        ]
    })
}

/// A large meeting whose organiser hid the guest list: Grace is shown only
/// her own entry.
pub fn calendar_hidden_invitation() -> Value {
    json!({
        "kind": "calendar#event", "etag": "\"222\"", "id": "evt1", "status": "confirmed", "summary": "All hands",
        "start": { "dateTime": AT_NINE }, "end": { "dateTime": AT_TEN },
        "organizer": { "email": "ada@example.test" },
        "guestsCanSeeOtherGuests": false,
        "attendees": [{ "email": "grace@example.test", "self": true, "responseStatus": "needsAction" }]
    })
}

/// A page of events.
pub fn calendar_events_page(items: Value) -> Value {
    json!({ "kind": "calendar#events", "etag": "\"p32c9b6vtqmbes0g\"", "summary": "Ada Lovelace", "updated": "2026-10-02T08:00:00.000Z",
        "timeZone": "America/Los_Angeles", "accessRole": "owner", "items": items })
}

/// When Ada is busy, and a calendar Google could not answer for.
pub fn freebusy() -> Value {
    json!({ "kind": "calendar#freeBusy", "timeMin": "2026-10-12T00:00:00.000Z", "timeMax": "2026-10-13T00:00:00.000Z", "calendars": {
        "primary": { "busy": [{ "start": "2026-10-12T18:00:00+02:00", "end": "2026-10-12T19:00:00+02:00" }] },
        "grace@example.test": { "errors": [{ "domain": "global", "reason": "notFound" }], "busy": [] } } })
}
