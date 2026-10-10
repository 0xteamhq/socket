//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "event": "AAMk…", "subject": "Review" }`.
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

use crate::Microsoft;
use crate::models::{
    Calendar, CancelEvent, CreateEvent, Event, EventResponse, FindMeetingTimes, GetSchedule, MeetingTimeSuggestions,
    Paging, RespondToEvent, ScheduleInformation, UpdateEvent,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Microsoft, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, microsoft: Microsoft, connection: Connection, input: Value) -> Running {
        (self.run)(microsoft, connection, input)
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidInput, message)
}

/// Builds an operation from a typed handler. The input type gives the input
/// schema and the parsing; the output type gives the output schema.
fn operation<I, O, F, Fut>(name: &str, description: &str, effect: Effect, scopes: &[&str], handler: F) -> Operation
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + JsonSchema + 'static,
    F: Fn(Microsoft, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("microsoft.{name}"),
        description: description.to_owned(),
        input_schema: schema_of::<I>(),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let run = move |microsoft: Microsoft, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // the text of an event or a credential. Only the field's name,
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
                let output = handler(microsoft, connection, input);
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

input!(Listing {} + Paging);
input!(OneCalendar {
    /// A calendar id.
    calendar: String
});
input!(
    Between {
        /// The start of the range, in ISO 8601: `2026-10-12T00:00:00Z`. A time without an offset is in UTC.
        start: String,
        /// The end of the range, in ISO 8601.
        end: String,
        /// A calendar id. The account's default calendar when not given.
        calendar: Option<String>
    } + Paging
);
input!(OneEvent {
    /// An event id.
    event: String
});
input!(
    Instances {
        /// The id of a repeating event's series.
        event: String,
        /// The start of the range, in ISO 8601.
        start: String,
        /// The end of the range, in ISO 8601.
        end: String
    } + Paging
);
input!(MeetingTimes {} + FindMeetingTimes);
input!(Schedule {} + GetSchedule);
input!(
    Create {
        /// A calendar id. The account's default calendar when not given.
        calendar: Option<String>
    } + CreateEvent
);
input!(
    Update {
        /// The id of the event to change.
        event: String
    } + UpdateEvent
);
input!(
    Respond {
        /// The id of the event that was invited to.
        event: String,
        /// `accept`, `tentatively_accept` or `decline`.
        response: EventResponse
    } + RespondToEvent
);
input!(
    Cancel {
        /// The id of a meeting the account organised.
        event: String
    } + CancelEvent
);

// `Destructive` is anything that deletes, removes or overwrites what was
// there, or that cannot be taken back: an answer to an invitation reaches its
// organiser at once and cannot be unsent. A host uses it to ask a person first.
use Effect::{Destructive, Read, Write};

/// Every Microsoft operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── calendars ──
        operation("calendars.list", "List the account's calendars, its own and those shared with it.", Read, &["Calendars.Read"],
            |m: Microsoft, c: Connection, i: Listing| async move { m.calendars(&c).list(i.options).await as Result<Page<Calendar>> }),
        operation("calendars.get", "Get one calendar.", Read, &["Calendars.Read"],
            |m: Microsoft, c: Connection, i: OneCalendar| async move { m.calendars(&c).get(&i.calendar).await as Result<Calendar> }),

        // ── events ──
        operation("events.list_between", "List the events between two times, with each occurrence of a repeating event as its own event. Times are in UTC.", Read, &["Calendars.Read"],
            |m: Microsoft, c: Connection, i: Between| async move { m.events(&c).list_between(&i.start, &i.end, i.calendar.as_deref(), i.options).await as Result<Page<Event>> }),
        operation("events.get", "Get one event, with its attendees, their answers and the link to join it. Times are in UTC.", Read, &["Calendars.Read"],
            |m: Microsoft, c: Connection, i: OneEvent| async move { m.events(&c).get(&i.event).await as Result<Event> }),
        operation("events.instances", "List the occurrences of a repeating event between two times.", Read, &["Calendars.Read"],
            |m: Microsoft, c: Connection, i: Instances| async move { m.events(&c).instances(&i.event, &i.start, &i.end, i.options).await as Result<Page<Event>> }),
        // Graph offers these two only as POST. They read calendars and change nothing.
        operation("events.find_meeting_times", "Suggest times when a meeting could be held, from the attendees' calendars. Changes nothing. Work and school accounts only.", Read, &["Calendars.Read.Shared"],
            |m: Microsoft, c: Connection, i: MeetingTimes| async move { m.events(&c).find_meeting_times(i.options).await as Result<MeetingTimeSuggestions> }),
        operation("events.schedule", "Read when people, distribution lists and rooms are free and busy. Changes nothing. Work and school accounts only.", Read, &["Calendars.Read"],
            |m: Microsoft, c: Connection, i: Schedule| async move { m.events(&c).schedule(i.options).await as Result<Vec<ScheduleInformation>> }),
        operation("events.create", "Create an event and invite its attendees. Set isOnlineMeeting for a Teams meeting; the event returned carries the link to join it.", Write, &["Calendars.ReadWrite"],
            |m: Microsoft, c: Connection, i: Create| async move { m.events(&c).create(i.calendar.as_deref(), i.options).await as Result<Event> }),
        operation("events.update", "Change an event, replacing the fields given and leaving the rest. Attendees left out of a new list are removed, and attendees are told of the change.", Destructive, &["Calendars.ReadWrite"],
            |m: Microsoft, c: Connection, i: Update| async move { m.events(&c).update(&i.event, i.options).await as Result<Event> }),
        operation("events.respond", "Answer an invitation: accept it, accept it tentatively, or decline it. The organiser is told, and the answer cannot be taken back.", Destructive, &["Calendars.ReadWrite"],
            |m: Microsoft, c: Connection, i: Respond| async move { m.events(&c).respond(&i.event, i.response, i.options).await as Result<()> }),
        operation("events.cancel", "Cancel a meeting the account organised, and tell its attendees.", Destructive, &["Calendars.ReadWrite"],
            |m: Microsoft, c: Connection, i: Cancel| async move { m.events(&c).cancel(&i.event, i.options).await as Result<()> }),
        operation("events.delete", "Delete an event from the account's calendar. Deleting a meeting the account organised cancels it for its attendees.", Destructive, &["Calendars.ReadWrite"],
            |m: Microsoft, c: Connection, i: OneEvent| async move { m.events(&c).delete(&i.event).await as Result<()> }),
    ]
}
