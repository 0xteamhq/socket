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
    Attachment, AttachmentText, Calendar, CancelEvent, CreateEvent, DraftMessage, Event, EventResponse,
    FindMeetingTimes, GetMessage, GetSchedule, ListFolders, ListMessages, MailFolder, MeetingTimeSuggestions, Message,
    Paging, ReplyContent, RespondToEvent, ScheduleInformation, SendMail, UpdateEvent, UpdateMessage,
};
use crate::models::{
    AttendanceRecord, AttendanceReport, OnlineMeeting, Recording, TextLimit, Transcript, TranscriptContent,
};
use crate::models::{Channel, Chat, ChatMessage, ConversationMember, CreateChat, Cursor, SendChatMessage, Team};

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
    F: Fn(Microsoft, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("microsoft.{name}"),
        description: description.to_owned(),
        input_schema: closed(schema_of::<I>()),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let schema = info.input_schema.clone();
    let run = move |microsoft: Microsoft, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        if let Some(found) = unknown_field(&schema, &schema, &input) {
            return Box::pin(std::future::ready(Err(not_a_field(found).with_provider(provider))));
        }
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
input!(
    AttachmentAsText {
        /// A message id.
        message: String,
        /// The id of one of its attachments.
        attachment: String
    } + TextLimit
);
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

input!(Place {} + Cursor);
input!(OneTeam {
    /// A team id.
    team: String
});
input!(
    InTeam {
        /// A team id.
        team: String
    } + Paging
);
input!(
    TeamPlace {
        /// A team id.
        team: String
    } + Cursor
);
input!(OneChannel {
    /// A team id.
    team: String,
    /// The id of one of the team's channels, such as `19:…@thread.tacv2`.
    channel: String
});
input!(
    InChannel {
        /// A team id.
        team: String,
        /// The id of one of the team's channels.
        channel: String
    } + Paging
);
input!(OneChannelMessage {
    /// A team id.
    team: String,
    /// The id of one of the team's channels.
    channel: String,
    /// The id of a message that starts a conversation in the channel.
    message: String
});
input!(
    UnderMessage {
        /// A team id.
        team: String,
        /// The id of one of the team's channels.
        channel: String,
        /// The id of the message the replies are to.
        message: String
    } + Paging
);
input!(
    ToChannel {
        /// A team id.
        team: String,
        /// The id of one of the team's channels.
        channel: String
    } + SendChatMessage
);
input!(
    ReplyInChannel {
        /// A team id.
        team: String,
        /// The id of one of the team's channels.
        channel: String,
        /// The id of the message to reply to.
        message: String
    } + SendChatMessage
);
input!(OneChat {
    /// A chat id, such as `19:…@thread.v2`.
    chat: String
});
input!(
    ChatPlace {
        /// A chat id.
        chat: String
    } + Cursor
);
input!(
    InChat {
        /// A chat id.
        chat: String
    } + Paging
);
input!(OneChatMessage {
    /// A chat id.
    chat: String,
    /// The id of one of the chat's messages.
    message: String
});
input!(
    ToChat {
        /// A chat id.
        chat: String
    } + SendChatMessage
);
input!(NewChat {} + CreateChat);

input!(OneMeeting {
    /// The id of an online meeting, as `online_meetings.find_by_join_url` returns it.
    meeting: String
});
input!(JoinUrl {
    /// The meeting's join link, exactly as the calendar event has it in `onlineMeeting.joinUrl`.
    join_url: String
});
input!(
    InMeeting {
        /// The id of an online meeting.
        meeting: String
    } + Paging
);
input!(OneTranscript {
    /// The id of an online meeting.
    meeting: String,
    /// The id of one of the meeting's transcripts.
    transcript: String
});
input!(OneRecording {
    /// The id of an online meeting.
    meeting: String,
    /// The id of one of the meeting's recordings.
    recording: String
});
input!(
    InReport {
        /// The id of an online meeting.
        meeting: String,
        /// The id of one of the meeting's attendance reports.
        report: String
    } + Paging
);

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
        operation("mail.attachment_get", "Describe one attachment: its name, type and size, without the file.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: OneAttachment| async move { m.mail(&c).attachment_get(&i.message, &i.attachment).await as Result<Attachment> }),
        operation("mail.attachment_text", "Read an attachment that is text, such as a CSV file or a calendar invitation. One megabyte unless maxBytes allows more, up to ten. Anything that is not text is refused: bytes are not returned.", Read, &["Mail.Read"],
            |m: Microsoft, c: Connection, i: AttachmentAsText| async move { m.mail(&c).attachment_text(&i.message, &i.attachment, i.options).await as Result<AttachmentText> }),
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

        // ── teams ──
        operation("teams.list_joined", "List the teams the account is a member of.", Read, &["Team.ReadBasic.All"],
            |m: Microsoft, c: Connection, i: Place| async move { m.teams(&c).list_joined(i.options).await as Result<Page<Team>> }),
        operation("teams.get", "Get one team.", Read, &["Team.ReadBasic.All"],
            |m: Microsoft, c: Connection, i: OneTeam| async move { m.teams(&c).get(&i.team).await as Result<Team> }),
        operation("teams.members", "List a team's members and owners. Needs an administrator's consent.", Read, &["TeamMember.Read.All"],
            |m: Microsoft, c: Connection, i: InTeam| async move { m.teams(&c).members(&i.team, i.options).await as Result<Page<ConversationMember>> }),

        // ── channels ──
        operation("channels.list", "List a team's channels.", Read, &["Channel.ReadBasic.All"],
            |m: Microsoft, c: Connection, i: TeamPlace| async move { m.channels(&c).list(&i.team, i.options).await as Result<Page<Channel>> }),
        operation("channels.get", "Get one channel of a team.", Read, &["Channel.ReadBasic.All"],
            |m: Microsoft, c: Connection, i: OneChannel| async move { m.channels(&c).get(&i.team, &i.channel).await as Result<Channel> }),
        operation("channels.members", "List a channel's members and owners. Needs an administrator's consent.", Read, &["ChannelMember.Read.All"],
            |m: Microsoft, c: Connection, i: InChannel| async move { m.channels(&c).members(&i.team, &i.channel, i.options).await as Result<Page<ConversationMember>> }),

        // ── channel messages ──
        operation("channel_messages.list", "List the messages that start a conversation in a channel, without their replies, each also as plain text. Needs an administrator's consent.", Read, &["ChannelMessage.Read.All"],
            |m: Microsoft, c: Connection, i: InChannel| async move { m.channel_messages(&c).list(&i.team, &i.channel, i.options).await as Result<Page<ChatMessage>> }),
        operation("channel_messages.get", "Get one message of a channel, also as plain text. Needs an administrator's consent.", Read, &["ChannelMessage.Read.All"],
            |m: Microsoft, c: Connection, i: OneChannelMessage| async move { m.channel_messages(&c).get(&i.team, &i.channel, &i.message).await as Result<ChatMessage> }),
        operation("channel_messages.replies", "List the replies to a message of a channel, each also as plain text. Needs an administrator's consent.", Read, &["ChannelMessage.Read.All"],
            |m: Microsoft, c: Connection, i: UnderMessage| async move { m.channel_messages(&c).replies(&i.team, &i.channel, &i.message, i.options).await as Result<Page<ChatMessage>> }),
        operation("channel_messages.send", "Post a new message to a channel, as the signed-in person. Everyone in the channel sees it.", Write, &["ChannelMessage.Send"],
            |m: Microsoft, c: Connection, i: ToChannel| async move { m.channel_messages(&c).send(&i.team, &i.channel, i.options).await as Result<ChatMessage> }),
        operation("channel_messages.reply", "Post a reply under a message of a channel, as the signed-in person.", Write, &["ChannelMessage.Send"],
            |m: Microsoft, c: Connection, i: ReplyInChannel| async move { m.channel_messages(&c).reply(&i.team, &i.channel, &i.message, i.options).await as Result<ChatMessage> }),

        // ── chats ──
        operation("chats.list", "List the chats the account is in: one-to-one, group and meeting chats.", Read, &["Chat.ReadBasic"],
            |m: Microsoft, c: Connection, i: Listing| async move { m.chats(&c).list(i.options).await as Result<Page<Chat>> }),
        operation("chats.get", "Get one chat.", Read, &["Chat.ReadBasic"],
            |m: Microsoft, c: Connection, i: OneChat| async move { m.chats(&c).get(&i.chat).await as Result<Chat> }),
        operation("chats.members", "List who is in a chat.", Read, &["Chat.ReadBasic"],
            |m: Microsoft, c: Connection, i: ChatPlace| async move { m.chats(&c).members(&i.chat, i.options).await as Result<Page<ConversationMember>> }),
        operation("chats.messages", "List a chat's messages, the most recently changed first, each also as plain text.", Read, &["Chat.Read"],
            |m: Microsoft, c: Connection, i: InChat| async move { m.chats(&c).messages(&i.chat, i.options).await as Result<Page<ChatMessage>> }),
        operation("chats.message_get", "Get one message of a chat, also as plain text.", Read, &["Chat.Read"],
            |m: Microsoft, c: Connection, i: OneChatMessage| async move { m.chats(&c).message_get(&i.chat, &i.message).await as Result<ChatMessage> }),
        operation("chats.send", "Send a message to a chat, as the signed-in person.", Write, &["ChatMessage.Send"],
            |m: Microsoft, c: Connection, i: ToChat| async move { m.chats(&c).send(&i.chat, i.options).await as Result<ChatMessage> }),
        operation("chats.create", "Create a chat between two people or among several. Returns the chat that already exists between two people, when there is one.", Write, &["Chat.Create"],
            |m: Microsoft, c: Connection, i: NewChat| async move { m.chats(&c).create(i.options).await as Result<Chat> }),

        // ── online meetings ──
        operation("online_meetings.get", "Get one Teams online meeting by its id.", Read, &["OnlineMeetings.Read"],
            |m: Microsoft, c: Connection, i: OneMeeting| async move { m.online_meetings(&c).get(&i.meeting).await as Result<OnlineMeeting> }),
        operation("online_meetings.find_by_join_url", "Find the Teams online meeting behind a join link from a calendar event. Returns its id, which transcripts, recordings and attendance are asked for by.", Read, &["OnlineMeetings.Read"],
            |m: Microsoft, c: Connection, i: JoinUrl| async move { m.online_meetings(&c).find_by_join_url(&i.join_url).await as Result<OnlineMeeting> }),

        // ── transcripts ──
        operation("transcripts.list", "List a meeting's transcripts. Empty when transcription was never switched on. Needs an administrator's consent.", Read, &["OnlineMeetingTranscript.Read.All"],
            |m: Microsoft, c: Connection, i: InMeeting| async move { m.transcripts(&c).list(&i.meeting, i.options).await as Result<Page<Transcript>> }),
        operation("transcripts.get", "Get one transcript's details: when it was made, and by whose meeting. Needs an administrator's consent.", Read, &["OnlineMeetingTranscript.Read.All"],
            |m: Microsoft, c: Connection, i: OneTranscript| async move { m.transcripts(&c).get(&i.meeting, &i.transcript).await as Result<Transcript> }),
        operation("transcripts.content", "Read what was said in a meeting: the transcript's text, and one entry for each thing said with the speaker, the start and the end. Needs an administrator's consent.", Read, &["OnlineMeetingTranscript.Read.All"],
            |m: Microsoft, c: Connection, i: OneTranscript| async move { m.transcripts(&c).content(&i.meeting, &i.transcript).await as Result<TranscriptContent> }),

        // ── recordings ──
        operation("recordings.list", "List a meeting's recordings. Empty when the meeting was not recorded. Needs an administrator's consent.", Read, &["OnlineMeetingRecording.Read.All"],
            |m: Microsoft, c: Connection, i: InMeeting| async move { m.recordings(&c).list(&i.meeting, i.options).await as Result<Page<Recording>> }),
        operation("recordings.get", "Get one recording's details, with the address its video is at. Needs an administrator's consent.", Read, &["OnlineMeetingRecording.Read.All"],
            |m: Microsoft, c: Connection, i: OneRecording| async move { m.recordings(&c).get(&i.meeting, &i.recording).await as Result<Recording> }),

        // ── attendance ──
        operation("attendance.reports", "List a meeting's attendance reports, one for each time it was held.", Read, &["OnlineMeetingArtifact.Read.All"],
            |m: Microsoft, c: Connection, i: InMeeting| async move { m.attendance(&c).reports(&i.meeting, i.options).await as Result<Page<AttendanceReport>> }),
        operation("attendance.records", "List who joined a meeting, in what role, when, and for how long.", Read, &["OnlineMeetingArtifact.Read.All"],
            |m: Microsoft, c: Connection, i: InReport| async move { m.attendance(&c).records(&i.meeting, &i.report, i.options).await as Result<Page<AttendanceRecord>> }),
    ]
}
