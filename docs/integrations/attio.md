# Attio

**Status:** built and tested against a local server that answers as Attio's documentation says. Not yet run against the real Attio.

Socket's Attio integration gives a program a workspace's CRM: the objects and attributes the workspace defined, its records, its lists and their entries, the notes, tasks and comments people wrote, its members, and its meetings with their recordings and transcripts. That is 38 typed methods, and the same 38 as operations callable by name with JSON, plus identity and lookup of an object. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against Attio's documentation.

Attio's data model is the workspace's own: which objects exist and which attributes each has differs from one workspace to the next. So the methods are generic, with the object or the list as an argument, and `objects.list`, `lists.list` and `attributes.list` are how a program learns what is there.

## Connect

Add the crate with the Attio feature:

```sh
cargo add socketkit --features attio
```

### With a workspace's API key

A workspace creates an access token in its settings, under Developers, and chooses its scopes there.

```rust
use std::sync::Arc;
use socketkit::attio::Attio;
use socketkit::{ConnectionKey, ProviderId, Socket};

let attio = Attio::with_token("…");
let socket = Socket::in_memory().integration(Arc::new(attio.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("attio")?, "acme")).await?;
```

Every call uses that token, as a bearer token. It does not expire.

### With your own app, to connect your users' workspaces

Create an app in Attio's developer console (build.attio.com), add your callback route as a redirect URL, and choose the app's scopes there.

```rust
use socketkit::attio::{Attio, AttioOAuth, TokenLevel};
use socketkit::{OAuthClient, SecretString};

let attio = Attio::with_oauth(AttioOAuth {
    client: OAuthClient {
        client_id: config.attio_client_id,
        client_secret: SecretString::new(config.attio_client_secret),
        redirect_uri: "https://yourapp.example/oauth/attio/callback".parse()?,
    },
    // Whose permissions the token acts with. `None` is Attio's default, the workspace's.
    token_level: Some(TokenLevel::Workspace),
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(attio.clone())).build()?;

// When a user clicks "Connect Attio":
let key = ConnectionKey::new(ProviderId::new("attio")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

Three things differ from most providers:

- **No scopes are asked for at sign-in.** Attio's authorise address takes none. What the app may do is set on the app in the developer console, and a workspace approves that when it installs the app. `required_scopes` on each operation says what to tick there. Passing scopes to `begin_authorization` has no effect on what Attio grants.
- **The token does not expire and there is no refresh token.** A connection keeps working until the workspace removes the app. When it has, `identity.get` reports `reconnect_required`.
- **A token has a level.** A workspace token can do whatever the app's scopes allow. A user token (`TokenLevel::User`) is limited to what the person who connected may see and change; Attio requires PKCE for it, which Socket sends on every Attio sign-in. `meetings.list` answers only for a workspace token.

## Identity and lookup

`attio.identity.get` returns the workspace the token was issued for: its id as the account's id, and its name. It needs no scope. The workspace is the account for a user token as well, so two members who connect the same workspace are the same account here; `email` is always absent.

`attio.resource.resolve` takes an object's slug, such as `people`, `companies` or `deals`, or its id, and confirms the connection can read it. The resource's id is the slug, which is what every other method takes. It needs `object_configuration:read`.

## Use the typed methods

Each group is reached from the integration with a connection: `attio.records(&connection)`. Identifiers are plain arguments; content and filters are structs from `socketkit::attio::models`.

```rust
use socketkit::attio::models::{CreateNote, Query, Sort, SortDirection, WriteRecord};

let records = attio.records(&connection);
let page = records.query("people", Query {
    filter: Some(serde_json::from_value(serde_json::json!({ "email_addresses": "ada@example.test" }))?),
    sorts: Some(vec![Sort::by("name", SortDirection::Asc)]),
    ..Query::default()
}).await?;
let ada = &page.items[0];
println!("{} works at {}", ada.current["name"], ada.current["company"]);

records.update("people", &ada.id.record_id, WriteRecord::with("job_title", "Analyst")).await?;
attio.notes(&connection)
    .create(CreateNote::on("people", &ada.id.record_id, "Call", "Asked about pricing."))
    .await?;
```

A scope ending in `:read-write` includes what the `:read` one allows.

### The schema: `objects`, `attributes`, `lists`

| Method | What it does | Effect | Scopes |
| --- | --- | --- | --- |
| `objects.list` | Every object of the workspace | read | `object_configuration:read` |
| `objects.get` | One object, by slug or id | read | `object_configuration:read` |
| `attributes.list` | The attributes of an object or a list | read | `object_configuration:read` for an object, `list_configuration:read` for a list |
| `attributes.get` | One attribute | read | the same |
| `attributes.options` | The options of a select attribute | read | the same |
| `attributes.statuses` | The statuses of a status attribute, such as a deal's stages | read | the same |
| `lists.list` | Every list the connection can see | read | `list_configuration:read` |
| `lists.get` | One list, by slug or id | read | `list_configuration:read` |

The attribute methods take a target, `objects` or `lists`, and the slug or id of that object or list. An attribute says its `type`, whether it `is_writable`, `is_unique` (so that `records.assert` can match on it) and `is_multiselect`. The four attribute operations list both scopes in `required_scopes`, because which one applies depends on the target.

### `records`

`object` is the object's slug or id throughout.

| Method | What it does | Effect | Scopes |
| --- | --- | --- | --- |
| `records.query` | Records that match a filter, sorted as asked | read | `record_permission:read`, `object_configuration:read` |
| `records.get` | One record | read | the same |
| `records.entries` | The lists a record is on, and its entry in each | read | the same, and `list_entry:read` |
| `records.create` | A new record | write | `record_permission:read-write`, `object_configuration:read` |
| `records.update` | Change the attributes named | write | the same |
| `records.assert` | Create a record, or change the one holding the same value of a unique attribute | write | the same |
| `records.delete` | Delete a record | destructive | the same |

`records.query` is a read that Attio takes as a `POST`. It is marked `read`.

`records.update` is Attio's `PATCH`: for an attribute that holds several values, the values given are added and nothing is taken away. `records.assert` is Attio's `PUT` with `matching_attribute`: for every attribute other than the matching one, the record ends with exactly the values given, so values that were there and are not given are removed. Attio's other `PUT`, which overwrites the several values of one record addressed by id, is not offered as a method.

### Values, and what a record holds now

Attio keeps a list of values for every attribute, each with its kind, who set it and the time it has held since. A `Record` keeps that shape under `values`, and beside it gives `current`: what each attribute holds now, in its plainest form.

```json
{
  "id": { "workspace_id": "…", "object_id": "…", "record_id": "…" },
  "values": {
    "name": [{ "attribute_type": "personal-name", "full_name": "Ada Lovelace", "first_name": "Ada", "last_name": "Lovelace", "active_from": "2026-10-01T09:00:00.000000000Z", "active_until": null, "created_by_actor": { "type": "workspace-member", "id": "…" } }],
    "job_title": []
  },
  "current": { "name": "Ada Lovelace", "job_title": null }
}
```

In `current`, an attribute with no value is `null`, with one value is that value, and with several is a list. An attribute that may hold several values and holds one is therefore given as that one. The plain form is the `value` of a text, number, date, timestamp, rating or checkbox; the address of an email; the domain; the phone number; the full name; the number of a currency; the title of a select option or a status. A reference to another record, an actor, a location and an interaction have no single plain form and are given whole.

An `Entry` does the same with `entry_values` and `current`.

Values to write go under `values` (or `entry_values`) by attribute slug, in the forms Attio's documentation gives for each attribute type: `"name": "Ada Lovelace"`, `"employees": 42`, `"email_addresses": ["ada@example.test"]`. They are sent exactly as written, a `null` or an empty list included.

### `entries`

`list` is the list's slug or id throughout.

| Method | What it does | Effect | Scopes |
| --- | --- | --- | --- |
| `entries.query` | Entries that match a filter, sorted as asked | read | `list_entry:read`, `list_configuration:read` |
| `entries.get` | One entry | read | the same |
| `entries.create` | Put a record on a list | write | `list_entry:read-write`, `list_configuration:read` |
| `entries.update` | Change the attributes named, such as a deal's stage | write | the same |
| `entries.delete` | Take a record off a list; the record stays | destructive | the same |

`entries.query` is a read that Attio takes as a `POST`. It is marked `read`. A record may be on a list more than once, so `entries.create` called twice makes two entries.

### `notes`, `tasks` and `threads`

| Method | What it does | Effect | Scopes |
| --- | --- | --- | --- |
| `notes.list` | The notes on one record, on the records of one object, or on every record | read | `note:read`, `object_configuration:read`, `record_permission:read` |
| `notes.get` | One note, as plain text and as Markdown | read | the same |
| `notes.create` | Write a note on a record | write | `note:read-write`, `object_configuration:read`, `record_permission:read` |
| `notes.delete` | Delete a note | destructive | `note:read-write` |
| `tasks.list` | Tasks, by record, assignee or whether done | read | `task:read`, `object_configuration:read`, `record_permission:read`, `user_management:read` |
| `tasks.get` | One task | read | the same |
| `tasks.create` | A new task | write | `task:read-write` and the three above |
| `tasks.update` | Its deadline, whether it is done, its records and assignees | write | the same |
| `tasks.delete` | Delete a task | destructive | `task:read-write` |
| `threads.list` | The comment threads on a record or a list entry | read | `comment:read` |
| `threads.get` | One thread and its comments, 250 at a time | read | `comment:read` |
| `threads.comment` | Write a comment in a thread, or open one on a record or an entry | write | `comment:read-write` |

The thread operations also need the scopes that read what the thread is on: `object_configuration:read` and `record_permission:read` for a record, `list_configuration:read` and `list_entry:read` for a list entry.

`tasks.list` takes an `assignee` that is a member's id or email address, or the word `null` for tasks assigned to nobody. A blank one is refused, because Attio reads it as "nobody" and a caller may have meant "anybody".

A note is written as plain text unless `format` is `markdown`. A task's text is plain text of at most 2000 characters and cannot be changed afterwards. In `tasks.update`, `deadline_at: null` takes the deadline away, and a list of records or of assignees replaces the one that was there.

A comment has to name its author, a workspace member's id: Attio takes no other author and shows the comment as that person's. `workspace_members.list` gives the ids.

### `workspace_members`

| Method | What it does | Effect | Scopes |
| --- | --- | --- | --- |
| `workspace_members.list` | Everyone with access to the workspace | read | `user_management:read` |
| `workspace_members.get` | One member | read | `user_management:read` |

### `meetings` and `call_recordings`

Attio marks these endpoints as beta.

| Method | What it does | Effect | Scopes |
| --- | --- | --- | --- |
| `meetings.list` | Meetings, by linked record, participants or time | read | `meeting:read`, `record_permission:read` |
| `meetings.get` | One meeting | read | the same |
| `call_recordings.list` | A meeting's recordings, without transcripts | read | `meeting:read`, `call_recording:read` |
| `call_recordings.get` | One recording, with its transcript | read | the same |

`call_recordings.get` carries the transcript: `segments`, one for each thing said with the speaker's name and the start and end in seconds from the start of the recording, and `raw_transcript`, the whole as text. `transcript` is absent while a recording is still being processed. The recording's `video_url` is an address on another host that stops working an hour after it was given, and is present only for recordings Attio's own recorder made; Socket does not download it.

## Filter and sort

`records.query` and `entries.query` take a `filter` in Attio's own form, by attribute slug, and it is sent as written:

```json
{ "object": "companies", "filter": { "$and": [{ "domains": { "domain": { "$contains": "example" } } }, { "employees": { "$gte": 50 } }] },
  "sorts": [{ "direction": "desc", "attribute": "employees" }] }
```

`filter_view_id` uses a saved view's filter instead; the two cannot be combined. A sort names an `attribute`, with an optional `field` of its value such as `last_name`, or a `path` through references.

## Page through a list

Every list returns `{ "items": […], "next_cursor": … }`. Pass `next_cursor` back as `cursor`, with the same filters and limit, until it is absent.

Attio pages most lists by position. For those the cursor is the position of the next item, and Attio does not say whether more follow; its rule is that a page shorter than the limit is the last. So a full page carries a cursor, and when the list ended exactly there the next page is empty.

| List | Page size when none is given | Largest |
| --- | --- | --- |
| `records.query`, `entries.query` | 50 | 500 |
| `tasks.list` | 50 | 500 |
| `records.entries` | 100 | 1000 |
| `notes.list`, `threads.list` | 10 | 50 |
| `attributes.list` | everything | not limited here |
| `threads.get` (comments) | Attio's 250 | 250 |
| `meetings.list`, `call_recordings.list` | Attio's 50 | 200 |

Attio's own default for the two queries and for tasks is 500. Socket asks for 50 when no limit is named, because each record comes with every value it holds and these operations are called by agents with a limited context.

`objects.list`, `lists.list`, `workspace_members.list`, `attributes.options` and `attributes.statuses` are not paged by Attio and return everything.

## Call an operation by name

Every typed method is an operation named `attio.<group>.<method>`. Its input is a JSON object with the method's arguments and its options side by side.

```rust
let record = socket.invoke(key.clone(), "attio.records.assert".into(), serde_json::json!({
    "object": "people",
    "matching_attribute": "email_addresses",
    "values": { "email_addresses": ["ada@example.test"], "name": "Ada Lovelace" }
})).await?;
```

Each operation carries the JSON Schema of its input and output, its `effect` and its `required_scopes`. A field an operation does not list is refused and named, except inside `values`, `entry_values` and `filter`, whose names are the workspace's own.

## Handle errors

| Attio answers | The error's kind | What to do |
| --- | --- | --- |
| `401` | `reconnect_required` | Connect the workspace again |
| `403` | `access_denied` | The token lacks a scope, or a user token lacks access. Attio's message is passed on |
| `404` | `not_found` | The object, list, record or id is not there, or the token cannot see it |
| `400` | `invalid_input` | Attio's message says what: an unknown attribute, a value of the wrong form, a unique value already held |
| `409` `concurrent_write_conflict` | `unexpected`, to retry later | Another request was changing the same record. Nothing was written; send it again |
| `413` | `invalid_input` | The note is too large; shorten it |
| `429` | `rate_limited` | Wait until the time Attio gives, which Socket reads from `Retry-After` |
| `5xx` | `unexpected` | Try again later |

Attio allows 100 reads and 25 writes a second, and scores `records.query` and `entries.query` by how complex the filter and sorts are. A throttled request was not carried out; Socket waits and sends it once more when the wait is short, for a write as well.

After a server error Socket repeats a `GET`, and also a `PUT` and a `DELETE`: `records.assert` and the four deletes. Each of those leaves the same state when it runs twice. One consequence is not hidden: a delete that had gone through before the error finds nothing the second time, and is reported as `not_found` though the thing is gone. It does not repeat a `POST` or a `PATCH`, so a create or an update is sent once; that includes the two queries, which are reads sent as `POST`.

Input that cannot work is refused before Attio is called: a blank id, a `limit` out of range, an update with nothing in it, a comment that names no place or more than one.

## Confirmed against Attio's documentation, and not

Confirmed against Attio's published OpenAPI document (`https://api.attio.com/openapi/api`) and its guides at `docs.attio.com`, on 11 October 2026:

- The API base `https://api.attio.com/v2/`, and every path, verb, query parameter and body used here.
- The authorise address `https://app.attio.com/authorize` and the token address `https://app.attio.com/oauth/token`, with the client id and secret in the body, PKCE with S256, and the `token_level` parameter.
- That the authorise address takes no `scope` parameter, and that scopes are the same for OAuth tokens and workspace API keys.
- That access tokens do not expire (`exp` is always `null`) and the token response carries no refresh token.
- `GET /v2/self` and its answer, including `200` with `{"active": false}` for a token that is no longer accepted.
- The scopes each operation lists, taken from each endpoint's "Required scopes".
- The page sizes and limits in the table above, where a largest is given. Offset paging and its end-of-list rule, and cursor paging with `pagination.next_cursor`.
- The shape of a value, with `active_from`, `active_until`, `created_by_actor` and `attribute_type`, and the fields of each of the 17 kinds.
- The error body (`status_code`, `type`, `code`, `message`), and `429` with a `Retry-After` that is a date.
- That meetings and call recordings are public, in beta, and that a recording read on its own carries its transcript. The separate transcript endpoint is deprecated in favour of it and is not used. A `video_url` expires an hour after it is given.
- `409` with `concurrent_write_conflict` on a record write, and `413` on a note that is too large.

Not confirmed:

- **The largest page of `records.query`, `entries.query` and `tasks.list`.** Attio documents their default of 500 and no maximum. Socket refuses a limit above 500.
- **The largest page of `attributes.list`**, for which Attio documents neither a default nor a maximum.
- **Whether a record or an entry ever carries values that have ended.** The reference describes only current values there; `current` leaves out any value whose `active_until` is set, so it is right either way.
- **The exact status of a missing scope.** The reference shows `403` with `auth_error`; it is passed on as `access_denied` with Attio's message.
- **Whether Attio's token endpoint accepts the `redirect_uri` Socket sends with the code.** Attio's reference does not list the parameter. The OAuth standard has it, and servers that do not need it ignore it.
- Nothing has been run against a live workspace.

## Not supported yet

- Incoming events (webhooks), which are phase 3 of the roadmap.
- Changing the schema: creating or changing objects, attributes, options, statuses and lists.
- Overwriting the several values of one attribute on a record or an entry addressed by id (Attio's `PUT` on a record or an entry), creating or changing an entry by its parent record, merging two records, and reading the history of one attribute's values.
- Changing a note; deleting a comment; creating, changing and deleting meetings and call recordings.
- Record search across objects, Attio's SQL endpoint, emails, files, sequences, activities, views and SCIM.
- Downloading a recording's video.

Any of these endpoints can still be called with the generic authenticated request.
