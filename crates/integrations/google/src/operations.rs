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
use crate::models::{
    GmailAttachmentBody, GmailDraft, GmailDraftRef, GmailGetMessage, GmailGetThread, GmailLabel, GmailListDrafts,
    GmailListMessages, GmailListThreads, GmailMessage, GmailMessageRef, GmailModifyMessage, GmailProfile, GmailReply,
    GmailSendMessage, GmailThread,
};

// ── calendar: types ──
use crate::models::{
    CalendarEvent, CalendarListEntry, CalendarListFilter, EventDelete, EventFilter, EventInsert, EventInstancesFilter,
    EventPatch, EventResponse, FreeBusy, FreeBusyQuery,
};

// ── meet: types ──
use crate::models::{
    ConferenceRecord, MeetListConferenceRecords, MeetParticipant, MeetParticipantSession, MeetReadTranscript,
    MeetRecording, MeetSpace, MeetTranscript, MeetTranscriptContent, MeetTranscriptEntry,
};

// ── drive: types ──
use crate::models::{
    DriveCopyFile, DriveCreateFolder, DriveExport, DriveExportFormat, DriveFile, DriveListFiles, DrivePermission,
    SharedDrive,
};

// ── docs and sheets: types ──
use crate::models::{
    DocsAppendText, DocsCreateDocument, Document, DocumentText, DocumentUpdate, SheetsAppendValues,
    SheetsAppendedValues, SheetsGetValues, SheetsUpdateValues, SheetsUpdatedValues, SheetsValueRanges, Spreadsheet,
    ValueRange,
};

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
                // An object that lists no fields, as the input of an operation
                // that takes nothing does, is closed too: it takes none.
                let lists_none = fields.get("type").is_some_and(|kind| kind == "object")
                    && !fields.contains_key("additionalProperties");
                if fields.contains_key("properties") || lists_none {
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
            let none = serde_json::Map::new();
            let known = match node["properties"].as_object() {
                Some(known) => known,
                // Closed, and listing no fields: any field is one too many.
                None if node["additionalProperties"] == false => &none,
                None => return None,
            };
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
        // A list would be read by position, as the arguments in their
        // order, and nothing would say which value was which.
        if !input.is_object() {
            let refused = invalid("the input is a JSON object, with each argument under its name".to_owned());
            return Box::pin(std::future::ready(Err(refused.with_provider(provider))));
        }
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
input!(
    GmailMessagesList {
        /// The `next_cursor` of the page before, unchanged; absent for the first page.
        cursor: Option<String>,
        /// The most messages to return in one page, from 1 to 500. Gmail returns 100 when not given.
        limit: Option<u32>
    } + GmailListMessages
);
input!(
    GmailOneMessage {
        /// A message id.
        message: String
    } + GmailGetMessage
);
input!(GmailThisMessage {
    /// A message id.
    message: String
});
input!(GmailOneAttachment {
    /// A message id.
    message: String,
    /// The `attachmentId` of one of its attachments.
    attachment: String
});
input!(GmailSendNow {} + GmailSendMessage);
input!(
    GmailAnswer {
        /// The id of the message that is answered.
        message: String
    } + GmailReply
);
input!(
    GmailLabelled {
        /// A message id.
        message: String
    } + GmailModifyMessage
);
input!(
    GmailThreadsList {
        /// The `next_cursor` of the page before, unchanged; absent for the first page.
        cursor: Option<String>,
        /// The most threads to return in one page, from 1 to 500. Gmail returns 100 when not given.
        limit: Option<u32>
    } + GmailListThreads
);
input!(
    GmailOneThread {
        /// A thread id: a message's `threadId`.
        thread: String
    } + GmailGetThread
);
input!(GmailNothing {});
input!(GmailOneLabel {
    /// A label id, such as `INBOX` or `Label_12`.
    label: String
});
input!(
    GmailDraftsList {
        /// The `next_cursor` of the page before, unchanged; absent for the first page.
        cursor: Option<String>,
        /// The most drafts to return in one page, from 1 to 500. Gmail returns 100 when not given.
        limit: Option<u32>
    } + GmailListDrafts
);
input!(
    GmailOneDraft {
        /// A draft id. It is not the id of the message the draft holds.
        draft: String
    } + GmailGetMessage
);
input!(GmailNewDraft {} + GmailSendMessage);
input!(
    GmailChangedDraft {
        /// A draft id. It is not the id of the message the draft holds.
        draft: String
    } + GmailSendMessage
);
input!(GmailThisDraft {
    /// A draft id. It is not the id of the message the draft holds.
    draft: String
});

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
// A conference record, and each thing in one, is given as its id or as the
// name Google returned for it: `conferenceRecords/{id}/transcripts/{id}`.
input!(MeetRecords {} + MeetListConferenceRecords);
input!(MeetOneRecord {
    /// A conference record: its name, `conferenceRecords/{id}`, or its id.
    record: String
});
input!(
    MeetInRecord {
        /// A conference record: its name, `conferenceRecords/{id}`, or its id.
        record: String
    } + Paging
);
input!(MeetOneParticipant {
    /// A conference record: its name, `conferenceRecords/{id}`, or its id.
    record: String,
    /// One of the record's participants: its name, as a transcript entry has it in `participant`, or its id.
    participant: String
});
input!(
    MeetInParticipant {
        /// A conference record: its name, `conferenceRecords/{id}`, or its id.
        record: String,
        /// One of the record's participants: its name or its id.
        participant: String
    } + Paging
);
input!(MeetOneTranscript {
    /// A conference record: its name, `conferenceRecords/{id}`, or its id.
    record: String,
    /// One of the record's transcripts: its name or its id.
    transcript: String
});
input!(
    MeetInTranscript {
        /// A conference record: its name, `conferenceRecords/{id}`, or its id.
        record: String,
        /// One of the record's transcripts: its name or its id.
        transcript: String
    } + Paging
);
input!(
    MeetWholeTranscript {
        /// A conference record: its name, `conferenceRecords/{id}`, or its id.
        record: String,
        /// One of the record's transcripts: its name or its id.
        transcript: String
    } + MeetReadTranscript
);
input!(MeetOneRecording {
    /// A conference record: its name, `conferenceRecords/{id}`, or its id.
    record: String,
    /// One of the record's recordings: its name or its id.
    recording: String
});
input!(MeetOneSpace {
    /// A space: its name (`spaces/{id}`), its id, a meeting code (`abc-mnop-xyz`), or the link people join by.
    space: String
});

// ── drive: inputs ──
input!(DriveFilesListed {} + DriveListFiles);
input!(DriveOneFile {
    /// A file or folder id. `google.resource.resolve` reads one from a pasted link.
    file: String
});
input!(DriveExported {
    /// The id of a Google Doc, Sheet or Slides presentation.
    file: String,
    /// The format of the text: `text/plain` or `text/markdown` for a Doc, `text/csv` for a Sheet.
    #[serde(rename = "mimeType")]
    mime_type: DriveExportFormat
});
input!(
    DrivePermissionsOf {
        /// A file or folder id.
        file: String
    } + Paging
);
input!(DriveListing {} + Paging);
input!(DriveNewFolder {} + DriveCreateFolder);
input!(
    DriveCopied {
        /// The id of the file to copy.
        file: String
    } + DriveCopyFile
);
input!(DriveMoved {
    /// The id of the file or folder to move.
    file: String,
    /// Where it goes: a folder id, or `root` for the top of the account's My Drive.
    folder: String
});
input!(DriveRenamed {
    /// The id of the file or folder to rename.
    file: String,
    /// Its new name.
    name: String
});

// ── docs and sheets: inputs ──
input!(DocsOneDocument {
    /// A document id: the part of its address after `/document/d/`.
    document: String
});
input!(DocsNewDocument {} + DocsCreateDocument);
input!(
    DocsAppend {
        /// The id of the document to add to.
        document: String
    } + DocsAppendText
);
input!(SheetsOneSpreadsheet {
    /// A spreadsheet id: the part of its address after `/spreadsheets/d/`.
    spreadsheet: String
});
input!(
    SheetsRange {
        /// A spreadsheet id.
        spreadsheet: String,
        /// The cells to read, in A1 notation: `Sheet1!A1:C10`, `A:A`, or a
        /// sheet's name alone for all of it. A name with a space or a symbol
        /// in it goes between single quotes: `'Q3 plan'!A1:B2`.
        range: String
    } + SheetsGetValues
);
input!(
    SheetsRanges {
        /// A spreadsheet id.
        spreadsheet: String,
        /// The ranges to read, each in A1 notation: `Sheet1!A1:C10`.
        ranges: Vec<String>
    } + SheetsGetValues
);
input!(
    SheetsUpdate {
        /// A spreadsheet id.
        spreadsheet: String,
        /// The cells to write over, in A1 notation: `Sheet1!A1:C10`. The
        /// values start at its first cell.
        range: String
    } + SheetsUpdateValues
);
input!(
    SheetsAppend {
        /// A spreadsheet id.
        spreadsheet: String,
        /// Where to look for the table to add to, in A1 notation: `Sheet1`
        /// or `Sheet1!A:E`.
        range: String
    } + SheetsAppendValues
);

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
        operation("gmail_messages.list", "List the messages a Gmail search finds. Returns ids only: each message's id and its thread's id. Read one with gmail_messages.get.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailMessagesList| async move { g.gmail_messages(&c).list(i.options, Paging { cursor: i.cursor, limit: i.limit }).await as Result<Page<GmailMessageRef>> }),
        operation("gmail_messages.get", "Get one Gmail message, decoded: its headers, its body as plain text and as HTML, and its attachments without their content.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailOneMessage| async move { g.gmail_messages(&c).get(&i.message, i.options).await as Result<GmailMessage> }),
        operation("gmail_messages.attachment_get", "Get the content of one attachment of a Gmail message, in URL-safe base64, with its size. A file over about 7 MB cannot be read yet.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailOneAttachment| async move { g.gmail_messages(&c).attachment_get(&i.message, &i.attachment).await as Result<GmailAttachmentBody> }),
        operation("gmail_messages.send", "Send a message at once from the Gmail account's own address. It cannot be taken back.", Destructive, &[scopes::GMAIL_SEND],
            |g: Google, c: Connection, i: GmailSendNow| async move { g.gmail_messages(&c).send(i.options).await as Result<GmailMessageRef> }),
        operation("gmail_messages.reply", "Answer a Gmail message in its thread and send the answer at once, to the people named in `to` and nobody else. `to` is required: nothing in the message answered decides who a reply goes to, so read its `from` and `replyTo` and name them. It cannot be taken back.", Destructive, &[scopes::GMAIL_READONLY, scopes::GMAIL_SEND],
            |g: Google, c: Connection, i: GmailAnswer| async move { g.gmail_messages(&c).reply(&i.message, i.options).await as Result<GmailMessageRef> }),
        operation("gmail_messages.send_draft", "Send a Gmail draft as it stands. It cannot be taken back, and the draft is gone once it is sent.", Destructive, &[scopes::GMAIL_COMPOSE],
            |g: Google, c: Connection, i: GmailThisDraft| async move { g.gmail_messages(&c).send_draft(&i.draft).await as Result<GmailMessageRef> }),
        operation("gmail_messages.modify", "Add labels to a Gmail message and remove others: remove INBOX to archive, remove UNREAD to mark as read, add STARRED to star. TRASH and SPAM cannot be added: gmail_messages.trash moves a message to the bin.", Write, &[scopes::GMAIL_MODIFY],
            |g: Google, c: Connection, i: GmailLabelled| async move { g.gmail_messages(&c).modify(&i.message, i.options).await as Result<GmailMessageRef> }),
        operation("gmail_messages.trash", "Move a Gmail message to the bin. It can be brought back with gmail_messages.untrash until Gmail empties the bin, after which it is gone for good.", Destructive, &[scopes::GMAIL_MODIFY],
            |g: Google, c: Connection, i: GmailThisMessage| async move { g.gmail_messages(&c).trash(&i.message).await as Result<GmailMessageRef> }),
        operation("gmail_messages.untrash", "Take a Gmail message out of the bin.", Write, &[scopes::GMAIL_MODIFY],
            |g: Google, c: Connection, i: GmailThisMessage| async move { g.gmail_messages(&c).untrash(&i.message).await as Result<GmailMessageRef> }),

        operation("gmail_threads.list", "List the Gmail threads a search finds. Each is its id and a snippet, without its messages. Read one with gmail_threads.get.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailThreadsList| async move { g.gmail_threads(&c).list(i.options, Paging { cursor: i.cursor, limit: i.limit }).await as Result<Page<GmailThread>> }),
        operation("gmail_threads.get", "Get one Gmail thread with its messages, each decoded. Ask for the metadata format to leave the bodies out of a long thread.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailOneThread| async move { g.gmail_threads(&c).get(&i.thread, i.options).await as Result<GmailThread> }),

        operation("gmail_labels.list", "List every label of the Gmail mailbox, Gmail's own and the person's, without their counts.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, _: GmailNothing| async move { g.gmail_labels(&c).list().await as Result<Vec<GmailLabel>> }),
        operation("gmail_labels.get", "Get one Gmail label, with how many messages and threads carry it and how many are unread.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailOneLabel| async move { g.gmail_labels(&c).get(&i.label).await as Result<GmailLabel> }),

        operation("gmail_profile.get", "Get the Gmail account's address, how many messages and threads it holds, and its current history id.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, _: GmailNothing| async move { g.gmail_profile(&c).get().await as Result<GmailProfile> }),

        operation("gmail_drafts.list", "List the Gmail drafts. Returns ids only: each draft's id and the ids of the message it holds. Read one with gmail_drafts.get.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailDraftsList| async move { g.gmail_drafts(&c).list(i.options, Paging { cursor: i.cursor, limit: i.limit }).await as Result<Page<GmailDraftRef>> }),
        operation("gmail_drafts.get", "Get one Gmail draft, with its message decoded: headers, text, HTML and attachments.", Read, &[scopes::GMAIL_READONLY],
            |g: Google, c: Connection, i: GmailOneDraft| async move { g.gmail_drafts(&c).get(&i.draft, i.options).await as Result<GmailDraft> }),
        operation("gmail_drafts.create", "Save a new Gmail draft. Nothing is sent.", Write, &[scopes::GMAIL_COMPOSE],
            |g: Google, c: Connection, i: GmailNewDraft| async move { g.gmail_drafts(&c).create(i.options).await as Result<GmailDraftRef> }),
        operation("gmail_drafts.update", "Replace everything a Gmail draft says. What is not given again is gone.", Destructive, &[scopes::GMAIL_COMPOSE],
            |g: Google, c: Connection, i: GmailChangedDraft| async move { g.gmail_drafts(&c).update(&i.draft, i.options).await as Result<GmailDraftRef> }),
        operation("gmail_drafts.delete", "Delete a Gmail draft for good. It does not go to the bin.", Destructive, &[scopes::GMAIL_COMPOSE],
            |g: Google, c: Connection, i: GmailThisDraft| async move { g.gmail_drafts(&c).delete(&i.draft).await as Result<()> }),

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
        operation("calendar_events.insert", "Create an event and invite its attendees, with a Google Meet link when createMeetLink is set. With sendUpdates, Google emails the invitation, which cannot be taken back.", Write, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventInsert| async move { g.calendar_events(&c).insert(&i.calendar, i.options).await as Result<CalendarEvent> }),
        operation("calendar_events.patch", "Change an event, replacing the fields given and leaving the rest. Attendees, when given, replace the whole guest list: anyone left out is uninvited, and a guest who stays keeps the answer they gave.", Destructive, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventPatch| async move { g.calendar_events(&c).patch(&i.calendar, &i.event, i.options).await as Result<CalendarEvent> }),
        operation("calendar_events.respond", "Answer an invitation on a calendar: accepted, declined, tentative, or needsAction. On primary this is the signed-in person's own answer. The organiser sees it at once, and a notice sent with sendUpdates cannot be taken back. Nobody else on the guest list is changed.", Destructive, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventRespond| async move { g.calendar_events(&c).respond(&i.calendar, &i.event, i.options).await as Result<CalendarEvent> }),
        operation("calendar_events.delete", "Delete an event. Deleting an event the account organised cancels it for its attendees.", Destructive, &[scopes::CALENDAR_EVENTS],
            |g: Google, c: Connection, i: CalendarEventDelete| async move { g.calendar_events(&c).delete(&i.calendar, &i.event, i.options).await as Result<()> }),
        // Google offers this only as POST. It reads calendars and changes nothing.
        operation("calendar_freebusy.query", "Read when calendars are busy inside a time window. Changes nothing.", Read, &[scopes::CALENDAR_READONLY],
            |g: Google, c: Connection, i: CalendarAvailability| async move { g.calendar_freebusy(&c).query(&i.calendars, i.options).await as Result<FreeBusy> }),

        // ── meet ──
        operation("meet_conference_records.list", "List the Meet meetings the account organised, newest first: one conference record for each time a meeting was held. Can be narrowed to one meeting code or space, or to meetings that began between two times. Google keeps a record for 30 days after the meeting ended. At most 100 in a page.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetRecords| async move { g.meet_conference_records(&c).list(i.options).await as Result<Page<ConferenceRecord>> }),
        operation("meet_conference_records.get", "Get one Meet conference record: when the meeting began and ended, and the space it was held in.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetOneRecord| async move { g.meet_conference_records(&c).get(&i.record).await as Result<ConferenceRecord> }),
        operation("meet_participants.list", "List who was in a Meet meeting, with the name each was shown under. Someone who left and came back is listed once. At most 250 in a page.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetInRecord| async move { g.meet_participants(&c).list(&i.record, i.options).await as Result<Page<MeetParticipant>> }),
        operation("meet_participants.get", "Get one participant of a Meet meeting: the name they were shown under, and when they first joined and last left.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetOneParticipant| async move { g.meet_participants(&c).get(&i.record, &i.participant).await as Result<MeetParticipant> }),
        operation("meet_participants.sessions", "List each time a participant was connected to a Meet meeting: one session for every time they joined, from every device. At most 250 in a page.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetInParticipant| async move { g.meet_participants(&c).sessions(&i.record, &i.participant, i.options).await as Result<Page<MeetParticipantSession>> }),
        operation("meet_transcripts.list", "List the transcripts of a Meet meeting. Empty when transcription was never switched on. Each names the Google Doc it was saved to. At most 100 in a page.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetInRecord| async move { g.meet_transcripts(&c).list(&i.record, i.options).await as Result<Page<MeetTranscript>> }),
        operation("meet_transcripts.get", "Get one Meet transcript's details: when it was made, whether its Google Doc has been written, and which Doc that is.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetOneTranscript| async move { g.meet_transcripts(&c).get(&i.record, &i.transcript).await as Result<MeetTranscript> }),
        operation("meet_transcripts.entries", "List a Meet transcript's entries as Meet returns them: what was said, when, in what language, and the speaker as a reference to a participant, not a name. Google deletes them 30 days after the meeting ended. At most 100 in a page. Use meet_transcripts.read for the whole transcript with names.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetInTranscript| async move { g.meet_transcripts(&c).entries(&i.record, &i.transcript, i.options).await as Result<Page<MeetTranscriptEntry>> }),
        operation("meet_transcripts.read", "Read what was said in a Meet meeting: the transcript as lines of `Speaker: what was said`, and one entry for each thing said with the speaker's name, the start and the end. Makes several requests. Stops at maxEntries (1000 unless given) and sets truncated when the transcript has more.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetWholeTranscript| async move { g.meet_transcripts(&c).read(&i.record, &i.transcript, i.options).await as Result<MeetTranscriptContent> }),
        operation("meet_recordings.list", "List the recordings of a Meet meeting. Empty when the meeting was not recorded. Each names the Drive file it was saved to. At most 100 in a page.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetInRecord| async move { g.meet_recordings(&c).list(&i.record, i.options).await as Result<Page<MeetRecording>> }),
        operation("meet_recordings.get", "Get one Meet recording: whether its file is ready, and the Drive file it was saved to.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetOneRecording| async move { g.meet_recordings(&c).get(&i.record, &i.recording).await as Result<MeetRecording> }),
        operation("meet_spaces.get", "Get a Meet space from its name, its id, a meeting code or the link people join by. Returns its name, its link and code, how it is set up, and the meeting going on in it now if there is one.", Read, &[scopes::MEETINGS_SPACE_READONLY],
            |g: Google, c: Connection, i: MeetOneSpace| async move { g.meet_spaces(&c).get(&i.space).await as Result<MeetSpace> }),

        // ── drive ──
        operation("drive_files.list", "List the files and folders that match a search in Drive's query language (name, full text, type, parent folder, modified time), or everything the account can see. What is in shared drives is included.", Read, &[scopes::DRIVE_READONLY],
            |g: Google, c: Connection, i: DriveFilesListed| async move { g.drive_files(&c).list(i.options).await as Result<Page<DriveFile>> }),
        operation("drive_files.get", "Get what describes one file or folder: its name, type, folder, owners, size and link. Not its content.", Read, &[scopes::DRIVE_READONLY],
            |g: Google, c: Connection, i: DriveOneFile| async move { g.drive_files(&c).get(&i.file).await as Result<DriveFile> }),
        operation("drive_files.export", "Return a Google document as text: a Doc as plain text or Markdown, a Sheet as CSV (its first sheet only). At most 10 MB; a file that is not a Google document cannot be exported.", Read, &[scopes::DRIVE_READONLY],
            |g: Google, c: Connection, i: DriveExported| async move { g.drive_files(&c).export(&i.file, i.mime_type).await as Result<DriveExport> }),
        operation("drive_files.permissions", "List who can see a file or folder and in what role: people, groups, whole domains, and anyone with the link.", Read, &[scopes::DRIVE_READONLY],
            |g: Google, c: Connection, i: DrivePermissionsOf| async move { g.drive_files(&c).permissions(&i.file, i.options).await as Result<Page<DrivePermission>> }),
        operation("drive_files.create_folder", "Create a folder, at the top of the account's My Drive or inside another folder.", Write, &[scopes::DRIVE_FILE],
            |g: Google, c: Connection, i: DriveNewFolder| async move { g.drive_files(&c).create_folder(i.options).await as Result<DriveFile> }),
        operation("drive_files.copy", "Make a copy of a file, beside it or in another folder, under a new name if one is given. A folder cannot be copied.", Write, &[scopes::DRIVE_FILE],
            |g: Google, c: Connection, i: DriveCopied| async move { g.drive_files(&c).copy(&i.file, i.options).await as Result<DriveFile> }),
        // A file is seen by whoever can see its folder. Moved, it is shown to
        // the people of the new folder and taken from those of the old one,
        // and a file moved into a shared drive becomes the drive's.
        operation("drive_files.move_to", "Move a file or folder into another folder, out of the one it is in. Who can see it changes with it: the people the new folder is shared with gain it, those who had it through the old folder lose it, and a file moved into a shared drive belongs to that drive.", Destructive, &[scopes::DRIVE_FILE],
            |g: Google, c: Connection, i: DriveMoved| async move { g.drive_files(&c).move_to(&i.file, &i.folder).await as Result<DriveFile> }),
        // A name is one small thing that the caller can read first and put back.
        operation("drive_files.rename", "Give a file or folder another name. It stays where it is, under the same id.", Write, &[scopes::DRIVE_FILE],
            |g: Google, c: Connection, i: DriveRenamed| async move { g.drive_files(&c).rename(&i.file, &i.name).await as Result<DriveFile> }),
        operation("drive_files.trash", "Put a file or folder in the bin, with everything inside a folder. Everyone who could see it loses it. It can be restored for 30 days, after which Google deletes it for good.", Destructive, &[scopes::DRIVE_FILE],
            |g: Google, c: Connection, i: DriveOneFile| async move { g.drive_files(&c).trash(&i.file).await as Result<DriveFile> }),
        operation("drive_shared_drives.list", "List the shared drives the account is a member of.", Read, &[scopes::DRIVE_READONLY],
            |g: Google, c: Connection, i: DriveListing| async move { g.drive_shared_drives(&c).list(i.options).await as Result<Page<SharedDrive>> }),

        // ── docs and sheets ──
        operation("docs_documents.get", "Get a Google Doc's title, revision and tabs, without what is written in it.", Read, &[scopes::DOCUMENTS_READONLY],
            |g: Google, c: Connection, i: DocsOneDocument| async move { g.docs_documents(&c).get(&i.document).await as Result<Document> }),
        operation("docs_documents.read", "Read a Google Doc as plain text: the whole document, and each of its tabs. Headings are Markdown headings, list items keep their markers, and a table has a line for each row.", Read, &[scopes::DOCUMENTS_READONLY],
            |g: Google, c: Connection, i: DocsOneDocument| async move { g.docs_documents(&c).read(&i.document).await as Result<DocumentText> }),
        operation("docs_documents.create", "Create a blank Google Doc with a title.", Write, &[scopes::DOCUMENTS],
            |g: Google, c: Connection, i: DocsNewDocument| async move { g.docs_documents(&c).create(i.options).await as Result<Document> }),
        // Adds to the end and touches nothing that was there. Google offers
        // it only through `batchUpdate`, a POST.
        operation("docs_documents.append_text", "Add text at the end of a Google Doc, or of one of its tabs. Nothing that was there is changed.", Write, &[scopes::DOCUMENTS],
            |g: Google, c: Connection, i: DocsAppend| async move { g.docs_documents(&c).append_text(&i.document, i.options).await as Result<DocumentUpdate> }),
        operation("sheets_spreadsheets.get", "Get a Google Sheet's title, locale and time zone, and its sheets with their names and sizes. No cell is read.", Read, &[scopes::SPREADSHEETS_READONLY],
            |g: Google, c: Connection, i: SheetsOneSpreadsheet| async move { g.sheets_spreadsheets(&c).get(&i.spreadsheet).await as Result<Spreadsheet> }),
        operation("sheets_spreadsheets.values_get", "Read the values of one range of a Google Sheet, in A1 notation.", Read, &[scopes::SPREADSHEETS_READONLY],
            |g: Google, c: Connection, i: SheetsRange| async move { g.sheets_spreadsheets(&c).values_get(&i.spreadsheet, &i.range, i.options).await as Result<ValueRange> }),
        operation("sheets_spreadsheets.values_batch_get", "Read the values of several ranges of a Google Sheet in one call. They come back in the order asked for.", Read, &[scopes::SPREADSHEETS_READONLY],
            |g: Google, c: Connection, i: SheetsRanges| async move { g.sheets_spreadsheets(&c).values_batch_get(&i.spreadsheet, &i.ranges, i.options).await as Result<SheetsValueRanges> }),
        // What was in the cells is gone once they are written over.
        operation("sheets_spreadsheets.values_update", "Write values over the cells of a range of a Google Sheet. What was in those cells is replaced.", Destructive, &[scopes::SPREADSHEETS],
            |g: Google, c: Connection, i: SheetsUpdate| async move { g.sheets_spreadsheets(&c).values_update(&i.spreadsheet, &i.range, i.options).await as Result<SheetsUpdatedValues> }),
        operation("sheets_spreadsheets.values_append", "Add rows under a table in a Google Sheet. Google finds the table in the range and writes after its last row. Rows are inserted for them, so nothing under the table is written over.", Write, &[scopes::SPREADSHEETS],
            |g: Google, c: Connection, i: SheetsAppend| async move { g.sheets_spreadsheets(&c).values_append(&i.spreadsheet, &i.range, i.options).await as Result<SheetsAppendedValues> }),
    ]
}
