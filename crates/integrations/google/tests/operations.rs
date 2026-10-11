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
    vec![]
}

// ── drive: cases ──
#[rustfmt::skip]
fn drive_cases() -> Vec<Case> {
    vec![]
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

        // ── drive: effects ──

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
