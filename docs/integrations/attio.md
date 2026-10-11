# Attio

**Status:** built and tested against a local server that answers as Attio's documentation says. Not yet run against a real Attio workspace.

Socket's Attio integration gives a program a workspace's CRM: its objects and their attributes, records, lists and their entries, notes, tasks, comment threads, workspace members, and meetings with their call recordings and transcripts. An Attio workspace defines its own data model, so the methods are generic: you ask the workspace what it holds, then read and write records by the names it gave. There are 39 typed methods, and the same 39 as operations callable by name with JSON, plus identity and lookup of an object or a list. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against Attio's documentation.

## Connect

Add the crate with the Attio feature:

```sh
cargo add socketkit --features attio
```

### With a workspace's API key

A workspace's own API key is an access token, and is sent the same way as one an app was granted.

```rust
use std::sync::Arc;
use socketkit::attio::Attio;
use socketkit::{ConnectionKey, ProviderId, Socket};

let attio = Attio::with_token("…");
let socket = Socket::in_memory().integration(Arc::new(attio.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("attio")?, "me")).await?;
```

Every call uses that token. Its scopes are the ones ticked when the key was made in the workspace's settings, and can be changed there.

### With your own Attio app, to connect your customers' workspaces

Create an app at build.attio.com, switch on OAuth, add your callback route as a redirect URI, and choose the app's scopes there.

```rust
use socketkit::attio::{Attio, AttioOAuth, TokenLevel};
use socketkit::{OAuthClient, SecretString};

let attio = Attio::with_oauth(AttioOAuth {
    client: OAuthClient {
        client_id: config.attio_client_id,
        client_secret: SecretString::new(config.attio_client_secret),
        redirect_uri: "https://yourapp.example/oauth/attio/callback".parse()?,
    },
    // Whose permissions a token has. `None` is Attio's default, the workspace's.
    token_level: Some(TokenLevel::Workspace),
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(attio.clone())).build()?;

// When a user clicks "Connect Attio":
let key = ConnectionKey::new(ProviderId::new("attio")?, customer.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

`Attio::with_oauth(client)` with a plain `OAuthClient` also works when the default is enough.

| Setting | What it does |
| --- | --- |
| `token_level` | `Workspace`: the token acts as the workspace as a whole, and only an administrator of the workspace can grant it. `User`: the token acts as the member who granted it, and reaches only what that member can. |

Four things are particular to Attio:

- **Scopes are not asked for at sign-in.** An Attio app's scopes are set in Attio's developer console, and every token the app is granted carries them. The sign-in page has no parameter for scopes, so Socket sends none, whatever is passed to `begin_authorization`, and there is no `scopes` setting. The provider's default scopes are empty. Each operation still says what it needs in `required_scopes`: that is the list of boxes to tick in the console for the operations you use.
- **A token does not expire and there is no refresh token.** Attio states no lifetime. If a token is ever rejected, the connection reports `ReconnectRequired`.
- **PKCE is on.** Attio requires it for a token that acts as one member and accepts it for any. The verifier is sent with the code, beside the client secret, which goes in the form.
- **A token says what it may do.** `attio.meta(&connection).identify()` returns the workspace, the token's scopes as a list, and whether it acts as the workspace or as a member. Use it to tell a person which scope is missing before an operation is refused.

## Identity and lookup

```rust
let workspace = attio.identity(&connection).await?;     // Account { id, name, email }
let people = attio.resolve(&connection, "people").await?; // Resource { id, label, description }
```

`identity` reads `GET /v2/self` and needs no scope. An Attio token belongs to a workspace, so the account is the workspace: `id` is the workspace's id, `name` its name (or its slug, or its id, when it has no name), and `email` is always absent. Attio answers this request with a success even for a token it no longer accepts, and says `"active": false` in the answer; Socket reports that as `ReconnectRequired`.

`resolve` accepts an object's slug or id (`people`, or `objects/people`), or a list's written `lists/enterprise_sales`, and confirms it exists and the token can see it. The resource's `id` is Attio's own id for the object or the list, which every method takes where it takes a slug, and which does not change when the slug does. It needs `object_configuration:read` for an object and `list_configuration:read` for a list.

## Use the typed methods

Methods are grouped as Attio groups its API. Each group is reached through the `Attio` value and a connection: `attio.records(&connection)`.

**How arguments are split.** What identifies the thing acted on is a plain argument: an object, a list, a record id. Content and optional filters are structs from `socketkit::attio::models`, where a field you leave unset is not sent.

**An object and a list are named by slug or by id.** `people` and `97052eb9-e65e-443f-a297-f2d9a4a7f795` name the same object. Socket lets through only letters, digits, `_` and `-`, at most 128 of them, so what is passed can only ever be one segment of the path; anything else is refused before a request is made, and is not repeated in the error.

**Names are Attio's own**: `api_slug`, `parent_record_id`, `content_plaintext`. What Attio's documentation says about a field holds here too.

### Learning what a workspace holds: `objects`, `attributes`, `lists`

| Method | Returns |
| --- | --- |
| `objects().list()` | `Vec<Object>`: every kind of record, Attio's own and the workspace's |
| `objects().get(object)` | `Object` |
| `attributes().list(target, identifier, ListAttributes)` | `Page<Attribute>`: the fields of an object or of a list |
| `attributes().get(target, identifier, attribute)` | `Attribute` |
| `attributes().options(target, identifier, attribute, ShowArchived)` | `Vec<SelectOption>`: what a select attribute can be set to |
| `attributes().statuses(target, identifier, attribute, ShowArchived)` | `Vec<Status>`: the stages of a status attribute |
| `lists().list()` | `Vec<List>` |
| `lists().get(list)` | `List` |

`target` is `Target::Objects` or `Target::Lists` (`"objects"` or `"lists"` in JSON) and `identifier` is the object or the list, exactly as Attio's own path has them.

Start with `objects().list()`, then `attributes().list(Target::Objects, "people", …)`. An `Attribute` carries its `api_slug`, which is the name its values go by in a record, a filter and a sort; its `type`; and `is_unique`, `is_required`, `is_multiselect` and `is_writable`. A list has attributes of its own, such as the stage of a pipeline: ask for them with `Target::Lists`.

Objects need `object_configuration:read`, lists `list_configuration:read`, and attributes whichever of the two their `target` is.

### `attio.records(&connection)`

| Method | Returns |
| --- | --- |
| `query(object, QueryRecords)` | `Page<RecordRow>` |
| `get(object, record)` | `Record` |
| `entries(object, record, Paging)` | `Page<RecordEntry>`: the lists a record is on |
| `create(object, WriteRecord)` | `Record` |
| `update(object, record, WriteRecord)` | `Record` |
| `assert(object, matching_attribute, WriteRecord)` | `Record` |
| `delete(object, record)` | nothing |

```rust
use socketkit::attio::models::{Direction, QueryRecords, Sort};

let found = attio.records(&connection).query("people", QueryRecords {
    filter: serde_json::from_value(serde_json::json!({ "email_addresses": "ada@example.com" }))?,
    sorts: Some(vec![Sort::by("name", Direction::Asc)]),
    attributes: Some(vec!["name".into(), "job_title".into()]),
    ..QueryRecords::default()
}).await?;
let ada = attio.records(&connection).get("people", &found.items[0].id.record_id).await?;
let title = ada.values.newest("job_title").map(|value| value.plain());
```

**Values, and the current value.** Attio keeps every attribute as a list of values, each with `active_from`, `active_until`, `created_by_actor`, its `attribute_type` and the fields of that type. That one shape serves an attribute with a single value, one with several at once, and the history of either. A `Record` keeps it as Attio sends it, in `values`, and adds the answer nearly every caller wants, in `current`:

```json
{
  "values": {
    "name": [{ "active_from": "2023-01-01T15:00:00.000000000Z", "active_until": null, "created_by_actor": { "type": "workspace-member", "id": "50cf…" },
               "attribute_type": "personal-name", "first_name": "Ada", "last_name": "Lovelace", "full_name": "Ada Lovelace" }],
    "email_addresses": [{ "…": "…", "email_address": "ada@example.com" }, { "…": "…", "email_address": "countess@example.org" }],
    "job_title": []
  },
  "current": { "name": "Ada Lovelace", "email_addresses": ["ada@example.com", "countess@example.org"], "job_title": null }
}
```

- A value is **active** while its `active_until` is null. `current` is read from the active values only.
- In `current`, an attribute with no active value is `null`, one with a single active value is that value, and one with several is a list of them in Attio's order. Whether an attribute *could* hold several is not something its values say, so a multi-select with one option chosen reads as that option, not as a list of one; `is_multiselect` on the attribute says which it is.
- A value by itself is the one thing its type comes down to: the text, number, date or flag; the title of a select option or of a status; the email address, phone number, domain or full name. A type with several parts (a currency with its code, a reference to a record, a location, an interaction) is an object of its own fields, without those that are null. So is a type Socket has not met: nothing is dropped for being unknown.
- In Rust, `record.values` answers for one attribute at a time: `all(slug)` is every value Attio listed, `active(slug)` those in force now, and `newest(slug)` the single active value that became active last (the first listed, when several became active together). `newest` is one value where `current` is all the active ones: for a multi-select they differ, and `summary()` is what fills `current`. `AttributeValue::plain()` is the value by itself, and `AttributeValue::fields` holds the type's own fields under Attio's names.

**A query returns light rows.** Attio answers a query with every value of every record. A `RecordRow` carries the record's id, `created_at`, `web_url` and `current`, and leaves when each value was set and by whom to `get`. `attributes` narrows `current` to the slugs you name. An attribute the record has no value for is there as `null`; a name the record has no attribute by, a misspelt slug for instance, is **left out of the row**, so the two cannot be mistaken for each other. An empty list is read as no list, and returns every attribute. Attio has no way to be told which attributes to return, so this is done after its answer arrives: it makes what you receive smaller, not what Attio sends.

**Filters and sorts** are Attio's own. `filter` is passed as given: `{ "name": "Ada Lovelace" }` for equality, `{ "twitter_follower_count": { "$gte": 100 } }` with an operator, `$and`, `$or` and `$not` to combine, and `path` with `constraints` to filter by a related record. A `Sort` names an `attribute` (and optionally a `field` of it, such as `last_name`) or a `path` to an attribute of a related record, and not both.

**Writing values.** `WriteRecord.values` is an object from attribute slug or id to the value to write, in the form Attio's documentation gives for the attribute's type: `"Ada Lovelace"`, `["ada@example.com"]`, `{ "currency_value": 100 }`. It is sent exactly as given. An unset field of a struct is not sent, but here a `null` is a value to write, and is.

- `create` is refused by Attio when a unique attribute, such as an email address, already belongs to another record.
- `update` sends a `PATCH`. An attribute that holds several values has the given ones **added** and none removed; any other attribute takes the given value in place of the one it had, and Attio keeps the old one in the attribute's history. A change that names no attribute is refused.
- `assert` creates a record, or changes the one that already has the same value for `matching_attribute`, which must be a unique attribute and be among the values given. **On a record that exists, an attribute that holds several values ends up with exactly the ones given: any it had beside them are removed.** Only the matching attribute itself is added to and never taken from.

Reading needs `record_permission:read` and `object_configuration:read`; `entries` needs `list_entry:read` as well. Writing and deleting need `record_permission:read-write` and `object_configuration:read`.

### `attio.entries(&connection)`

| Method | Returns |
| --- | --- |
| `query(list, QueryEntries)` | `Page<EntryRow>` |
| `get(list, entry)` | `Entry` |
| `create(list, parent_object, parent_record, WriteEntry)` | `Entry` |
| `update(list, entry, WriteEntry)` | `Entry` |
| `delete(list, entry)` | nothing |

An entry is a record's place on a list, with values for the list's own attributes in `entry_values` and their current values in `current`, read the same way as a record's. The record's own values are not repeated: `parent_object` and `parent_record_id` say which record to `get`. `update` adds to an attribute that holds several values and replaces any other, as `records().update` does. `delete` takes the entry off the list and leaves the record.

Reading needs `list_entry:read` and `list_configuration:read`; writing and deleting need `list_entry:read-write` and `list_configuration:read`.

### `attio.notes(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListNotes)` | `Page<NoteRow>`: what each note is on and what it is called, without its text |
| `get(note)` | `Note`, with `content_plaintext` and `content_markdown` |
| `create(parent_object, parent_record, CreateNote)` | `Note` |
| `delete(note)` | nothing |

`ListNotes` narrows the list to one record (`parent_object` with `parent_record_id`) or to the records of one object (`parent_object` alone). A new note is plain text unless `format` is `NoteFormat::Markdown`, which takes headings to the third level, lists, bold, italic, strikethrough, `==highlight==` and links, and no images. `created_at` records a note from the past; Attio refuses a time in the future.

Reading needs `note:read`, `object_configuration:read` and `record_permission:read`; `create` needs `note:read-write` with the other two; `delete` needs `note:read-write`.

### `attio.tasks(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListTasks)` | `Page<Task>` |
| `get(task)` | `Task` |
| `create(CreateTask)` | `Task` |
| `update(task, UpdateTask)` | `Task` |
| `delete(task)` | nothing |

`ListTasks` filters by a record (`linked_object` with `linked_record_id`), by `assignee` (a member's id or email address; the word `null` asks for tasks assigned to nobody), by `is_completed`, and sorts by `TaskSort`. Attio reads an empty `assignee` as "assigned to nobody" too; Socket refuses a blank one, so that a filter left empty by mistake does not become that question. A blank is refused the same way for the other filters that take text, here and in `meetings().list`, and for a deadline. A new task's `content` is plain text of at most 2000 characters: Attio takes no formatting and no links to records in it. Each of `linked_records` names a record by `target_object` and `target_record_id`; each of `assignees` names a member by `referenced_actor_id` or by `workspace_member_email_address`, and not by both. Attio asks for every field of a new task, so Socket sends the ones you leave out as no deadline, not done, no records and nobody.

`update` changes the deadline, whether the task is done, its records or its assignees, and nothing else: Attio does not let a task's text be changed. `deadline_at: null` removes the deadline; leaving it out leaves it as it is. An update with nothing set is refused.

Reading needs `task:read`, `object_configuration:read`, `record_permission:read` and `user_management:read`; `create` and `update` need `task:read-write` with the other three; `delete` needs `task:read-write`.

### `attio.threads(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListThreads)` | `Page<ThreadRow>`: where each thread is and how many comments it has, without them |
| `get(thread, Paging)` | `Thread`, with its comments oldest first and a `next_cursor` when there are more |
| `comment(author, CreateComment)` | `Comment` |

`ListThreads` names one record (`object` with `record_id`) or one list entry (`list` with `entry_id`); one of the two is required. `comment` writes plain text as `author`, a workspace member's id, which Attio requires: a reply when `thread_id` is given, or the first comment of a new thread when `record` or `entry` is. A member's email address in the text mentions them.

Attio names the endpoint that writes a comment `comments`; it is a method of this group because a comment only ever belongs to a thread.

Everything needs `comment:read` (`comment:read-write` to write), with `object_configuration:read` and `record_permission:read` for a thread on a record, and `list_configuration:read` and `list_entry:read` for one on a list entry.

### `attio.workspace_members(&connection)`

`list()` returns every member, suspended ones included, and `get(member)` one. Both need `user_management:read`. A member's id is what `created_by_actor.id` holds when its `type` is `workspace-member`, and what `threads().comment` takes as its author.

### Meetings and transcripts: `meetings`, `call_recordings`

Attio's public API offers meetings, their call recordings, and the transcript of a recording. Attio marks all of it **beta**: it says it will avoid breaking changes, and that small ones may happen.

| Method | Returns |
| --- | --- |
| `meetings().list(ListMeetings)` | `Page<Meeting>` |
| `meetings().get(meeting)` | `Meeting` |
| `call_recordings().list(meeting, Paging)` | `Page<CallRecordingRow>`: each recording and whether it is ready |
| `call_recordings().get(meeting, recording)` | `CallRecording`, with its transcript |

`ListMeetings` filters by a record the meeting concerns (`linked_object` with `linked_record_id`), by `participants` (email addresses; a meeting with any of them is returned, as is one that concerns the record), and by time (`ends_from`, inclusive, and `starts_before`, exclusive).

A recording's `status` is `processing` until Attio has read it, then `completed` or `failed`. Its `transcript` is absent while it is being read and when it has none. **The transcript has the shape Socket's other integrations give one**, so a caller reads it the same way wherever it came from:

```json
{ "text": "[00:00] Alex Bell: Hello, Mr Watson, come here.\n[00:04] Tom Watson: I'm here.",
  "entries": [{ "speaker": "Alex Bell", "startMs": 510, "endMs": 810, "text": "Hello," }] }
```

`text` is Attio's `raw_transcript`; each entry is one of Attio's `segments`, with `speech` as `text`, `speaker.name` as `speaker`, and `start_time` and `end_time`, which Attio counts in seconds, as whole milliseconds. **`video_url` is a signed download link.** Only a recording made by Attio's own recorder has one; it works for about an hour after the recording was read, and it works by itself: anyone who holds it can download the video without a token. `call_recordings.get` is a `read`, so a host runs it without asking, and the link is then part of the result, wherever that goes: an agent's context, a log, a trace. Treat it as a secret for as long as it lasts, and leave it out of what you store. It is on another host than the API, and Socket does not fetch it.

Meetings need `meeting:read` and `record_permission:read`; call recordings need `meeting:read` and `call_recording:read`.

### `attio.meta(&connection)`

`identify()` returns a `TokenInfo`: `workspace_id`, `workspace_name`, `workspace_slug`, `scope` as Attio writes it and `scopes` as a list, `token_level`, `client_id` and `authorized_by_workspace_member_id`. It needs no scope.

## Page through a list

A list returns a `Page` with `items` and `next_cursor`. Pass the cursor back for the next page; `None` means the last page.

```rust
let mut query = QueryRecords { limit: Some(100), ..QueryRecords::default() };
loop {
    let page = attio.records(&connection).query("companies", query.clone()).await?;
    for row in &page.items { /* … */ }
    match page.next_cursor {
        Some(cursor) => query.cursor = Some(cursor),
        None => break,
    }
}
```

Attio pages in two ways, and both are a `cursor` here.

- **Most lists are paged by `limit` and `offset`**, in the query, or in the body for the two queries. Their cursor is the offset of the next page, written as a number. Attio does not say whether more follow, so a page as long as was asked for has a `next_cursor` and a shorter one does not: **a list whose length is an exact number of pages ends with one empty page.** Keep the same `limit`, filter and sort from page to page. A cursor that is not a number is refused before a request is made.
- **Meetings, call recordings and the comments of a thread are paged by Attio's own cursor**, which Socket passes back to Attio as the value of its `cursor` parameter and as nothing else. Whatever a cursor holds, it cannot change where the request goes.

`limit` is always sent, so that what makes a page full is known and not left to a default Attio may change. When you give none, Socket asks for 25 rows of a query and 50 of anything else, where Attio's own default is as much as 500 records with every value: a caller with a limited context could not take that in.

| List | `limit` | When not given |
| --- | --- | --- |
| `records().query`, `entries().query` | 1 to 500 | 25 |
| `attributes().list`, `tasks().list` | 1 to 500 | 50 |
| `records().entries` | 1 to 1000 | 50 |
| `notes().list`, `threads().list` | 1 to 50 | 50 |
| `threads().get` (comments) | 1 to 250 | 50 |
| `meetings().list`, `call_recordings().list` | 1 to 200 | 50 |

`objects().list`, `lists().list`, `workspace_members().list`, `attributes().options` and `attributes().statuses` are not paged: Attio returns them whole.

## Call an operation by name

Every method is also an operation an agent, an MCP server or another language can call with JSON. The plain arguments and the options sit side by side in one object.

```rust
let output = socket
    .invoke(key, "attio.records.assert".into(), serde_json::json!({
        "object": "people",
        "matching_attribute": "email_addresses",
        "values": { "email_addresses": ["ada@example.com"], "name": "Ada Lovelace" }
    }))
    .await?;
```

`socket.operations()` returns each operation's name, description, input schema, output schema, effect and scopes. The effect lets a host ask a person before a change:

- **read** changes nothing.
- **write** adds something or changes a value: creating a record, an entry, a note, a task or a comment; changing a record's or an entry's values, or a task.
- **destructive** deletes: a record, an entry, a note, a task.

| Operation | Effect | Scopes | What it does |
| --- | --- | --- | --- |
| `attio.identity.get` | read | none | Return the workspace this connection is authorised for, confirming the token still works. |
| `attio.resource.resolve` | read | `object_configuration:read`, `list_configuration:read` | Confirm that an object or a list exists and the token can see it. |
| `attio.meta.identify` | read | none | Describe the token: its workspace, its scopes, and whether it acts as the workspace or as one member. |
| `attio.objects.list` | read | `object_configuration:read` | List the kinds of record the workspace keeps. |
| `attio.objects.get` | read | `object_configuration:read` | Get one object. |
| `attio.attributes.list` | read | `object_configuration:read`, `list_configuration:read` | List the fields of an object or of a list. |
| `attio.attributes.get` | read | `object_configuration:read`, `list_configuration:read` | Get one field. |
| `attio.attributes.options` | read | `object_configuration:read`, `list_configuration:read` | List what a select attribute can be set to. |
| `attio.attributes.statuses` | read | `object_configuration:read`, `list_configuration:read` | List the statuses a status attribute can be at. |
| `attio.records.query` | read | `record_permission:read`, `object_configuration:read` | Find records by a filter, in a chosen order. Changes nothing. |
| `attio.records.get` | read | `record_permission:read`, `object_configuration:read` | Get one record, with every value and each attribute's current value. |
| `attio.records.entries` | read | `record_permission:read`, `object_configuration:read`, `list_entry:read` | List the lists a record is on. |
| `attio.records.create` | write | `record_permission:read-write`, `object_configuration:read` | Create a record. |
| `attio.records.update` | write | `record_permission:read-write`, `object_configuration:read` | Change a record's values, adding to an attribute that holds several. |
| `attio.records.assert` | write | `record_permission:read-write`, `object_configuration:read` | Create a record, or change the one a unique attribute matches. |
| `attio.records.delete` | destructive | `record_permission:read-write`, `object_configuration:read` | Delete a record. |
| `attio.lists.list` | read | `list_configuration:read` | List the workspace's lists. |
| `attio.lists.get` | read | `list_configuration:read` | Get one list. |
| `attio.entries.query` | read | `list_entry:read`, `list_configuration:read` | Find entries of a list by a filter, in a chosen order. Changes nothing. |
| `attio.entries.get` | read | `list_entry:read`, `list_configuration:read` | Get one entry, with every value and each attribute's current value. |
| `attio.entries.create` | write | `list_entry:read-write`, `list_configuration:read` | Put a record on a list. |
| `attio.entries.update` | write | `list_entry:read-write`, `list_configuration:read` | Change an entry's values, such as its stage. |
| `attio.entries.delete` | destructive | `list_entry:read-write`, `list_configuration:read` | Take an entry off its list. |
| `attio.notes.list` | read | `note:read`, `object_configuration:read`, `record_permission:read` | List notes, without their text. |
| `attio.notes.get` | read | `note:read`, `object_configuration:read`, `record_permission:read` | Get one note, with what it says. |
| `attio.notes.create` | write | `note:read-write`, `object_configuration:read`, `record_permission:read` | Write a note on a record. |
| `attio.notes.delete` | destructive | `note:read-write` | Delete a note. |
| `attio.tasks.list` | read | `task:read`, `object_configuration:read`, `record_permission:read`, `user_management:read` | List tasks. |
| `attio.tasks.get` | read | `task:read`, `object_configuration:read`, `record_permission:read`, `user_management:read` | Get one task. |
| `attio.tasks.create` | write | `task:read-write`, `object_configuration:read`, `record_permission:read`, `user_management:read` | Create a task. |
| `attio.tasks.update` | write | `task:read-write`, `object_configuration:read`, `record_permission:read`, `user_management:read` | Change a task's deadline, completion, records or assignees. |
| `attio.tasks.delete` | destructive | `task:read-write` | Delete a task. |
| `attio.threads.list` | read | `comment:read`, `object_configuration:read`, `record_permission:read`, `list_configuration:read`, `list_entry:read` | List the threads on a record or on a list entry, without their comments. |
| `attio.threads.get` | read | the same five | Get one thread with its comments. |
| `attio.threads.comment` | write | `comment:read-write`, and the other four | Write a comment as a workspace member. |
| `attio.workspace_members.list` | read | `user_management:read` | List the people who work in the workspace. |
| `attio.workspace_members.get` | read | `user_management:read` | Get one workspace member. |
| `attio.meetings.list` | read | `meeting:read`, `record_permission:read` | List meetings. Beta. |
| `attio.meetings.get` | read | `meeting:read`, `record_permission:read` | Get one meeting. Beta. |
| `attio.call_recordings.list` | read | `meeting:read`, `call_recording:read` | List a meeting's call recordings. Beta. |
| `attio.call_recordings.get` | read | `meeting:read`, `call_recording:read` | Get one call recording with its transcript. Beta. |

`records.query` and `entries.query` are reads that Attio takes as a `POST`. They are marked `read` because they change nothing, which is what the effect is for. Because they are a `POST`, they are not sent again after a server error, as a `GET` is.

**Where an operation's scopes depend on what it is given, all of them are listed.** An attribute belongs to an object or to a list, and a thread is on a record or on a list entry; each needs its own scopes, and `required_scopes` names both sets. An application that only ever reads the attributes of objects does not need `list_configuration:read`.

**The three updates and `assert` are `write`, not `destructive`.** `records.update` and `entries.update` add to an attribute that holds several values and remove nothing, and a single value they replace stays in the attribute's history. `records.assert` is the one to know about: on a record that already exists it removes the values of a multi-select that are not in the list it is given. `tasks.update` replaces the lists of records and assignees it is given. A host that asks a person only before a destructive operation does not ask before these.

**A field the operation does not know is refused**, at any depth, and named: `sorts[0].atribute`, `record.entry_id`. A misspelt field would otherwise be dropped in silence, and with it the filter, the deadline or the text it carried. Each operation's input schema says the same, with `additionalProperties: false`; an operation that takes nothing refuses anything. The exceptions are the two places whose fields are the workspace's own, `filter` and `values` (and `entry_values`): nothing there is unknown to Socket, and all of it is passed to Attio as given.

## Limits

- **Requests.** Attio allows 100 reads and 25 writes a second for the whole API, and may lower either for a while. `notes.list` is held to 10 a second. A request over the limit is answered `429` with the time the limit resets, usually the next second; Attio did not carry it out, so Socket waits and sends it again, a write included.
- **Heavy queries.** `records.query` and `entries.query` are also given a score for their filters, sorts and the number of records behind them. One query can be too heavy by itself, and the scores of all a workspace's queries are added up over ten seconds, across every app and token. Attio does not publish the numbers.
- **A workspace token against a member's.** A token at the `workspace` level sees what the workspace allows, and only an administrator can grant one. A token at the `user` level sees what that member sees. `meetings.list` is documented for workspace tokens only.
- **Lists have their own access.** A list can be private, or open to named members only (`workspace_access` and `workspace_member_access` on a `List`). Which lists a token sees is Attio's to decide.
- **What Attio fills in cannot be written.** An attribute with `is_writable: false` is one Attio protects or works out itself.
- **A task's text** is at most 2000 characters, plain, and cannot be changed after it is written.
- **A thread's comments in a list.** Attio returns at most 80 comments with each thread of a list, so `comment_count` stops at 80. `threads.get` pages through all of them.
- **History.** A record read by itself carries only its active values. Attio offers an attribute's past values at another endpoint, which this integration does not call yet; the model reads them the same way when it does.

## Handle errors

Attio writes an error as `{ "status_code", "type", "code", "message" }`.

| Kind | What it means for Attio | What to do |
| --- | --- | --- |
| `ReconnectRequired` | Attio answered 401, or said the token is not active. There is no refresh token | Connect again, or make a new API key |
| `AccessDenied` | 403: the token lacks a scope, or the member lacks the permission. The message carries Attio's own reason | Add the scope to the app or the key, or ask an administrator |
| `NotFound` | 404. The message carries Attio's reason, which says whether it was the object, the list or the record | Check the slug or the id |
| `InvalidInput` | 400, 409, 413 or 422: a value Attio does not have, a filter it cannot read, a unique value another record holds, content too large. The message carries Attio's reason, except for 413 | Fix the input |
| `RateLimited` | 429. `retry()` carries the wait until the time Attio stated | Socket tries again by itself when the wait is short |
| `Unexpected` | Attio failed, or answered 409 with `concurrent_write_conflict`: another request changed the same record while this one was being checked. `retry()` is `Later` | Send it again |
| `Decode` | Attio answered success without what was asked for, or with something that could not be read | Report it; this should not happen |

An error never repeats the token: if Attio echoes it, it is replaced with `[redacted]`. An id or a cursor Socket refuses is not repeated either. Attio's own reason is repeated, up to 300 characters, and may name the slug, the id or the option title you sent. An answer that cannot be read says where the unreadable value was, such as `values.name`, and nothing of what it held.

A request sent as `GET` is retried on a throttle or a server error. Creating and changing are sent again in only two cases, both of which mean Attio did not carry them out: Attio throttled the request, or it answered `concurrent_write_conflict` to an `assert`. **If a create, an update or a comment fails with a server error, check before sending it again**: it may have happened. `assert` is the safe way to create something that may already exist.

**`assert` and the deletes are repeated after a server error**, because the transport repeats a `PUT` and a `DELETE`. An `assert` sent twice writes the same values to the same record. A delete that worked the first time is answered "not found" the second, and the call reports `NotFound`: treat `NotFound` from a delete as "it is gone".

## Confirmed against Attio's documentation, and not

Everything here was read from docs.attio.com in October 2026. Nothing was run against a live workspace.

Confirmed:

- The API base `https://api.attio.com/v2/`, and a bearer token for an API key and for an OAuth token alike.
- The authorise address `https://app.attio.com/authorize` with `client_id`, `response_type=code`, `redirect_uri`, `state`, `token_level`, `code_challenge` and `code_challenge_method=S256`; that it has no scope parameter, and that an app's scopes are chosen in the developer console.
- The token address `https://app.attio.com/oauth/token`, a form with `grant_type=authorization_code`, `code`, `client_id`, `client_secret` and `code_verifier`; an answer of `access_token` and `token_type`, with no lifetime and no refresh token; that a token does not expire (`exp` is always null).
- `GET /v2/self`, its fields, that it needs no scope, and that an unknown, revoked or deleted token is answered `200` with `{ "active": false }`.
- Every endpoint above, with its verb, path, parameters and scopes: `GET /objects` and `/objects/{object}`; `GET /{target}/{identifier}/attributes`, `/attributes/{attribute}`, `/options` and `/statuses`; `POST /objects/{object}/records/query`; `GET`, `PATCH` and `DELETE /objects/{object}/records/{record_id}`; `POST` and `PUT /objects/{object}/records` with `matching_attribute`; `GET …/records/{record_id}/entries`; `GET /lists` and `/lists/{list}`; `POST /lists/{list}/entries/query` and `/entries`; `GET`, `PATCH` and `DELETE /lists/{list}/entries/{entry_id}`; `GET` and `POST /notes`, `GET` and `DELETE /notes/{note_id}`; `GET` and `POST /tasks`, `GET`, `PATCH` and `DELETE /tasks/{task_id}`; `GET /threads` and `/threads/{thread_id}`; `POST /comments`; `GET /workspace_members` and `/workspace_members/{workspace_member_id}`; `GET /meetings` and `/meetings/{meeting_id}`; `GET /meetings/{meeting_id}/call_recordings` and `/call_recordings/{call_recording_id}`.
- That every answer is wrapped in `data`, and that a delete answers `200` with an empty object.
- The shape of an attribute value (`active_from`, `active_until` null while active, `created_by_actor`, `attribute_type`) and the fields of all seventeen types.
- That `PATCH` adds to a multi-select and `PUT` overwrites it; that an `assert` overwrites every multi-select but the matching attribute.
- The body of a new entry, a new note, a new task and a new comment, with which fields are required; that a task's text cannot be changed and is plain text of at most 2000 characters; that a comment needs an author who is a workspace member.
- Paging by `limit` and `offset` and by `cursor` with `pagination.next_cursor`, and the defaults and maximums in the table above where Attio states them (notes 10 and 50, threads 10 and 50, a record's entries 100 and 1000, meetings and call recordings 50 and 200, a thread's comments 250, queries and tasks 500 by default).
- The limits of 100 reads and 25 writes a second, `429` with `rate_limit_error` and `rate_limit_exceeded`, and that `Retry-After` is a date.
- The error shape, and these codes: `not_found`, `value_not_found`, `validation_type`, `filter_error`, `unauthorized` (403), `concurrent_write_conflict` (409), `merge_in_progress`, `limit_reached`.
- That meetings, call recordings and transcripts are in the public API, in beta, with the transcript as `segments` and `raw_transcript` on one recording.

Not confirmed:

- **What Attio answers to a token it rejects on an ordinary endpoint.** No page documents a 401. Socket reads any 401 as "connect again", and the documented `{ "active": false }` of `/v2/self` the same way.
- **What a missing scope looks like.** The documented 403 is `auth_error` with `unauthorized` and speaks of permissions. Whether a missing scope is answered the same way, or names the scope, was not found. Socket passes on Attio's message either way.
- **The code of a uniqueness conflict.** The page for creating a record says it "will throw on conflicts of unique attributes" and documents no code or status for it. Socket reads any 400, 409 or 422 as an input error; only `concurrent_write_conflict` is treated as "try again".
- **What a query that is too heavy by itself is answered with**, and the numbers of the score limits.
- **The largest `limit`** of the two queries, of `tasks.list` and of `attributes.list`. Attio states only the default, 500 where it states one, so Socket takes up to 500 and no more. Whether an attribute list has a default at all is not stated.
- **Whether `redirect_uri` is needed at the token endpoint.** Attio's tutorial sends it and its reference does not list it. Socket sends it.
- **Whether PKCE is accepted for a workspace-level token.** The reference lists `code_challenge` as optional and required for `user`; Socket always sends it.
- **What the sign-in page does with a `scope` parameter.** Socket never sends one.
- **Which characters a slug may hold.** Attio says a slug is written in snake case. Socket lets through letters, digits, `_` and `-`; a slug with anything else could not be used here.
- **Whether notes can be listed by `parent_object` alone.** The two parameters are documented separately and both as optional; Socket allows the object alone and refuses the record alone.
- **Whether threads can be listed without naming a record or an entry.** The page describes the two filters and no unfiltered list, so Socket requires one.
- **Whether `tasks.update` replaces `linked_records` and `assignees` or adds to them.** The page does not say; this guide assumes it replaces them.
- **The order of a record's values.** One page says history is listed oldest first, another that new multi-select values are put first. Socket goes by `active_from`, not by position.
- **The id of a list's attribute.** The documented id of an attribute has `object_id`; what it holds for an attribute of a list was not found. It is read as sent.
- **`has_more_comments`** on a thread of a list is mentioned in a description and is not in the schema, so it is not in the model.
- **Whether `meetings.list` works with a member's token.** The page lists only `workspace`.
- **Whether a `call-recording` without a transcript sends `null` or leaves the field out.** Both read as no transcript.
- **Which fields Attio ever sends as `null` where it documents a list, a flag or a number.** Socket reads a `null` there as an empty list, `false` or nought, and a `null` in place of an attribute's list of values as no values, so that one such field does not make an answer unreadable. An id is not read that way: an answer without the id of the thing asked for is an error.

## Not supported yet

- **Incoming events** (webhooks).
- **Changing the data model**: creating or changing objects, attributes, select options, statuses and lists.
- **Overwriting a record or an entry** (`PUT` on one record), which is the way to take a value out of a multi-select. `assert` does it for a record matched by a unique attribute.
- **An attribute's history** (`GET …/records/{record_id}/attributes/{attribute}/values` with `show_historic`), and writing one attribute by itself.
- **Searching records by text** across objects, **merging two records**, and **views**, including a query by a saved view (`filter_view_id`).
- **Asserting a list entry** by its parent record.
- **Changing or deleting a note**, **deleting or reading one comment**, and resolving a thread.
- **Linking a task to a record by a matching attribute** (an email address or a domain) instead of by its id.
- **Creating, changing and deleting meetings and call recordings**, and **downloading a recording's video**.
- **The other areas of Attio's API**, such as files.

Anything Attio offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("notes").with_query("limit", "5")).await?;
```
