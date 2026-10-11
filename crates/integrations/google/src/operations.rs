//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "calendar": "primary", "q": "review" }`.
//! Both schemas are generated from the same types the typed methods use, so
//! the two ways of calling cannot drift apart.

use std::future::Future;
use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use socketkit_core::{Connection, Effect, Error, ErrorKind, OperationInfo, Page, Result, schema_of};

use crate::Google;
use crate::models::Paging;
use crate::scopes;

// ── gmail: types ──

// ── calendar: types ──
use crate::models::{
    CalendarEvent, CalendarListEntry, CalendarListFilter, EventDelete, EventFilter, EventInsert, EventInstancesFilter,
    EventPatch, EventResponse, FreeBusy, FreeBusyQuery,
};

// ── meet: types ──

// ── drive: types ──

// ── docs and sheets: types ──

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Google, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, google: Google, connection: Connection, input: Value) -> Running {
        (self.run)(google, connection, input)
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

/// `schema` with every object in it closed: a field it does not list is not
/// allowed. The schema then says what [`unknown_field`] enforces.
fn closed(mut schema: Value) -> Value {
    fn close(node: &mut Value) {
        match node {
            Value::Object(fields) => {
                if fields.contains_key("properties") {
                    fields.insert("additionalProperties".to_owned(), Value::Bool(false));
                }
                fields.values_mut().for_each(close);
            }
            Value::Array(items) => items.iter_mut().for_each(close),
            _ => {}
        }
    }
    close(&mut schema);
    schema
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// a subject, a zone, the people in copy. The input types cannot refuse one
/// themselves, because their options are flattened into one object.
fn unknown_field(root: &Value, node: &Value, input: &Value) -> Option<(String, String)> {
    // The schema of an object or a list, behind a reference or beside `null`.
    let mut node = node;
    for _ in 0..8 {
        let defined = node["$ref"].as_str().and_then(|name| name.strip_prefix("#/$defs/"));
        let optional = node["anyOf"]
            .as_array()
            .and_then(|arms| arms.iter().find(|arm| arm["type"] != "null"));
        match (defined, optional) {
            (Some(name), _) => node = &root["$defs"][name],
            (None, Some(arm)) => node = arm,
            (None, None) => break,
        }
    }
    let within = |place: String, (inner, name): (String, String)| {
        let joint = if inner.is_empty() || inner.starts_with('[') {
            ""
        } else {
            "."
        };
        (format!("{place}{joint}{inner}"), name)
    };
    match input {
        Value::Object(fields) => {
            let known = node["properties"].as_object()?;
            fields.iter().find_map(|(name, value)| match known.get(name) {
                None => Some((String::new(), name.clone())),
                Some(schema) => unknown_field(root, schema, value).map(|found| within(name.clone(), found)),
            })
        }
        Value::Array(items) => {
            let schema = node.get("items")?;
            items
                .iter()
                .enumerate()
                .find_map(|(at, item)| unknown_field(root, schema, item).map(|found| within(format!("[{at}]"), found)))
        }
        _ => None,
    }
}

/// The refusal for a field that is not known. Its name is the caller's own
/// text, so it is repeated only when it looks like a name.
fn not_a_field((place, name): (String, String)) -> Error {
    let named = (1..=40).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '$' | '@' | '.' | '-'));
    let joint = if place.is_empty() { "" } else { "." };
    invalid(match (named, place.is_empty()) {
        (true, _) => format!("`{place}{joint}{name}` is not a field of this operation; check its spelling"),
        (false, true) => "the input has a field this operation does not know".to_owned(),
        (false, false) => format!("`{place}` has a field this operation does not know"),
    })
}

/// Builds an operation from a typed handler. The input type gives the input
/// schema and the parsing; the output type gives the output schema.
fn operation<I, O, F, Fut>(name: &str, description: &str, effect: Effect, scopes: &[&str], handler: F) -> Operation
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(Google, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("google.{name}"),
        description: description.to_owned(),
        input_schema: closed(schema_of::<I>()),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let schema = info.input_schema.clone();
    let run = move |google: Google, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        if let Some(found) = unknown_field(&schema, &schema, &input) {
            return Box::pin(std::future::ready(Err(not_a_field(found).with_provider(provider))));
        }
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // the text of a message or a credential. Only the field's name,
                // which comes from our own types, goes into the error.
                let path = e.path().to_string();
                let inner = e.inner().to_string();
                let message = if inner.starts_with("missing field") {
                    inner
                } else if path == "." {
                    "the input has a field of the wrong type".to_owned()
                } else {
                    format!("`{path}` has the wrong type")
                };
                Box::pin(std::future::ready(Err(invalid(message).with_provider(provider))))
            }
            Ok(input) => {
                let output = handler(google, connection, input);
                Box::pin(async move {
                    serde_json::to_value(output.await?).map_err(|e| {
                        Error::new(ErrorKind::Unexpected, "could not encode the result")
                            .with_provider(provider)
                            .with_source(e)
                    })
                })
            }
        }
    };
    Operation {
        info,
        run: Box::new(run),
    }
}

/// Defines an operation's input: its plain arguments, and optionally one
/// options struct whose fields sit beside them.
macro_rules! input {
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? }) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        struct $name { $($(#[$doc])* $field: $kind,)* }
    };
    ($name:ident { $($(#[$doc:meta])* $field:ident : $kind:ty),* $(,)? } + $options:ty) => {
        #[derive(Debug, Deserialize, JsonSchema)]
        struct $name {
            $($(#[$doc])* $field: $kind,)*
            #[serde(flatten)]
            options: $options,
        }
    };
}

// ── gmail: inputs ──

// ── calendar: inputs ──
input!(
    CalendarListing {
        #[serde(flatten)]
        paging: Paging
    } + CalendarListFilter
);
input!(CalendarOne {
    /// A calendar's id, or `primary` for the signed-in person's own calendar.
    calendar: String
});
input!(
    CalendarEventListing {
        /// A calendar's id, or `primary` for the signed-in person's own calendar.
        calendar: String,
        #[serde(flatten)]
        paging: Paging
    } + EventFilter
);
input!(CalendarOneEvent {
    /// A calendar's id, or `primary` for the signed-in person's own calendar.
    calendar: String,
    /// An event's id.
    event: String
});
input!(
    CalendarEventInstances {
        /// A calendar's id, or `primary` for the signed-in person's own calendar.
        calendar: String,
        /// The id of the recurring event.
        event: String,
        #[serde(flatten)]
        paging: Paging
    } + EventInstancesFilter
);
input!(
    CalendarEventInsert {
        /// The calendar to put the event on, or `primary` for the signed-in person's own.
        calendar: String
    } + EventInsert
);
input!(
    CalendarEventPatch {
        /// A calendar's id, or `primary` for the signed-in person's own calendar.
        calendar: String,
        /// The id of the event to change.
        event: String
    } + EventPatch
);
input!(
    CalendarEventRespond {
        /// The calendar the invitation is on. `primary` answers for the signed-in
        /// person; another calendar's id answers for that calendar's owner.
        calendar: String,
        /// The id of the event that was invited to.
        event: String
    } + EventResponse
);
input!(
    CalendarEventDelete {
        /// A calendar's id, or `primary` for the signed-in person's own calendar.
        calendar: String,
        /// The id of the event to delete.
        event: String
    } + EventDelete
);
input!(
    CalendarAvailability {
        /// The calendars to check: ids, or `primary`. A person's calendar id is their email address.
        calendars: Vec<String>
    } + FreeBusyQuery
);

// ── meet: inputs ──

// ── drive: inputs ──

// ── docs and sheets: inputs ──

// `Destructive` is anything that deletes, removes or overwrites what was
// there, or that cannot be taken back: mail that was sent cannot be unsent.
// A host uses it to ask a person first, and may let a `Read` run unasked, so
// nothing that changes anything at Google is a `Read`.
use Effect::{Destructive, Read, Write};

/// Every Google operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── gmail ──

        // ── calendar ──
        operation("calendar_list.list", "List the calendars on the signed-in person's calendar list.", Read, &[scopes::CALENDAR_READONLY],
            |g: Google, c: Connection, i: CalendarListing| async move { g.calendar_list(&c).list(i.options, i.paging).await as Result<Page<CalendarListEntry>> }),
        operation("calendar_list.get", "Get one calendar from the signed-in person's calendar list.", Read, &[scopes::CALENDAR_READONLY],
            |g: Google, c: Connection, i: CalendarOne| async move { g.calendar_list(&c).get(&i.calendar).await as Result<CalendarListEntry> }),
        operation("calendar_events.list", "List a calendar's events: inside a time window, matching free text, with recurring events expanded when singleEvents is set.", Read, &[scopes::CALENDAR_READONLY],
            |g: Google, c: Connection, i: CalendarEventListing| async move { g.calendar_events(&c).list(&i.calendar, i.options, i.paging).await as Result<Page<CalendarEvent>> }),
        operation("calendar_events.get", "Get one event, with its attendees and their answers, its meeting link and its attachments.", Read, &[scopes::CALENDAR_READONLY],
            |g: Google, c: Connection, i: CalendarOneEvent| async move { g.calendar_events(&c).get(&i.calendar, &i.event).await as Result<CalendarEvent> }),
        operation("calendar_events.instances", "List the occurrences of a recurring event.", Read, &[scopes::CALENDAR_READONLY],
            |g: Google, c: Connection, i: CalendarEventInstances| async move { g.calendar_events(&c).instances(&i.calendar, &i.event, i.options, i.paging).await as Result<Page<CalendarEvent>> }),
        operation("calendar_events.insert", "Create an event and invite its attendees, with a Google Meet link when createMeetLink is set.", Write, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventInsert| async move { g.calendar_events(&c).insert(&i.calendar, i.options).await as Result<CalendarEvent> }),
        operation("calendar_events.patch", "Change an event, replacing the fields given and leaving the rest. Attendees, when given, replace the whole guest list: anyone left out is uninvited.", Destructive, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventPatch| async move { g.calendar_events(&c).patch(&i.calendar, &i.event, i.options).await as Result<CalendarEvent> }),
        operation("calendar_events.respond", "Answer an invitation on a calendar: accepted, declined, tentative, or needsAction to take an answer back. On primary this is the signed-in person's own answer. Nobody else on the guest list is changed.", Write, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventRespond| async move { g.calendar_events(&c).respond(&i.calendar, &i.event, i.options).await as Result<CalendarEvent> }),
        operation("calendar_events.delete", "Delete an event. Deleting an event the account organised cancels it for its attendees.", Destructive, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventDelete| async move { g.calendar_events(&c).delete(&i.calendar, &i.event, i.options).await as Result<()> }),
        // Google offers this only as POST. It reads calendars and changes nothing.
        operation("calendar_freebusy.query", "Read when calendars are busy inside a time window. Changes nothing.", Read, &[scopes::CALENDAR_READONLY],
            |g: Google, c: Connection, i: CalendarAvailability| async move { g.calendar_freebusy(&c).query(&i.calendars, i.options).await as Result<FreeBusy> }),

        // ── meet ──

        // ── drive ──

        // ── docs and sheets ──
    ]
}
