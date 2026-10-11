//! Every typed method as a named operation.
//!
//! An operation's input is a JSON object with the method's plain arguments
//! and its options side by side: `{ "object": "people", "record": "bf07…", "values": { … } }`.
//! Both schemas are generated from the same types the typed methods use, so
//! the two ways of calling cannot drift apart.

use std::future::Future;
use std::sync::OnceLock;

use schemars::JsonSchema;
use serde::Deserialize;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Effect, Error, ErrorKind, OperationInfo, Page, Result, schema_of};

use crate::Attio;
use crate::models::{
    Attribute, CallRecording, CallRecordingRow, Comment, CreateComment, CreateNote, CreateTask, Entry, EntryRow, List,
    ListAttributes, ListMeetings, ListNotes, ListTasks, ListThreads, Meeting, Note, NoteRow, Object, Paging,
    QueryEntries, QueryRecords, Record, RecordEntry, RecordRow, SelectOption, ShowArchived, Status, Target, Task,
    Thread, ThreadRow, TokenInfo, UpdateTask, WorkspaceMember, WriteEntry, WriteRecord,
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
///
/// An object that lists no fields at all is left open when it is somewhere
/// inside, because that is how a filter and the values of a record are
/// described: their fields are the workspace's own. The input itself is
/// always closed, so an operation that takes nothing refuses anything.
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
    if let Value::Object(input) = &mut schema {
        input.entry("properties").or_insert_with(|| Value::Object(Map::new()));
    }
    close(&mut schema);
    schema
}

/// Where in `input` the first field is that the schema at `node` does not
/// list, and that field's name.
///
/// A field that is not known would be dropped in silence, with what it said:
/// a deadline, a filter, the text of a note. The input types cannot refuse one
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
                // what a workspace knows about a customer. Only the field's name,
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
    /// An object, by its slug such as `people` or by its id.
    object: String
});
input!(
    AttributesOf {
        /// Whether the attributes belong to an object or to a list: `objects` or `lists`.
        target: Target,
        /// The object or the list, by its slug or id.
        identifier: String
    } + ListAttributes
);
input!(OneAttribute {
    /// Whether the attribute belongs to an object or to a list: `objects` or `lists`.
    target: Target,
    /// The object or the list, by its slug or id.
    identifier: String,
    /// The attribute, by its slug or id.
    attribute: String
});
input!(
    ChoicesOf {
        /// Whether the attribute belongs to an object or to a list: `objects` or `lists`.
        target: Target,
        /// The object or the list, by its slug or id.
        identifier: String,
        /// The attribute, by its slug or id.
        attribute: String
    } + ShowArchived
);
input!(
    RecordsOf {
        /// An object, by its slug such as `people` or by its id.
        object: String
    } + QueryRecords
);
input!(OneRecord {
    /// The object the record belongs to, by its slug or id.
    object: String,
    /// A record id.
    record: String
});
input!(
    ListsOfRecord {
        /// The object the record belongs to, by its slug or id.
        object: String,
        /// A record id.
        record: String
    } + Paging
);
input!(
    NewRecord {
        /// The object to create a record of, by its slug or id.
        object: String
    } + WriteRecord
);
input!(
    ChangeRecord {
        /// The object the record belongs to, by its slug or id.
        object: String,
        /// The id of the record to change.
        record: String
    } + WriteRecord
);
input!(
    AssertRecord {
        /// The object the record belongs to, by its slug or id.
        object: String,
        /// The slug or id of the unique attribute that says whether the record exists already, such as `email_addresses`.
        matching_attribute: String
    } + WriteRecord
);
input!(OneList {
    /// A list, by its slug or id.
    list: String
});
input!(
    EntriesOf {
        /// A list, by its slug or id.
        list: String
    } + QueryEntries
);
input!(OneEntry {
    /// The list the entry is on, by its slug or id.
    list: String,
    /// An entry id.
    entry: String
});
input!(
    NewEntry {
        /// The list to add to, by its slug or id.
        list: String,
        /// The object of the record to add, by its slug or id.
        parent_object: String,
        /// The id of the record to add.
        parent_record: String
    } + WriteEntry
);
input!(
    ChangeEntry {
        /// The list the entry is on, by its slug or id.
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
input!(
    NewNote {
        /// The object of the record the note is about, by its slug or id.
        parent_object: String,
        /// The id of the record the note is about.
        parent_record: String
    } + CreateNote
);
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
    } + Paging
);
input!(
    NewComment {
        /// The id of the workspace member the comment is written as.
        author: String
    } + CreateComment
);
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
    /// The id of the meeting the recording belongs to.
    meeting: String,
    /// A call recording id.
    recording: String
});

// A host lets a `Read` run freely and asks a person before anything else.
// `Write` adds something or changes a value, and Attio keeps what the value
// was in the attribute's history. `Destructive` deletes.
use Effect::{Destructive, Read, Write};

// What each area needs, as Attio's documentation lists it for every endpoint.
const OBJECTS: &[&str] = &["object_configuration:read"];
const LISTS: &[&str] = &["list_configuration:read"];
// An attribute belongs to an object or to a list, and each has its own scope.
const ATTRIBUTES: &[&str] = &["object_configuration:read", "list_configuration:read"];
const RECORDS: &[&str] = &["record_permission:read", "object_configuration:read"];
const RECORD_LISTS: &[&str] = &["record_permission:read", "object_configuration:read", "list_entry:read"];
const RECORDS_WRITE: &[&str] = &["record_permission:read-write", "object_configuration:read"];
const ENTRIES: &[&str] = &["list_entry:read", "list_configuration:read"];
const ENTRIES_WRITE: &[&str] = &["list_entry:read-write", "list_configuration:read"];
const NOTES: &[&str] = &["note:read", "object_configuration:read", "record_permission:read"];
const NOTES_WRITE: &[&str] = &["note:read-write", "object_configuration:read", "record_permission:read"];
const NOTES_DELETE: &[&str] = &["note:read-write"];
const TASKS: &[&str] = &[
    "task:read",
    "object_configuration:read",
    "record_permission:read",
    "user_management:read",
];
const TASKS_WRITE: &[&str] = &[
    "task:read-write",
    "object_configuration:read",
    "record_permission:read",
    "user_management:read",
];
const TASKS_DELETE: &[&str] = &["task:read-write"];
// A thread is on a record or on a list entry, and each has its own scopes.
const THREADS: &[&str] = &[
    "comment:read",
    "object_configuration:read",
    "record_permission:read",
    "list_configuration:read",
    "list_entry:read",
];
const THREADS_WRITE: &[&str] = &[
    "comment:read-write",
    "object_configuration:read",
    "record_permission:read",
    "list_configuration:read",
    "list_entry:read",
];
const MEMBERS: &[&str] = &["user_management:read"];
const MEETINGS: &[&str] = &["meeting:read", "record_permission:read"];
const RECORDINGS: &[&str] = &["meeting:read", "call_recording:read"];

/// Every Attio operation except identity and resource lookup, which the
/// integration adds itself.
pub(crate) fn all() -> &'static [Operation] {
    static ALL: OnceLock<Vec<Operation>> = OnceLock::new();
    ALL.get_or_init(build)
}

#[rustfmt::skip]
fn build() -> Vec<Operation> {
    vec![
        // ── meta ──
        operation("meta.identify", "Describe the connection's token: the workspace it belongs to, the scopes it was given, and whether it acts as the workspace or as one member.", Read, &[],
            |a: Attio, c: Connection, _: Nothing| async move { a.meta(&c).identify().await as Result<TokenInfo> }),

        // ── objects ──
        operation("objects.list", "List the kinds of record the workspace keeps: Attio's own, such as people and companies, and those the workspace made. Start here to learn what can be queried.", Read, OBJECTS,
            |a: Attio, c: Connection, _: Nothing| async move { a.objects(&c).list().await as Result<Vec<Object>> }),
        operation("objects.get", "Get one object, by its slug or id.", Read, OBJECTS,
            |a: Attio, c: Connection, i: OneObject| async move { a.objects(&c).get(&i.object).await as Result<Object> }),

        // ── attributes ──
        operation("attributes.list", "List the fields of an object or of a list: each one's slug, its type, and whether it is unique, required or holds several values. This is how to learn what a record's values are called.", Read, ATTRIBUTES,
            |a: Attio, c: Connection, i: AttributesOf| async move { a.attributes(&c).list(i.target, &i.identifier, i.options).await as Result<Page<Attribute>> }),
        operation("attributes.get", "Get one field of an object or of a list.", Read, ATTRIBUTES,
            |a: Attio, c: Connection, i: OneAttribute| async move { a.attributes(&c).get(i.target, &i.identifier, &i.attribute).await as Result<Attribute> }),
        operation("attributes.options", "List what a select attribute can be set to.", Read, ATTRIBUTES,
            |a: Attio, c: Connection, i: ChoicesOf| async move { a.attributes(&c).options(i.target, &i.identifier, &i.attribute, i.options).await as Result<Vec<SelectOption>> }),
        operation("attributes.statuses", "List the statuses a status attribute can be at, such as the stages of a pipeline.", Read, ATTRIBUTES,
            |a: Attio, c: Connection, i: ChoicesOf| async move { a.attributes(&c).statuses(i.target, &i.identifier, &i.attribute, i.options).await as Result<Vec<Status>> }),

        // ── records ──
        // Attio takes a query as POST. It reads records and changes nothing.
        operation("records.query", "Find records of an object by a filter, in a chosen order. Changes nothing. Each row carries the record's current values by themselves; records.get returns them in full.", Read, RECORDS,
            |a: Attio, c: Connection, i: RecordsOf| async move { a.records(&c).query(&i.object, i.options).await as Result<Page<RecordRow>> }),
        operation("records.get", "Get one record: every value it holds, with when each was set and by whom, and each attribute's current value by itself.", Read, RECORDS,
            |a: Attio, c: Connection, i: OneRecord| async move { a.records(&c).get(&i.object, &i.record).await as Result<Record> }),
        operation("records.entries", "List the lists a record is on, with the id of its entry on each.", Read, RECORD_LISTS,
            |a: Attio, c: Connection, i: ListsOfRecord| async move { a.records(&c).entries(&i.object, &i.record, i.options).await as Result<Page<RecordEntry>> }),
        operation("records.create", "Create a record of an object. Refused when a unique attribute, such as an email address, already belongs to another record.", Write, RECORDS_WRITE,
            |a: Attio, c: Connection, i: NewRecord| async move { a.records(&c).create(&i.object, i.options).await as Result<Record> }),
        operation("records.update", "Change a record's values. An attribute that holds several values has the given ones added and none removed; any other takes the given value in place of the one it had.", Write, RECORDS_WRITE,
            |a: Attio, c: Connection, i: ChangeRecord| async move { a.records(&c).update(&i.object, &i.record, i.options).await as Result<Record> }),
        operation("records.assert", "Create a record, or change the one that already has the same value for a unique attribute. On a record that exists, an attribute that holds several values ends up with exactly the ones given: others it had are removed.", Write, RECORDS_WRITE,
            |a: Attio, c: Connection, i: AssertRecord| async move { a.records(&c).assert(&i.object, &i.matching_attribute, i.options).await as Result<Record> }),
        operation("records.delete", "Delete a record.", Destructive, RECORDS_WRITE,
            |a: Attio, c: Connection, i: OneRecord| async move { a.records(&c).delete(&i.object, &i.record).await as Result<()> }),

        // ── lists ──
        operation("lists.list", "List the workspace's lists, such as its pipelines, with the objects each one holds.", Read, LISTS,
            |a: Attio, c: Connection, _: Nothing| async move { a.lists(&c).list().await as Result<Vec<List>> }),
        operation("lists.get", "Get one list, by its slug or id.", Read, LISTS,
            |a: Attio, c: Connection, i: OneList| async move { a.lists(&c).get(&i.list).await as Result<List> }),

        // ── entries ──
        // Attio takes a query as POST. It reads entries and changes nothing.
        operation("entries.query", "Find entries of a list by a filter, in a chosen order. Changes nothing. Each row carries the record it is for and the current values of the list's own attributes; entries.get returns them in full.", Read, ENTRIES,
            |a: Attio, c: Connection, i: EntriesOf| async move { a.entries(&c).query(&i.list, i.options).await as Result<Page<EntryRow>> }),
        operation("entries.get", "Get one entry of a list: every value of the list's attributes, and each one's current value by itself.", Read, ENTRIES,
            |a: Attio, c: Connection, i: OneEntry| async move { a.entries(&c).get(&i.list, &i.entry).await as Result<Entry> }),
        operation("entries.create", "Put a record on a list, with values for the list's own attributes.", Write, ENTRIES_WRITE,
            |a: Attio, c: Connection, i: NewEntry| async move { a.entries(&c).create(&i.list, &i.parent_object, &i.parent_record, i.options).await as Result<Entry> }),
        operation("entries.update", "Change an entry's values, such as its stage. An attribute that holds several values has the given ones added and none removed; any other takes the given value in place of the one it had.", Write, ENTRIES_WRITE,
            |a: Attio, c: Connection, i: ChangeEntry| async move { a.entries(&c).update(&i.list, &i.entry, i.options).await as Result<Entry> }),
        operation("entries.delete", "Take an entry off its list. The record it was for stays.", Destructive, ENTRIES_WRITE,
            |a: Attio, c: Connection, i: OneEntry| async move { a.entries(&c).delete(&i.list, &i.entry).await as Result<()> }),

        // ── notes ──
        operation("notes.list", "List the notes on one record, on the records of one object, or on everything: what each is on and what it is called, without its text.", Read, NOTES,
            |a: Attio, c: Connection, i: NotesOf| async move { a.notes(&c).list(i.options).await as Result<Page<NoteRow>> }),
        operation("notes.get", "Get one note, with what it says as plain text and as Markdown.", Read, NOTES,
            |a: Attio, c: Connection, i: OneNote| async move { a.notes(&c).get(&i.note).await as Result<Note> }),
        operation("notes.create", "Write a note on a record, in plain text or Markdown.", Write, NOTES_WRITE,
            |a: Attio, c: Connection, i: NewNote| async move { a.notes(&c).create(&i.parent_object, &i.parent_record, i.options).await as Result<Note> }),
        operation("notes.delete", "Delete a note.", Destructive, NOTES_DELETE,
            |a: Attio, c: Connection, i: OneNote| async move { a.notes(&c).delete(&i.note).await as Result<()> }),

        // ── tasks ──
        operation("tasks.list", "List tasks: all of them, those about one record, one person's, those done or those still to do.", Read, TASKS,
            |a: Attio, c: Connection, i: TasksOf| async move { a.tasks(&c).list(i.options).await as Result<Page<Task>> }),
        operation("tasks.get", "Get one task.", Read, TASKS,
            |a: Attio, c: Connection, i: OneTask| async move { a.tasks(&c).get(&i.task).await as Result<Task> }),
        operation("tasks.create", "Create a task in plain text, with a deadline, the records it concerns and who is to do it.", Write, TASKS_WRITE,
            |a: Attio, c: Connection, i: NewTask| async move { a.tasks(&c).create(i.options).await as Result<Task> }),
        operation("tasks.update", "Change a task's deadline, whether it is done, the records it concerns or who it is assigned to. Its text cannot be changed.", Write, TASKS_WRITE,
            |a: Attio, c: Connection, i: ChangeTask| async move { a.tasks(&c).update(&i.task, i.options).await as Result<Task> }),
        operation("tasks.delete", "Delete a task.", Destructive, TASKS_DELETE,
            |a: Attio, c: Connection, i: OneTask| async move { a.tasks(&c).delete(&i.task).await as Result<()> }),

        // ── threads ──
        operation("threads.list", "List the threads of comments on one record or on one list entry: where each is and how much was said, without the comments.", Read, THREADS,
            |a: Attio, c: Connection, i: ThreadsOf| async move { a.threads(&c).list(i.options).await as Result<Page<ThreadRow>> }),
        operation("threads.get", "Get one thread with its comments, oldest first.", Read, THREADS,
            |a: Attio, c: Connection, i: OneThread| async move { a.threads(&c).get(&i.thread, i.options).await as Result<Thread> }),
        operation("threads.comment", "Write a comment as a workspace member: a reply in a thread, or the first comment of a new thread on a record or on a list entry. Others in the workspace see it.", Write, THREADS_WRITE,
            |a: Attio, c: Connection, i: NewComment| async move { a.threads(&c).comment(&i.author, i.options).await as Result<Comment> }),

        // ── workspace members ──
        operation("workspace_members.list", "List the people who work in the workspace, suspended ones included.", Read, MEMBERS,
            |a: Attio, c: Connection, _: Nothing| async move { a.workspace_members(&c).list().await as Result<Vec<WorkspaceMember>> }),
        operation("workspace_members.get", "Get one workspace member by id.", Read, MEMBERS,
            |a: Attio, c: Connection, i: OneMember| async move { a.workspace_members(&c).get(&i.member).await as Result<WorkspaceMember> }),

        // ── meetings ──
        operation("meetings.list", "List meetings: all of them, those in a range of time, those that concern a record, or those with certain people in them. Attio marks this as beta.", Read, MEETINGS,
            |a: Attio, c: Connection, i: MeetingsOf| async move { a.meetings(&c).list(i.options).await as Result<Page<Meeting>> }),
        operation("meetings.get", "Get one meeting, with who was invited and the records it concerns. Attio marks this as beta.", Read, MEETINGS,
            |a: Attio, c: Connection, i: OneMeeting| async move { a.meetings(&c).get(&i.meeting).await as Result<Meeting> }),

        // ── call recordings ──
        operation("call_recordings.list", "List a meeting's call recordings and whether each is ready, without what was said. Attio marks this as beta.", Read, RECORDINGS,
            |a: Attio, c: Connection, i: RecordingsOf| async move { a.call_recordings(&c).list(&i.meeting, i.options).await as Result<Page<CallRecordingRow>> }),
        operation("call_recordings.get", "Get one call recording with its transcript: the whole text, and one entry for each thing said with the speaker, the start and the end. Attio marks this as beta.", Read, RECORDINGS,
            |a: Attio, c: Connection, i: OneRecording| async move { a.call_recordings(&c).get(&i.meeting, &i.recording).await as Result<CallRecording> }),
    ]
}
