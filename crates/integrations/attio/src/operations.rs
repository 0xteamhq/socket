//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "object": "people", "record": "…", "values": { … } }`.
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

use crate::Attio;
use crate::models::{
    Attribute, AttributeTarget, CallRecording, Comment, CreateComment, CreateEntry, CreateNote, CreateTask, Entry,
    GetThread, List, ListAttributes, ListMeetings, ListNotes, ListTasks, ListThreads, Meeting, Note, Object, Paging,
    Query, Record, RecordEntry, SelectOption, ShowArchived, Status, Task, Thread, UpdateTask, WorkspaceMember,
    WriteEntry, WriteRecord,
};

type Running = std::pin::Pin<Box<dyn Future<Output = Result<Value>> + Send>>;

/// One named operation: what it says about itself, and how to run it.
pub(crate) struct Operation {
    pub(crate) info: OperationInfo,
    run: Box<dyn Fn(Attio, Connection, Value) -> Running + Send + Sync>,
}

impl Operation {
    pub(crate) fn run(&self, attio: Attio, connection: Connection, input: Value) -> Running {
        (self.run)(attio, connection, input)
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
    // An input with no fields lists none, and so allows none.
    if let Some(fields) = schema.as_object_mut() {
        fields
            .entry("properties")
            .or_insert_with(|| Value::Object(serde_json::Map::new()));
    }
    close(&mut schema);
    schema
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// a filter, a deadline, the record a note is on. The input types cannot refuse one
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
    F: Fn(Attio, Connection, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<O>> + Send + 'static,
{
    let info = OperationInfo {
        name: format!("attio.{name}"),
        description: description.to_owned(),
        input_schema: closed(schema_of::<I>()),
        output_schema: schema_of::<O>(),
        effect,
        required_scopes: scopes.iter().map(|scope| (*scope).to_owned()).collect(),
    };
    let schema = info.input_schema.clone();
    let run = move |attio: Attio, connection: Connection, input: Value| -> Running {
        let provider = connection.provider().id.clone();
        if let Some(found) = unknown_field(&schema, &schema, &input) {
            return Box::pin(std::future::ready(Err(not_a_field(found).with_provider(provider))));
        }
        match serde_path_to_error::deserialize::<_, I>(input) {
            Err(e) => {
                // serde's own message quotes the offending value, which may be
                // a customer's details or a credential. Only the field's name,
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
                let output = handler(attio, connection, input);
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

input!(Nothing {});
input!(OneObject {
    /// An object's slug, such as `people`, `companies` or `deals`, or its id.
    object: String
});
input!(
    AttributesOf {
        /// Where the attributes are defined: `objects` or `lists`.
        target: AttributeTarget,
        /// The slug or id of the object or the list.
        identifier: String
    } + ListAttributes
);
input!(OneAttribute {
    /// Where the attribute is defined: `objects` or `lists`.
    target: AttributeTarget,
    /// The slug or id of the object or the list.
    identifier: String,
    /// The attribute's slug or id.
    attribute: String
});
input!(
    ChoicesOf {
        /// Where the attribute is defined: `objects` or `lists`.
        target: AttributeTarget,
        /// The slug or id of the object or the list.
        identifier: String,
        /// The attribute's slug or id.
        attribute: String
    } + ShowArchived
);
input!(
    RecordsOf {
        /// An object's slug, such as `people`, or its id.
        object: String
    } + Query
);
input!(OneRecord {
    /// An object's slug, such as `people`, or its id.
    object: String,
    /// A record id.
    record: String
});
input!(
    ListsOfRecord {
        /// An object's slug, such as `people`, or its id.
        object: String,
        /// A record id.
        record: String
    } + Paging
);
input!(
    NewRecord {
        /// An object's slug, such as `people`, or its id.
        object: String
    } + WriteRecord
);
input!(
    ChangeRecord {
        /// An object's slug, such as `people`, or its id.
        object: String,
        /// The id of the record to change.
        record: String
    } + WriteRecord
);
input!(
    AssertRecord {
        /// An object's slug, such as `people`, or its id.
        object: String,
        /// The slug or id of a unique attribute to find an existing record by, such as `email_addresses`.
        matching_attribute: String
    } + WriteRecord
);
input!(OneList {
    /// A list's slug or id.
    list: String
});
input!(
    EntriesOf {
        /// A list's slug or id.
        list: String
    } + Query
);
input!(OneEntry {
    /// A list's slug or id.
    list: String,
    /// An entry id.
    entry: String
});
input!(
    NewEntry {
        /// A list's slug or id.
        list: String
    } + CreateEntry
);
input!(
    ChangeEntry {
        /// A list's slug or id.
        list: String,
        /// The id of the entry to change.
        entry: String
    } + WriteEntry
);
input!(NotesOf {} + ListNotes);
input!(OneNote {
    /// A note id.
    note: String
});
input!(NewNote {} + CreateNote);
input!(TasksOf {} + ListTasks);
input!(OneTask {
    /// A task id.
    task: String
});
input!(NewTask {} + CreateTask);
input!(
    ChangeTask {
        /// The id of the task to change.
        task: String
    } + UpdateTask
);
input!(ThreadsOf {} + ListThreads);
input!(
    OneThread {
        /// A thread id.
        thread: String
    } + GetThread
);
input!(NewComment {} + CreateComment);
input!(OneMember {
    /// A workspace member id.
    member: String
});
input!(MeetingsOf {} + ListMeetings);
input!(OneMeeting {
    /// A meeting id.
    meeting: String
});
input!(
    RecordingsOf {
        /// A meeting id.
        meeting: String
    } + Paging
);
input!(OneRecording {
    /// A meeting id.
    meeting: String,
    /// The id of one of the meeting's call recordings.
    call_recording: String
});

// `Destructive` is what deletes a record, an entry, a note or a task. A host
// uses it, and `Write`, to ask a person first; only a `Read` runs unasked.
use Effect::{Destructive, Read, Write};

// What Attio asks of a token, by area. A `read-write` scope includes the read.
const SCHEMA: &str = "object_configuration:read";
const LIST_SCHEMA: &str = "list_configuration:read";
const RECORDS: &str = "record_permission:read";
const RECORDS_WRITE: &str = "record_permission:read-write";
const ENTRIES: &str = "list_entry:read";
const ENTRIES_WRITE: &str = "list_entry:read-write";
const NOTES: &str = "note:read";
const NOTES_WRITE: &str = "note:read-write";
const TASKS: &str = "task:read";
const TASKS_WRITE: &str = "task:read-write";
const COMMENTS: &str = "comment:read";
const COMMENTS_WRITE: &str = "comment:read-write";
const MEMBERS: &str = "user_management:read";
const MEETINGS: &str = "meeting:read";
const RECORDINGS: &str = "call_recording:read";

/// Every Attio operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── objects ──
        operation("objects.list", "List the objects of the workspace: people, companies, deals and those it defined itself. An object's slug is what records are asked for by.", Read, &[SCHEMA],
            |a: Attio, c: Connection, _: Nothing| async move { a.objects(&c).list().await as Result<Vec<Object>> }),
        operation("objects.get", "Get one object, by its slug or its id.", Read, &[SCHEMA],
            |a: Attio, c: Connection, i: OneObject| async move { a.objects(&c).get(&i.object).await as Result<Object> }),

        // ── attributes ──
        // Attio asks for the object scope when the attributes are an object's, and the list scope when they are a list's.
        operation("attributes.list", "List the attributes of an object or a list: each one's slug, type, and whether it may be written, is unique or holds several values. This is how the fields of a record or an entry are learnt. Needs object_configuration:read for an object and list_configuration:read for a list.", Read, &[SCHEMA, LIST_SCHEMA],
            |a: Attio, c: Connection, i: AttributesOf| async move { a.attributes(&c).list(i.target, &i.identifier, i.options).await as Result<Page<Attribute>> }),
        operation("attributes.get", "Get one attribute of an object or a list. Needs object_configuration:read for an object and list_configuration:read for a list.", Read, &[SCHEMA, LIST_SCHEMA],
            |a: Attio, c: Connection, i: OneAttribute| async move { a.attributes(&c).get(i.target, &i.identifier, &i.attribute).await as Result<Attribute> }),
        operation("attributes.options", "List the options of a select attribute: the values it may hold. Needs object_configuration:read for an object and list_configuration:read for a list.", Read, &[SCHEMA, LIST_SCHEMA],
            |a: Attio, c: Connection, i: ChoicesOf| async move { a.attributes(&c).options(i.target, &i.identifier, &i.attribute, i.options).await as Result<Vec<SelectOption>> }),
        operation("attributes.statuses", "List the statuses of a status attribute, such as the stages of a deal. Needs object_configuration:read for an object and list_configuration:read for a list.", Read, &[SCHEMA, LIST_SCHEMA],
            |a: Attio, c: Connection, i: ChoicesOf| async move { a.attributes(&c).statuses(i.target, &i.identifier, &i.attribute, i.options).await as Result<Vec<Status>> }),

        // ── records ──
        // Attio offers the query only as POST. It reads records and changes nothing.
        operation("records.query", "List the records of an object that match a filter, sorted as asked. Each record carries every value it holds, and what each attribute holds now under current. Returns 50 at a time unless a limit is given. Changes nothing.", Read, &[RECORDS, SCHEMA],
            |a: Attio, c: Connection, i: RecordsOf| async move { a.records(&c).query(&i.object, i.options).await as Result<Page<Record>> }),
        operation("records.get", "Get one record, with every value it holds and what each attribute holds now under current.", Read, &[RECORDS, SCHEMA],
            |a: Attio, c: Connection, i: OneRecord| async move { a.records(&c).get(&i.object, &i.record).await as Result<Record> }),
        operation("records.entries", "List the lists a record is on, with its entry in each. The entries' own values are read with entries.get.", Read, &[RECORDS, SCHEMA, ENTRIES],
            |a: Attio, c: Connection, i: ListsOfRecord| async move { a.records(&c).entries(&i.object, &i.record, i.options).await as Result<Page<RecordEntry>> }),
        operation("records.create", "Create a record of an object. Refused when a unique attribute, such as a person's email address, is already held by another record.", Write, &[RECORDS_WRITE, SCHEMA],
            |a: Attio, c: Connection, i: NewRecord| async move { a.records(&c).create(&i.object, i.options).await as Result<Record> }),
        operation("records.update", "Change a record, touching only the attributes named. For an attribute that holds several values, those given are added and nothing is taken away.", Write, &[RECORDS_WRITE, SCHEMA],
            |a: Attio, c: Connection, i: ChangeRecord| async move { a.records(&c).update(&i.object, &i.record, i.options).await as Result<Record> }),
        operation("records.assert", "Create a record, or change the one that already holds the same value of a unique attribute. For any other attribute that holds several values, values that were there and are not given are removed.", Write, &[RECORDS_WRITE, SCHEMA],
            |a: Attio, c: Connection, i: AssertRecord| async move { a.records(&c).assert(&i.object, &i.matching_attribute, i.options).await as Result<Record> }),
        operation("records.delete", "Delete a record, with the entries it has on lists.", Destructive, &[RECORDS_WRITE, SCHEMA],
            |a: Attio, c: Connection, i: OneRecord| async move { a.records(&c).delete(&i.object, &i.record).await as Result<()> }),

        // ── lists ──
        operation("lists.list", "List the lists the connection can see. A list's slug is what its entries are asked for by.", Read, &[LIST_SCHEMA],
            |a: Attio, c: Connection, _: Nothing| async move { a.lists(&c).list().await as Result<Vec<List>> }),
        operation("lists.get", "Get one list, by its slug or its id.", Read, &[LIST_SCHEMA],
            |a: Attio, c: Connection, i: OneList| async move { a.lists(&c).get(&i.list).await as Result<List> }),

        // ── entries ──
        // Attio offers the query only as POST. It reads entries and changes nothing.
        operation("entries.query", "List the entries of a list that match a filter, sorted as asked. Each entry names its record and carries the values of the list's own attributes. Returns 50 at a time unless a limit is given. Changes nothing.", Read, &[ENTRIES, LIST_SCHEMA],
            |a: Attio, c: Connection, i: EntriesOf| async move { a.entries(&c).query(&i.list, i.options).await as Result<Page<Entry>> }),
        operation("entries.get", "Get one entry of a list, with the values of the list's own attributes.", Read, &[ENTRIES, LIST_SCHEMA],
            |a: Attio, c: Connection, i: OneEntry| async move { a.entries(&c).get(&i.list, &i.entry).await as Result<Entry> }),
        operation("entries.create", "Put a record on a list. A record may be on a list more than once, so doing this twice makes two entries.", Write, &[ENTRIES_WRITE, LIST_SCHEMA],
            |a: Attio, c: Connection, i: NewEntry| async move { a.entries(&c).create(&i.list, i.options).await as Result<Entry> }),
        operation("entries.update", "Change an entry, touching only the attributes named: move a deal to another stage, for one. For an attribute that holds several values, those given are added and nothing is taken away.", Write, &[ENTRIES_WRITE, LIST_SCHEMA],
            |a: Attio, c: Connection, i: ChangeEntry| async move { a.entries(&c).update(&i.list, &i.entry, i.options).await as Result<Entry> }),
        operation("entries.delete", "Take a record off a list by deleting its entry. The record itself stays.", Destructive, &[ENTRIES_WRITE, LIST_SCHEMA],
            |a: Attio, c: Connection, i: OneEntry| async move { a.entries(&c).delete(&i.list, &i.entry).await as Result<()> }),

        // ── notes ──
        operation("notes.list", "List the notes on one record, or on every record, with their text. Returns 10 at a time unless a limit is given.", Read, &[NOTES, SCHEMA, RECORDS],
            |a: Attio, c: Connection, i: NotesOf| async move { a.notes(&c).list(i.options).await as Result<Page<Note>> }),
        operation("notes.get", "Get one note, as plain text and as Markdown.", Read, &[NOTES, SCHEMA, RECORDS],
            |a: Attio, c: Connection, i: OneNote| async move { a.notes(&c).get(&i.note).await as Result<Note> }),
        operation("notes.create", "Write a note on a record. Everyone who can see the record sees it.", Write, &[NOTES_WRITE, SCHEMA, RECORDS],
            |a: Attio, c: Connection, i: NewNote| async move { a.notes(&c).create(i.options).await as Result<Note> }),
        operation("notes.delete", "Delete a note.", Destructive, &[NOTES_WRITE],
            |a: Attio, c: Connection, i: OneNote| async move { a.notes(&c).delete(&i.note).await as Result<()> }),

        // ── tasks ──
        operation("tasks.list", "List tasks: all of them, or those about one record, assigned to one person, done or not done. Returns 50 at a time unless a limit is given.", Read, &[TASKS, SCHEMA, RECORDS, MEMBERS],
            |a: Attio, c: Connection, i: TasksOf| async move { a.tasks(&c).list(i.options).await as Result<Page<Task>> }),
        operation("tasks.get", "Get one task.", Read, &[TASKS, SCHEMA, RECORDS, MEMBERS],
            |a: Attio, c: Connection, i: OneTask| async move { a.tasks(&c).get(&i.task).await as Result<Task> }),
        operation("tasks.create", "Create a task, about records and assigned to workspace members when those are given. The people it is assigned to are told.", Write, &[TASKS_WRITE, SCHEMA, RECORDS, MEMBERS],
            |a: Attio, c: Connection, i: NewTask| async move { a.tasks(&c).create(i.options).await as Result<Task> }),
        operation("tasks.update", "Change a task: its deadline, whether it is done, the records it is about and who it is assigned to. Its text cannot be changed. A list of records or of people replaces the one that was there.", Write, &[TASKS_WRITE, SCHEMA, RECORDS, MEMBERS],
            |a: Attio, c: Connection, i: ChangeTask| async move { a.tasks(&c).update(&i.task, i.options).await as Result<Task> }),
        operation("tasks.delete", "Delete a task.", Destructive, &[TASKS_WRITE],
            |a: Attio, c: Connection, i: OneTask| async move { a.tasks(&c).delete(&i.task).await as Result<()> }),

        // ── threads ──
        // Attio also asks for the scopes of what the thread is on: a record's, or a list entry's.
        operation("threads.list", "List the comment threads on one record or on one list entry, each with its first comments. Also needs the scopes to read the record or the entry.", Read, &[COMMENTS],
            |a: Attio, c: Connection, i: ThreadsOf| async move { a.threads(&c).list(i.options).await as Result<Page<Thread>> }),
        operation("threads.get", "Get one thread and its comments, oldest first, up to 250 at a time. Also needs the scopes to read the record or the entry.", Read, &[COMMENTS],
            |a: Attio, c: Connection, i: OneThread| async move { a.threads(&c).get(&i.thread, i.options).await as Result<Thread> }),
        operation("threads.comment", "Write a comment as one of the workspace's members: a reply in a thread, or the first comment on a record or a list entry. Everyone who can see the record or the entry sees it. Also needs the scopes to read the record or the entry.", Write, &[COMMENTS_WRITE],
            |a: Attio, c: Connection, i: NewComment| async move { a.threads(&c).comment(i.options).await as Result<Comment> }),

        // ── workspace members ──
        operation("workspace_members.list", "List the people who have access to the workspace. Their ids are what a task is assigned to and what a comment is written as.", Read, &[MEMBERS],
            |a: Attio, c: Connection, _: Nothing| async move { a.workspace_members(&c).list().await as Result<Vec<WorkspaceMember>> }),
        operation("workspace_members.get", "Get one member of the workspace.", Read, &[MEMBERS],
            |a: Attio, c: Connection, i: OneMember| async move { a.workspace_members(&c).get(&i.member).await as Result<WorkspaceMember> }),

        // ── meetings ──
        operation("meetings.list", "List meetings: all of them, or those linked to one record, with given people, or within a span of time. Works only for a token that acts for the whole workspace.", Read, &[MEETINGS, RECORDS],
            |a: Attio, c: Connection, i: MeetingsOf| async move { a.meetings(&c).list(i.options).await as Result<Page<Meeting>> }),
        operation("meetings.get", "Get one meeting, with who was invited and the records it is linked to.", Read, &[MEETINGS, RECORDS],
            |a: Attio, c: Connection, i: OneMeeting| async move { a.meetings(&c).get(&i.meeting).await as Result<Meeting> }),

        // ── call recordings ──
        operation("call_recordings.list", "List a meeting's recordings, without their transcripts. Empty when the meeting was not recorded.", Read, &[MEETINGS, RECORDINGS],
            |a: Attio, c: Connection, i: RecordingsOf| async move { a.call_recordings(&c).list(&i.meeting, i.options).await as Result<Page<CallRecording>> }),
        operation("call_recordings.get", "Get one recording of a meeting with what was said in it: one entry for each thing said, with the speaker, the start and the end, and the whole as text.", Read, &[MEETINGS, RECORDINGS],
            |a: Attio, c: Connection, i: OneRecording| async move { a.call_recordings(&c).get(&i.meeting, &i.call_recording).await as Result<CallRecording> }),
    ]
}
