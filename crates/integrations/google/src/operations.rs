//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side, under Google's own names:
//! `{ "calendarId": "primary", "timeMin": "…", "singleEvents": true }`. Both
//! schemas are generated from the same types the typed methods use, so the
//! two ways of calling cannot drift apart.

use std::future::Future;
use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::Serialize;
use serde::de::DeserializeOwned;
use socketkit_core::operation_input as input;
use socketkit_core::{Connection, Effect, Page, Result, TypedOperation, typed_operation};

use crate::models::{
    CalendarListEntry, DeleteEvent, Event, FreeBusy, FreeBusyQuery, InsertEvent, Instances, ListCalendars, ListEvents,
    PatchEvent, Respond,
};
use crate::{CALENDAR_EVENTS_SCOPE, CALENDAR_READONLY_SCOPE, Google};

/// One Google operation.
pub(crate) type Operation = TypedOperation<Google>;

/// Builds an operation named `google.<name>` from a typed handler.
fn operation<I, O, F, Fut>(name: &str, description: &str, effect: Effect, scopes: &[&str], handler: F) -> Operation
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(Google, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    typed_operation(format!("google.{name}"), description, effect, scopes, handler)
}

input!(CalendarListing {} + ListCalendars);
input!(OneCalendar {
    /// A calendar's id, or `primary` for the signed-in person's own calendar.
    #[serde(rename = "calendarId")]
    calendar_id: String
});
input!(
    EventListing {
        /// A calendar's id, or `primary` for the signed-in person's own calendar.
        #[serde(rename = "calendarId")]
        calendar_id: String
    } + ListEvents
);
input!(OneEvent {
    /// A calendar's id, or `primary` for the signed-in person's own calendar.
    #[serde(rename = "calendarId")]
    calendar_id: String,
    #[serde(rename = "eventId")]
    event_id: String
});
input!(
    EventInstances {
        #[serde(rename = "calendarId")]
        calendar_id: String,
        /// The id of the recurring event.
        #[serde(rename = "eventId")]
        event_id: String
    } + Instances
);
input!(
    EventInsert {
        /// The calendar to put the event on, or `primary` for the signed-in person's own.
        #[serde(rename = "calendarId")]
        calendar_id: String
    } + InsertEvent
);
input!(
    EventPatch {
        #[serde(rename = "calendarId")]
        calendar_id: String,
        #[serde(rename = "eventId")]
        event_id: String
    } + PatchEvent
);
input!(
    EventRespond {
        /// The calendar the invitation is on. `primary` answers for the signed-in
        /// person; another calendar's id answers for that calendar's owner.
        #[serde(rename = "calendarId")]
        calendar_id: String,
        #[serde(rename = "eventId")]
        event_id: String
    } + Respond
);
input!(
    EventDelete {
        #[serde(rename = "calendarId")]
        calendar_id: String,
        #[serde(rename = "eventId")]
        event_id: String
    } + DeleteEvent
);
input!(
    Availability {
        /// The calendars to check: ids, or `primary`. A person's calendar id is their email address.
        #[serde(rename = "calendarIds")]
        calendar_ids: Vec<String>
    } + FreeBusyQuery
);

// `Destructive` is what deletes. A host may let a `Read` run unasked and ask a
// person before anything else, so nothing that changes Google is a `Read`.
use Effect::{Destructive, Read, Write};

/// What covers every Calendar read, and what covers changing events. Neither
/// is among the provider's default scopes.
const READS: &[&str] = &[CALENDAR_READONLY_SCOPE];
const WRITES: &[&str] = &[CALENDAR_EVENTS_SCOPE];

/// Every Google operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── calendar list ──
        operation("calendar_list.list", "List the calendars on the signed-in person's calendar list.", Read, READS,
            |g: Google, c: Connection, i: CalendarListing| async move { g.calendar_list(&c).list(i.options).await as Result<Page<CalendarListEntry>> }),
        operation("calendar_list.get", "Get one calendar from the signed-in person's calendar list.", Read, READS,
            |g: Google, c: Connection, i: OneCalendar| async move { g.calendar_list(&c).get(&i.calendar_id).await as Result<CalendarListEntry> }),

        // ── events ──
        operation("calendar_events.list", "List a calendar's events: inside a time window, matching free text, with recurring events expanded when singleEvents is set.", Read, READS,
            |g: Google, c: Connection, i: EventListing| async move { g.calendar_events(&c).list(&i.calendar_id, i.options).await as Result<Page<Event>> }),
        operation("calendar_events.get", "Get one event, with its attendees, its meeting link and its attachments.", Read, READS,
            |g: Google, c: Connection, i: OneEvent| async move { g.calendar_events(&c).get(&i.calendar_id, &i.event_id).await as Result<Event> }),
        operation("calendar_events.instances", "List the occurrences of a recurring event.", Read, READS,
            |g: Google, c: Connection, i: EventInstances| async move { g.calendar_events(&c).instances(&i.calendar_id, &i.event_id, i.options).await as Result<Page<Event>> }),
        operation("calendar_events.insert", "Create an event, with a Google Meet link when createMeetLink is set.", Write, WRITES,
            |g: Google, c: Connection, i: EventInsert| async move { g.calendar_events(&c).insert(&i.calendar_id, i.options).await as Result<Event> }),
        operation("calendar_events.patch", "Change an event. Only the fields given are changed; attendees, when given, replace the whole guest list.", Write, WRITES,
            |g: Google, c: Connection, i: EventPatch| async move { g.calendar_events(&c).patch(&i.calendar_id, &i.event_id, i.options).await as Result<Event> }),
        operation("calendar_events.respond", "Answer an invitation on a calendar: accepted, declined, tentative, or needsAction to take an answer back. On primary this is the signed-in person's own answer.", Write, WRITES,
            |g: Google, c: Connection, i: EventRespond| async move { g.calendar_events(&c).respond(&i.calendar_id, &i.event_id, i.options).await as Result<Event> }),
        operation("calendar_events.delete", "Delete an event.", Destructive, WRITES,
            |g: Google, c: Connection, i: EventDelete| async move { g.calendar_events(&c).delete(&i.calendar_id, &i.event_id, i.options).await as Result<()> }),

        // ── free/busy ──
        operation("calendar_freebusy.query", "Find when calendars are busy inside a time window.", Read, READS,
            |g: Google, c: Connection, i: Availability| async move { g.calendar_freebusy(&c).query(&i.calendar_ids, i.options).await as Result<FreeBusy> }),
    ]
}
