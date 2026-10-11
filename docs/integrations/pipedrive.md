# Pipedrive

**Status:** built and tested against a local server that answers as Pipedrive's documentation says. Wire-tested only: not yet run against a real Pipedrive account.

Socket's Pipedrive integration gives a program a company's CRM: deals, persons, organisations, leads, activities and notes, with the pipelines, fields and users that explain them, and one search across all of it. That is 42 typed methods, and the same 42 as operations callable by name with JSON, plus identity and lookup of a record by its link. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against Pipedrive's documentation.

## Connect

Add the crate with the Pipedrive feature:

```sh
cargo add socketkit --features pipedrive
```

Pipedrive has two ways in, and they are sent differently: an OAuth access token as a bearer, a personal API token bare in the `x-api-token` header. A provider definition has one way of authenticating, so the two forms are **two definitions with the same id**, `provider()` and `api_token_provider()`, and one `Socket` holds one of them. An application that connects its users through OAuth and also holds API tokens for others needs a second `Socket` for those.

### With a personal API token

```rust
use std::sync::Arc;
use socketkit::pipedrive::Pipedrive;
use socketkit::{ConnectionKey, ProviderId, Socket};

let pipedrive = Pipedrive::with_token("9f1c…");
let socket = Socket::in_memory().integration(Arc::new(pipedrive.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("pipedrive")?, "me")).await?;
```

A user finds their token in Pipedrive under personal preferences. Every call uses it, in the `x-api-token` header. **It is never written into an address**, where it would end up in logs and proxies: a definition that would put the token in the query string is refused when the `Socket` is built. The token does whatever its user can do; scopes do not apply to it.

Calls go to `api.pipedrive.com`. To call the company's own host, as Pipedrive's documentation shows requests, name the company:

```rust
use socketkit::pipedrive::{Pipedrive, PipedriveToken};
use socketkit::SecretString;

let pipedrive = Pipedrive::with_token(PipedriveToken {
    token: SecretString::new(config.pipedrive_token),
    // The first part of the company's Pipedrive address: acme for acme.pipedrive.com.
    company_domain: Some("acme".into()),
});
```

The name becomes part of the host the token is sent to, so anything that is not one name of a host is reported when the `Socket` is built and never reaches an address.

To keep a different API token for each of your tenants, register `Pipedrive::with_spec(api_token_provider())` and save each token in the token store with `TokenSet::bearer(token)`; it is sent in the same header.

### With your own app, to connect your users

Create an app in Pipedrive's Developer Hub, choose its scopes there, and add your callback route as its callback URL.

```rust
use socketkit::pipedrive::Pipedrive;
use socketkit::{OAuthClient, SecretString};

let pipedrive = Pipedrive::with_oauth(OAuthClient {
    client_id: config.pipedrive_client_id,
    client_secret: SecretString::new(config.pipedrive_client_secret),
    redirect_uri: "https://yourapp.example/oauth/pipedrive/callback".parse()?,
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(pipedrive.clone())).build()?;

// When a user clicks "Connect Pipedrive":
let key = ConnectionKey::new(ProviderId::new("pipedrive")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

Four things are particular to Pipedrive:

- **Scopes are set in Developer Hub, not at sign-in.** The sign-in address has no parameter for them, so Socket writes none, whatever is passed to `begin_authorization`. `PipedriveOAuth` has no `scopes` setting for the same reason. Each operation's `required_scopes` says which scope to tick for it; `base`, which identifies the account, is always granted.
- **Each company has its own API host.** Sign-in is at `oauth.pipedrive.com` for everyone; the token response names the company's host in `api_domain` (`https://acme.pipedrive.com`), and that connection's calls go to `https://acme.pipedrive.com/api/…` from then on. The address is stored with the connection's tokens and kept through every refresh. Socket accepts a host only under `pipedrive.com`, refuses any other before anything is stored, and sends a connection's token to its own company's host and no other company's.
- **The client's id and secret travel as HTTP Basic**, in a header, to the token endpoint only.
- **A refresh token lasts 60 days from its last use.** An access token lasts about an hour, and Socket renews it. A connection left unused for 60 days fails with `ReconnectRequired`, and the person connects again.

There is no PKCE: Pipedrive documents none.

A person who declines is sent back to your callback with `error=user_denied` and no code. That redirect is your application's to handle.

## Identity and lookup

```rust
let me = pipedrive.identity(&connection).await?;        // Account { id, name, email }
let deal = pipedrive.resolve(&connection, link).await?;  // Resource { id, label, description }
```

`identity` reads `GET /api/v1/users/me` and needs only `base`. The id is the user's id, as `owner_id` holds it. The name is the user's with the company's beside it, `Ada Lovelace (Acme Ltd)`: one person can be a user of several companies, and a connection is to one of them. `users().me()` returns the company in full: `company_id`, `company_name`, `company_domain`, `company_country`.

`resolve` confirms a deal, a person, an organisation or a lead exists and the account can see it. It accepts the link to the record in Pipedrive (`https://acme.pipedrive.com/deal/42`, `/person/7`, `/organization/3`, `/leads/inbox/<uuid>`) or the short form `deal/42`, `person/7`, `organization/3`, `lead/<uuid>`. The resource's `id` is that short form and its label is the record's title or name. It needs the read scope of the kind of record and no other: `deals:read`, `contacts:read` or `leads:read`.

**A link to another company's Pipedrive is refused**, and the record is never fetched: the same number is a different record there, and would otherwise be reported found. A connection that calls its company's own host knows which company it is to. One that calls `api.pipedrive.com` (an API token without `company_domain`, or an OAuth connection whose token response named no host) asks Pipedrive with `users/me` first, which is one more request for a link and none for the short form. If Pipedrive does not name the company, the link is refused with `Decode` and not taken on trust; the short form still works.

## Use the typed methods

Methods are grouped as Pipedrive groups its API. Each group is reached through the `Pipedrive` value and a connection: `pipedrive.deals(&connection)`.

**How arguments are split.** What identifies the record acted on is a plain argument: a number (`u64`) for a deal, a person, an organisation, an activity or a note, and a UUID (`&str`) for a lead. Content and optional filters are structs from `socketkit::pipedrive::models`, where every field you leave unset is not sent, so Pipedrive applies its own default and a change touches only what you named.

**Names are Pipedrive's own**, as version 2 of its API writes them: `org_id`, `stage_id`, `expected_close_date`, `custom_fields`.

```rust
use socketkit::pipedrive::models::{CreateNote, DealStatus, DealStatusFilter, ListDeals, UpdateDeal};

let deals = pipedrive.deals(&connection);
let open = deals.list(ListDeals { org_id: Some(5), status: Some(vec![DealStatusFilter::Open]), ..Default::default() }).await?;

let deal = deals.update(42, UpdateDeal { stage_id: Some(4), status: Some(DealStatus::Won), ..Default::default() }).await?;
pipedrive.notes(&connection).create(CreateNote {
    content: "<p>They want a two-year term.</p>".into(),
    deal_id: Some(42),
    ..Default::default()
}).await?;
```

### Two versions of the API, one shape

Pipedrive is moving from version 1 of its API to version 2, one kind of record at a time. Socket calls version 2 wherever Pipedrive offers it and version 1 where it does not, and a caller sees one model for each kind of record and one way of paging. Both versions are below `/api/` on the same host.

| Group | list | get | search | create | update | delete |
| --- | --- | --- | --- | --- | --- | --- |
| `deals` | v2 | v2 | v2 | v2 `POST` | v2 `PATCH` | v2 |
| `persons` | v2 | v2 | v2 | v2 `POST` | v2 `PATCH` | v2 |
| `organizations` | v2 | v2 | v2 | v2 `POST` | v2 `PATCH` | v2 |
| `leads` | v1 | v1 | v2 | v1 `POST` | v1 `PATCH` | v1 |
| `activities` | v2 | v2 | | v2 `POST` | v2 `PATCH` | v2 |
| `notes` | v1 | v1 | | v1 `POST` | v1 `PUT` | v1 |

`pipelines.list` and `pipelines.stages` (`/api/v2/pipelines`, `/api/v2/stages`), the three `fields` lists (`/api/v2/dealFields`, `/personFields`, `/organizationFields`) and `search.items` (`/api/v2/itemSearch`) are version 2. `users.list` and `users.me` (`/api/v1/users`, `/api/v1/users/me`) are version 1.

What Socket evens out between the two:

- **Paging.** A cursor in version 2, an offset in version 1; one `cursor` here. See below.
- **A lead's custom fields.** Version 1 writes them beside the lead's own fields; Socket moves them under `custom_fields`, where version 2 has them.
- **An empty list.** Version 1 writes `null`; a caller gets an empty list either way.
- **A note's update** is a `PUT` in version 1 and still changes only what is sent.

### `pipedrive.deals(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListDeals)` | `Page<Deal>`: by owner, person, organisation, pipeline, stage, status, a saved filter, or when it changed |
| `get(deal)` | `Deal`, with its custom fields |
| `search(SearchDeals)` | `Page<SearchResult>` |
| `create(CreateDeal)` | `Deal` |
| `update(deal, UpdateDeal)` | `Deal` |
| `delete(deal)` | nothing |

Reading needs `deals:read`; the rest `deals:full`. Moving a deal to another stage, and marking it won or lost, are `update` with `stage_id` or `status`.

**A change cannot delete a deal.** Pipedrive's own create and update take a fourth status, `deleted`, and a deal given it is deleted. Socket's `create`, `update` and `search` take `open`, `won` and `lost` only (`DealStatus`): the fourth is refused before anything is sent, and each operation's schema does not list it. Deleting is `delete`, which is marked `destructive`, so a host that asks a person before a deletion is always asked. The flags `is_deleted` and `is_archived`, which Pipedrive's update also takes, are not fields of any create or update here for the same reason. A list is a read, and its `status` (`DealStatusFilter`) takes several values, `deleted` among them, which lists the deals deleted in the last 30 days.

### `pipedrive.persons(&connection)` and `pipedrive.organizations(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListPersons)` / `list(ListOrganizations)` | `Page<Person>` / `Page<Organization>` |
| `get(id)` | the record, with its custom fields |
| `search(SearchPersons)` / `search(SearchOrganizations)` | `Page<SearchResult>` |
| `create(CreatePerson)` / `create(CreateOrganization)` | the record |
| `update(id, UpdatePerson)` / `update(id, UpdateOrganization)` | the record |
| `delete(id)` | nothing |

Reading needs `contacts:read`; the rest `contacts:full`. A person's `emails` and `phones` are lists of `{ value, primary, label }`; a list sent in an update replaces the one that was there.

### `pipedrive.leads(&connection)`

`list(ListLeads)`, `get(lead)`, `search(SearchLeads)`, `create(CreateLead)`, `update(lead, UpdateLead)`, `delete(lead)`. Reading needs `leads:read`; the rest `leads:full`. A lead's id is a UUID. A lead has to be linked to a person, an organisation or both. `list` returns the leads that are not archived; `update` with `is_archived` archives one or brings it back, which is the one such flag an update takes: an archived lead is hidden from the list, and the same operation brings it back. A lead has a deal's custom fields.

### `pipedrive.activities(&connection)`

`list(ListActivities)`, `get(activity)`, `create(CreateActivity)`, `update(activity, UpdateActivity)`, `delete(activity)`. Reading needs `activities:read`; the rest `activities:full`. An activity is a call, a meeting, a task or an email logged against a deal, a lead, a person or an organisation; `type` says which, and a company can add its own. The persons taking part are `participants`; the one marked `primary` is the activity's `person_id`. Logging a call that took place is `create` with `done: true`.

### `pipedrive.notes(&connection)`

`list(ListNotes)`, `get(note)`, `create(CreateNote)`, `update(note, UpdateNote)`, `delete(note)`. A note is written on a deal, a person, an organisation or a lead, and `content` is HTML. The operations list both `deals:read` and `contacts:read` for reading, and both `deals:full` and `contacts:full` for the rest. **Either one is enough**: Pipedrive accepts either scope at the endpoint, and each of these operations says so in its description, so that a host does not ask for both. Pipedrive describes each scope as covering the notes of its own records.

### `pipedrive.pipelines(&connection)`, `pipedrive.users(&connection)` and `pipedrive.search(&connection)`

| Method | Returns | Scope |
| --- | --- | --- |
| `pipelines().list(Paging)` | `Page<Pipeline>` | `deals:read` |
| `pipelines().stages(pipeline, Paging)` | `Page<Stage>`: of one pipeline, or of all when `None` | `deals:read` |
| `users().list()` | `Vec<User>`: Pipedrive returns them all at once | `users:read` |
| `users().me()` | `User`, with the company | `base` |
| `search().items(SearchItems)` | `Page<SearchResult>` across every kind of record | `search:read` |

A search needs a term of two characters, or one with `exact_match`. A result is enough of a record to tell it from the others (`id`, `type`, a title or a name, what it is linked to); the record itself is read with its group's `get`. `deals.search`, `persons.search`, `organizations.search` and `leads.search` are the same search kept to one kind, and each also works with that kind's read scope.

## Custom fields

A company adds its own fields to deals, persons and organisations, and Pipedrive stores each value under a 40-character key such as `4d1d7a5b1b5a2c5b6a3e9f8d7c6b5a4f3e2d1c0b`. The key says nothing by itself. **`pipedrive.fields(&connection)` is what gives it a meaning:**

| Method | Returns | Scope |
| --- | --- | --- |
| `deal_fields(Paging)` | `Page<Field>`: also the custom fields of a lead | `deals:read` |
| `person_fields(Paging)` | `Page<Field>` | `contacts:read` |
| `organization_fields(Paging)` | `Page<Field>` | `contacts:read` |

A `Field` has the key (`field_code`), the name the company gave it (`field_name`), its kind (`field_type`), whether the company added it (`is_custom_field`), and for a field of choices its `options`, each with the `id` a record holds and the `label` people see.

Records come back with custom fields **as Pipedrive sends them**, under `custom_fields`, key and value. Socket does not rename them in the models. Putting names in would need the list of fields beside every record, which is a second request hidden inside each call, or a cache that can be out of date; either would also make a record's shape depend on something the call did not ask for. The join is one function instead, to run against a list you fetched once:

```rust
use socketkit::pipedrive::models::{Paging, named_custom_fields};

let fields = pipedrive.fields(&connection).deal_fields(Paging { limit: Some(500), ..Default::default() }).await?.items;
let deal = pipedrive.deals(&connection).get(42).await?;
for field in named_custom_fields(&deal.custom_fields, &fields) {
    // field.field_name: Some("Industry"), field.value: 12, field.labels: ["Software"], field.field_code: the key
}
```

Each `NamedValue` keeps the key beside the name, the value as it was sent, and the labels of the choices the value names. An operation called by name does the same join from `fields.deal_fields` and a record's `custom_fields`: the key is the same string in both.

To write a custom field, give its key in `custom_fields` on a create or an update. In an update a value of `null` clears that field, and a key that is not named is left alone. Only the 40-character key of a custom field is taken there. Anything else is refused before a request is made, so one of a record's own fields, such as `status` or `is_deleted`, cannot be set under that name.

## Lists are small

A list is for finding records, and an agent that reads it has a limited context, so a row leaves out the heavy part, which `get` returns. **What a row did not fetch is absent from it**, never written as `null` or as an empty list: a row that said `attendees: []` would say nobody was invited.

| List | What a row leaves out |
| --- | --- |
| `deals`, `persons`, `organizations` | The custom fields, unless `custom_fields` names up to 15 keys to return |
| `leads` | The custom fields |
| `activities` | `note`, `public_description` and `attendees`: all three are absent from a row |
| `notes` | Everything after the first 500 characters of `content`; `truncated` is then `true` |

A search returns matches, not records, and a match says only what its kind of record has: a deal found by a search carries no `emails` field.

## Page through a list

A list returns a `Page` with `items` and `next_cursor`. Pass the cursor back for the next page, to the same method with the same filters; `None` means the last page.

```rust
let mut options = ListDeals { status: Some(vec![DealStatusFilter::Open]), limit: Some(200), ..Default::default() };
loop {
    let page = pipedrive.deals(&connection).list(options.clone()).await?;
    for deal in &page.items { /* … */ }
    match page.next_cursor {
        Some(cursor) => options.cursor = Some(cursor),
        None => break,
    }
}
```

Version 2 pages by an opaque cursor of Pipedrive's own. Version 1 (`leads.list`, `notes.list`) pages by the number of items to skip, which Socket writes as a cursor that says what it is: `offset:200`. So the two kinds cannot be confused: a version 1 list takes only `offset:` followed by digits, and a version 2 list refuses an `offset:` cursor, each before any request is made.

A cursor comes back from the caller, so Socket does not trust it to be what Pipedrive sent. It is only ever sent as the value of one query parameter, percent-encoded whole. Nothing in it can become a host, a path or another parameter, so a cursor written by hand cannot move a listing off its own list or send the token anywhere else; at worst Pipedrive refuses it.

`limit` is from 1 to 500 for a list and from 1 to 100 for a search, and anything else is refused. Pipedrive returns 100 when it is not given. `users.list` is not paged.

## Call an operation by name

Every method is also an operation an agent, an MCP server or another language can call with JSON. The plain arguments and the options sit side by side in one object.

```rust
let output = socket
    .invoke(key, "pipedrive.deals.update".into(), serde_json::json!({ "deal": 42, "stage_id": 4 }))
    .await?;
```

`socket.operations()` returns each operation's name, description, input schema, output schema, effect and scopes. The effect lets a host ask a person before a change:

- **read** changes nothing. Every read here is a `GET`.
- **write** creates a record or changes one: a new deal, a note, a logged call, a deal moved to another stage or marked won.
- **destructive** deletes. Pipedrive keeps a deleted deal, person, organisation or activity for 30 days before removing it for good; the documentation does not say the same of a lead or a note.

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `pipedrive.identity.get` | read | `base` | Return the account this connection is authorised as, confirming the token still works. |
| `pipedrive.resource.resolve` | read | `deals:read`, `contacts:read` or `leads:read` | Confirm that a resource exists and the account can reach it. Accepts the link to a deal, a person, an organization or a lead in Pipedrive, or the short form deal/42. Of the scopes listed, only the one for the kind of record given is needed: deals:read for a deal, contacts:read for a person or an organization, leads:read for a lead. |
| `pipedrive.deals.list` | read | `deals:read` | List deals: all that are not deleted, or those of one owner, person, organization, pipeline, stage or status. Rows carry no custom fields unless some are asked for. |
| `pipedrive.deals.get` | read | `deals:read` | Get one deal, with its custom fields under their keys. fields.deal_fields names them. |
| `pipedrive.deals.search` | read | `deals:read` | Search deals by title, notes and custom fields. Returns matches, best first, not whole deals. |
| `pipedrive.deals.create` | write | `deals:full` | Create a deal. |
| `pipedrive.deals.update` | write | `deals:full` | Change a deal: move it to another stage, mark it won or lost, or change any field given. Fields not given are left as they are. |
| `pipedrive.deals.delete` | destructive | `deals:full` | Delete a deal. Pipedrive keeps it for 30 days, then removes it for good. |
| `pipedrive.persons.list` | read | `contacts:read` | List persons: all that are not deleted, or those of one owner, organization or deal. Rows carry no custom fields unless some are asked for. |
| `pipedrive.persons.get` | read | `contacts:read` | Get one person, with their custom fields under their keys. fields.person_fields names them. |
| `pipedrive.persons.search` | read | `contacts:read` | Search persons by name, email, phone, notes and custom fields. Returns matches, best first, not whole persons. |
| `pipedrive.persons.create` | write | `contacts:full` | Create a person. |
| `pipedrive.persons.update` | write | `contacts:full` | Change a person. Fields not given are left as they are; a list of emails, phones or labels replaces the one that was there. |
| `pipedrive.persons.delete` | destructive | `contacts:full` | Delete a person. Pipedrive keeps them for 30 days, then removes them for good. |
| `pipedrive.organizations.list` | read | `contacts:read` | List organizations: all that are not deleted, or those of one owner. Rows carry no custom fields unless some are asked for. |
| `pipedrive.organizations.get` | read | `contacts:read` | Get one organization, with its custom fields under their keys. fields.organization_fields names them. |
| `pipedrive.organizations.search` | read | `contacts:read` | Search organizations by name, address, notes and custom fields. Returns matches, best first, not whole organizations. |
| `pipedrive.organizations.create` | write | `contacts:full` | Create an organization. |
| `pipedrive.organizations.update` | write | `contacts:full` | Change an organization. Fields not given are left as they are. |
| `pipedrive.organizations.delete` | destructive | `contacts:full` | Delete an organization. Pipedrive keeps it for 30 days, then removes it for good. |
| `pipedrive.leads.list` | read | `leads:read` | List the leads that are not archived: all of them, or those of one owner, person or organization. Rows carry no custom fields. |
| `pipedrive.leads.get` | read | `leads:read` | Get one lead, with its custom fields under their keys. A lead has a deal's fields; fields.deal_fields names them. |
| `pipedrive.leads.search` | read | `leads:read` | Search leads by title, notes and custom fields. Returns matches, best first, not whole leads. |
| `pipedrive.leads.create` | write | `leads:full` | Create a lead, linked to a person, an organization or both. |
| `pipedrive.leads.update` | write | `leads:full` | Change a lead, or archive it. Fields not given are left as they are. |
| `pipedrive.leads.delete` | destructive | `leads:full` | Delete a lead. |
| `pipedrive.activities.list` | read | `activities:read` | List activities (calls, meetings, tasks, emails): all that are not deleted, or those of one owner, deal, lead, person or organization, done or not. Rows leave out note, public_description and attendees: those fields are absent from a row, not empty, and activities.get returns them. |
| `pipedrive.activities.get` | read | `activities:read` | Get one activity, with its note, its public_description and its attendees. |
| `pipedrive.activities.create` | write | `activities:full` | Create an activity: a task to do, or a call, a meeting or an email that took place. |
| `pipedrive.activities.update` | write | `activities:full` | Change an activity, or mark it done. Fields not given are left as they are. |
| `pipedrive.activities.delete` | destructive | `activities:full` | Delete an activity. Pipedrive keeps it for 30 days, then removes it for good. |
| `pipedrive.notes.list` | read | `deals:read` or `contacts:read` | List notes: all of them, or those on one deal, person, organization or lead. A long note is cut short and marked truncated. Either one of the scopes listed is enough. |
| `pipedrive.notes.get` | read | `deals:read` or `contacts:read` | Get one note, with its whole text. Either one of the scopes listed is enough. |
| `pipedrive.notes.create` | write | `deals:full` or `contacts:full` | Write a note on a deal, a person, an organization or a lead. Either one of the scopes listed is enough. |
| `pipedrive.notes.update` | write | `deals:full` or `contacts:full` | Change a note. New content replaces the text that was there. Either one of the scopes listed is enough. |
| `pipedrive.notes.delete` | destructive | `deals:full` or `contacts:full` | Delete a note. Either one of the scopes listed is enough. |
| `pipedrive.pipelines.list` | read | `deals:read` | List the company's pipelines. |
| `pipedrive.pipelines.stages` | read | `deals:read` | List the stages of one pipeline, or of every pipeline. A deal's stage_id is one of these. |
| `pipedrive.fields.deal_fields` | read | `deals:read` | List the fields of a deal, with the name and choices of each custom field. A record holds a custom field under its field_code. Leads have the same custom fields. |
| `pipedrive.fields.person_fields` | read | `contacts:read` | List the fields of a person, with the name and choices of each custom field. A record holds a custom field under its field_code. |
| `pipedrive.fields.organization_fields` | read | `contacts:read` | List the fields of an organization, with the name and choices of each custom field. A record holds a custom field under its field_code. |
| `pipedrive.users.list` | read | `users:read` | List every user of the company's Pipedrive. |
| `pipedrive.users.me` | read | `base` | Get the signed-in user, with the company the connection is to. |
| `pipedrive.search.items` | read | `search:read` | Search every kind of record at once (deals, persons, organizations, leads and more), or only the kinds named. Returns matches, best first, not whole records. |

Where more than one scope is named, the operation's `required_scopes` lists them all and **one of them is enough**, which the operation's own description says: either scope for a note, and for `resource.resolve` the one for the kind of record it is given. A host should not read such a list as needing every scope in it.

An update is marked `write`, as the issue that asked for this provider sets out, and unlike Microsoft's and Slack's updates, which are `destructive`. A host that asks a person only before a destructive operation therefore does not ask before a deal changes stage; one that asks before any write does. What makes that safe is that no `write` here can delete: a create or an update takes no status and no flag that removes a record.

## Limits

Pipedrive limits a company's use of the API in two ways, and both apply to OAuth and API-token traffic together:

- **A daily budget of tokens for the whole company.** `30,000 × the plan's multiplier × the number of seats`, where the multiplier is 1 for Lite, 2 for Growth, 5 for Premium and 7 for Ultimate. Every call costs tokens: in version 2 a `get` costs 1, a list 10 (5 for pipelines and stages), a search 20, a create or an update 5, a delete 3; in version 1 a `get` costs 2, a list 20, a create or an update 10, a delete 6. The budget is shared by every application the company uses and is new at midnight, by Pipedrive's clock. Once it is spent, every call is refused until then.
- **A burst limit for each user**, over a rolling two seconds: from 20 requests (an API token on Lite) to 480 (an OAuth app on Ultimate). Search has its own, 10 requests in two seconds.

Either one answers `429`, which Socket reports as `RateLimited`. `retry()` carries the wait: `Retry-After` when Pipedrive sends it, and otherwise `x-ratelimit-reset`, the time left in the two-second window. Pipedrive's answer does not say which of the two limits was reached. If a wait of a second or two is answered with another `429`, the day's budget is probably spent, and trying again will not help before the next day.

A client that keeps calling through repeated `429`s is blocked for a while with a `403` that is a web page and not JSON. Socket reports that as `AccessDenied` and says so in the message, and does not retry it.

What a plan or a permission prevents comes back as `AccessDenied`, with the reason: a scope the app was not given, a limit of the company's plan (Pipedrive's `code` is in the message, such as `feature_capping_deals_limit`), a record the user may not see (`visible_to`), or an account that is not open (`402`, a trial that ended or a payment that is missing).

## Handle errors

| Kind | What it means for Pipedrive | What to do |
| --- | --- | --- |
| `ReconnectRequired` | Pipedrive answered 401, or no longer accepts the refresh token | Socket renews an OAuth token once by itself; if that fails, connect again. An API token that is refused has been changed or revoked |
| `AccessDenied` | A scope is missing, the plan has reached a limit, the user may not see the record, the account is not open, or the client is blocked after repeated throttling. The message says which | Add the scope in Developer Hub and connect again, or act on the reason |
| `NotFound` | No such record, or one the user cannot see | Check the id |
| `InvalidInput` | Pipedrive refused the request, and the message carries its reason; or Socket refused it before sending | Fix the input |
| `RateLimited` | Either limit above; `retry()` says how long to wait when Pipedrive did | Wait and try again |
| `Unexpected` | Pipedrive failed, or said the request failed inside a successful answer, or has retired the address (410) | Try again later; report a 410 |
| `Decode` | Pipedrive answered success without what was asked for, or with something that could not be read; or a token response named an API address that is not Pipedrive's | Report it; this should not happen |
| `Config` | The definition or a setting is wrong: a company name that is not one, an API token placed in the address | Fix the settings |

Every answer of Pipedrive's carries `success`. An answer that says `success: false` is a failure whatever its status, and an answer without `success: true` is never read as a record, an empty list or a finished delete.

An input error never repeats the value you sent. It names the field when a required one is missing or a plain argument has the wrong type. **A field the operation does not know is refused**, at any depth, and named: `stage`, `emails[1].valeu`, `address.cuntry`. A dropped field would take what it said with it. Each operation's input schema says the same, with `additionalProperties: false`. Inside `custom_fields` the keys are the company's own, so the schema cannot list them; only the 40-character key of a custom field is taken there, and anything else is refused. An answer that cannot be read is reported the same way: the error says where the unreadable value was, and neither its message nor its cause carries anything from the record.

An id goes into a path only as what it is: a number, or a UUID for a lead. Anything else is refused before a request is made, so an id cannot add a segment, a query or a fragment.

A read is retried on a throttle or a server error. A create (`POST`) and an update (`PATCH`) are sent again in only two cases, both of which mean Pipedrive did not carry them out: it throttled the request, or it rejected the access token and Socket renewed it. **If one of them fails with a server error, check before sending it again**: Pipedrive would create a second record.

**Deleting, and changing a note, are the exceptions today.** The transport repeats a `DELETE` and a `PUT` after a server error. If the first try did delete the record, the second is answered "not found", and the call reports `NotFound` for a delete that worked. Treat `NotFound` from `delete` as "it is gone". A repeated change to a note sets the same text twice.

## Confirmed against Pipedrive's documentation, and not

Read from developers.pipedrive.com and pipedrive.readme.io in October 2026, and from Pipedrive's own client library (`pipedrive/client-nodejs`, generated from its API description) for what the reference pages keep folded: the fields of each response and the scopes of each endpoint. Nothing was run against a live account.

Confirmed:

- The authorise and token addresses at `oauth.pipedrive.com`, the client's id and secret as HTTP Basic, the form fields of both grants, and that the authorise address takes `client_id`, `redirect_uri` and `state` and no scopes.
- The token response's fields, `api_domain` among them, and that calls then go to `{api_domain}/api/v1/…`; Pipedrive's client builds the same address.
- An access token lasts 60 minutes; a refresh returns the same refresh token, which lasts 60 days from its last use.
- The personal API token goes in the `x-api-token` header. The page shows it against the company's own host with `/api/v2/…`.
- Every endpoint above, with its verb and version, as in the table of versions: `/api/v2/deals`, `/persons`, `/organizations` and `/activities`, each with `/{id}`; `/api/v2/deals/search`, `/persons/search`, `/organizations/search`, `/leads/search` and `/itemSearch`; `/api/v1/leads` and `/api/v1/notes`, each with `/{id}`; `/api/v2/pipelines`, `/stages`, `/dealFields`, `/personFields`, `/organizationFields`; `/api/v1/users` and `/users/me`. That version 2 changes a record with `PATCH`, version 1 a lead with `PATCH` and a note with `PUT`.
- The filters of each list, the fields of each create and update, and the rules beside them: a lead needs a person or an organisation; a note needs something to be written on; a search term needs two characters, or one with `exact_match`; `custom_fields` on a list takes at most 15 keys; a `null` clears a custom field.
- The scopes of every endpoint, from the client library: `deals:read`/`deals:full`, `contacts:read`/`contacts:full`, `leads:read`/`leads:full`, `activities:read`/`activities:full`, either pair for notes, `deals:read` for pipelines, stages and deal fields, `contacts:read` for person and organisation fields, `users:read` for users, `base` for `users/me`, `search:read` for the item search and, beside each kind's read scope, for its own search.
- The fields of a deal, a person, an organisation, an activity, a pipeline, a stage, a field and its options, a lead, a note, a user and the signed-in user's company, and of a search result of each kind, with their spelling.
- Paging: `cursor` and `limit` with `additional_data.next_cursor` in version 2; `start` and `limit` with `additional_data.pagination` (`more_items_in_collection`, `next_start`) in version 1; a limit of at most 500, and 100 for a search.
- The error body (`success: false`, `error`, `error_info`, and on a plan limit `code`), the list of statuses with 402, 403, 404, 410 and 429, the daily budget and its formula, the burst limits, the rate-limit headers, the cost of each endpoint, and the web-page 403 after repeated throttling.
- A delete marks a deal, a person, an organisation or an activity as deleted for 30 days. A deleted note answers `data: true`, the others the id.

Where the documentation differs from the issue that asked for this provider:

- **Scopes are not part of sign-in.** The issue speaks of read scopes by default and write scopes on request. Pipedrive takes an app's scopes from its settings, so Socket's definition lists none and an application cannot ask for more at connection time.
- **Custom fields are returned as they come**, with the list of fields made easy to join, and not renamed in the models, which the issue suggested considering. The reason is in the section on custom fields.
- **Fields, pipelines and stages are version 2 now**, as are deals, persons, organisations and activities. Only leads (but for their search), notes and users are still version 1.
- **An activity takes its person through `participants`**, not a `person_id` of its own.

Not confirmed:

- **`api.pipedrive.com` with `/api/v1/…`.** The reference lists version 1 paths as `/api/v1/…` and names no host; Pipedrive's client library calls `api.pipedrive.com/v1/…` and `api.pipedrive.com/api/v2/…`. Socket calls `/api/v1/…` and `/api/v2/…` on whichever host a connection has. On a company's own host both are documented. This matters only to an API token used without `company_domain`; name the company to be on documented ground.
- **A token response without `api_domain`.** Pipedrive documents it as always there. If it is ever absent, the connection calls `api.pipedrive.com`, as Pipedrive's client library does.
- **`x-ratelimit-reset` in seconds.** The documentation gives no unit. Socket reads a whole number of at most a day as seconds and ignores anything else. `Retry-After` is not documented at all; it is used when sent.
- **Which limit a 429 is for.** Nothing in the answer is documented to tell the daily budget from the burst limit.
- **`success: false` inside a 200, and `errorCode`.** Neither is documented. Socket treats the first as a failure and reads the second, when it is an HTTP status, to say which.
- **The words of a missing-scope refusal.** The documentation says such a request is denied and gives no body. Socket calls a 403 a missing scope when Pipedrive's `error` mentions a scope, and passes on Pipedrive's words either way.
- **Several statuses on `deals.list`.** The reference describes `status` as a list separated by commas; the client library types it as one value. Socket sends what it is given, joined by commas, which is the same request for one.
- **Whether a notes scope reaches every note.** The endpoint accepts either `deals:…` or `contacts:…`; the page on scopes describes each as covering the notes of its own records. The operations name both.
- **The shape of a field's `subfields`**, which Socket returns as Pipedrive sends them, and **of a lead's `visible_to`**, which is read whether Pipedrive writes it as a string or a number.
- **How a space in a query is read.** Socket writes `%20`, never `+`, which Pipedrive's own instruction to percent-encode a search term implies.
- **The links `resolve` accepts.** The addresses of records in Pipedrive's web application are not part of its API documentation. The short form `deal/42` does not depend on them.
- **The scope parameter at sign-in.** Socket sends none. Whether Pipedrive would ignore one or refuse it was not confirmed.
- **Whether a deleted lead or note can be restored.**

## Not supported yet

- **Products, files, the mailbox, webhooks and automations**, which the issue leaves out. Webhooks are phase 3 of the roadmap.
- **Deleting a deal through its status, and archiving a deal.** `delete` deletes; nothing here archives a deal.
- **Clearing one of a record's own fields** by sending `null`: unlinking a person from a deal, removing a lead's value. A custom field can be cleared.
- **Changing a field's definition**, and creating pipelines, stages and users, which need the `admin` scope.
- **Labels, followers, participants of a deal, comments on a note, and projects.**
- **Archived deals and archived leads** as lists of their own, and converting a lead to a deal.
- **Related items in `search.items`**, which Pipedrive returns in a second list beside the results.
- **Option labels inside a record** (`include_option_labels`), which `named_custom_fields` gives without asking Pipedrive again.
- **OAuth connections and API-token connections in one `Socket`**, since a definition has one way of authenticating.
