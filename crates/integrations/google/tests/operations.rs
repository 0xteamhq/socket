//! Every Google Calendar operation, called by name against a local server that answers as Google does.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{AuthScheme, ConnectionKey, Effect, ErrorKind, Integration, Retry, Socket};
use socketkit_google::models::{EventTime, FreeBusyQuery, InsertEvent, ListEvents, PatchEvent, Respond};
use socketkit_google::{CALENDAR_EVENTS_SCOPE, CALENDAR_READONLY_SCOPE, Google, provider};
use socketkit_testkit::wiremock::matchers::{method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// One operation's expected behaviour.
struct Case {
    name: &'static str,
    input: Value,
    verb: &'static str,
    /// The path Google is called at.
    at: &'static str,
    /// Exactly the query parameters that reach Google.
    query: Value,
    /// Exactly the JSON body that reaches Google; `null` when there is none.
    body: Value,
    status: u16,
    response: Value,
    /// What the operation returns. Checked as a subset, so models may carry more fields.
    returns: Value,
    /// What a GET of the same path answers first, for an operation that reads before it writes.
    reads_first: Option<Value>,
}

const EVENTS: &str = "/calendar/v3/calendars/primary/events";
const EVENT: &str = "/calendar/v3/calendars/primary/events/evt1";
const NINE: &str = "2026-10-12T09:00:00-07:00";
const TEN: &str = "2026-10-12T10:00:00-07:00";

fn calendar() -> Value {
    json!({ "kind": "calendar#calendarListEntry", "id": "ada@example.test", "summary": "Ada Lovelace", "timeZone": "America/Los_Angeles",
        "accessRole": "owner", "primary": true, "selected": true, "backgroundColor": "#0088aa" })
}

/// A meeting Ada organises, with a Meet link and a recording attached.
fn event() -> Value {
    json!({
        "kind": "calendar#event", "etag": "\"3181161784712000\"", "id": "evt1", "status": "confirmed",
        "htmlLink": "https://www.google.com/calendar/event?eid=ZXZ0MQ",
        "summary": "Design review", "description": "Walk through the plan.", "location": "Room 4",
        "start": { "dateTime": NINE, "timeZone": "America/Los_Angeles" },
        "end": { "dateTime": TEN, "timeZone": "America/Los_Angeles" },
        "creator": { "email": "ada@example.test", "self": true },
        "organizer": { "email": "ada@example.test", "displayName": "Ada Lovelace", "self": true },
        "attendees": [
            { "email": "ada@example.test", "organizer": true, "self": true, "responseStatus": "accepted" },
            { "email": "grace@example.test", "displayName": "Grace Hopper", "responseStatus": "needsAction", "optional": true }
        ],
        "hangoutLink": "https://meet.google.com/abc-defg-hij",
        "conferenceData": {
            "conferenceId": "abc-defg-hij",
            "conferenceSolution": { "key": { "type": "hangoutsMeet" }, "name": "Google Meet", "iconUri": "https://fonts.gstatic.com/meet.png" },
            "entryPoints": [
                { "entryPointType": "video", "uri": "https://meet.google.com/abc-defg-hij", "label": "meet.google.com/abc-defg-hij" },
                { "entryPointType": "phone", "uri": "tel:+1-555-0100", "pin": "123456789" }
            ]
        },
        "attachments": [{ "fileUrl": "https://drive.google.com/open?id=1AbC", "title": "Design review - Recording", "mimeType": "video/mp4", "fileId": "1AbC" }],
        "iCalUID": "evt1@google.com", "eventType": "default"
    })
}

/// The same meeting as Grace sees it: she is invited and has not answered.
fn invitation() -> Value {
    json!({
        "kind": "calendar#event", "etag": "\"111\"", "id": "evt1", "status": "confirmed", "summary": "Design review",
        "start": { "dateTime": NINE }, "end": { "dateTime": TEN },
        "organizer": { "email": "ada@example.test" },
        "attendees": [
            { "email": "ada@example.test", "organizer": true, "responseStatus": "accepted" },
            { "email": "grace@example.test", "self": true, "responseStatus": "needsAction", "additionalGuests": 1, "futureField": "kept" }
        ]
    })
}

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, at, query, body, status, response, returns| Case { name, input, verb, at, query, body, status, response, returns, reads_first: None };
    let one = json!({ "calendarId": "primary", "eventId": "evt1" });
    let mut answered = invitation();
    answered["attendees"][1]["responseStatus"] = json!("accepted");
    answered["attendees"][1]["comment"] = json!("See you there");
    vec![
        // calendar list
        case("calendar_list.list", json!({ "minAccessRole": "writer", "showHidden": true, "maxResults": 50, "pageToken": "page-1" }), "GET", "/calendar/v3/users/me/calendarList",
            json!({ "minAccessRole": "writer", "showHidden": "true", "maxResults": "50", "pageToken": "page-1" }), json!(null), 200,
            json!({ "kind": "calendar#calendarList", "items": [calendar()], "nextPageToken": "page-2" }),
            json!({ "items": [{ "id": "ada@example.test", "summary": "Ada Lovelace", "accessRole": "owner", "primary": true }], "next_cursor": "page-2" })),
        case("calendar_list.get", json!({ "calendarId": "primary" }), "GET", "/calendar/v3/users/me/calendarList/primary", json!({}), json!(null), 200, calendar(),
            json!({ "id": "ada@example.test", "timeZone": "America/Los_Angeles", "primary": true, "hidden": false })),

        // events
        case("calendar_events.list", json!({ "calendarId": "primary", "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-19T00:00:00Z", "q": "design", "singleEvents": true, "orderBy": "startTime", "maxResults": 10 }), "GET", EVENTS,
            json!({ "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-19T00:00:00Z", "q": "design", "singleEvents": "true", "orderBy": "startTime", "maxResults": "10" }), json!(null), 200,
            json!({ "kind": "calendar#events", "summary": "Ada Lovelace", "timeZone": "America/Los_Angeles", "items": [event()], "nextPageToken": "page-2" }),
            json!({ "items": [{ "id": "evt1", "summary": "Design review", "start": { "dateTime": NINE }, "hangoutLink": "https://meet.google.com/abc-defg-hij" }], "next_cursor": "page-2" })),
        case("calendar_events.get", one.clone(), "GET", EVENT, json!({}), json!(null), 200, event(),
            json!({ "id": "evt1", "organizer": { "email": "ada@example.test", "self": true }, "attendees": [{ "email": "ada@example.test", "responseStatus": "accepted" }, { "email": "grace@example.test", "optional": true }] })),
        case("calendar_events.instances", json!({ "calendarId": "primary", "eventId": "evt1", "timeMin": "2026-10-01T00:00:00Z", "maxResults": 5 }), "GET", "/calendar/v3/calendars/primary/events/evt1/instances",
            json!({ "timeMin": "2026-10-01T00:00:00Z", "maxResults": "5" }), json!(null), 200,
            json!({ "kind": "calendar#events", "items": [{ "id": "evt1_20261012T160000Z", "recurringEventId": "evt1", "originalStartTime": { "dateTime": NINE }, "start": { "dateTime": NINE }, "end": { "dateTime": TEN } }] }),
            json!({ "items": [{ "id": "evt1_20261012T160000Z", "recurringEventId": "evt1", "originalStartTime": { "dateTime": NINE } }], "next_cursor": null })),
        case("calendar_events.insert", json!({ "calendarId": "primary", "summary": "Design review", "location": "Room 4", "start": { "dateTime": NINE }, "end": { "dateTime": TEN }, "attendees": [{ "email": "grace@example.test", "optional": true }], "sendUpdates": "all" }), "POST", EVENTS,
            json!({ "sendUpdates": "all" }),
            json!({ "summary": "Design review", "location": "Room 4", "start": { "dateTime": NINE }, "end": { "dateTime": TEN }, "attendees": [{ "email": "grace@example.test", "optional": true }] }), 200, event(),
            json!({ "id": "evt1", "htmlLink": "https://www.google.com/calendar/event?eid=ZXZ0MQ" })),
        case("calendar_events.patch", json!({ "calendarId": "primary", "eventId": "evt1", "summary": "Design review (moved)", "location": "Room 5" }), "PATCH", EVENT,
            json!({}), json!({ "summary": "Design review (moved)", "location": "Room 5" }), 200, event(), json!({ "id": "evt1" })),
        Case { reads_first: Some(invitation()), ..case("calendar_events.respond", json!({ "calendarId": "primary", "eventId": "evt1", "responseStatus": "accepted", "comment": "See you there", "sendUpdates": "all" }), "PATCH", EVENT,
            json!({ "sendUpdates": "all" }), json!({ "attendees": answered["attendees"] }), 200, answered.clone(),
            json!({ "id": "evt1", "attendees": [{ "email": "ada@example.test" }, { "email": "grace@example.test", "self": true, "responseStatus": "accepted", "comment": "See you there" }] })) },
        case("calendar_events.delete", json!({ "calendarId": "primary", "eventId": "evt1", "sendUpdates": "all" }), "DELETE", EVENT, json!({ "sendUpdates": "all" }), json!(null), 204, json!(null), json!(null)),

        // free/busy
        case("calendar_freebusy.query", json!({ "calendarIds": ["primary", "grace@example.test"], "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z", "timeZone": "Europe/Zurich" }), "POST", "/calendar/v3/freeBusy",
            json!({}), json!({ "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z", "timeZone": "Europe/Zurich", "items": [{ "id": "primary" }, { "id": "grace@example.test" }] }), 200,
            json!({ "kind": "calendar#freeBusy", "timeMin": "2026-10-12T00:00:00.000Z", "timeMax": "2026-10-13T00:00:00.000Z", "calendars": {
                "primary": { "busy": [{ "start": "2026-10-12T18:00:00+02:00", "end": "2026-10-12T19:00:00+02:00" }] },
                "grace@example.test": { "errors": [{ "domain": "global", "reason": "notFound" }], "busy": [] } } }),
            json!({ "calendars": { "primary": { "busy": [{ "start": "2026-10-12T18:00:00+02:00", "end": "2026-10-12T19:00:00+02:00" }], "errors": [] },
                "grace@example.test": { "busy": [], "errors": [{ "domain": "global", "reason": "notFound" }] } } })),
    ]
}

/// True when `actual` has everything `expected` has. Arrays must match in length.
fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => e.iter().all(|(k, v)| a.get(k).is_some_and(|av| contains(av, v))),
        (Value::Array(a), Value::Array(e)) => a.len() == e.len() && a.iter().zip(e).all(|(av, ev)| contains(av, ev)),
        _ => actual == expected,
    }
}

async fn google() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Google::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, "ya29.good").await;
    (server, socket, key)
}

async fn answer(server: &MockServer, status: u16, body: Value) {
    let template = if body.is_null() {
        ResponseTemplate::new(status)
    } else {
        ResponseTemplate::new(status).set_body_json(body)
    };
    Mock::given(socketkit_testkit::wiremock::matchers::any())
        .respond_with(template)
        .mount(server)
        .await;
}

async fn answer_to(server: &MockServer, verb: &str, at: &str, status: u16, body: Value) {
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(server)
        .await;
}

fn query_of(request: &Request) -> Value {
    Value::Object(
        request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
            .collect(),
    )
}

fn body_of(request: &Request) -> Value {
    if request.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&request.body).unwrap()
    }
}

/// What Google says when it refuses, in the shape its documentation gives.
fn refusal(code: u16, reason: &str, message: &str) -> Value {
    json!({ "error": { "code": code, "message": message, "errors": [{ "domain": "global", "reason": reason, "message": message }] } })
}

#[tokio::test]
async fn the_table_below_covers_every_operation_google_offers() {
    let listed: Vec<String> = Google::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = cases().iter().map(|c| format!("google.{}", c.name)).collect();
    tested.extend(["google.identity.get".to_owned(), "google.resource.resolve".to_owned()]);
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist"
    );
    assert_eq!(listed.len(), 12);
}

#[tokio::test]
async fn every_operation_calls_the_right_endpoint_and_returns_what_google_sent() {
    for case in cases() {
        let (server, socket, key) = google().await;
        if let Some(current) = &case.reads_first {
            answer_to(&server, "GET", case.at, 200, current.clone()).await;
        }
        let template = if case.response.is_null() {
            ResponseTemplate::new(case.status)
        } else {
            ResponseTemplate::new(case.status).set_body_json(case.response.clone())
        };
        Mock::given(method(case.verb))
            .and(path(case.at))
            .respond_with(template)
            .mount(&server)
            .await;

        let output = socket
            .invoke(key, format!("google.{}", case.name), case.input.clone())
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(
            contains(&output, &case.returns),
            "{}: returned {output}, expected {}",
            case.name,
            case.returns
        );

        let received = server.received_requests().await.unwrap();
        let calls = if case.reads_first.is_some() { 2 } else { 1 };
        assert_eq!(received.len(), calls, "{}: {calls} call(s) to Google", case.name);
        for request in &received {
            assert_eq!(
                request.headers.get("authorization").unwrap(),
                "Bearer ya29.good",
                "{}",
                case.name
            );
        }
        let request = received.last().unwrap();
        assert_eq!(request.method.as_str(), case.verb, "{}", case.name);
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
    }

    // What a case sends as GET is a read, and a change is never a read. The
    // one read that is not a GET is the free/busy query: Google offers it
    // only as a POST, and it changes nothing.
    for case in cases() {
        let effect = find(&format!("google.{}", case.name)).effect;
        if case.name == "calendar_freebusy.query" {
            assert_eq!((effect, case.verb), (Effect::Read, "POST"));
        } else {
            assert_eq!(
                effect == Effect::Read,
                case.verb == "GET",
                "{}: a read is sent as GET, a change is not",
                case.name
            );
        }
    }
    // A host lets a read run freely and asks a person before anything else,
    // so each effect is stated here and not derived.
    let effects = [
        ("calendar_list.list", Effect::Read),
        ("calendar_list.get", Effect::Read),
        ("calendar_events.list", Effect::Read),
        ("calendar_events.get", Effect::Read),
        ("calendar_events.instances", Effect::Read),
        ("calendar_freebusy.query", Effect::Read),
        ("calendar_events.insert", Effect::Write),
        ("calendar_events.patch", Effect::Write),
        ("calendar_events.respond", Effect::Write),
        ("calendar_events.delete", Effect::Destructive),
    ];
    assert_eq!(effects.len(), cases().len());
    for (name, effect) in effects {
        let operation = find(&format!("google.{name}"));
        assert_eq!(operation.effect, effect, "{name}");
        let scope = if effect == Effect::Read {
            CALENDAR_READONLY_SCOPE
        } else {
            CALENDAR_EVENTS_SCOPE
        };
        assert_eq!(operation.required_scopes, [scope], "{name}");
    }

    let insert = find("google.calendar_events.insert");
    let mut required: Vec<&str> = insert.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["calendarId", "end", "start"]);
    for field in [
        "calendarId",
        "summary",
        "description",
        "location",
        "start",
        "end",
        "attendees",
        "createMeetLink",
        "sendUpdates",
    ] {
        assert!(
            insert.input_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    // What leads from a meeting to its recording, transcript and notes is part of what comes back.
    for field in [
        "id",
        "summary",
        "description",
        "start",
        "end",
        "organizer",
        "attendees",
        "location",
        "hangoutLink",
        "conferenceData",
        "attachments",
    ] {
        assert!(
            insert.output_schema["properties"].get(field).is_some(),
            "{field} is returned"
        );
    }
}

#[tokio::test]
async fn the_calendar_scopes_are_asked_for_only_when_the_application_names_them() {
    let AuthScheme::OAuth2(oauth) = provider().auth else {
        panic!("google uses OAuth")
    };
    assert!(
        !oauth.default_scopes.iter().any(|scope| scope.contains("calendar")),
        "{:?}",
        oauth.default_scopes
    );
    assert_eq!(
        CALENDAR_READONLY_SCOPE,
        "https://www.googleapis.com/auth/calendar.readonly"
    );
    assert_eq!(CALENDAR_EVENTS_SCOPE, "https://www.googleapis.com/auth/calendar.events");
}

#[tokio::test]
async fn an_event_keeps_what_leads_to_its_recording_transcript_and_notes() {
    let (server, socket, key) = google().await;
    answer_to(&server, "GET", EVENT, 200, event()).await;
    let connection = socket.connection(key).await.unwrap();
    let found = Google::new()
        .calendar_events(&connection)
        .get("primary", "evt1")
        .await
        .unwrap();

    assert_eq!(
        found.hangout_link.as_deref(),
        Some("https://meet.google.com/abc-defg-hij")
    );
    let conference = found.conference_data.expect("the conference is kept");
    assert_eq!(conference.conference_id.as_deref(), Some("abc-defg-hij"));
    let solution = conference.conference_solution.expect("which product hosts it");
    assert_eq!(solution.key.unwrap().kind, "hangoutsMeet");
    assert_eq!(conference.entry_points.len(), 2);
    assert_eq!(conference.entry_points[0].entry_point_type, "video");
    assert_eq!(
        conference.entry_points[0].uri.as_deref(),
        Some("https://meet.google.com/abc-defg-hij")
    );
    assert_eq!(conference.entry_points[1].pin.as_deref(), Some("123456789"));
    assert_eq!(found.attachments.len(), 1);
    assert_eq!(found.attachments[0].file_id.as_deref(), Some("1AbC"));
    assert_eq!(found.attachments[0].mime_type.as_deref(), Some("video/mp4"));
    assert_eq!(found.attachments[0].file_url, "https://drive.google.com/open?id=1AbC");

    assert_eq!(found.organizer.unwrap().email.as_deref(), Some("ada@example.test"));
    assert!(found.attendees[0].is_self && found.attendees[0].organizer);
    assert_eq!(found.attendees[1].response_status, "needsAction");
    assert!(found.attendees[1].optional);
    let start = found.start.unwrap();
    assert_eq!(start.date_time.as_deref(), Some(NINE));
    assert_eq!(start.date, None, "a timed event has no all-day date");
    assert_eq!(found.ical_uid.as_deref(), Some("evt1@google.com"));
}

#[tokio::test]
async fn an_all_day_event_and_a_cancelled_instance_read_too() {
    let (server, socket, key) = google().await;
    answer_to(
        &server,
        "GET",
        EVENTS,
        200,
        json!({ "kind": "calendar#events", "items": [
            { "id": "holiday", "status": "confirmed", "summary": "Offsite", "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-14" } },
            // An instance removed from a series carries almost nothing.
            { "id": "evt1_20261019T160000Z", "status": "cancelled", "recurringEventId": "evt1", "originalStartTime": { "dateTime": "2026-10-19T09:00:00-07:00" } }
        ] }),
    )
    .await;
    let connection = socket.connection(key).await.unwrap();
    let events = Google::new()
        .calendar_events(&connection)
        .list("primary", ListEvents::default())
        .await
        .unwrap();
    assert_eq!(events.next_cursor, None);
    let offsite = &events.items[0];
    assert_eq!(offsite.start.as_ref().unwrap().date.as_deref(), Some("2026-10-12"));
    assert_eq!(offsite.start.as_ref().unwrap().date_time, None);
    assert_eq!(
        offsite.end.as_ref().unwrap().date.as_deref(),
        Some("2026-10-14"),
        "the end is exclusive"
    );
    assert!(offsite.attendees.is_empty() && offsite.attachments.is_empty());
    assert_eq!(offsite.conference_data, None);
    let cancelled = &events.items[1];
    assert_eq!(cancelled.status.as_deref(), Some("cancelled"));
    assert_eq!(cancelled.start, None);
    assert_eq!(cancelled.recurring_event_id.as_deref(), Some("evt1"));
}

#[tokio::test]
async fn a_list_with_nothing_in_it_is_empty_and_not_an_error() {
    // Google leaves `items` out of some empty lists.
    for body in [
        json!({ "kind": "calendar#events", "items": [] }),
        json!({ "kind": "calendar#events" }),
    ] {
        let (server, socket, key) = google().await;
        answer(&server, 200, body).await;
        let events = socket
            .invoke(
                key,
                "google.calendar_events.list".into(),
                json!({ "calendarId": "primary" }),
            )
            .await
            .unwrap();
        assert_eq!(events, json!({ "items": [], "next_cursor": null }));
    }
}

#[tokio::test]
async fn asking_for_a_meet_link_sends_a_fresh_create_request_each_time() {
    let (server, socket, key) = google().await;
    answer_to(&server, "POST", EVENTS, 200, event()).await;
    let input = json!({ "calendarId": "primary", "summary": "Design review", "start": { "dateTime": NINE }, "end": { "dateTime": TEN }, "createMeetLink": true });
    let mut request_ids = Vec::new();
    for _ in 0..2 {
        let created = socket
            .invoke(key.clone(), "google.calendar_events.insert".into(), input.clone())
            .await
            .unwrap();
        assert_eq!(created["hangoutLink"], "https://meet.google.com/abc-defg-hij");
        assert_eq!(
            created["conferenceData"]["entryPoints"][0]["uri"],
            "https://meet.google.com/abc-defg-hij"
        );
    }
    for request in server.received_requests().await.unwrap() {
        // Without this parameter Google ignores the conference in the body.
        assert_eq!(query_of(&request), json!({ "conferenceDataVersion": "1" }));
        let mut body = body_of(&request);
        let create = body["conferenceData"]["createRequest"].take();
        assert_eq!(create["conferenceSolutionKey"], json!({ "type": "hangoutsMeet" }));
        let id = create["requestId"].as_str().unwrap().to_owned();
        assert!(id.len() >= 16, "{id}");
        request_ids.push(id);
        assert_eq!(
            body,
            json!({ "summary": "Design review", "start": { "dateTime": NINE }, "end": { "dateTime": TEN }, "conferenceData": { "createRequest": null } }),
            "the flag itself is not a field Google knows"
        );
    }
    assert_ne!(
        request_ids[0], request_ids[1],
        "Google ignores a create request whose id it has seen"
    );
}

#[tokio::test]
async fn an_event_without_a_meet_link_sends_no_conference() {
    for flag in [json!(false), json!(null)] {
        let (server, socket, key) = google().await;
        answer_to(&server, "POST", EVENTS, 200, event()).await;
        let input = json!({ "calendarId": "primary", "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-13" }, "createMeetLink": flag });
        socket
            .invoke(key, "google.calendar_events.insert".into(), input)
            .await
            .unwrap();
        let received = server.received_requests().await.unwrap();
        assert_eq!(query_of(&received[0]), json!({}));
        assert_eq!(
            body_of(&received[0]),
            json!({ "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-13" } })
        );
    }
}

#[tokio::test]
async fn an_event_needs_a_start_and_an_end_that_are_each_a_time_or_a_date() {
    let (server, socket, key) = google().await;
    let at = json!({ "dateTime": NINE });
    let bad = [
        (json!({}), at.clone(), "start"),
        (at.clone(), json!({ "timeZone": "Europe/Zurich" }), "end"),
        (json!({ "date": "2026-10-12", "dateTime": NINE }), at.clone(), "start"),
        (at.clone(), json!({ "date": " " }), "end"),
        (json!({ "date": "", "dateTime": NINE }), at.clone(), "start"),
    ];
    for (start, end, names) in bad {
        let input = json!({ "calendarId": "primary", "start": start, "end": end });
        let err = socket
            .invoke(key.clone(), "google.calendar_events.insert".into(), input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{names}");
        assert!(err.message().contains(names), "{}", err.message());
    }
    // Leaving one out altogether is caught when the input is read.
    let err = socket
        .invoke(
            key,
            "google.calendar_events.insert".into(),
            json!({ "calendarId": "primary", "start": at }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().contains("end"), "{}", err.message());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_patch_sends_only_what_was_set_and_refuses_to_send_nothing() {
    let (server, socket, key) = google().await;
    answer_to(&server, "PATCH", EVENT, 200, event()).await;
    let patch = |changes: Value| {
        let mut input = json!({ "calendarId": "primary", "eventId": "evt1" });
        input
            .as_object_mut()
            .unwrap()
            .extend(changes.as_object().unwrap().clone());
        socket.invoke(key.clone(), "google.calendar_events.patch".into(), input)
    };

    for nothing in [
        json!({}),
        json!({ "sendUpdates": "all" }),
        json!({ "createMeetLink": false }),
    ] {
        let err = patch(nothing.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{nothing}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // Moving an all-day event to a time. Google merges a patch into what it
    // has, so the old `date` must be cleared or the event would have both.
    patch(json!({ "start": { "dateTime": NINE, "timeZone": "America/Los_Angeles" }, "end": { "dateTime": TEN } }))
        .await
        .unwrap();
    // And the other way round.
    patch(json!({ "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-13" } }))
        .await
        .unwrap();
    // Replacing the guests and adding a Meet link to a meeting that had none.
    patch(json!({ "attendees": [{ "email": "grace@example.test", "responseStatus": "accepted" }, { "email": "alan@example.test" }], "createMeetLink": true, "sendUpdates": "externalOnly" }))
        .await
        .unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(
        body_of(&received[0]),
        json!({ "start": { "dateTime": NINE, "timeZone": "America/Los_Angeles", "date": null }, "end": { "dateTime": TEN, "date": null } })
    );
    assert_eq!(
        body_of(&received[1]),
        json!({ "start": { "date": "2026-10-12", "dateTime": null }, "end": { "date": "2026-10-13", "dateTime": null } })
    );
    assert_eq!(
        query_of(&received[2]),
        json!({ "sendUpdates": "externalOnly", "conferenceDataVersion": "1" })
    );
    let mut third = body_of(&received[2]);
    assert_eq!(
        third["conferenceData"]["createRequest"]["conferenceSolutionKey"]["type"],
        "hangoutsMeet"
    );
    third.as_object_mut().unwrap().remove("conferenceData");
    assert_eq!(
        third,
        json!({ "attendees": [{ "email": "grace@example.test", "responseStatus": "accepted" }, { "email": "alan@example.test" }] }),
        "a guest who stays is sent with the answer they gave"
    );

    let err = patch(json!({ "start": { "timeZone": "Europe/Zurich" } }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn answering_an_invitation_changes_only_the_signed_in_attendee() {
    let (server, socket, key) = google().await;
    answer_to(&server, "GET", EVENT, 200, invitation()).await;
    answer_to(&server, "PATCH", EVENT, 200, invitation()).await;
    socket
        .invoke(
            key,
            "google.calendar_events.respond".into(),
            json!({ "calendarId": "primary", "eventId": "evt1", "responseStatus": "declined" }),
        )
        .await
        .unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(received[0].method.as_str(), "GET");
    let write = &received[1];
    assert_eq!(write.method.as_str(), "PATCH");
    assert_eq!(query_of(write), json!({}), "nobody is emailed unless asked");
    // Google replaces the whole list, so everyone else goes back exactly as
    // they came, including what these types do not describe.
    let mut expected = invitation()["attendees"].clone();
    expected[1]["responseStatus"] = json!("declined");
    assert_eq!(body_of(write), json!({ "attendees": expected }));
    assert_eq!(expected[1]["futureField"], "kept");
    // If the guest list changed between the read and the write, Google refuses
    // the write instead of silently dropping the newcomer.
    assert_eq!(write.headers.get("if-match").unwrap(), "\"111\"");
}

#[tokio::test]
async fn an_answer_that_cannot_be_given_safely_is_refused_before_anything_is_written() {
    let not_invited = {
        let mut event = invitation();
        event["attendees"][1]["self"] = json!(false);
        event
    };
    let no_guests = {
        let mut event = invitation();
        event.as_object_mut().unwrap().remove("attendees");
        event
    };
    let truncated = {
        let mut event = invitation();
        event["attendeesOmitted"] = json!(true);
        event
    };
    let unversioned = {
        let mut event = invitation();
        event.as_object_mut().unwrap().remove("etag");
        event
    };
    for (current, kind, says) in [
        (not_invited, ErrorKind::InvalidInput, "not invited"),
        (no_guests, ErrorKind::InvalidInput, "not invited"),
        // Sending back a list Google cut short would uninvite whoever it left out.
        (truncated, ErrorKind::Decode, "left guests out"),
        // A success that is not the event says nothing about who is invited.
        (json!({}), ErrorKind::Decode, "no id"),
        // Without the version it read, the write could not be made conditional.
        (unversioned, ErrorKind::Decode, "without its version"),
    ] {
        let (server, socket, key) = google().await;
        answer_to(&server, "GET", EVENT, 200, current).await;
        answer_to(&server, "PATCH", EVENT, 200, invitation()).await;
        let err = socket
            .invoke(
                key,
                "google.calendar_events.respond".into(),
                json!({ "calendarId": "primary", "eventId": "evt1", "responseStatus": "accepted" }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind(), kind, "{says}: {err}");
        assert!(err.message().contains(says), "{}", err.message());
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1, "{says}: only the read");
        assert_eq!(received[0].method.as_str(), "GET");
    }

    // An answer Google does not know is refused before even the read.
    let (server, socket, key) = google().await;
    for status in ["yes", "", "Accepted"] {
        let err = socket
            .invoke(
                key.clone(),
                "google.calendar_events.respond".into(),
                json!({ "calendarId": "primary", "eventId": "evt1", "responseStatus": status }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{status:?}");
        assert!(err.message().contains("needsAction"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_event_that_changed_while_it_was_being_answered_is_reported_and_can_be_tried_again() {
    let (server, socket, key) = google().await;
    answer_to(&server, "GET", EVENT, 200, invitation()).await;
    answer_to(
        &server,
        "PATCH",
        EVENT,
        412,
        refusal(412, "conditionNotMet", "Precondition Failed"),
    )
    .await;
    let err = socket
        .invoke(
            key,
            "google.calendar_events.respond".into(),
            json!({ "calendarId": "primary", "eventId": "evt1", "responseStatus": "accepted" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(err.retry(), Retry::Later);
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "the stale write is not sent again"
    );
}

#[tokio::test]
async fn an_id_is_one_path_segment_whatever_it_contains() {
    let (server, socket, key) = google().await;
    answer(&server, 200, json!({ "kind": "calendar#events", "items": [] })).await;
    // A holiday calendar's id has a `#`, which would otherwise cut the path short.
    let ids = [
        (
            "en.usa#holiday@group.v.calendar.google.com",
            "/calendar/v3/calendars/en.usa%23holiday%40group.v.calendar.google.com/events",
        ),
        ("ada@example.test", "/calendar/v3/calendars/ada%40example.test/events"),
        ("a/b?c", "/calendar/v3/calendars/a%2Fb%3Fc/events"),
        (" primary ", "/calendar/v3/calendars/primary/events"),
    ];
    for (id, _) in ids {
        socket
            .invoke(
                key.clone(),
                "google.calendar_events.list".into(),
                json!({ "calendarId": id }),
            )
            .await
            .unwrap();
    }
    let received = server.received_requests().await.unwrap();
    for (request, (id, at)) in received.iter().zip(ids) {
        assert_eq!(request.url.path(), at, "{id}");
        assert_eq!(request.url.query(), None, "{id}");
    }

    // Nothing that would name a different endpoint is sent at all.
    let calls = [
        ("google.calendar_events.list", json!({ "calendarId": "" })),
        ("google.calendar_events.list", json!({ "calendarId": ".." })),
        ("google.calendar_list.get", json!({ "calendarId": "." })),
        (
            "google.calendar_events.get",
            json!({ "calendarId": "primary", "eventId": " " }),
        ),
        (
            "google.calendar_events.delete",
            json!({ "calendarId": "primary", "eventId": ".." }),
        ),
        (
            "google.calendar_events.patch",
            json!({ "calendarId": "..", "eventId": "evt1", "summary": "x" }),
        ),
        (
            "google.calendar_events.respond",
            json!({ "calendarId": "primary", "eventId": "", "responseStatus": "accepted" }),
        ),
        (
            "google.calendar_events.instances",
            json!({ "calendarId": "primary", "eventId": "." }),
        ),
    ];
    for (operation, input) in calls {
        let err = socket
            .invoke(key.clone(), operation.into(), input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation} {input}");
    }
    assert_eq!(server.received_requests().await.unwrap().len(), ids.len());
}

#[tokio::test]
async fn availability_needs_a_window_and_at_least_one_calendar() {
    let (server, socket, key) = google().await;
    let window = json!({ "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z" });
    let with = |extra: Value| {
        let mut input = window.clone();
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        input
    };
    let bad = [
        with(json!({ "calendarIds": [] })),
        with(json!({ "calendarIds": [" "] })),
        json!({ "calendarIds": ["primary"], "timeMin": "", "timeMax": "2026-10-13T00:00:00Z" }),
        json!({ "calendarIds": ["primary"], "timeMin": "2026-10-12T00:00:00Z", "timeMax": " " }),
    ];
    for input in bad {
        let err = socket
            .invoke(key.clone(), "google.calendar_freebusy.query".into(), input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_change_is_not_sent_again_after_google_fails_whatever_its_verb() {
    let one = json!({ "calendarId": "primary", "eventId": "evt1" });
    let writes = [
        (
            "google.calendar_events.insert",
            json!({ "calendarId": "primary", "start": { "dateTime": NINE }, "end": { "dateTime": TEN } }),
        ),
        (
            "google.calendar_events.patch",
            json!({ "calendarId": "primary", "eventId": "evt1", "summary": "x" }),
        ),
        // A second delete of an event the first one removed would be answered 410.
        ("google.calendar_events.delete", one.clone()),
    ];
    for (operation, input) in writes {
        let (server, socket, key) = google().await;
        answer(&server, 502, refusal(502, "backendError", "Backend Error")).await;
        let err = socket.invoke(key, operation.into(), input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{operation}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{operation}: it may have happened, so it is not repeated"
        );
    }

    // Answering reads first; the read succeeds and the write is sent once.
    let (server, socket, key) = google().await;
    answer_to(&server, "GET", EVENT, 200, invitation()).await;
    answer_to(
        &server,
        "PATCH",
        EVENT,
        502,
        refusal(502, "backendError", "Backend Error"),
    )
    .await;
    let mut input = one.clone();
    input["responseStatus"] = json!("accepted");
    socket
        .invoke(key, "google.calendar_events.respond".into(), input)
        .await
        .unwrap_err();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_read_is_tried_again_after_google_fails_except_the_one_google_takes_as_a_post() {
    let (server, socket, key) = google().await;
    answer(&server, 503, refusal(503, "backendError", "Backend Error")).await;
    socket
        .invoke(
            key.clone(),
            "google.calendar_events.list".into(),
            json!({ "calendarId": "primary" }),
        )
        .await
        .unwrap_err();
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "the testkit allows two attempts"
    );

    // The transport tells a read from a change by the verb alone, so the
    // free/busy query is sent once. That costs a retry and risks nothing.
    let (server, socket, key) = google().await;
    answer(&server, 503, refusal(503, "backendError", "Backend Error")).await;
    socket
        .invoke(
            key,
            "google.calendar_freebusy.query".into(),
            json!({ "calendarIds": ["primary"], "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z" }),
        )
        .await
        .unwrap_err();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn googles_refusals_arrive_as_the_right_kind_of_error() {
    let one = json!({ "calendarId": "primary", "eventId": "evt1" });
    let checks = [
        (
            "google.calendar_events.get",
            one.clone(),
            404,
            refusal(404, "notFound", "Not Found"),
            ErrorKind::NotFound,
            "",
        ),
        // An event that was already deleted. Google says no action is needed; the caller is told it is gone.
        (
            "google.calendar_events.delete",
            one.clone(),
            410,
            refusal(410, "deleted", "Resource has been deleted"),
            ErrorKind::NotFound,
            "no longer has",
        ),
        // The same status for something else entirely: nothing is gone, the question was too old to answer.
        (
            "google.calendar_events.list",
            json!({ "calendarId": "primary", "updatedMin": "2020-01-01T00:00:00Z" }),
            410,
            refusal(
                410,
                "updatedMinTooLongAgo",
                "The requested minimum modification time lies too far in the past.",
            ),
            ErrorKind::InvalidInput,
            "too far in the past",
        ),
        (
            "google.calendar_events.list",
            json!({ "calendarId": "primary", "timeMin": "2026-10-13T00:00:00Z", "timeMax": "2026-10-12T00:00:00Z" }),
            400,
            refusal(400, "timeRangeEmpty", "The specified time range is empty."),
            ErrorKind::InvalidInput,
            "The specified time range is empty.",
        ),
        (
            "google.calendar_events.patch",
            json!({ "calendarId": "primary", "eventId": "evt1", "summary": "x" }),
            403,
            refusal(403, "forbidden", "Forbidden"),
            ErrorKind::AccessDenied,
            "Forbidden",
        ),
        // A token without the calendar scopes.
        (
            "google.calendar_list.list",
            json!({}),
            403,
            refusal(
                403,
                "insufficientPermissions",
                "Request had insufficient authentication scopes.",
            ),
            ErrorKind::AccessDenied,
            "insufficient authentication scopes",
        ),
        // Google's abuse limit is not something waiting a moment fixes.
        (
            "google.calendar_events.insert",
            json!({ "calendarId": "primary", "start": { "dateTime": NINE }, "end": { "dateTime": TEN } }),
            403,
            refusal(403, "quotaExceeded", "Calendar usage limits exceeded."),
            ErrorKind::AccessDenied,
            "Calendar usage limits exceeded.",
        ),
        (
            "google.calendar_freebusy.query",
            json!({ "calendarIds": ["primary"], "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z" }),
            401,
            refusal(401, "authError", "Invalid Credentials"),
            ErrorKind::ReconnectRequired,
            "",
        ),
    ];
    for (operation, input, status, body, kind, says) in checks {
        let (server, socket, key) = google().await;
        answer(&server, status, body).await;
        let err = socket.invoke(key, operation.into(), input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{operation} on HTTP {status}: {err}");
        assert!(err.to_string().contains(says), "{operation} on HTTP {status}: {err}");
    }
}

#[tokio::test]
async fn a_throttle_google_reports_as_403_is_a_rate_limit_and_not_a_refusal() {
    for reason in ["rateLimitExceeded", "userRateLimitExceeded"] {
        let (server, socket, key) = google().await;
        answer(&server, 403, refusal(403, reason, "Rate Limit Exceeded")).await;
        let err = socket
            .invoke(
                key,
                "google.calendar_events.insert".into(),
                json!({ "calendarId": "primary", "start": { "dateTime": NINE }, "end": { "dateTime": TEN } }),
            )
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::RateLimited, "{reason}");
        assert_eq!(err.retry(), Retry::Later, "{reason}");
        // A throttled request was not carried out, so even a write is tried again.
        assert_eq!(server.received_requests().await.unwrap().len(), 2, "{reason}");
    }
}

#[tokio::test]
async fn a_throttle_that_says_how_long_to_wait_keeps_the_wait() {
    let (server, socket, key) = google().await;
    Mock::given(path(EVENT))
        .respond_with(
            ResponseTemplate::new(403)
                .insert_header("retry-after", "3600")
                .set_body_json(refusal(403, "rateLimitExceeded", "Rate Limit Exceeded")),
        )
        .mount(&server)
        .await;
    let err = socket
        .invoke(
            key,
            "google.calendar_events.get".into(),
            json!({ "calendarId": "primary", "eventId": "evt1" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(err.retry(), Retry::After(std::time::Duration::from_secs(3600)));
}

#[tokio::test]
async fn an_answer_on_another_persons_calendar_is_that_persons_answer() {
    // Google marks as `self` the entry of the calendar the event was read
    // from, not of whoever is signed in. Ada, managing Grace's calendar,
    // answers for Grace.
    let (server, socket, key) = google().await;
    let at = "/calendar/v3/calendars/grace%40example.test/events/evt1";
    answer_to(&server, "GET", at, 200, invitation()).await;
    answer_to(&server, "PATCH", at, 200, invitation()).await;
    socket
        .invoke(
            key,
            "google.calendar_events.respond".into(),
            json!({ "calendarId": "grace@example.test", "eventId": "evt1", "responseStatus": "tentative" }),
        )
        .await
        .unwrap();
    let received = server.received_requests().await.unwrap();
    let sent = body_of(&received[1]);
    assert_eq!(sent["attendees"][0]["email"], "ada@example.test");
    assert_eq!(
        sent["attendees"][0]["responseStatus"], "accepted",
        "Ada's own answer stands"
    );
    assert_eq!(sent["attendees"][1]["email"], "grace@example.test");
    assert_eq!(sent["attendees"][1]["responseStatus"], "tentative");

    let respond = Google::new()
        .operations()
        .into_iter()
        .find(|o| o.name == "google.calendar_events.respond")
        .unwrap();
    assert!(
        respond.description.contains("On primary"),
        "a person approving the call is told whose answer it is: {}",
        respond.description
    );
}

#[tokio::test]
async fn a_success_that_does_not_carry_the_result_is_an_error() {
    let one = json!({ "calendarId": "primary", "eventId": "evt1" });
    let window =
        json!({ "calendarIds": ["primary"], "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-13T00:00:00Z" });
    let checks = [
        ("google.calendar_events.get", one.clone(), json!({})),
        ("google.calendar_events.get", one.clone(), json!({ "summary": "no id" })),
        (
            "google.calendar_events.insert",
            json!({ "calendarId": "primary", "start": { "dateTime": NINE }, "end": { "dateTime": TEN } }),
            json!({ "status": "confirmed" }),
        ),
        (
            "google.calendar_events.patch",
            json!({ "calendarId": "primary", "eventId": "evt1", "summary": "x" }),
            json!({ "id": "" }),
        ),
        (
            "google.calendar_events.list",
            json!({ "calendarId": "primary" }),
            json!({}),
        ),
        (
            "google.calendar_events.list",
            json!({ "calendarId": "primary" }),
            json!({ "items": [] }),
        ),
        (
            "google.calendar_events.list",
            json!({ "calendarId": "primary" }),
            json!({ "kind": "calendar#events", "items": "none" }),
        ),
        (
            "google.calendar_events.list",
            json!({ "calendarId": "primary" }),
            json!({ "kind": "calendar#events", "items": [{ "summary": "no id" }] }),
        ),
        (
            "google.calendar_events.instances",
            one.clone(),
            json!({ "kind": "calendar#event", "id": "evt1" }),
        ),
        (
            "google.calendar_list.list",
            json!({}),
            json!({ "kind": "calendar#events", "items": [] }),
        ),
        (
            "google.calendar_list.get",
            json!({ "calendarId": "primary" }),
            json!({ "summary": "no id" }),
        ),
        ("google.calendar_freebusy.query", window.clone(), json!({})),
        (
            "google.calendar_freebusy.query",
            window.clone(),
            json!({ "kind": "calendar#freeBusy" }),
        ),
        // Asked about one calendar and told about none: that is not "free all day".
        (
            "google.calendar_freebusy.query",
            window,
            json!({ "kind": "calendar#freeBusy", "calendars": {} }),
        ),
    ];
    for (operation, input, body) in checks {
        let (server, socket, key) = google().await;
        answer(&server, 200, body.clone()).await;
        let err = socket.invoke(key, operation.into(), input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{operation} given {body}: {err}");
    }
}

#[tokio::test]
async fn an_input_error_names_the_field_but_never_repeats_the_callers_values() {
    let (server, socket, key) = google().await;
    let secret = "ya29.PRIVATE-do-not-log";
    let bad = [
        (
            "google.calendar_events.get",
            json!({ "calendarId": { "nested": secret }, "eventId": "evt1" }),
            "calendarId",
        ),
        (
            "google.calendar_events.get",
            json!({ "calendarId": "primary" }),
            "eventId",
        ),
        (
            "google.calendar_freebusy.query",
            json!({ "calendarIds": secret, "timeMin": "a", "timeMax": "b" }),
            "calendarIds",
        ),
        (
            "google.calendar_events.list",
            json!({ "calendarId": "primary", "maxResults": secret }),
            "input",
        ),
    ];
    for (operation, input, names) in bad {
        let err = socket.invoke(key.clone(), operation.into(), input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation}");
        let everything = format!("{err} {err:?} {}", serde_json::to_string(&err.to_wire()).unwrap());
        assert!(!everything.contains("PRIVATE"), "{operation}: {everything}");
        assert!(err.message().contains(names), "{operation}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_typed_methods_take_identifiers_plainly_and_content_as_structs() {
    let (server, socket, key) = google().await;
    answer_to(&server, "POST", EVENTS, 200, event()).await;
    answer_to(&server, "PATCH", EVENT, 200, event()).await;
    answer_to(&server, "GET", EVENT, 200, event()).await;
    answer_to(
        &server,
        "GET",
        EVENTS,
        200,
        json!({ "kind": "calendar#events", "items": [event()], "nextPageToken": "page-2" }),
    )
    .await;
    answer_to(
        &server,
        "POST",
        "/calendar/v3/freeBusy",
        200,
        json!({ "kind": "calendar#freeBusy", "calendars": { "primary": { "busy": [{ "start": NINE, "end": TEN }] } } }),
    )
    .await;
    Mock::given(method("DELETE"))
        .and(path(EVENT))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let connection = socket.connection(key).await.unwrap();
    let google = Google::new();
    let events = google.calendar_events(&connection);

    let created = events
        .insert(
            "primary",
            InsertEvent::new(EventTime::at(NINE), EventTime::at(TEN))
                .summary("Design review")
                .invite("grace@example.test")
                .with_meet_link(),
        )
        .await
        .unwrap();
    assert_eq!(created.id, "evt1");
    assert_eq!(
        created.hangout_link.as_deref(),
        Some("https://meet.google.com/abc-defg-hij")
    );

    let changes = PatchEvent {
        location: Some("Room 5".into()),
        ..PatchEvent::default()
    };
    events.patch("primary", &created.id, changes).await.unwrap();
    // Ada organises the meeting and is on its guest list, so she can answer too.
    events
        .respond("primary", &created.id, Respond::tentative().comment("Might be late"))
        .await
        .unwrap();

    let week = ListEvents {
        time_min: Some("2026-10-12T00:00:00Z".into()),
        time_max: Some("2026-10-19T00:00:00Z".into()),
        single_events: Some(true),
        ..ListEvents::default()
    };
    let page = events.list("primary", week).await.unwrap();
    assert_eq!(page.items[0].summary.as_deref(), Some("Design review"));
    assert_eq!(page.next_cursor.as_deref(), Some("page-2"));

    let busy = google
        .calendar_freebusy(&connection)
        .query(
            &["primary".to_owned()],
            FreeBusyQuery::between("2026-10-12T00:00:00Z", "2026-10-13T00:00:00Z"),
        )
        .await
        .unwrap();
    assert_eq!(busy.calendars["primary"].busy[0].start, NINE);
    assert!(busy.calendars["primary"].errors.is_empty());

    events.delete("primary", &created.id, Default::default()).await.unwrap();

    let received = server.received_requests().await.unwrap();
    let sent: Vec<(String, Value)> = received
        .iter()
        .map(|r| (format!("{} {}", r.method, r.url.path()), body_of(r)))
        .collect();
    assert_eq!(sent[0].0, format!("POST {EVENTS}"));
    assert_eq!(sent[0].1["summary"], "Design review");
    assert_eq!(sent[0].1["attendees"], json!([{ "email": "grace@example.test" }]));
    assert_eq!(sent[1], (format!("PATCH {EVENT}"), json!({ "location": "Room 5" })));
    assert_eq!(sent[2].0, format!("GET {EVENT}"));
    assert_eq!(sent[3].0, format!("PATCH {EVENT}"));
    assert_eq!(sent[3].1["attendees"][0]["responseStatus"], "tentative");
    assert_eq!(sent[3].1["attendees"][0]["comment"], "Might be late");
    assert_eq!(
        sent[3].1["attendees"][1]["responseStatus"], "needsAction",
        "Grace's answer is hers"
    );
    assert_eq!(sent[6], (format!("DELETE {EVENT}"), Value::Null));
}
