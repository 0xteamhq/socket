# HubSpot

**Status:** built and tested against a local server that answers as HubSpot's documentation says. Not yet run against a real HubSpot account.

Socket's HubSpot integration gives a program the HubSpot CRM. HubSpot's CRM is one API over many object types, so the integration is too: the same 18 typed methods read and write contacts, companies, deals, tickets, the activities logged on them (notes, calls, meetings, emails, tasks) and custom objects, with the object type as an argument. The same 18 are operations callable by name with JSON, beside identity and lookup of an object type. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against HubSpot's documentation.

## Connect

Add the crate with the HubSpot feature:

```sh
cargo add socketkit --features hubspot
```

### With a private app's access token

Create a private app in the HubSpot account, tick the scopes it needs, and copy its access token.

```rust
use std::sync::Arc;
use socketkit::hubspot::HubSpot;
use socketkit::{ConnectionKey, ProviderId, Socket};

let hubspot = HubSpot::with_token("pat-na1-…");
let socket = Socket::in_memory().integration(Arc::new(hubspot.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("hubspot")?, "me")).await?;
```

Every call uses that token, as a bearer token. A private app's token belongs to one account and there is no refresh token beside it, so this suits a program that works on your own account.

### With your own app, to connect your users' accounts

Create an app in a HubSpot developer account, set your callback route as its redirect URL, and choose its scopes.

```rust
use socketkit::hubspot::{HubSpot, HubSpotOAuth};
use socketkit::{OAuthClient, SecretString};

let hubspot = HubSpot::with_oauth(HubSpotOAuth {
    client: OAuthClient {
        client_id: config.hubspot_client_id,
        client_secret: SecretString::new(config.hubspot_client_secret),
        redirect_uri: "https://yourapp.example/oauth/hubspot/callback".parse()?,
    },
    // What every account has to grant. `None` asks for the default: oauth.
    scopes: Some(vec!["crm.objects.contacts.read".into(), "crm.objects.deals.read".into()]),
    // What an account grants only if its plan has it.
    optional_scopes: Some(vec!["crm.objects.custom.read".into()]),
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(hubspot.clone())).build()?;

// When a user clicks "Connect HubSpot":
let key = ConnectionKey::new(ProviderId::new("hubspot")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

`HubSpot::with_oauth(client)` with a plain `OAuthClient` also works when the default is enough.

| Setting | What it does |
| --- | --- |
| `scopes` | The scopes every account has to grant, in place of the default. They have to be the required scopes the app itself is set up with. |
| `optional_scopes` | Scopes to ask for where the account has them. They go out as HubSpot's `optional_scope` parameter. |

**Why there are two lists.** HubSpot refuses the whole authorisation when the app asks, in `scopes`, for a scope the account's plan does not have: the person sees an error and no connection is made. A scope in `optional_scopes` is quietly left out for such an account. Custom objects are the usual case: `crm.objects.custom.read` exists on Enterprise accounts only, so an app that should also install on smaller accounts lists it as optional. Which optional scopes an account granted is in the connection's stored `scopes`.

Four things Socket does for every HubSpot connection made through OAuth:

- **`oauth` is always asked for.** HubSpot requires it of every app, and it is the scope that reads the account's details. It is added to whatever scopes are given, in the settings or at `begin_authorization`.
- **The granted scopes are read from `scopes`.** HubSpot lists them there, as a list, where the OAuth standard has one string under `scope`.
- **A refresh keeps the refresh token.** A HubSpot access token lasts 30 minutes, so Socket refreshes often. The refresh token is kept as it is, whether HubSpot sends it back or not.
- **The client id and secret go in the body** of the token request, and nowhere else.

The person approves at `https://app.hubspot.com/oauth/authorize`. Tokens are exchanged at `https://api.hubapi.com/oauth/2026-09/token`. Credentials are sent to `api.hubapi.com` only; the approval page is never sent one, so it is not among the hosts the provider allows.

## Identity and lookup

```rust
let account = hubspot.identity(&connection).await?;          // Account { id, name, email: None }
let contacts = hubspot.resolve(&connection, "contacts").await?; // Resource { id, label, description }
```

`identity` reads `GET /account-info/2026-09/details` and needs `oauth`. The id is the account's number, which HubSpot also calls the portal id or the hub id; the name is `portalName`, or the number when HubSpot sends no name. A connection is to an account and not to a person, so there is no email.

That endpoint was chosen over HubSpot's token lookups for three reasons. It takes the token in the `Authorization` header like every other call; the older lookup, `GET /oauth/v1/access-tokens/{token}`, puts the token in the address, where proxies and logs keep it, and HubSpot retires it in February 2027. Its replacement, `POST /oauth/2026-09/token/introspect`, needs the app's client id and secret, which a private app's token does not have. And the account details endpoint works the same for both kinds of token.

`resolve` accepts an object type by name (`contacts`, `deals`) or by type id (`0-1`, or `2-12345` for a custom object), and confirms the connection can read it by asking for one record of it. The resource's `id` is the type as given, which is what every `objects` method takes. It needs the read scope of that type. Anything that could not be an object type is refused before HubSpot is called.

## Use the typed methods

Methods are grouped by area. Each group is reached through the `HubSpot` value and a connection: `hubspot.objects(&connection)`.

**How arguments are split.** The object type and the ids are plain arguments. Content and optional filters are structs from `socketkit::hubspot::models`, where every field you leave unset is not sent, so HubSpot applies its own default.

**Names are HubSpot's own.** In Rust the fields are written the Rust way (`filter_groups`); in JSON they are written as HubSpot writes them (`filterGroups`, `associationTypeId`), so HubSpot's documentation of a field holds here too.

**The object type** is the first argument of every `objects` method: `contacts`, `companies`, `deals`, `tickets`, `notes`, `calls`, `meetings`, `emails`, `tasks`, a type id (`0-1` is contacts, `0-3` deals), or a custom object's type id such as `2-12345`.

```rust
use socketkit::hubspot::models::{CreateObject, Filter, FilterGroup, GetObject, Operator, Search};

let objects = hubspot.objects(&connection);

// Which properties are there to ask for?
let fields = hubspot.properties(&connection).list("deals", Default::default()).await?;

let deal = objects.get("deals", "9001", GetObject {
    properties: Some(vec!["dealname".into(), "amount".into(), "dealstage".into()]),
    associations: Some(vec!["contacts".into()]),
    ..Default::default()
}).await?;
let amount = deal.properties["amount"].as_deref(); // Some("1500"), or None when it has no value

let open = objects.search("deals", Search {
    filter_groups: Some(vec![FilterGroup { filters: vec![Filter::new("dealstage", Operator::Neq, "closedwon")] }]),
    properties: Some(vec!["dealname".into()]),
    ..Default::default()
}).await?;

let note = objects.create("notes", CreateObject::with([
    ("hs_timestamp", "2026-10-09T08:00:00Z"),
    ("hs_note_body", "Called, will sign Friday."),
])).await?;
```

### `hubspot.objects(&connection)`

| Method | Returns |
| --- | --- |
| `list(object_type, ListObjects)` | `Page<Record>` |
| `get(object_type, record, GetObject)` | `Record` |
| `batch_read(object_type, BatchRead)` | `BatchResult`: up to 100 records |
| `search(object_type, Search)` | `Page<Record>` |
| `create(object_type, CreateObject)` | `Record` |
| `update(object_type, record, UpdateObject)` | `Record` |
| `batch_create(object_type, BatchCreate)` | `BatchResult` |
| `batch_update(object_type, BatchUpdate)` | `BatchResult` |
| `archive(object_type, record)` | nothing |

**What a `Record` carries:** `id`, `properties`, `createdAt`, `updatedAt`, `archived`, `archivedAt`, and `associations` when they were asked for.

**A record returns only the properties you ask for.** Without `properties`, HubSpot returns a handful it chooses for the type. Name the ones you want by their internal names, and use `properties.list` to learn those names. Nothing here fetches more than was asked for.

**Every value is a string.** `properties` is a map from a property's name to its value as HubSpot stores it: `"1500"`, `"true"`, `"2026-10-09T08:00:00.000Z"`. A property that was asked for and has no value is `null`. What you send is written the same way; an empty string clears a property. A number or a boolean in place of a string is refused, so that nothing is converted by guesswork.

**Associations beside a record.** `associations: ["contacts"]` on `get` or `list` returns the ids of the associated records under `associations.contacts.results`. When there are more than HubSpot returns beside a record, `paging.next.after` is set, and `associations.list` reads the rest.

**A record by something other than its id.** `idProperty` names a property with unique values, such as `email` for a contact; `record` is then that value.

**Batches.** A batch takes at most 100 inputs, and an empty batch is refused. `batch_read` takes `ids`, and `idProperty` when they are values of a unique property. HubSpot answers a batch it carried out in part with success: the records that worked are in `results`, and what went wrong with the rest is in `errors`, each with a `category` such as `OBJECT_NOT_FOUND` and a `context` naming the ids. A record that does not exist is therefore not a failure of `batch_read`.

**Creating with associations.** `CreateObject.associations` associates the new record at once: `{ "to": { "id": "512" }, "types": [{ "associationCategory": "HUBSPOT_DEFINED", "associationTypeId": 202 }] }` puts a note on contact 512. HubSpot lists its own type ids in its documentation.

**Changing.** `update` sets the properties you name and leaves the rest. An update with no properties is refused.

**Archiving** moves a record to HubSpot's recycling bin, where it can be restored in HubSpot for 90 days. `archived: true` on `list`, `get` and `batch_read` reads what is in the bin.

**Activities** are records like any other. A note and an email need `hs_timestamp`, which places them on the timeline. A note's text is `hs_note_body`; an email has `hs_email_subject`, `hs_email_direction`, `hs_email_text` and `hs_email_html`. `properties.list` names the properties of calls, meetings and tasks. An activity is tied to a record by an association, which can be made when it is created.

### `hubspot.associations(&connection)`

| Method | Returns |
| --- | --- |
| `list(object_type, record, to_object_type, Paging)` | `Page<Association>`: the records of one type a record is associated with |
| `create(object_type, record, to_object_type, to_record, CreateAssociation)` | `Associated` |
| `remove(object_type, record, to_object_type, to_record)` | nothing |

An `Association` has `toObjectId` and `associationTypes`, each with its `category`, `typeId` and `label`.

**Creating.** With nothing set, `create` makes HubSpot's default, unlabelled association between the two object types. With `types`, it sets the kinds you name, which is how a label made in the account (`USER_DEFINED`) is set. They become all the labels between the two records: a label that was there and is not named is removed, so to add one, name the ones to keep as well. `associations.list` returns what is there. Making a default association that is already there changes nothing.

**Removing** takes away every association between the two records, labelled or not. The records themselves stay.

### `hubspot.properties(&connection)`

| Method | Returns |
| --- | --- |
| `list(object_type, ListProperties)` | `Vec<Property>`: every property of the type, at once |
| `get(object_type, property)` | `Property` |

A `Property` has its internal `name`, its `label`, its `type` (`string`, `number`, `date`, `datetime`, `enumeration`, `bool`), its `fieldType`, its `groupName`, its `options` (each with a `label` and the `value` to write) and what may be changed about it (`modificationMetadata.readOnlyValue`).

### `hubspot.pipelines(&connection)`

| Method | Returns |
| --- | --- |
| `list(object_type)` | `Vec<Pipeline>`, each with its stages |
| `get(object_type, pipeline)` | `Pipeline` |

Deals and tickets have pipelines. A pipeline's `id` is what a deal's `pipeline` property holds (`hs_pipeline` for a ticket), and a stage's `id` is what `dealstage` holds (`hs_pipeline_stage` for a ticket). A stage's `metadata` says what it means, each value as a string: `probability` and `isClosed` for a deal, `ticketState` for a ticket.

### `hubspot.owners(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListOwners)` | `Page<Owner>`: everyone records can be assigned to, or the one with an email address |
| `get(owner, GetOwner)` | `Owner` |

An owner's `id` is what a record's `hubspot_owner_id` holds. It is not the user's id, which is `userId`; `GetOwner.idProperty` set to `userId` looks an owner up by that instead. Needs `crm.objects.owners.read`.

## The scope each object type needs

HubSpot's scopes are per object type and per direction, so the scope an operation needs depends on the object type it is given. Each operation says how its scope is formed, and lists a scope of its own only where it is the same for every object type.

| Object type | To read | To create, change, archive | For `properties` |
| --- | --- | --- | --- |
| `contacts` | `crm.objects.contacts.read` | `crm.objects.contacts.write` | `crm.schemas.contacts.read` |
| `companies` | `crm.objects.companies.read` | `crm.objects.companies.write` | `crm.schemas.companies.read` |
| `deals` | `crm.objects.deals.read` | `crm.objects.deals.write` | `crm.schemas.deals.read` |
| `tickets` | `crm.objects.tickets.read` | `crm.objects.tickets.write` | `crm.objects.tickets.read` |
| `notes`, `calls`, `meetings`, `tasks` | `crm.objects.contacts.read` | `crm.objects.contacts.write` | as to read |
| `emails` | `crm.objects.contacts.read`, and `sales-email-read` for the content | `crm.objects.contacts.write` | as to read |
| custom objects (`2-…`) | `crm.objects.custom.read` | `crm.objects.custom.write` | `crm.schemas.custom.read` |

- **`properties` and `pipelines`** are answered for the type's schema scope or for its read scope: HubSpot lists both among the scopes that grant them. `pipelines` for `deals` needs `crm.objects.deals.read`, and for `tickets` `crm.objects.tickets.read`.
- **`associations`** need the scope of both object types: read to list, write to create and remove.
- **`owners`** need `crm.objects.owners.read`, and `identity` needs `oauth`.
- **Custom objects exist on Enterprise accounts only.** Ask for their scopes in `optional_scopes` unless every account you connect is one.
- **Sensitive properties** have scopes of their own (`crm.objects.contacts.sensitive.read` and the like), most of them on Enterprise accounts only.
- **Tickets** also answer to the older scope `tickets`, which HubSpot has replaced with the two above.

**The body of a logged email needs `sales-email-read`.** Reading emails needs the contacts scope like any other activity. HubSpot's guide to emails lists one scope more, `sales-email-read`, which it describes as access to one-to-one email activities; by HubSpot's announcement of that scope, the content of an email (`hs_email_text`, `hs_email_html`) comes back redacted for a connection without it.

When a scope is missing, HubSpot answers 403 and names the scopes that would do. Socket reports `AccessDenied` and repeats those names, so that one of them can be added to the app.

## Page through a list

A list returns a `Page` with `items` and `next_cursor`. Pass the cursor back for the next page; `None` means the last page.

```rust
let mut options = ListObjects { properties: Some(vec!["email".into()]), limit: Some(100), ..Default::default() };
loop {
    let page = hubspot.objects(&connection).list("contacts", options.clone()).await?;
    for contact in &page.items { /* … */ }
    match page.next_cursor {
        Some(cursor) => options.cursor = Some(cursor),
        None => break,
    }
}
```

The cursor is the value HubSpot gave as `paging.next.after`. Pass it back unchanged, with the same arguments it came from. It goes to HubSpot as the `after` parameter, or as `after` in the body of a search, and is used for nothing else, so whatever a cursor holds cannot change which address is read.

| List | Largest `limit` | When not given |
| --- | --- | --- |
| `objects.list` | 100 | 10 |
| `objects.search` | 200 | 10 |
| `associations.list` | 500 | 500 |
| `owners.list` | 500 | 100 |

A `limit` outside its range is refused before HubSpot is called. `properties.list` and `pipelines.list` are not paged: HubSpot returns them whole.

## The limits of search

`search` takes `filterGroups`, `sorts`, a free-text `query` and the `properties` to return. A record matches when it passes every filter of any one group. A search with nothing set matches every record of the type. Archived records are never found, and a record that was just created or changed can take a moment to appear.

A filter is `{ "propertyName": "amount", "operator": "GTE", "value": "1000" }`. The operators are `EQ`, `NEQ`, `LT`, `LTE`, `GT`, `GTE`, `BETWEEN` (with `value` and `highValue`), `IN` and `NOT_IN` (with `values`), `HAS_PROPERTY` and `NOT_HAS_PROPERTY` (with nothing), `CONTAINS_TOKEN` and `NOT_CONTAINS_TOKEN`. A sort is `{ "propertyName": "createdate", "direction": "DESCENDING" }`.

What HubSpot's search refuses, Socket refuses before calling it: more than 5 groups, more than 6 filters in a group, more than 18 filters in all, more than one sort, a `query` of more than 3,000 characters, a page of more than 200, and a filter without what its operator compares with.

Two limits belong to search alone, and each is reported as its own error:

- **At most 10,000 results for one search**, however it is paged. HubSpot answers a page past them with a bare 400. Socket reports `InvalidInput` with a message that says so and what to do: narrow the filters, or sort by a property such as `hs_object_id` and filter on the last value read, which starts a new search from there. A cursor at or past 10,000 is refused without calling HubSpot.
- **About five searches a second for the whole account**, apart from the account's other limits. Socket reports `RateLimited` with a message that names search, and the wait when HubSpot gives one. A search that meets the daily or the ten-second limit instead is told which.

## Rate limits

HubSpot answers 429 when a limit is met, and names the limit in the body as `policyName`. Both of an account's general limits arrive as `RateLimited`, each with its own message:

| `policyName` | The limit | What the message says |
| --- | --- | --- |
| `TEN_SECONDLY_ROLLING` | Requests in any ten seconds, for each app: 100 on Free and Starter, 190 on Professional and Enterprise, 110 for an app installed through OAuth | Wait a few seconds and try again |
| `DAILY` | Requests in a day, for the whole account, shared by its private apps: 250,000 to 1,000,000 by plan | It starts again at midnight in the account's time zone |

`retry()` carries the wait when HubSpot states one in `Retry-After`, and says "later" when it does not. HubSpot's documentation does not promise that header on a 429, so do not count on a number. A 429 that names neither limit is reported as a rate limit all the same.

Two more answers carry a wait:

- **423**, a lock HubSpot puts on what it was sent a large amount of in a short time, such as thousands of records to change. It lasts two seconds, and `retry()` says so.
- **477**, HubSpot's own status for an account that is being moved between data centres. `retry()` carries the wait HubSpot states, which can be up to a day.

## Call an operation by name

Every method is also an operation an agent, an MCP server or another language can call with JSON. The plain arguments and the options sit side by side in one object.

```rust
let output = socket
    .invoke(key, "hubspot.objects.search".into(), serde_json::json!({
        "object_type": "contacts",
        "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "EQ", "value": "ada@example.test" }] }],
        "properties": ["email", "firstname", "lifecyclestage"]
    }))
    .await?;
```

`socket.operations()` returns each operation's name, description, input schema, output schema, effect and scope. The effect lets a host ask a person before a change:

- **read** changes nothing.
- **write** adds something or sets a value: creating a record, setting its properties, associating two records.
- **destructive** deletes or removes: archiving a record, removing an association.

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `hubspot.identity.get` | read | oauth | Return the account this connection is authorised as, confirming the token still works. |
| `hubspot.resource.resolve` | read | the type's read scope | Confirm that an object type exists and the account can read it. |
| `hubspot.objects.list` | read | `crm.objects.{type}.read` | List the records of an object type, each with only the properties asked for. |
| `hubspot.objects.get` | read | `crm.objects.{type}.read` | Get one record, with the properties and the ids of associated records asked for. |
| `hubspot.objects.batch_read` | read | `crm.objects.{type}.read` | Read up to 100 records at once, by id or by a property with unique values. |
| `hubspot.objects.search` | read | `crm.objects.{type}.read` | Search the records of an object type by filters, by words, or both, with one sort. |
| `hubspot.objects.create` | write | `crm.objects.{type}.write` | Create a record, and associate it with the records named. |
| `hubspot.objects.update` | write | `crm.objects.{type}.write` | Change a record, setting the properties given and leaving the rest. |
| `hubspot.objects.batch_create` | write | `crm.objects.{type}.write` | Create up to 100 records at once. |
| `hubspot.objects.batch_update` | write | `crm.objects.{type}.write` | Change up to 100 records at once. |
| `hubspot.objects.archive` | destructive | `crm.objects.{type}.write` | Move a record to the recycling bin. |
| `hubspot.associations.list` | read | the read scope of both types | List the records of one type that a record is associated with. |
| `hubspot.associations.create` | write | the write scope of both types | Associate two records, by default or with the kinds given. |
| `hubspot.associations.remove` | destructive | the write scope of both types | Remove every association between two records. |
| `hubspot.properties.list` | read | `crm.schemas.{type}.read` | List every property of an object type: name, label, type and options. |
| `hubspot.properties.get` | read | `crm.schemas.{type}.read` | Get one property of an object type. |
| `hubspot.pipelines.list` | read | `crm.objects.{type}.read` | List the pipelines of deals or tickets, each with its stages. |
| `hubspot.pipelines.get` | read | `crm.objects.{type}.read` | Get one pipeline, with its stages. |
| `hubspot.owners.list` | read | crm.objects.owners.read | List the people and queues records can be assigned to. |
| `hubspot.owners.get` | read | crm.objects.owners.read | Get one owner. |

`{type}` stands for the object type's own scope name, as the table of scopes above gives it. Because that depends on an argument, an operation's `required_scopes` lists a scope only where it is fixed: `oauth` for identity and `crm.objects.owners.read` for owners. For the rest it is empty, and the description says how the scope is formed.

`search` and `batch_read` are reads that HubSpot offers only as POST. They are marked `read` because they change nothing, which is what the effect is for.

`update` is marked `write`, not `destructive`: it sets the properties that are named and deletes nothing. A host that asks a person only before a destructive operation therefore lets an update run, and should know that an update can clear a property by setting it to an empty string.

## Handle errors

| Kind | What it means for HubSpot | What to do |
| --- | --- | --- |
| `ReconnectRequired` | HubSpot answered 401, or the refresh token is no longer accepted, as when the app was uninstalled from the account | Socket renews an OAuth token once by itself; if that fails, connect again |
| `AccessDenied` | A scope is missing, or the user behind the connection may not see the record. The message names the scopes that would do, or carries HubSpot's own reason | Add the scope to the app and connect again |
| `NotFound` | No such record, property, pipeline, owner or object type | Check the id |
| `InvalidInput` | HubSpot refused the request, a search ran past 10,000 results, or a record that has to be unique already exists; the message carries the reason | Fix the input |
| `RateLimited` | A limit was met; the message says which, and `retry()` how long to wait when HubSpot says | Wait and try again |
| `Unexpected` | HubSpot failed, a record is locked, or the account is being moved | Try again later |
| `Decode` | HubSpot answered success without what was asked for, or with something that could not be read | Report it; this should not happen |

An input error never repeats the value you sent. It names the field when a required one is missing or a plain argument has the wrong type. For a wrong type inside the options, such as a `limit` written as text or one input of a batch, it says only that a field has the wrong type, and not which.

**A field the operation does not know is refused**, at any depth, and named: `filter_groups`, `filterGroups[0].filters[1].Value`. A search that silently dropped a misspelt filter would return every record. Each operation's input schema says the same, with `additionalProperties: false`. A record's `properties` is the exception: it takes any name, because the properties are whatever the account defines. An answer that cannot be read is reported the same way: the error says where the unreadable value was, and neither its message nor its cause carries anything from the CRM.

A request sent as GET is retried on a rate limit or a server error. Creating and changing, in batch or not, are sent again in only two cases, both of which mean HubSpot did not carry them out: HubSpot rate limited the request, or HubSpot rejected the access token and Socket renewed it to a different one. **If one of them fails with a server error, check before sending it again**: a second `create` makes a second record. `search` and `batch_read` are sent as POST, so they are not retried after a server error either, although repeating them would do no harm.

**Archiving, removing an association, and associating with named kinds are the exceptions today.** HubSpot takes them as DELETE and PUT, which the transport still repeats after a server error. Repeating any of them asks for what was already asked: a record already in the bin, an association already there or already gone.

## Confirmed against HubSpot's documentation, and not

Everything here was read in October 2026 from HubSpot's published OpenAPI descriptions and from developers.hubspot.com. Nothing was run against a live account.

**Which version of the API.** The issue that asked for this provider named CRM v3, associations v4 and the token endpoint `/oauth/v1/token`. HubSpot's documentation has since moved on, and Socket follows it:

- HubSpot now names a version by its month (`/crm/objects/2026-09/…`) and says "for new integrations, always use the latest date version". `2026-09` is the latest; each version is supported for eighteen months.
- HubSpot retires `/oauth/v1/token` on 16 February 2027, and ends support for its v4 APIs, associations among them, on 30 March 2027. v3 is still supported, with no end date given, under "legacy".
- So every address Socket calls carries `2026-09`: records and associations under `/crm/objects/2026-09`, default associations under `/crm/associations/2026-09`, and `/crm/properties/2026-09`, `/crm/pipelines/2026-09`, `/crm/owners/2026-09`, `/oauth/2026-09/token` and `/account-info/2026-09/details`.
- The OpenAPI descriptions of v3, v4, `2026-03` and `2026-09` were compared for every endpoint used. Parameters, bodies and answers are the same, with two additions: a pipeline stage's `writePermissions` from `2026-03`, and the account's `portalName` and `createdAt` in `2026-09`. That name is why identity needs `2026-09`.

Confirmed:

- The authorise address, its `scope` and `optional_scope` parameters, both separated by spaces, and that an account lacking a required scope gets an error where an optional one is dropped.
- The token endpoint takes the client id and secret in the form body, answers with `access_token`, `refresh_token`, `expires_in` (1800) and `scopes` as a list, and an access token lasts 30 minutes.
- `oauth` is required of every app, and is the scope of the account details endpoint.
- `GET /account-info/2026-09/details` and its fields: `portalId` required, `portalName` optional.
- The token lookup that puts the token in the address is deprecated, and its replacement takes the client's id and secret.
- Every objects endpoint, with its verb, parameters, body and status: `GET /{type}` with `limit`, `after`, `properties`, `associations`, `archived`; `GET /{type}/{id}` with those and `idProperty`; `POST /{type}` (201); `PATCH /{type}/{id}` with `idProperty`; `DELETE /{type}/{id}` (204); `POST /{type}/batch/read`, `/batch/create` (201) and `/batch/update`, each also answering 207 with `errors`; `POST /{type}/search`.
- A record's shape: `id`, `properties`, `createdAt`, `updatedAt`, `archived`, `archivedAt`, `associations`; and `paging.next.after`.
- Batches take 100 inputs.
- Search: five groups, six filters in a group, eighteen in all, one sort, a page of 200 at most and 10 by default, 3,000 characters, 10,000 results with a 400 past them, five requests a second for an account, the operators, and that search is outside the ten-second limit of an OAuth app and sends no rate-limit headers.
- The rate limits by plan, the 429 body with `policyName` and its `DAILY` value, the name `TEN_SECONDLY_ROLLING`, and that the daily limit starts again at midnight in the account's time zone.
- 423 with its two seconds, and 477 with `Retry-After` in seconds.
- Associations: `GET /{type}/{id}/associations/{toType}` with `limit` (500 by default) and `after`; `PUT …/{toType}/{toId}` with a list of `associationCategory` and `associationTypeId` (201, answering with `labels`); `DELETE …/{toType}/{toId}` (204); `POST /crm/associations/2026-09/{type}/{toType}/batch/associate/default` with `inputs` of `from` and `to`.
- Properties, pipelines and owners: their addresses, parameters and fields, that properties and pipelines are not paged, and that owners return 100 by default.
- The scopes of contacts, companies, deals, tickets and custom objects, that custom objects are Enterprise only, and `crm.objects.owners.read`.
- Notes and emails list `crm.objects.contacts.read` and `crm.objects.contacts.write` as their scopes, and emails also `sales-email-read`, which HubSpot describes as access to one-to-one email activities.
- The object type ids (`0-1` contacts, `0-2` companies, `0-3` deals, `0-5` tickets, `0-46` notes, `0-48` calls, `0-47` meetings, `0-49` emails, `0-27` tasks, `2-…` custom).

Not confirmed:

- **Whether the refresh token changes.** The issue says it does not. HubSpot's documentation does not say either way, and its description of the token answer always includes a `refresh_token`. Socket keeps the stored one when none comes back and saves the one that does, which is right in both cases.
- **PKCE.** The token endpoint's description lists `code_verifier`, but no page documents a code challenge for the authorise address, so Socket sends none.
- **What the token endpoint answers when it refuses.** No page shows it. Socket treats a 400 as a declined code or refresh token.
- **The largest page of a list of records.** The guide speaks of a number "under 100" and the description gives no maximum. Socket takes 1 to 100.
- **The largest page of associations and of owners.** Only the defaults are given, 500 and 100. Socket takes up to 500 for both.
- **The body of a 429 for the ten-second limit, and for search.** Only the daily one is shown. Socket takes `policyName` to be `TEN_SECONDLY_ROLLING` for the ten-second limit, as the text names it, and treats a 429 on a search that names neither general limit as search's own.
- **`Retry-After` on a 429.** Not documented. Socket reads it when it is there.
- **The message of the 400 past 10,000 results, and that a search's cursor is a count of results.** HubSpot's examples show such cursors and no page says so. Socket's own check applies only to a cursor that is a number. A 400 on a page that reaches past 10,000 is passed on with HubSpot's words and the cap named beside them; any other 400 is passed on as it is.
- **Fields the descriptions mark as required and HubSpot's own examples leave out:** `associations` when creating, `properties` and `propertiesWithHistory` in a batch read, and `after`, `limit`, `sorts`, `properties` and `filterGroups` in a search. Socket follows the examples and sends only what is set.
- **How a sort is written.** The description says a list of strings; the guide shows `{ propertyName, direction }`, which is what Socket sends. `ASCENDING` is not written on the page that shows `DESCENDING`.
- **Whether an id is a string or a number.** The descriptions say a string; HubSpot's examples of associations write `toObjectId` as a number. Socket reads either and returns a string.
- **The scopes of calls, meetings and tasks.** Only the pages for notes and emails were read; the other three are taken to be the same. The OpenAPI description also lists `crm.objects.notes.read`, `crm.objects.calls.read` and the like among the scopes that grant a read, which HubSpot's list of scopes does not have.
- **Which scope reads the properties of an activity type.** The description lists `crm.schemas.notes.read` and the like beside the contacts scopes. The table above gives the scope that reads the records.
- **`crm.schemas.tickets.read`.** The tickets guide lists it and the list of scopes does not, so the table above gives the ticket read scope for a ticket's properties.
- **What a connection without `sales-email-read` gets for an email's content.** A 2019 announcement says it is redacted; the current guide lists the scope and does not say.
- **Where the scopes are in a 403.** The guide's example carries `requiredGranularScopes`. Socket looks for that and for `requiredScopes`, in the error's details and in its context, and repeats only what looks like a scope.
- **That one pair through the batch address is the same as the address of the pair.** HubSpot offers `PUT …/associations/default/{toType}/{toId}`, which takes no body, for a default association. Socket's transport sends no length with a request that has no body, which a server may refuse, so Socket asks through the batch address with one pair. Both answer in the same shape.
- **Accounts in HubSpot's EU data centre.** HubSpot offers `api-eu1.hubapi.com` beside `api.hubapi.com`. That every account is served from `api.hubapi.com` was not confirmed.
- **What archiving a record twice, or removing an association that is not there, answers.**

## Not supported yet

- **Marketing, CMS, the conversations inbox, workflows and webhooks.** This integration is the CRM.
- **Creating and changing properties, pipelines, stages and custom object definitions.** They are read only here.
- **Merging records, upserts, batch archive, and permanently deleting a contact (`gdpr-delete`).**
- **A property's history** (`propertiesWithHistory`).
- **Batch reads and removals of associations, and association labels' own definitions.**
- **Sensitive and highly sensitive properties** in `properties.list` (`dataSensitivity`).
- **The total of a search.** HubSpot returns it; a `Page` has no place for it.
- **Files and attachments** of notes and emails, and **sending** an email: an email record is a log of one.
- **Imports, exports and lists.**
- **Client credentials and JWT bearer grants**, which HubSpot's token endpoint also takes.

Anything HubSpot offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("crm/objects/2026-09/products").with_query("limit", "5")).await?;
```
