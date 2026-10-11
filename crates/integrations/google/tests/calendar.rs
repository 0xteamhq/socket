//! Google Calendar against a local server that answers as Google does.
//!
//! What every operation sends and returns is in the table in `operations.rs`.
//! This file holds what is particular to Calendar.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Integration, Retry};
use socketkit_google::models::{
    EventDelete, EventFilter, EventInsert, EventPatch, EventResponse, EventTime, FreeBusyQuery, Paging,
};
use socketkit_google::{Google, scopes};
use socketkit_testkit::wiremock::matchers::{method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};

mod support;
use support::calendar::{
    AT_NINE, AT_TEN, CALENDAR_EVENT_PATH, CALENDAR_EVENTS_PATH, CALENDAR_LIST_PATH, FREEBUSY_PATH, MEET_LINK,
    calendar_entry, calendar_event, calendar_events_page, calendar_hidden_invitation, calendar_invitation, freebusy,
};
use support::{answer, answering, body_of, google, google_error, invoke, only_request, query_of};

const ONE_DAY: [&str; 2] = ["2026-10-12T00:00:00Z", "2026-10-13T00:00:00Z"];

/// Answers `verb` at `at` with `status` and `body`.
async fn mount(server: &MockServer, verb: &str, at: &str, status: u16, body: Value) {
    Mock::given(method(verb))
        .and(path(at))
        .respond_with(answer(status, &body))
        .mount(server)
        .await;
}

/// `fields` beside the ids of the one event these tests use.
fn on_the_event(fields: Value) -> Value {
    let mut input = json!({ "calendar": "primary", "event": "evt1" });
    let more = fields.as_object().unwrap().clone();
    input.as_object_mut().unwrap().extend(more);
    input
}

fn timed_event() -> Value {
    json!({ "calendar": "primary", "start": { "dateTime": AT_NINE }, "end": { "dateTime": AT_TEN } })
}

fn one_day_of(calendars: Value) -> Value {
    json!({ "calendars": calendars, "timeMin": ONE_DAY[0], "timeMax": ONE_DAY[1] })
}

// ── What is described to a caller ────────────────────────────────────────────

#[tokio::test]
async fn the_calendar_scopes_are_the_two_google_documents() {
    assert_eq!(
        scopes::CALENDAR_READONLY,
        "https://www.googleapis.com/auth/calendar.readonly"
    );
    assert_eq!(
        scopes::CALENDAR_EVENTS,
        "https://www.googleapis.com/auth/calendar.events"
    );
    // Neither is asked for unless the application names it.
    let socketkit_core::AuthScheme::OAuth2(oauth) = socketkit_google::provider().auth else {
        panic!("google uses OAuth")
    };
    assert!(
        !oauth.default_scopes.iter().any(|scope| scope.contains("calendar")),
        "{:?}",
        oauth.default_scopes
    );
}

#[tokio::test]
async fn an_event_is_described_with_what_it_needs_and_what_leads_to_its_recording() {
    let operations = Google::new().operations();
    let find = |name: &str| operations.iter().find(|o| o.name == name).unwrap();
    let names = |schema: &Value| -> Vec<String> { schema["properties"].as_object().unwrap().keys().cloned().collect() };

    let insert = find("google.calendar_events.insert");
    let mut required: Vec<&str> = insert.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    required.sort_unstable();
    assert_eq!(required, ["calendar", "end", "start"]);
    let described = names(&insert.input_schema);
    for field in [
        "calendar",
        "summary",
        "description",
        "location",
        "start",
        "end",
        "attendees",
        "recurrence",
        "createMeetLink",
        "sendUpdates",
    ] {
        assert!(described.iter().any(|name| name == field), "{field} is described");
    }
    // What leads from a meeting to its recording, transcript and notes is part of what comes back.
    let returned = names(&insert.output_schema);
    for field in [
        "id",
        "summary",
        "description",
        "start",
        "end",
        "organizer",
        "attendees",
        "location",
        "recurrence",
        "recurringEventId",
        "hangoutLink",
        "conferenceData",
        "attachments",
    ] {
        assert!(returned.iter().any(|name| name == field), "{field} is returned");
    }

    // A list is paged as every list is, and filtered under Google's names.
    let mut listed = names(&find("google.calendar_events.list").input_schema);
    listed.sort_unstable();
    assert_eq!(
        listed,
        [
            "calendar",
            "cursor",
            "limit",
            "orderBy",
            "q",
            "showDeleted",
            "singleEvents",
            "timeMax",
            "timeMin",
            "timeZone",
            "updatedMin"
        ]
    );
}

#[tokio::test]
async fn a_field_that_is_not_one_of_an_operations_is_refused_and_not_dropped() {
    let (server, socket, key) = google().await;
    for (operation, input, named) in [
        // Google's own names for what is `calendar`, `event`, `cursor` and `limit` here.
        ("calendar_events.list", json!({ "calendarId": "primary" }), "calendarId"),
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "pageToken": "page-2" }),
            "pageToken",
        ),
        ("calendar_list.list", json!({ "maxResults": 10 }), "maxResults"),
        (
            "calendar_events.insert",
            json!({ "calendar": "primary", "start": { "dateTime": AT_NINE, "timezone": "Europe/Zurich" }, "end": { "dateTime": AT_TEN } }),
            "start.timezone",
        ),
        (
            "calendar_events.patch",
            on_the_event(json!({ "attendees": [{ "email": "grace@example.test", "name": "Grace" }] })),
            "attendees[0].name",
        ),
    ] {
        let err = invoke(&socket, &key, operation, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation}");
        assert!(err.message().contains(named), "{operation}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_input_error_names_the_field_but_never_repeats_the_callers_values() {
    let (server, socket, key) = google().await;
    let secret = "ya29.PRIVATE-do-not-log";
    let bad = [
        (
            "calendar_events.get",
            json!({ "calendar": { "nested": secret }, "event": "evt1" }),
            "calendar",
        ),
        ("calendar_events.get", json!({ "calendar": "primary" }), "event"),
        (
            "calendar_freebusy.query",
            json!({ "calendars": secret, "timeMin": ONE_DAY[0], "timeMax": ONE_DAY[1] }),
            "calendars",
        ),
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "limit": secret }),
            "input",
        ),
        // What Socket itself refuses is named by its field as well.
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "timeMin": secret }),
            "timeMin",
        ),
        (
            "calendar_events.respond",
            on_the_event(json!({ "responseStatus": secret })),
            "responseStatus",
        ),
        (
            "calendar_events.insert",
            json!({ "calendar": "primary", "start": { "dateTime": secret }, "end": { "dateTime": AT_TEN } }),
            "start.dateTime",
        ),
        ("calendar_freebusy.query", one_day_of(json!([secret, " "])), "calendars"),
    ];
    for (operation, input, names) in bad {
        let err = invoke(&socket, &key, operation, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation}");
        let everything = format!("{err} {err:?} {}", serde_json::to_string(&err.to_wire()).unwrap());
        assert!(!everything.contains("PRIVATE"), "{operation}: {everything}");
        assert!(err.message().contains(names), "{operation}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Reading ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_event_keeps_what_leads_to_its_recording_transcript_and_notes() {
    let (server, socket, key) = google().await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_event()).await;
    let connection = socket.connection(key).await.unwrap();
    let found = Google::new()
        .calendar_events(&connection)
        .get("primary", "evt1")
        .await
        .unwrap();

    assert_eq!(found.hangout_link.as_deref(), Some(MEET_LINK));
    let conference = found.conference_data.expect("the conference is kept");
    assert_eq!(conference.conference_id.as_deref(), Some("abc-defg-hij"));
    let solution = conference.conference_solution.expect("which product hosts it");
    assert_eq!(solution.key.unwrap().kind, "hangoutsMeet");
    assert_eq!(conference.entry_points.len(), 2);
    assert_eq!(conference.entry_points[0].entry_point_type, "video");
    assert_eq!(conference.entry_points[0].uri.as_deref(), Some(MEET_LINK));
    assert_eq!(conference.entry_points[1].pin.as_deref(), Some("123456789"));
    assert_eq!(found.attachments.len(), 1);
    assert_eq!(found.attachments[0].file_id.as_deref(), Some("1AbC"));
    assert_eq!(found.attachments[0].mime_type.as_deref(), Some("video/mp4"));
    assert_eq!(found.attachments[0].file_url, "https://drive.google.com/open?id=1AbC");

    assert_eq!(found.organizer.unwrap().email.as_deref(), Some("ada@example.test"));
    assert!(found.attendees[0].is_self && found.attendees[0].organizer);
    assert_eq!(found.attendees[1].response_status, "needsAction");
    assert!(found.attendees[1].optional);
    assert_eq!(found.location.as_deref(), Some("Room 4"));
    let start = found.start.unwrap();
    assert_eq!(start.date_time.as_deref(), Some(AT_NINE));
    assert_eq!(start.date, None, "a timed event has no all-day date");
    assert_eq!(found.ical_uid.as_deref(), Some("evt1@google.com"));
    assert_eq!(found.guests_can_see_other_guests, None, "Google did not say");
    assert!(!found.attendees_omitted);
    assert_eq!(query_of(&only_request(&server).await), json!({}));
}

#[tokio::test]
async fn an_all_day_event_a_series_and_a_cancelled_instance_read_too() {
    let (server, socket, key) = google().await;
    let items = json!([
        { "id": "holiday", "status": "confirmed", "summary": "Offsite", "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-14" } },
        { "id": "evt1", "summary": "Design review", "start": { "dateTime": "2026-10-12T09:00:00", "timeZone": "America/Los_Angeles" },
          "end": { "dateTime": "2026-10-12T10:00:00", "timeZone": "America/Los_Angeles" }, "recurrence": ["RRULE:FREQ=WEEKLY;COUNT=10", "EXDATE;TZID=America/Los_Angeles:20261019T090000"] },
        // An instance removed from a series carries almost nothing.
        { "id": "evt1_20261019T160000Z", "status": "cancelled", "recurringEventId": "evt1", "originalStartTime": { "dateTime": "2026-10-19T09:00:00-07:00" } }
    ]);
    mount(&server, "GET", CALENDAR_EVENTS_PATH, 200, calendar_events_page(items)).await;
    let connection = socket.connection(key).await.unwrap();
    let events = Google::new()
        .calendar_events(&connection)
        .list("primary", EventFilter::default(), Paging::default())
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
    assert!(offsite.recurrence.is_empty());

    let series = &events.items[1];
    assert_eq!(series.recurrence.len(), 2);
    assert_eq!(series.recurrence[0], "RRULE:FREQ=WEEKLY;COUNT=10");
    assert_eq!(
        series.recurring_event_id, None,
        "the series is not an instance of itself"
    );
    assert_eq!(
        series.start.as_ref().unwrap().time_zone.as_deref(),
        Some("America/Los_Angeles")
    );

    let cancelled = &events.items[2];
    assert_eq!(cancelled.status.as_deref(), Some("cancelled"));
    assert_eq!(cancelled.start, None);
    assert_eq!(cancelled.recurring_event_id.as_deref(), Some("evt1"));
    let original = cancelled.original_start_time.as_ref().unwrap();
    assert_eq!(original.date_time.as_deref(), Some("2026-10-19T09:00:00-07:00"));
}

#[tokio::test]
async fn a_list_with_nothing_in_it_is_empty_and_not_an_error() {
    // Google leaves `items` out of some empty lists.
    for body in [
        json!({ "kind": "calendar#events", "items": [] }),
        json!({ "kind": "calendar#events" }),
    ] {
        let (_server, socket, key) = answering(200, body).await;
        let events = invoke(&socket, &key, "calendar_events.list", json!({ "calendar": "primary" }))
            .await
            .unwrap();
        assert_eq!(events, json!({ "items": [], "next_cursor": null }));
    }
    let (_server, socket, key) = answering(200, json!({ "kind": "calendar#calendarList" })).await;
    let calendars = invoke(&socket, &key, "calendar_list.list", json!({})).await.unwrap();
    assert_eq!(calendars, json!({ "items": [], "next_cursor": null }));
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent_and_false_is_a_value() {
    let (server, socket, key) = answering(200, calendar_events_page(json!([]))).await;
    invoke(&socket, &key, "calendar_events.list", json!({ "calendar": "primary" }))
        .await
        .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({}),
        "Google's own defaults apply"
    );

    let (server, socket, key) = answering(200, calendar_events_page(json!([]))).await;
    let input = json!({ "calendar": "primary", "q": "design", "singleEvents": false, "showDeleted": false, "timeMin": null, "cursor": null });
    invoke(&socket, &key, "calendar_events.list", input).await.unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "q": "design", "singleEvents": "false", "showDeleted": "false" }),
        "only what is unset is left out"
    );

    let (server, socket, key) = answering(
        200,
        json!({ "kind": "calendar#calendarList", "items": [calendar_entry()] }),
    )
    .await;
    invoke(&socket, &key, "calendar_list.list", json!({})).await.unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({}));

    let (server, socket, key) = answering(200, calendar_events_page(json!([]))).await;
    invoke(&socket, &key, "calendar_events.instances", on_the_event(json!({})))
        .await
        .unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({}));
}

#[tokio::test]
async fn a_list_is_paged_with_the_cursor_and_limit_under_googles_names() {
    let mut page = calendar_events_page(json!([calendar_event()]));
    page["nextPageToken"] = json!("page-3");
    for (operation, input, at) in [
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "cursor": "page-2", "limit": 2500 }),
            CALENDAR_EVENTS_PATH.to_owned(),
        ),
        (
            "calendar_events.instances",
            on_the_event(json!({ "cursor": "page-2", "limit": 2500 })),
            format!("{CALENDAR_EVENT_PATH}/instances"),
        ),
    ] {
        let (server, socket, key) = answering(200, page.clone()).await;
        let listed = invoke(&socket, &key, operation, input).await.unwrap();
        assert_eq!(listed["next_cursor"], "page-3", "{operation}");
        assert_eq!(listed["items"][0]["id"], "evt1", "{operation}");
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), at, "{operation}");
        assert_eq!(
            query_of(&request),
            json!({ "pageToken": "page-2", "maxResults": "2500" }),
            "{operation}"
        );
    }

    let listing = json!({ "kind": "calendar#calendarList", "items": [calendar_entry()], "nextPageToken": "" });
    let (server, socket, key) = answering(200, listing).await;
    let calendars = invoke(
        &socket,
        &key,
        "calendar_list.list",
        json!({ "cursor": " ", "limit": 250 }),
    )
    .await
    .unwrap();
    assert_eq!(calendars["next_cursor"], Value::Null, "an empty token is no token");
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "maxResults": "250" }),
        "a blank cursor is the first page"
    );
}

#[tokio::test]
async fn a_limit_google_would_refuse_is_refused_before_it_is_asked() {
    let (server, socket, key) = google().await;
    for (operation, input, most) in [
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "limit": 0 }),
            "2500",
        ),
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "limit": 2501 }),
            "2500",
        ),
        (
            "calendar_events.instances",
            on_the_event(json!({ "limit": 2501 })),
            "2500",
        ),
        ("calendar_list.list", json!({ "limit": 0 }), "250"),
        ("calendar_list.list", json!({ "limit": 251 }), "250"),
    ] {
        let err = invoke(&socket, &key, operation, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation} {input}");
        assert!(err.message().contains("limit"), "{}", err.message());
        assert!(err.message().contains(most), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn sorting_by_start_time_needs_recurring_events_expanded() {
    let (server, socket, key) = answering(200, calendar_events_page(json!([]))).await;
    // Google refuses these: a series has no one start to sort by.
    for unexpanded in [
        json!({ "calendar": "primary", "orderBy": "startTime" }),
        json!({ "calendar": "primary", "orderBy": "startTime", "singleEvents": false }),
    ] {
        let err = invoke(&socket, &key, "calendar_events.list", unexpanded.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{unexpanded}");
        assert!(err.message().contains("orderBy"), "{}", err.message());
        assert!(err.message().contains("singleEvents"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The other order needs nothing, and the two together are what Google takes.
    for (input, sent) in [
        (
            json!({ "calendar": "primary", "orderBy": "updated" }),
            json!({ "orderBy": "updated" }),
        ),
        (
            json!({ "calendar": "primary", "orderBy": "startTime", "singleEvents": true }),
            json!({ "orderBy": "startTime", "singleEvents": "true" }),
        ),
    ] {
        invoke(&socket, &key, "calendar_events.list", input).await.unwrap();
        let received = server.received_requests().await.unwrap();
        assert_eq!(query_of(received.last().unwrap()), sent);
    }
}

#[tokio::test]
async fn a_time_without_its_offset_is_refused_before_google_is_asked() {
    let (server, socket, key) = google().await;
    let unplaced = "2026-10-12T09:00:00";
    let calls = [
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "timeMin": unplaced }),
            "timeMin",
        ),
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "timeMin": ONE_DAY[0], "timeMax": "2026-10-13" }),
            "timeMax",
        ),
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "updatedMin": "" }),
            "updatedMin",
        ),
        (
            "calendar_events.instances",
            on_the_event(json!({ "timeMin": unplaced })),
            "timeMin",
        ),
        (
            "calendar_events.instances",
            on_the_event(json!({ "timeMax": "next week" })),
            "timeMax",
        ),
        (
            "calendar_freebusy.query",
            json!({ "calendars": ["primary"], "timeMin": unplaced, "timeMax": ONE_DAY[1] }),
            "timeMin",
        ),
        (
            "calendar_freebusy.query",
            json!({ "calendars": ["primary"], "timeMin": ONE_DAY[0], "timeMax": " " }),
            "timeMax",
        ),
    ];
    for (operation, input, field) in calls {
        let err = invoke(&socket, &key, operation, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation} {input}");
        assert!(
            err.message().contains(&format!("`{field}`")),
            "{operation}: {}",
            err.message()
        );
        assert!(err.message().contains("offset"), "{}", err.message());
        assert!(!err.message().contains("next week"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // Any offset will do, and it reaches Google as it was written.
    let (server, socket, key) = answering(200, calendar_events_page(json!([]))).await;
    let window =
        json!({ "calendar": "primary", "timeMin": "2026-10-12T00:00:00+05:30", "timeMax": "2026-10-13T00:00:00.000Z" });
    invoke(&socket, &key, "calendar_events.list", window).await.unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "timeMin": "2026-10-12T00:00:00+05:30", "timeMax": "2026-10-13T00:00:00.000Z" })
    );
}

#[tokio::test]
async fn an_id_is_one_path_segment_whatever_it_contains() {
    let (server, socket, key) = answering(200, calendar_events_page(json!([]))).await;
    // A holiday calendar's id has a `#`, which would otherwise cut the path short.
    let ids = [
        (
            "en.usa#holiday@group.v.calendar.google.com",
            "/calendar/v3/calendars/en.usa%23holiday%40group.v.calendar.google.com/events",
        ),
        ("ada@example.test", "/calendar/v3/calendars/ada%40example.test/events"),
        ("a/b?c", "/calendar/v3/calendars/a%2Fb%3Fc/events"),
        (
            "../../drive/v3/files",
            "/calendar/v3/calendars/..%2F..%2Fdrive%2Fv3%2Ffiles/events",
        ),
        (" primary ", "/calendar/v3/calendars/primary/events"),
    ];
    for (id, _) in ids {
        invoke(&socket, &key, "calendar_events.list", json!({ "calendar": id }))
            .await
            .unwrap();
    }
    let received = server.received_requests().await.unwrap();
    for (request, (id, at)) in received.iter().zip(ids) {
        assert_eq!(request.url.path(), at, "{id}");
        assert_eq!(request.url.query(), None, "{id}");
        assert_eq!(request.url.fragment(), None, "{id}");
    }

    // An event's id is kept whole in the same way, under each verb.
    let (server, socket, key) = answering(200, calendar_event()).await;
    invoke(
        &socket,
        &key,
        "calendar_events.get",
        json!({ "calendar": "primary", "event": "evt/1?x#y" }),
    )
    .await
    .unwrap();
    assert_eq!(
        only_request(&server).await.url.path(),
        "/calendar/v3/calendars/primary/events/evt%2F1%3Fx%23y"
    );
    let (server, socket, key) = answering(204, json!(null)).await;
    invoke(
        &socket,
        &key,
        "calendar_events.delete",
        json!({ "calendar": "primary", "event": "a/../b" }),
    )
    .await
    .unwrap();
    assert_eq!(
        only_request(&server).await.url.path(),
        "/calendar/v3/calendars/primary/events/a%2F..%2Fb"
    );
    let (server, socket, key) = answering(200, calendar_entry()).await;
    invoke(&socket, &key, "calendar_list.get", json!({ "calendar": "a/b#c" }))
        .await
        .unwrap();
    assert_eq!(
        only_request(&server).await.url.path(),
        "/calendar/v3/users/me/calendarList/a%2Fb%23c"
    );

    // Nothing that would name a different endpoint is sent at all.
    let (server, socket, key) = google().await;
    let calls = [
        ("calendar_events.list", json!({ "calendar": "" })),
        ("calendar_events.list", json!({ "calendar": ".." })),
        ("calendar_list.get", json!({ "calendar": "." })),
        ("calendar_events.get", json!({ "calendar": "primary", "event": " " })),
        (
            "calendar_events.delete",
            json!({ "calendar": "primary", "event": ".." }),
        ),
        (
            "calendar_events.patch",
            json!({ "calendar": "..", "event": "evt1", "summary": "x" }),
        ),
        (
            "calendar_events.respond",
            json!({ "calendar": "primary", "event": "", "responseStatus": "accepted" }),
        ),
        (
            "calendar_events.instances",
            json!({ "calendar": "primary", "event": "." }),
        ),
        ("calendar_events.insert", {
            let mut event = timed_event();
            event["calendar"] = json!(" ");
            event
        }),
    ];
    for (operation, input) in calls {
        let err = invoke(&socket, &key, operation, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation} {input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn availability_needs_a_window_and_calendars_that_are_all_named() {
    let (server, socket, key) = google().await;
    let bad = [
        one_day_of(json!([])),
        one_day_of(json!([" "])),
        one_day_of(json!(["primary", ""])),
        json!({ "calendars": ["primary"], "timeMin": "", "timeMax": ONE_DAY[1] }),
        json!({ "calendars": ["primary"], "timeMin": ONE_DAY[0], "timeMax": " " }),
        // Without a window there is nothing to ask.
        json!({ "calendars": ["primary"], "timeMin": ONE_DAY[0] }),
    ];
    for input in bad {
        let err = invoke(&socket, &key, "calendar_freebusy.query", input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_calendar_google_could_not_answer_for_is_not_reported_as_free() {
    let (server, socket, key) = google().await;
    mount(&server, "POST", FREEBUSY_PATH, 200, freebusy()).await;
    let connection = socket.connection(key).await.unwrap();
    let busy = Google::new()
        .calendar_freebusy(&connection)
        .query(
            &[" primary ".to_owned(), "grace@example.test".to_owned()],
            FreeBusyQuery::between(ONE_DAY[0], ONE_DAY[1]),
        )
        .await
        .unwrap();
    assert_eq!(busy.calendars["primary"].busy.len(), 1);
    assert_eq!(busy.calendars["primary"].busy[0].end, "2026-10-12T19:00:00+02:00");
    assert!(busy.calendars["primary"].errors.is_empty());
    let unseen = &busy.calendars["grace@example.test"];
    assert!(unseen.busy.is_empty());
    assert_eq!(unseen.errors[0].reason, "notFound", "empty is not free");

    let sent = body_of(&only_request(&server).await);
    assert_eq!(
        sent,
        json!({ "timeMin": ONE_DAY[0], "timeMax": ONE_DAY[1], "items": [{ "id": "primary" }, { "id": "grace@example.test" }] }),
        "no time zone is sent unless one is asked for"
    );
}

// ── Creating and changing ────────────────────────────────────────────────────

#[tokio::test]
async fn asking_for_a_meet_link_sends_a_fresh_create_request_each_time() {
    let (server, socket, key) = google().await;
    mount(&server, "POST", CALENDAR_EVENTS_PATH, 200, calendar_event()).await;
    let mut input = timed_event();
    input["summary"] = json!("Design review");
    input["createMeetLink"] = json!(true);
    for _ in 0..3 {
        let created = invoke(&socket, &key, "calendar_events.insert", input.clone())
            .await
            .unwrap();
        assert_eq!(created["hangoutLink"], MEET_LINK);
        assert_eq!(created["conferenceData"]["entryPoints"][0]["uri"], MEET_LINK);
    }
    let mut request_ids = Vec::new();
    for request in server.received_requests().await.unwrap() {
        // Without this parameter Google ignores the conference in the body.
        assert_eq!(query_of(&request), json!({ "conferenceDataVersion": "1" }));
        let mut body = body_of(&request);
        let create = body["conferenceData"]["createRequest"].take();
        assert_eq!(create["conferenceSolutionKey"], json!({ "type": "hangoutsMeet" }));
        let id = create["requestId"].as_str().unwrap().to_owned();
        // Long enough that two cannot be expected to meet: at least 128 bits, written out.
        assert!(id.len() >= 32, "{id}");
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'), "{id}");
        request_ids.push(id);
        assert_eq!(
            body,
            json!({ "summary": "Design review", "start": { "dateTime": AT_NINE }, "end": { "dateTime": AT_TEN }, "conferenceData": { "createRequest": null } }),
            "the flag itself is not a field Google knows"
        );
    }
    request_ids.sort_unstable();
    request_ids.dedup();
    assert_eq!(
        request_ids.len(),
        3,
        "Google ignores a create request whose id it has seen"
    );
}

#[tokio::test]
async fn an_event_without_a_meet_link_sends_no_conference() {
    for flag in [json!(false), json!(null)] {
        let (server, socket, key) = google().await;
        mount(&server, "POST", CALENDAR_EVENTS_PATH, 200, calendar_event()).await;
        let input = json!({ "calendar": "primary", "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-13" }, "createMeetLink": flag });
        invoke(&socket, &key, "calendar_events.insert", input).await.unwrap();
        let request = only_request(&server).await;
        assert_eq!(query_of(&request), json!({}), "nobody is emailed unless asked");
        assert_eq!(
            body_of(&request),
            json!({ "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-13" } })
        );
    }
}

#[tokio::test]
async fn an_event_needs_a_start_and_an_end_that_are_each_a_time_or_a_date() {
    let (server, socket, key) = google().await;
    let at = json!({ "dateTime": AT_NINE });
    let bad = [
        (json!({}), at.clone(), "start"),
        (at.clone(), json!({ "timeZone": "Europe/Zurich" }), "end"),
        (
            json!({ "date": "2026-10-12", "dateTime": AT_NINE }),
            at.clone(),
            "start",
        ),
        (at.clone(), json!({ "date": " " }), "end"),
        (json!({ "date": "", "dateTime": AT_NINE }), at.clone(), "start"),
        // A time that says neither its offset nor its zone could be anywhere.
        (
            json!({ "dateTime": "2026-10-12T09:00:00" }),
            at.clone(),
            "start.dateTime",
        ),
        (
            at.clone(),
            json!({ "dateTime": "2026-10-12T10:00:00", "timeZone": " " }),
            "end.dateTime",
        ),
    ];
    for (start, end, names) in bad {
        let input = json!({ "calendar": "primary", "start": start, "end": end });
        let err = invoke(&socket, &key, "calendar_events.insert", input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{names}");
        assert!(err.message().contains(names), "{}", err.message());
    }
    // Leaving one out altogether is caught when the input is read.
    let err = invoke(
        &socket,
        &key,
        "calendar_events.insert",
        json!({ "calendar": "primary", "start": at }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().contains("end"), "{}", err.message());
    // And so is a guest with no address to invite.
    let mut event = timed_event();
    event["attendees"] = json!([{ "email": "grace@example.test" }, { "email": " " }]);
    let err = invoke(&socket, &key, "calendar_events.insert", event)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().contains("email"), "{}", err.message());
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_time_may_leave_its_offset_to_the_zone_named_beside_it() {
    // This is the form a recurring event takes.
    let (server, socket, key) = google().await;
    mount(&server, "POST", CALENDAR_EVENTS_PATH, 200, calendar_event()).await;
    let weekly = json!({
        "calendar": "primary",
        "start": { "dateTime": "2026-10-12T09:00:00", "timeZone": "Europe/Zurich" },
        "end": { "dateTime": "2026-10-12T10:00:00", "timeZone": "Europe/Zurich" },
        "recurrence": ["RRULE:FREQ=WEEKLY;COUNT=10"]
    });
    invoke(&socket, &key, "calendar_events.insert", weekly.clone())
        .await
        .unwrap();
    let mut sent = weekly;
    sent.as_object_mut().unwrap().remove("calendar");
    assert_eq!(body_of(&only_request(&server).await), sent);
}

#[tokio::test]
async fn a_patch_sends_only_what_was_set_and_refuses_to_send_nothing() {
    let (server, socket, key) = google().await;
    mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_event()).await;
    let patch = |changes: Value| {
        let (socket, key) = (&socket, &key);
        async move { invoke(socket, key, "calendar_events.patch", on_the_event(changes)).await }
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
    patch(
        json!({ "start": { "dateTime": AT_NINE, "timeZone": "America/Los_Angeles" }, "end": { "dateTime": AT_TEN } }),
    )
    .await
    .unwrap();
    // And the other way round.
    patch(json!({ "start": { "date": "2026-10-12" }, "end": { "date": "2026-10-13" } }))
        .await
        .unwrap();
    // Adding a Meet link to a meeting that had none.
    patch(json!({ "createMeetLink": true, "sendUpdates": "externalOnly" }))
        .await
        .unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(query_of(&received[0]), json!({}), "nobody is emailed unless asked");
    assert_eq!(
        body_of(&received[0]),
        json!({ "start": { "dateTime": AT_NINE, "timeZone": "America/Los_Angeles", "date": null }, "end": { "dateTime": AT_TEN, "date": null } })
    );
    assert_eq!(
        body_of(&received[1]),
        json!({ "start": { "date": "2026-10-12", "dateTime": null }, "end": { "date": "2026-10-13", "dateTime": null } })
    );
    assert_eq!(
        query_of(&received[2]),
        json!({ "sendUpdates": "externalOnly", "conferenceDataVersion": "1" })
    );
    let third = body_of(&received[2]);
    assert_eq!(
        third["conferenceData"]["createRequest"]["conferenceSolutionKey"]["type"],
        "hangoutsMeet"
    );
    assert_eq!(third.as_object().unwrap().len(), 1, "and nothing else");

    for bad in [
        json!({ "start": { "timeZone": "Europe/Zurich" } }),
        json!({ "end": { "dateTime": "2026-10-12T10:00:00" } }),
        json!({ "attendees": [{ "email": "" }] }),
    ] {
        let err = patch(bad.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad}");
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 3);
}

#[tokio::test]
async fn a_new_guest_list_keeps_everyone_who_stays_as_they_were_and_is_not_written_over_a_change() {
    let (server, socket, key) = google().await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
    mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
    // Grace stays, named in another case and with nothing said of her answer;
    // Ada, the organiser, is made optional; Alan is new.
    let guests = json!([{ "email": "GRACE@example.test" }, { "email": "ada@example.test", "optional": true }, { "email": "alan@example.test" }]);
    invoke(
        &socket,
        &key,
        "calendar_events.patch",
        on_the_event(json!({ "attendees": guests, "location": "Room 5" })),
    )
    .await
    .unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2, "the event is read, then changed");
    assert_eq!(received[0].method.as_str(), "GET");
    let write = &received[1];
    // Google replaces the list with what it is sent. A guest sent bare would
    // lose their answer and whoever they were bringing.
    assert_eq!(
        body_of(write),
        json!({
            "location": "Room 5",
            "attendees": [
                { "email": "GRACE@example.test", "self": true, "responseStatus": "needsAction", "additionalGuests": 1, "futureField": "kept" },
                { "email": "ada@example.test", "organizer": true, "responseStatus": "accepted", "optional": true },
                { "email": "alan@example.test" }
            ]
        })
    );
    // Someone added between the read and the write is not uninvited by a
    // list written before they were on it: Google refuses the stale version.
    assert_eq!(write.headers.get("if-match").unwrap(), "\"111\"");

    // A change that names no guests reads nothing and names no version.
    let (server, socket, key) = google().await;
    mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_event()).await;
    invoke(
        &socket,
        &key,
        "calendar_events.patch",
        on_the_event(json!({ "location": "Room 5" })),
    )
    .await
    .unwrap();
    let only = only_request(&server).await;
    assert!(only.headers.get("if-match").is_none());
}

#[tokio::test]
async fn a_guest_list_is_not_replaced_when_it_cannot_be_read_whole_or_has_changed() {
    // Only part of the list is shown on this calendar. Sent back, a new list
    // would uninvite everyone who was not shown.
    let mut cut_short = calendar_invitation();
    cut_short["attendeesOmitted"] = json!(true);
    let mut unversioned = calendar_invitation();
    unversioned.as_object_mut().unwrap().remove("etag");
    for (read, kind) in [
        (calendar_hidden_invitation(), ErrorKind::InvalidInput),
        (cut_short, ErrorKind::InvalidInput),
        (unversioned, ErrorKind::Decode),
    ] {
        let (server, socket, key) = google().await;
        mount(&server, "GET", CALENDAR_EVENT_PATH, 200, read).await;
        let err = invoke(
            &socket,
            &key,
            "calendar_events.patch",
            on_the_event(json!({ "attendees": [{ "email": "alan@example.test" }] })),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(!err.message().contains("grace"), "{err}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "read, and not written"
        );
    }

    // The event changed after it was read: Google answers 412, and the list is not sent again.
    let (server, socket, key) = google().await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
    Mock::given(method("PATCH"))
        .respond_with(google_error(412, "conditionNotMet", "Precondition Failed"))
        .mount(&server)
        .await;
    let err = invoke(
        &socket,
        &key,
        "calendar_events.patch",
        on_the_event(json!({ "attendees": [{ "email": "alan@example.test" }] })),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().contains("changed first"), "{err}");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

// ── Answering an invitation ──────────────────────────────────────────────────

#[tokio::test]
async fn answering_an_invitation_changes_only_the_signed_in_attendee() {
    let (server, socket, key) = google().await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
    mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
    invoke(
        &socket,
        &key,
        "calendar_events.respond",
        on_the_event(json!({ "responseStatus": "declined" })),
    )
    .await
    .unwrap();

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
    assert_eq!(received[0].method.as_str(), "GET");
    assert_eq!(query_of(&received[0]), json!({}), "the whole guest list is read");
    let write = &received[1];
    assert_eq!(write.method.as_str(), "PATCH");
    assert_eq!(query_of(write), json!({}), "nobody is emailed unless asked");
    // Google replaces the whole list, so everyone else goes back exactly as
    // they came, including what these types do not describe.
    let mut expected = calendar_invitation()["attendees"].clone();
    expected[1]["responseStatus"] = json!("declined");
    let sent = body_of(write);
    assert_eq!(sent, json!({ "attendees": expected }));
    assert_eq!(sent["attendees"][1]["futureField"], "kept");
    assert_eq!(
        sent["attendees"][0],
        calendar_invitation()["attendees"][0],
        "Ada is as she was"
    );
    // If the guest list changed between the read and the write, Google refuses
    // the write instead of silently dropping the newcomer.
    assert_eq!(write.headers.get("if-match").unwrap(), "\"111\"");
}

#[tokio::test]
async fn who_is_told_of_an_answer_is_the_callers_choice() {
    for (send_updates, sent) in [
        (json!("all"), json!({ "sendUpdates": "all" })),
        (json!("externalOnly"), json!({ "sendUpdates": "externalOnly" })),
        (json!("none"), json!({ "sendUpdates": "none" })),
        (json!(null), json!({})),
    ] {
        let (server, socket, key) = google().await;
        mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
        mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
        let input = on_the_event(json!({ "responseStatus": "accepted", "sendUpdates": send_updates }));
        invoke(&socket, &key, "calendar_events.respond", input).await.unwrap();
        let received = server.received_requests().await.unwrap();
        assert_eq!(query_of(&received[0]), json!({}), "the read tells nobody anything");
        assert_eq!(query_of(&received[1]), sent);
        assert!(
            body_of(&received[1]).get("sendUpdates").is_none(),
            "it is a parameter, not a field of the event"
        );
    }
}

#[tokio::test]
async fn an_answer_on_a_hidden_guest_list_never_stands_for_the_whole_list() {
    // Google shows a guest only their own entry when the organiser hid the
    // list, and says so when it cut a list short. Sent back as the list,
    // either would uninvite everyone else.
    let truncated = {
        let mut event = calendar_invitation();
        event["attendeesOmitted"] = json!(true);
        event["attendees"].as_array_mut().unwrap().remove(0);
        event
    };
    for (current, what) in [(calendar_hidden_invitation(), "hidden"), (truncated, "cut short")] {
        let (server, socket, key) = google().await;
        mount(&server, "GET", CALENDAR_EVENT_PATH, 200, current.clone()).await;
        mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, current.clone()).await;
        let input = on_the_event(json!({ "responseStatus": "accepted", "comment": "On my way", "sendUpdates": "all" }));
        invoke(&socket, &key, "calendar_events.respond", input).await.unwrap();

        let received = server.received_requests().await.unwrap();
        let write = &received[1];
        assert_eq!(write.method.as_str(), "PATCH", "{what}");
        assert_eq!(query_of(write), json!({ "sendUpdates": "all" }), "{what}");
        let sent = body_of(write);
        // This is what tells Google the list is not the whole of it.
        assert_eq!(sent["attendeesOmitted"], true, "{what}");
        let attendees = sent["attendees"].as_array().unwrap();
        assert_eq!(attendees.len(), 1, "{what}: only the answer");
        assert_eq!(attendees[0]["email"], "grace@example.test", "{what}");
        assert_eq!(attendees[0]["responseStatus"], "accepted", "{what}");
        assert_eq!(attendees[0]["comment"], "On my way", "{what}");
        assert_eq!(
            sent.as_object().unwrap().len(),
            2,
            "{what}: nothing else of the event is touched: {sent}"
        );
        // Nothing but the answer is written, so a change made meanwhile is not at risk.
        assert!(write.headers.get("if-match").is_none(), "{what}");
    }

    // A hidden list the calendar's owner is not on cannot be answered at all.
    let mut uninvited = calendar_hidden_invitation();
    uninvited["attendees"] = json!([]);
    let (server, socket, key) = google().await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, uninvited).await;
    mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_hidden_invitation()).await;
    let err = invoke(
        &socket,
        &key,
        "calendar_events.respond",
        on_the_event(json!({ "responseStatus": "accepted" })),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(only_request(&server).await.method.as_str(), "GET");
}

#[tokio::test]
async fn an_answer_that_cannot_be_given_safely_is_refused_before_anything_is_written() {
    let not_invited = {
        let mut event = calendar_invitation();
        event["attendees"][1]["self"] = json!(false);
        event
    };
    let no_guests = {
        let mut event = calendar_invitation();
        event.as_object_mut().unwrap().remove("attendees");
        event
    };
    let unversioned = {
        let mut event = calendar_invitation();
        event.as_object_mut().unwrap().remove("etag");
        event
    };
    for (current, kind, says) in [
        (not_invited, ErrorKind::InvalidInput, "not invited"),
        (no_guests, ErrorKind::InvalidInput, "not invited"),
        // A success that is not the event says nothing about who is invited.
        (json!({}), ErrorKind::Decode, "without an event"),
        (json!({ "kind": "calendar#event" }), ErrorKind::Decode, "no id"),
        // Without the version it read, the write could not be made conditional.
        (unversioned, ErrorKind::Decode, "without its version"),
    ] {
        let (server, socket, key) = google().await;
        mount(&server, "GET", CALENDAR_EVENT_PATH, 200, current).await;
        mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
        let err = invoke(
            &socket,
            &key,
            "calendar_events.respond",
            on_the_event(json!({ "responseStatus": "accepted" })),
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
        let err = invoke(
            &socket,
            &key,
            "calendar_events.respond",
            on_the_event(json!({ "responseStatus": status })),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{status:?}");
        assert!(err.message().contains("needsAction"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_event_that_changed_while_it_was_being_answered_is_reported_and_not_overwritten() {
    let (server, socket, key) = google().await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
    Mock::given(method("PATCH"))
        .and(path(CALENDAR_EVENT_PATH))
        .respond_with(google_error(412, "conditionNotMet", "Precondition Failed"))
        .mount(&server)
        .await;
    let err = invoke(
        &socket,
        &key,
        "calendar_events.respond",
        on_the_event(json!({ "responseStatus": "accepted" })),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().contains("changed first"), "{}", err.message());
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        2,
        "the stale write is not sent again"
    );
}

#[tokio::test]
async fn an_answer_on_another_persons_calendar_is_that_persons_answer() {
    // Google marks as `self` the entry of the calendar the event was read
    // from, not of whoever is signed in. Ada, managing Grace's calendar,
    // answers for Grace.
    let (server, socket, key) = google().await;
    let at = "/calendar/v3/calendars/grace%40example.test/events/evt1";
    mount(&server, "GET", at, 200, calendar_invitation()).await;
    mount(&server, "PATCH", at, 200, calendar_invitation()).await;
    invoke(
        &socket,
        &key,
        "calendar_events.respond",
        json!({ "calendar": "grace@example.test", "event": "evt1", "responseStatus": "tentative" }),
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

// ── What Google answers ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_success_that_does_not_carry_the_result_is_an_error() {
    let one = on_the_event(json!({}));
    let window = one_day_of(json!(["primary"]));
    let listing = json!({ "calendar": "primary" });
    let checks = [
        ("calendar_events.get", one.clone(), json!({})),
        ("calendar_events.get", one.clone(), json!(null)),
        ("calendar_events.get", one.clone(), json!({ "summary": "no id" })),
        (
            "calendar_events.get",
            one.clone(),
            json!({ "kind": "calendar#event", "summary": "no id" }),
        ),
        // A list is not an event.
        ("calendar_events.get", one.clone(), calendar_events_page(json!([]))),
        (
            "calendar_events.insert",
            timed_event(),
            json!({ "status": "confirmed" }),
        ),
        (
            "calendar_events.patch",
            on_the_event(json!({ "summary": "x" })),
            json!({ "kind": "calendar#event", "id": "" }),
        ),
        ("calendar_events.list", listing.clone(), json!({})),
        ("calendar_events.list", listing.clone(), json!({ "items": [] })),
        ("calendar_events.list", listing.clone(), json!([calendar_event()])),
        (
            "calendar_events.list",
            listing.clone(),
            json!({ "kind": "calendar#events", "items": "none" }),
        ),
        (
            "calendar_events.list",
            listing.clone(),
            json!({ "kind": "calendar#events", "items": [{ "summary": "no id" }] }),
        ),
        ("calendar_events.instances", one.clone(), calendar_event()),
        (
            "calendar_list.list",
            json!({}),
            json!({ "kind": "calendar#events", "items": [] }),
        ),
        (
            "calendar_list.list",
            json!({}),
            json!({ "kind": "calendar#calendarList", "items": [{ "summary": "no id" }] }),
        ),
        (
            "calendar_list.get",
            json!({ "calendar": "primary" }),
            json!({ "summary": "no id" }),
        ),
        (
            "calendar_list.get",
            json!({ "calendar": "primary" }),
            json!({ "kind": "calendar#calendarListEntry", "summary": "no id" }),
        ),
        ("calendar_freebusy.query", window.clone(), json!({})),
        (
            "calendar_freebusy.query",
            window.clone(),
            json!({ "kind": "calendar#freeBusy" }),
        ),
        // Asked about one calendar and told about none: that is not "free all day".
        (
            "calendar_freebusy.query",
            window,
            json!({ "kind": "calendar#freeBusy", "calendars": {} }),
        ),
    ];
    for (operation, input, body) in checks {
        let (_server, socket, key) = answering(200, body.clone()).await;
        let err = invoke(&socket, &key, operation, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{operation} given {body}: {err}");
    }
}

#[tokio::test]
async fn busy_times_that_cannot_be_read_do_not_name_the_person_whose_calendar_it_is() {
    // Google keys the answer by each calendar's id, which is its owner's address.
    let answer = json!({
        "kind": "calendar#freeBusy",
        "calendars": { "grace@example.test": { "busy": [{ "start": "2026-10-12T09:00:00Z" }, { "start": 9 }] } }
    });
    let (_server, socket, key) = answering(200, answer).await;
    let input = json!({ "calendars": ["grace@example.test"], "timeMin": ONE_DAY[0], "timeMax": ONE_DAY[1] });
    let err = invoke(&socket, &key, "calendar_freebusy.query", input)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(
        err.message().ends_with("at `calendars.*.busy[1].start`"),
        "{}",
        err.message()
    );
    assert!(!format!("{err} {err:?}").contains("grace"), "{err:?}");
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_field_and_never_repeats_what_google_sent() {
    let private = "PRIVATE notes of the meeting";
    let mut unreadable = calendar_event();
    unreadable["description"] = json!(private);
    unreadable["attendees"][1]["email"] = json!({ "address": private });
    let mut mistimed = calendar_event();
    mistimed["summary"] = json!(private);
    mistimed["start"] = json!(private);
    let mut unbusy = freebusy();
    unbusy["calendars"]["primary"]["busy"] = json!(private);
    let checks = [
        (
            "calendar_events.get",
            on_the_event(json!({})),
            unreadable,
            "attendees[1].email",
        ),
        (
            "calendar_events.list",
            json!({ "calendar": "primary" }),
            calendar_events_page(json!([calendar_event(), mistimed])),
            "[1].start",
        ),
        (
            "calendar_list.get",
            json!({ "calendar": "primary" }),
            json!({ "kind": "calendar#calendarListEntry", "id": "ada@example.test", "summary": private, "primary": private }),
            "primary",
        ),
        (
            "calendar_freebusy.query",
            one_day_of(json!(["primary"])),
            unbusy,
            "calendars.primary.busy",
        ),
    ];
    for (operation, input, body, place) in checks {
        let (_server, socket, key) = answering(200, body).await;
        let err = invoke(&socket, &key, operation, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{operation}: {err}");
        assert!(err.message().contains(place), "{operation}: {}", err.message());
        let everything = format!("{err} {err:?} {}", serde_json::to_string(&err.to_wire()).unwrap());
        assert!(!everything.contains("PRIVATE"), "{operation}: {everything}");
    }
}

#[tokio::test]
async fn googles_refusals_arrive_as_the_right_kind_of_error() {
    let one = on_the_event(json!({}));
    let checks = [
        (
            "calendar_events.get",
            one.clone(),
            google_error(404, "notFound", "Not Found"),
            ErrorKind::NotFound,
            "",
        ),
        // An event that was already deleted. Google says no action is needed; the caller is told it is gone.
        (
            "calendar_events.delete",
            one.clone(),
            google_error(410, "deleted", "Resource has been deleted"),
            ErrorKind::NotFound,
            "no longer has",
        ),
        (
            "calendar_events.get",
            one.clone(),
            google_error(410, "deleted", "Resource has been deleted"),
            ErrorKind::NotFound,
            "no longer has",
        ),
        // The same status for something else entirely: nothing is gone, the question was too old to answer.
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "updatedMin": "2020-01-01T00:00:00Z" }),
            google_error(
                410,
                "updatedMinTooLongAgo",
                "The requested minimum modification time lies too far in the past.",
            ),
            ErrorKind::InvalidInput,
            "too far in the past",
        ),
        (
            "calendar_events.list",
            json!({ "calendar": "primary", "timeMin": "2026-10-13T00:00:00Z", "timeMax": "2026-10-12T00:00:00Z" }),
            google_error(400, "timeRangeEmpty", "The specified time range is empty."),
            ErrorKind::InvalidInput,
            "The specified time range is empty.",
        ),
        (
            "calendar_events.patch",
            on_the_event(json!({ "summary": "x" })),
            google_error(403, "forbidden", "Forbidden"),
            ErrorKind::AccessDenied,
            "Forbidden",
        ),
        // A guest who changes what only the organiser may.
        (
            "calendar_events.patch",
            on_the_event(json!({ "summary": "x" })),
            google_error(
                403,
                "forbiddenForNonOrganizer",
                "Shared properties can only be changed by the organizer of the event.",
            ),
            ErrorKind::AccessDenied,
            "organizer",
        ),
        // A token without the calendar scopes.
        (
            "calendar_list.list",
            json!({}),
            google_error(
                403,
                "insufficientPermissions",
                "Request had insufficient authentication scopes.",
            ),
            ErrorKind::AccessDenied,
            "insufficient authentication scopes",
        ),
        // Google's abuse limit is not something waiting a moment fixes.
        (
            "calendar_events.insert",
            timed_event(),
            google_error(403, "quotaExceeded", "Calendar usage limits exceeded."),
            ErrorKind::AccessDenied,
            "Calendar usage limits exceeded.",
        ),
        (
            "calendar_freebusy.query",
            one_day_of(json!(["primary"])),
            google_error(401, "authError", "Invalid Credentials"),
            ErrorKind::ReconnectRequired,
            "",
        ),
    ];
    for (operation, input, refusal, kind, says) in checks {
        let (server, socket, key) = google().await;
        Mock::given(socketkit_testkit::wiremock::matchers::any())
            .respond_with(refusal)
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, operation, input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{operation}: {err}");
        assert!(err.to_string().contains(says), "{operation}: {err}");
    }
}

#[tokio::test]
async fn deleting_an_event_that_is_already_gone_says_so_and_is_not_asked_again() {
    let (server, socket, key) = google().await;
    Mock::given(method("DELETE"))
        .and(path(CALENDAR_EVENT_PATH))
        .respond_with(google_error(410, "deleted", "Resource has been deleted"))
        .mount(&server)
        .await;
    let connection = socket.connection(key).await.unwrap();
    let err = Google::new()
        .calendar_events(&connection)
        .delete("primary", "evt1", EventDelete::default())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(err.retry(), Retry::Never, "it will not come back");
    let request = only_request(&server).await;
    assert_eq!(query_of(&request), json!({}), "nobody is emailed unless asked");
    assert_eq!(body_of(&request), Value::Null);
}

#[tokio::test]
async fn a_throttle_google_reports_as_403_is_a_rate_limit_and_not_a_refusal() {
    for reason in ["rateLimitExceeded", "userRateLimitExceeded"] {
        let (server, socket, key) = google().await;
        Mock::given(path(CALENDAR_EVENTS_PATH))
            .respond_with(google_error(403, reason, "Rate Limit Exceeded"))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, "calendar_events.insert", timed_event())
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
    Mock::given(path(CALENDAR_EVENT_PATH))
        .respond_with(
            google_error(403, "rateLimitExceeded", "Rate Limit Exceeded").insert_header("retry-after", "3600"),
        )
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "calendar_events.get", on_the_event(json!({})))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(err.retry(), Retry::After(std::time::Duration::from_secs(3600)));
}

#[tokio::test]
async fn a_change_is_not_sent_again_after_google_fails() {
    let failed = || google_error(502, "backendError", "Backend Error");
    let writes = [
        ("calendar_events.insert", timed_event()),
        ("calendar_events.patch", on_the_event(json!({ "summary": "x" }))),
    ];
    for (operation, input) in writes {
        let (server, socket, key) = google().await;
        Mock::given(socketkit_testkit::wiremock::matchers::any())
            .respond_with(failed())
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, operation, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{operation}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{operation}: it may have happened, so it is not repeated"
        );
    }

    // Answering reads first; the read succeeds and the write is sent once.
    let (server, socket, key) = google().await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_invitation()).await;
    Mock::given(method("PATCH"))
        .and(path(CALENDAR_EVENT_PATH))
        .respond_with(failed())
        .mount(&server)
        .await;
    invoke(
        &socket,
        &key,
        "calendar_events.respond",
        on_the_event(json!({ "responseStatus": "accepted" })),
    )
    .await
    .unwrap_err();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_read_is_tried_again_after_google_fails_except_the_one_google_takes_as_a_post() {
    let failed = || google_error(503, "backendError", "Backend Error");
    let (server, socket, key) = google().await;
    Mock::given(path(CALENDAR_EVENTS_PATH))
        .respond_with(failed())
        .mount(&server)
        .await;
    invoke(&socket, &key, "calendar_events.list", json!({ "calendar": "primary" }))
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
    Mock::given(path(FREEBUSY_PATH))
        .respond_with(failed())
        .mount(&server)
        .await;
    invoke(&socket, &key, "calendar_freebusy.query", one_day_of(json!(["primary"])))
        .await
        .unwrap_err();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let (server, socket, key) = google().await;
    mount(&server, "POST", CALENDAR_EVENTS_PATH, 200, calendar_event()).await;
    mount(&server, "PATCH", CALENDAR_EVENT_PATH, 200, calendar_event()).await;
    mount(&server, "GET", CALENDAR_EVENT_PATH, 200, calendar_event()).await;
    let mut page = calendar_events_page(json!([calendar_event()]));
    page["nextPageToken"] = json!("page-2");
    mount(&server, "GET", CALENDAR_EVENTS_PATH, 200, page).await;
    let listing = json!({ "kind": "calendar#calendarList", "items": [calendar_entry()] });
    mount(&server, "GET", CALENDAR_LIST_PATH, 200, listing).await;
    mount(&server, "POST", FREEBUSY_PATH, 200, freebusy()).await;
    Mock::given(method("DELETE"))
        .and(path(CALENDAR_EVENT_PATH))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let connection = socket.connection(key).await.unwrap();
    let google = Google::new();
    let events = google.calendar_events(&connection);

    let calendars = google
        .calendar_list(&connection)
        .list(Default::default(), Paging::default())
        .await
        .unwrap();
    assert!(calendars.items[0].primary);
    assert_eq!(calendars.items[0].id, "ada@example.test");

    let created = events
        .insert(
            "primary",
            EventInsert::between(EventTime::at(AT_NINE), EventTime::at(AT_TEN))
                .summary("Design review")
                .invite("grace@example.test")
                .with_meet_link(),
        )
        .await
        .unwrap();
    assert_eq!(created.id, "evt1");
    assert_eq!(created.hangout_link.as_deref(), Some(MEET_LINK));

    let changes = EventPatch {
        location: Some("Room 5".into()),
        ..EventPatch::default()
    };
    events.patch("primary", &created.id, changes).await.unwrap();
    // Ada organises the meeting and is on its guest list, so she can answer too.
    events
        .respond(
            "primary",
            &created.id,
            EventResponse::tentative().comment("Might be late"),
        )
        .await
        .unwrap();

    let week = EventFilter {
        time_min: Some("2026-10-12T00:00:00Z".into()),
        time_max: Some("2026-10-19T00:00:00Z".into()),
        single_events: Some(true),
        order_by: Some("startTime".into()),
        ..EventFilter::default()
    };
    let paging = Paging {
        limit: Some(50),
        ..Paging::default()
    };
    let page = events.list("primary", week, paging).await.unwrap();
    assert_eq!(page.items[0].summary.as_deref(), Some("Design review"));
    assert_eq!(page.next_cursor.as_deref(), Some("page-2"));

    let busy = google
        .calendar_freebusy(&connection)
        .query(&["primary".to_owned()], FreeBusyQuery::between(ONE_DAY[0], ONE_DAY[1]))
        .await
        .unwrap();
    assert_eq!(busy.calendars["primary"].busy[0].start, "2026-10-12T18:00:00+02:00");

    events
        .delete("primary", &created.id, EventDelete::default())
        .await
        .unwrap();

    let received = server.received_requests().await.unwrap();
    let sent: Vec<(String, Value)> = received
        .iter()
        .map(|r| (format!("{} {}", r.method, r.url.path()), body_of(r)))
        .collect();
    assert_eq!(sent[0], (format!("GET {CALENDAR_LIST_PATH}"), Value::Null));
    assert_eq!(sent[1].0, format!("POST {CALENDAR_EVENTS_PATH}"));
    assert_eq!(sent[1].1["summary"], "Design review");
    assert_eq!(sent[1].1["attendees"], json!([{ "email": "grace@example.test" }]));
    assert_eq!(query_of(&received[1]), json!({ "conferenceDataVersion": "1" }));
    assert_eq!(
        sent[2],
        (format!("PATCH {CALENDAR_EVENT_PATH}"), json!({ "location": "Room 5" }))
    );
    assert_eq!(sent[3].0, format!("GET {CALENDAR_EVENT_PATH}"));
    assert_eq!(sent[4].0, format!("PATCH {CALENDAR_EVENT_PATH}"));
    assert_eq!(sent[4].1["attendees"][0]["responseStatus"], "tentative");
    assert_eq!(sent[4].1["attendees"][0]["comment"], "Might be late");
    assert_eq!(
        sent[4].1["attendees"][1]["responseStatus"], "needsAction",
        "Grace's answer is hers"
    );
    assert_eq!(sent[5].0, format!("GET {CALENDAR_EVENTS_PATH}"));
    assert_eq!(
        query_of(&received[5]),
        json!({ "timeMin": "2026-10-12T00:00:00Z", "timeMax": "2026-10-19T00:00:00Z", "singleEvents": "true", "orderBy": "startTime", "maxResults": "50" })
    );
    assert_eq!(sent[6].0, format!("POST {FREEBUSY_PATH}"));
    assert_eq!(sent[7], (format!("DELETE {CALENDAR_EVENT_PATH}"), Value::Null));
}
