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
    Attachment, Calendar, CancelEvent, CreateEvent, DraftMessage, Event, EventResponse, FindMeetingTimes, GetMessage,
    GetSchedule, ListFolders, ListMessages, MailFolder, MeetingTimeSuggestions, Message, Paging, ReplyContent,
    RespondToEvent, ScheduleInformation, SendMail, UpdateEvent, UpdateMessage,
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

input!(Messages {} + ListMessages);
input!(
    OneMessage {
        /// A message id.
        message: String
    } + GetMessage
);
input!(
    Conversation {
        /// A message's `conversationId`.
        conversation: String
    } + Paging
);
input!(
    MessageAttachments {
        /// A message id.
        message: String
    } + Paging
);
input!(OneAttachment {
    /// A message id.
    message: String,
    /// The id of one of its attachments.
    attachment: String
});
input!(Folders {} + ListFolders);
input!(OneFolder {
    /// A folder id, or a well-known name such as `inbox`.
    folder: String
});
input!(NewDraft {} + DraftMessage);
input!(
    Draft {
        /// The id of a draft.
        message: String
    } + DraftMessage
);
input!(
    Answer {
        /// The id of the message that is answered or forwarded.
        message: String
    } + ReplyContent
);
input!(SendNow {} + SendMail);
input!(ThisMessage {
    /// A message id.
    message: String
});
input!(
    Mark {
        /// A message id.
        message: String
    } + UpdateMessage
);
input!(Move {
    /// A message id.
    message: String,
    /// Where it goes: a folder id, or a well-known name such as `archive` or `deleteditems`.
    folder: String
});

// `Destructive` is anything that deletes, removes or overwrites what was
// there, or that cannot be taken back: an answer to an invitation reaches its
// organiser at once and cannot be unsent, and neither can mail. A host uses it
// to ask a person first.
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

        // ── mail ──
        operation("mail.list", "List the messages of one folder or of the whole mailbox, with a filter, a search or a sort. Bodies are plain text unless HTML is asked for.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: Messages| async move { m.mail(&c).list(i.options).await as Result<Page<Message>> }),
        operation("mail.get", "Get one message, with its body as plain text unless HTML is asked for.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: OneMessage| async move { m.mail(&c).get(&i.message, i.options).await as Result<Message> }),
        operation("mail.conversation", "List every message of one conversation, oldest first.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: Conversation| async move { m.mail(&c).conversation(&i.conversation, i.options).await as Result<Page<Message>> }),
        operation("mail.attachments_list", "List what is attached to a message: names, types and sizes, without the files.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: MessageAttachments| async move { m.mail(&c).attachments_list(&i.message, i.options).await as Result<Page<Attachment>> }),
        operation("mail.attachment_get", "Get one attachment. A file comes with its content, in base64.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: OneAttachment| async move { m.mail(&c).attachment_get(&i.message, &i.attachment).await as Result<Attachment> }),
        // A draft is the person's own until it is sent, and can be thrown away.
        operation("mail.create_draft", "Save a new message in Drafts. Nothing is sent.", Write, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: NewDraft| async move { m.mail(&c).create_draft(i.options).await as Result<Message> }),
        operation("mail.update_draft", "Change a draft, replacing the fields given and leaving the rest.", Destructive, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: Draft| async move { m.mail(&c).update_draft(&i.message, i.options).await as Result<Message> }),
        operation("mail.create_reply", "Save a reply to the sender of a message as a draft. Nothing is sent.", Write, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: Answer| async move { m.mail(&c).create_reply(&i.message, i.options).await as Result<Message> }),
        operation("mail.create_reply_all", "Save a reply to everyone on a message as a draft. Nothing is sent.", Write, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: Answer| async move { m.mail(&c).create_reply_all(&i.message, i.options).await as Result<Message> }),
        operation("mail.create_forward", "Save a forward of a message as a draft, addressed to toRecipients. Nothing is sent.", Write, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: Answer| async move { m.mail(&c).create_forward(&i.message, i.options).await as Result<Message> }),
        operation("mail.send", "Send a message at once, from the account's own address. It cannot be taken back.", Destructive, &["Mail.Send"],
            |m: Microsoft, c: Connection, i: SendNow| async move { m.mail(&c).send(i.options).await as Result<()> }),
        operation("mail.send_draft", "Send a draft as it stands. It cannot be taken back.", Destructive, &["Mail.Send"],
            |m: Microsoft, c: Connection, i: ThisMessage| async move { m.mail(&c).send_draft(&i.message).await as Result<()> }),
        operation("mail.reply", "Reply to the sender of a message and send the reply at once. It cannot be taken back.", Destructive, &["Mail.Send"],
            |m: Microsoft, c: Connection, i: Answer| async move { m.mail(&c).reply(&i.message, i.options).await as Result<()> }),
        operation("mail.update", "Mark a message: read or unread, its categories, its follow-up flag.", Write, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: Mark| async move { m.mail(&c).update(&i.message, i.options).await as Result<Message> }),
        operation("mail.move_to", "Move a message to another folder, Deleted Items included. It comes back with a new id; the old one stops working.", Destructive, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: Move| async move { m.mail(&c).move_to(&i.message, &i.folder).await as Result<Message> }),
        operation("mail.delete", "Delete a message.", Destructive, &["Mail.ReadWrite"],
            |m: Microsoft, c: Connection, i: ThisMessage| async move { m.mail(&c).delete(&i.message).await as Result<()> }),

        // ── mail folders ──
        operation("mail_folders.list", "List the folders at the top of the mailbox, or those inside one folder.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: Folders| async move { m.mail_folders(&c).list(i.options).await as Result<Page<MailFolder>> }),
        operation("mail_folders.get", "Get one folder, by its id or by a well-known name such as inbox.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: OneFolder| async move { m.mail_folders(&c).get(&i.folder).await as Result<MailFolder> }),
    ]
}
