# HubSpot

**Status:** built and tested against a local server that answers as HubSpot's documentation says. Not yet run against a real HubSpot account.

Socket's HubSpot integration gives a program the CRM: the records of any object type, the associations between them, an object type's properties, the pipelines of deals and tickets, and the owners records are assigned to. That is 18 typed methods, and the same 18 as operations callable by name with JSON, plus identity and lookup of a record by its link. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against HubSpot's documentation.

HubSpot's CRM is one API over every object type, so the methods are generic too. The object type is a plain argument: `contacts`, `companies`, `deals`, `tickets`, the engagements `notes`, `calls`, `meetings`, `emails` and `tasks`, or a custom object by its type id, such as `2-3465404`.

Every call is made at HubSpot's API version `2026-09`. HubSpot names a version by the month it was released, and supports it for eighteen months.

## Connect

Add the crate with the HubSpot feature:

```sh
cargo add socketkit --features hubspot
```

### With a token you already hold

A private app's access token, or the static token of an application installed in one account, is sent as a bearer token on every call.

```rust
use std::sync::Arc;
use socketkit::hubspot::HubSpot;
use socketkit::{ConnectionKey, ProviderId, Socket};

let hubspot = HubSpot::with_token("pat-na1-…");
let socket = Socket::in_memory().integration(Arc::new(hubspot.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("hubspot")?, "me")).await?;
```

Such a token comes with no refresh token, so Socket never renews it. It has the scopes it was given when the app was made, and it is one account's: every tenant of the `Socket` shares it.

### With your own HubSpot app, to connect your users' accounts

Create an app in HubSpot with OAuth as its authentication, and add your callback route as a redirect URL.

```rust
use socketkit::hubspot::{HubSpot, HubSpotOAuth};
use socketkit::{OAuthClient, SecretString};

let hubspot = HubSpot::with_oauth(HubSpotOAuth {
    client: OAuthClient {
        client_id: config.hubspot_client_id,
        client_secret: SecretString::new(config.hubspot_client_secret),
        redirect_uri: "https://yourapp.example/oauth/hubspot/callback".parse()?,
    },
    // What every account has to grant. `None` asks for `oauth` alone.
    scopes: Some(vec!["oauth".into(), "crm.objects.contacts.read".into(), "crm.objects.deals.read".into()]),
    // What to ask for where the account's plan has it.
    optional_scopes: vec!["crm.objects.custom.read".into(), "sales-email-read".into()],
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(hubspot.clone())).build()?;

// When a user clicks "Connect HubSpot":
let key = ConnectionKey::new(ProviderId::new("hubspot")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
let tokens = socket.complete_authorization(pending, code, state).await?;
// `tokens.scopes` says which of the optional scopes this account granted.
let connection = socket.connection(key).await?;
```

`HubSpot::with_oauth(client)` with a plain `OAuthClient` also works when the defaults are enough.

| Setting | What it does |
| --- | --- |
| `scopes` | The scopes every account has to grant, in place of the default, which is `oauth` alone. They have to match the scopes marked required in the app's own settings. |
| `optional_scopes` | Scopes sent as HubSpot's `optional_scope`. One the account's plan lacks is left out and the authorisation still goes through. |

Why two lists: **HubSpot refuses the whole authorisation when the account's plan lacks a scope that is asked for outright.** A scope only some plans have, such as `crm.objects.custom.read` (Enterprise only), belongs in `optional_scopes`. A person approving the app can also untick an optional scope. Either way the scopes that were granted come back with the tokens and are stored on them.

Four things Socket does for every HubSpot connection:

- **`oauth` is always asked for.** It is the scope every HubSpot application has, it is all that identity needs, and HubSpot shows an error for a link that leaves out a scope the app requires. It is added to whatever scopes are given, in the settings or at `begin_authorization`.
- **A scope asked for outright is not also asked for as optional.** Scopes passed to `begin_authorization` replace `scopes` for that one connection; `optional_scopes` stay, less any that are now required.
- **The granted scopes are read from HubSpot's list.** HubSpot writes them as an array under `scopes`, where the standard has one string under `scope`. Socket reads both.
- **The refresh token is kept through every refresh.** An access token lasts thirty minutes. Socket renews it before it expires, and once more if HubSpot rejects it early. HubSpot's refresh token stays the same; whether a refresh repeats it or leaves it out, the stored one goes on being used.

The client id and secret are sent in the body of the token request, never in an address. PKCE is off: HubSpot's sign-in page documents no code challenge.

### Which scopes to ask for

HubSpot grants access object by object, and by direction.

| Object type | To read | To create, change or archive |
| --- | --- | --- |
| `contacts` | `crm.objects.contacts.read` | `crm.objects.contacts.write` |
| `companies` | `crm.objects.companies.read` | `crm.objects.companies.write` |
| `deals` | `crm.objects.deals.read` | `crm.objects.deals.write` |
| `tickets` | `crm.objects.tickets.read` | `crm.objects.tickets.write` |
| `notes`, `calls`, `meetings`, `tasks` | `crm.objects.contacts.read` | `crm.objects.contacts.write` |
| `emails` | `crm.objects.contacts.read` and `sales-email-read` | `crm.objects.contacts.write` and `sales-email-read` |
| a custom object | `crm.objects.custom.read` (Enterprise) | `crm.objects.custom.write` (Enterprise) |
| properties | `crm.schemas.contacts.read`, `crm.schemas.companies.read`, `crm.schemas.deals.read`; for tickets `crm.objects.tickets.read` | |
| pipelines | `crm.objects.deals.read`, `crm.objects.tickets.read` | |
| owners | `crm.objects.owners.read` | |

**Reading the body of a logged email needs `sales-email-read`**, beside the contacts scope. HubSpot's reference lists it for the emails API and describes it as what "grants access to read and manage one-to-one email engagements"; its older engagements reference says that without it the details of an email are not returned. Ask for it as an optional scope unless every account you connect is to grant it.

Because the operations take the object type as an argument, one operation cannot name one scope. Each generic operation's `required_scopes` therefore lists the scopes for the four kinds of record every account has: contacts, companies, deals and tickets. That set also covers notes, calls, meetings and tasks, and every plan has it, so asking for an operation's `required_scopes` never makes HubSpot refuse an account. It does not cover `sales-email-read` or the custom-object scopes: add those as optional scopes.

An application that works with one object type can ask for less. `object_scopes` says exactly what a call on one object type needs:

```rust
use socketkit::hubspot::object_scopes;
use socketkit::Effect;

object_scopes("deals", Effect::Read);      // ["crm.objects.deals.read"]
object_scopes("notes", Effect::Write);     // ["crm.objects.contacts.write"]
object_scopes("emails", Effect::Read);     // ["crm.objects.contacts.read", "sales-email-read"]
object_scopes("2-3465404", Effect::Read);  // ["crm.objects.custom.read"]
```

When a call needs a scope the connection was not granted, HubSpot answers 403 and Socket reports `AccessDenied` with the scopes HubSpot names.

## Identity and lookup

```rust
let account = hubspot.identity(&connection).await?;    // Account { id, name, email: None }
let record = hubspot.resolve(&connection, link).await?; // Resource { id, label, description }
```

`identity` reads `GET /account-info/2026-09/details` and needs `oauth`. The `id` is the account's id, which HubSpot calls the portal id or Hub ID; the `name` is the account's name when HubSpot gives one, and `HubSpot account {id}` when it does not. HubSpot names no person here, so `email` is `None`.

**The token is sent in the `Authorization` header and nowhere else.** HubSpot has two other ways to ask whose a token is, and neither is used: `GET /oauth/v1/access-tokens/{token}` writes the token into the address, where proxies and logs keep it, and `POST /oauth/2026-09/token/introspect` needs the application's client secret, which a private app's token does not come with. The account-details call works the same for both kinds of token.

`resolve` accepts a record's link from HubSpot (`https://app.hubspot.com/contacts/{account}/record/0-1/12345`, on `app.hubspot.com` or a regional host such as `app-eu1.hubspot.com`), or its object type and id written as `contacts/12345`. It confirms the record exists and the account can read it. The resource's `id` is `{object type}/{record id}`, which are the two arguments `objects.get` takes; the label is the record's name (a contact's names or email, a company's name, a deal's name, a ticket's subject, an engagement's title), read from those properties alone. It needs the read scope of the record's object type.

**A link is only resolved in the account it is from.** A link names its account, and record ids are unique only within one account: contact 12345 of another account is a different person from contact 12345 of this one. So for a link, `resolve` first reads the connection's own account (one more call, which needs `oauth`) and refuses a link to another account with `NotFound`, without reading any record. An object type and an id name no account and are looked up in the connection's own.

## Use the typed methods

Methods are grouped by area. Each group is reached through the `HubSpot` value and a connection: `hubspot.objects(&connection)`.

**How arguments are split.** What identifies the thing acted on is a plain argument: an object type, a record id, a property's name. Content and optional filters are structs from `socketkit::hubspot::models`, where every field you leave unset is not sent, so HubSpot applies its own default.

**Names are HubSpot's own.** In Rust the fields are written the Rust way (`filter_groups`); in JSON they are written as HubSpot writes them (`filterGroups`, `propertiesWithHistory`, `associationTypeId`), so HubSpot's documentation of a field holds here too.

**Every property value is text.** HubSpot writes a number, a date and a flag as strings (`"4800"`, `"2026-11-30T00:00:00.000Z"`, `"true"`), and takes them the same way. A property without a value is `null`. Setting a property to an empty string clears it.

```rust
use std::collections::BTreeMap;
use socketkit::hubspot::models::{
    AssociationCategory, AssociationSpec, CreateObject, ListObjects, NewAssociation, ObjectId, UpdateObject,
};

let objects = hubspot.objects(&connection);

// Which properties does a deal have in this account?
let fields = hubspot.properties(&connection).list("deals", Default::default()).await?;

let deals = objects.list("deals", ListObjects {
    properties: Some(vec!["dealname".into(), "amount".into(), "dealstage".into()]),
    limit: Some(50),
    ..Default::default()
}).await?;

// Log a note on a contact's timeline.
let note = objects.create("notes", CreateObject {
    properties: BTreeMap::from([
        ("hs_timestamp".into(), "2026-10-10T10:00:00Z".into()),
        ("hs_note_body".into(), "Agreed to renew in November.".into()),
    ]),
    associations: Some(vec![NewAssociation {
        to: ObjectId { id: "12345".into() },
        types: vec![AssociationSpec { association_category: AssociationCategory::HubspotDefined, association_type_id: 202 }],
    }]),
}).await?;

// Move a deal to another stage.
objects.update("deals", "777", UpdateObject {
    properties: BTreeMap::from([("dealstage".into(), "closedwon".into())]),
    id_property: None,
}).await?;
```

### `hubspot.objects(&connection)`

| Method | Returns |
| --- | --- |
| `list(object_type, ListObjects)` | `Page<Object>`: the records of an object type, at most 100 a page, each with the properties asked for |
| `get(object_type, id, GetObject)` | `Object`: one record, by its id or by a unique property, with the properties, the history and the associated record ids asked for |
| `batch_read(object_type, BatchRead)` | `BatchResult`: up to 100 records by their ids or by a unique property. A read, sent as POST |
| `search(object_type, Search)` | `SearchResults`: the records that match words, conditions on properties, or both, with one sort. A read, sent as POST |
| `create(object_type, CreateObject)` | `Object`: the new record |
| `update(object_type, id, UpdateObject)` | `Object`: the record, with the properties given changed and the rest left |
| `batch_create(object_type, BatchCreate)` | `BatchResult`: up to 100 new records |
| `batch_update(object_type, BatchUpdate)` | `BatchResult`: up to 100 changed records |
| `archive(object_type, id)` | nothing: the record goes to HubSpot's recycling bin |

- **A record returns only the properties it is asked for**, beside a few HubSpot always sends. This is what keeps a list small. Name the properties you need in `properties`; `properties.list` says which there are.
- **`get` can find a record by a unique property.** With `idProperty` set to `email`, the `id` is an email address. The same works for `update`, `batch_read` and each input of `batch_update`.
- **`batch_read` succeeds when only some records exist.** The ones found are in `results`; `errors` names the ids that were not, and `numErrors` counts them. It cannot return associations; use `associations.list`.
- **`batch_create` is all or nothing.** HubSpot creates every record or none.
- **Archiving is not deleting.** The record goes to HubSpot's recycling bin, where a person can restore it; HubSpot says a contact can be restored for 90 days. Nothing here restores one.
- **An engagement shows on a record's timeline only when it is associated with it.** Give `associations` when creating a note, a call, a meeting, an email or a task, or associate it afterwards. A note needs `hs_timestamp`; HubSpot's guide for each engagement lists its properties.

### `hubspot.associations(&connection)`

| Method | Returns |
| --- | --- |
| `list(from_object_type, from_id, to_object_type, Paging)` | `Page<Association>`: the records of one object type a record is associated with, at most 500 a page, each with the labels of the association |
| `create(from_object_type, from_id, to_object_type, to_id, CreateAssociation)` | `AssociationCreated` |
| `remove(from_object_type, from_id, to_object_type, to_id)` | nothing: every association between the two records is removed |

`create` without `types` makes the plain association HubSpot has between the two object types. With `types` it sets labels, each by its category and type id, and **the labels given replace those the association has**; to add one, give the old ones too. A type id depends on the direction: contact to company is `279`, company to contact `280`. HubSpot's associations guide lists its own type ids; an account's own labels are numbered by the account.

`remove` takes away every association between the two records, labelled or not. The records stay.

### `hubspot.properties(&connection)`

| Method | Returns |
| --- | --- |
| `list(object_type, ListProperties)` | `Vec<PropertySummary>`: every property of the object type |
| `get(object_type, name)` | `Property`: one property, with its description and the values it can take |

Every account adds its own properties, and a record returns only what it is asked for, so this is where a caller finds out what can be asked. HubSpot returns all of an object type's properties in one answer, several hundred for a contact, each with its description and every option. So a row of `list` is kept to what is needed to choose a property: `name`, `label`, `type`, `fieldType`, `groupName`, and three flags (`calculated`, `hasUniqueValue`, `hidden`) when HubSpot sends them. `get` returns the rest: `options` (a record is set to an option's `value`, not its `label`) and `modificationMetadata`, whose `readOnlyValue` says a property cannot be set.

### `hubspot.pipelines(&connection)`

| Method | Returns |
| --- | --- |
| `list(object_type)` | `Vec<Pipeline>`: the pipelines of `deals` or of `tickets`, each with its stages |
| `get(object_type, pipeline)` | `Pipeline` |

A stage's `id` is what a deal's `dealstage`, or a ticket's `hs_pipeline_stage`, is set to; a pipeline's `id` is what `pipeline`, or `hs_pipeline`, is set to. A stage's `metadata` says what it means, each value as text: `probability` and `isClosed` for a deal, `ticketState` for a ticket.

### `hubspot.owners(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListOwners)` | `Page<Owner>`: the people and queues records can be assigned to, or the one with an email address |
| `get(owner, GetOwner)` | `Owner`, by the owner's id, or by the user's id with `idProperty` set to `userId` |

An owner has two ids. `id` is what a record's `hubspot_owner_id` is set to. `userId` is the HubSpot user behind the owner, and setting a record's owner to it is an error.

## Page through a list

A list returns a `Page` with `items` and `next_cursor`. Pass the cursor back for the next page; `None` means the last page.

```rust
let mut options = ListObjects { limit: Some(100), ..Default::default() };
loop {
    let page = hubspot.objects(&connection).list("contacts", options.clone()).await?;
    for contact in &page.items { /* … */ }
    match page.next_cursor {
        Some(cursor) => options.cursor = Some(cursor),
        None => break,
    }
}
```

The cursor is the token HubSpot gives as `paging.next.after`. Given back, it is sent as the `after` parameter of the same request, with the same arguments. HubSpot also sends a link to the next page beside the token; Socket never follows it, so a cursor can say where in a list to go on and nothing else: whatever it holds, it is one value of one parameter.

`limit` is sent when you give one, and has to be given with each cursor: a cursor says where to go on and does not carry the page size, so a page asked for without a `limit` is HubSpot's own size, 10 records or 100 owners. It is from 1 to 100 for records, 1 to 200 for a search and 1 to 500 for associations, and anything else is refused before HubSpot is called. `properties.list` and `pipelines.list` are not paged: HubSpot returns them whole.

## Search

```rust
use socketkit::hubspot::models::{Filter, FilterGroup, FilterOperator, Search, Sort, SortDirection};

let open = hubspot.objects(&connection).search("deals", Search {
    query: Some("renewal".into()),
    filter_groups: Some(vec![FilterGroup { filters: vec![Filter {
        property_name: "amount".into(),
        operator: FilterOperator::Gt,
        value: Some("1000".into()),
        high_value: None,
        values: None,
    }] }]),
    sorts: Some(vec![Sort { property_name: "closedate".into(), direction: Some(SortDirection::Descending) }]),
    properties: Some(vec!["dealname".into(), "amount".into()]),
    ..Default::default()
}).await?;
println!("{} of {} deals", open.items.len(), open.total);
```

- `query` looks for words in the object type's default text properties: a contact's names, email and phone, a deal's name, a note's body.
- A record matches when it meets every filter of any one group. The operators are `EQ`, `NEQ`, `LT`, `LTE`, `GT`, `GTE`, `BETWEEN` (with `value` and `highValue`), `IN` and `NOT_IN` (with `values`), `HAS_PROPERTY`, `NOT_HAS_PROPERTY`, `CONTAINS_TOKEN` and `NOT_CONTAINS_TOKEN` (where `*` stands for any characters).
- The pseudo-property `associations.contact`, and the like for other object types, matches the records associated with the record whose id is the value. HubSpot does not search through a custom object's associations.
- A search with nothing set returns every record, oldest first.

Four limits, each said out loud:

- **A search returns the first 10,000 matches and no more.** `total` says how many match in all, so a `total` above 10,000 means the filters are too wide to read every match. Paging until `next_cursor` is `None` ends cleanly: the page that reaches the 10,000th match is asked for only as far as it (HubSpot refuses a page that would end past it), and comes back with no cursor, as a last page does. A cursor past the 10,000th, given directly, is refused with `InvalidInput` and a message that says so. To read more, narrow the filters, or sort by a property and filter on the last value read.
- **HubSpot allows five searches a second for the whole account**, apart from its other limits, and sends none of its rate-limit headers on a search. A search that HubSpot limits by the second (`SECONDLY`) is `RateLimited` with a message that names this limit and says that listing and reading records are not held to it. A search that reaches the ten-second or the daily limit is reported as that limit, which holds for every other call too. When HubSpot names no limit, the message says so, and claims nothing about other calls.
- **A search sees a change a few moments late.** A record just created or changed may not be found yet. Archived records are never found.
- **One sort**, a `query` of at most 3,000 characters and a page of at most 200 are checked before HubSpot is called. HubSpot takes at most 18 filters in all; how many groups it takes its own pages disagree on (five or six), so the count is left to HubSpot, which answers with `InvalidInput`.

## Call an operation by name

Every method is also an operation an agent, an MCP server or another language can call with JSON. The plain arguments and the options sit side by side in one object.

```rust
let output = socket
    .invoke(key, "hubspot.objects.search".into(), serde_json::json!({
        "object_type": "contacts",
        "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "CONTAINS_TOKEN", "value": "*@example.com" }] }],
        "properties": ["email", "firstname", "lastname"],
        "limit": 20
    }))
    .await?;
```

`socket.operations()` returns each operation's name, description, input and output JSON Schema, effect and required scopes. `contacts, companies, deals, tickets` below stands for the four scopes of that kind, `crm.objects.contacts.read` and so on; see [Which scopes to ask for](#which-scopes-to-ask-for).

| Operation | Effect | Required scopes | What it does |
| --- | --- | --- | --- |
| `hubspot.identity.get` | read | `oauth` | Return the account this connection is authorised for. |
| `hubspot.resource.resolve` | read | `oauth`, and read: contacts, companies, deals, tickets | Confirm that a record exists and the account can read it. |
| `hubspot.objects.list` | read | read: contacts, companies, deals, tickets | List the records of an object type, each with the properties asked for. |
| `hubspot.objects.get` | read | read: contacts, companies, deals, tickets | Get one record by its id or by a unique property. |
| `hubspot.objects.batch_read` | read | read: contacts, companies, deals, tickets | Read up to 100 records by their ids or by a unique property. Changes nothing. |
| `hubspot.objects.search` | read | read: contacts, companies, deals, tickets | Search the records of an object type. Changes nothing. |
| `hubspot.objects.create` | write | write: contacts, companies, deals, tickets | Create a record, associated with the records it belongs to. |
| `hubspot.objects.update` | write | write: contacts, companies, deals, tickets | Change the properties given on a record and leave the rest. |
| `hubspot.objects.batch_create` | write | write: contacts, companies, deals, tickets | Create up to 100 records of one object type. |
| `hubspot.objects.batch_update` | write | write: contacts, companies, deals, tickets | Change up to 100 records of one object type. |
| `hubspot.objects.archive` | destructive | write: contacts, companies, deals, tickets | Move a record to HubSpot's recycling bin. |
| `hubspot.associations.list` | read | read: contacts, companies, deals, tickets | List the records of one object type that a record is associated with. |
| `hubspot.associations.create` | write | write: contacts, companies, deals, tickets | Associate two records, plainly or with labels. |
| `hubspot.associations.remove` | destructive | write: contacts, companies, deals, tickets | Remove every association between two records. |
| `hubspot.properties.list` | read | `crm.schemas.contacts.read`, `crm.schemas.companies.read`, `crm.schemas.deals.read`, `crm.objects.tickets.read` | List every property of an object type in this account. |
| `hubspot.properties.get` | read | the same four | Get one property, with its description and the values it can take. |
| `hubspot.pipelines.list` | read | `crm.objects.deals.read`, `crm.objects.tickets.read` | List the pipelines of deals or of tickets, each with its stages. |
| `hubspot.pipelines.get` | read | `crm.objects.deals.read`, `crm.objects.tickets.read` | Get one pipeline, with its stages. |
| `hubspot.owners.list` | read | `crm.objects.owners.read` | List the people and queues records can be assigned to. |
| `hubspot.owners.get` | read | `crm.objects.owners.read` | Get one owner. |

`objects.search` and `objects.batch_read` are reads that HubSpot takes only as POST. They are marked `read` because they change nothing, which is what the effect is for.

`create`, `update` and the two batch writes are `write`: they add a record or set fields that can be set back. A host that lets writes run without asking should know that an update overwrites the values that were there, and that HubSpot keeps each property's earlier values in its history.

`archive` and `associations.remove` are `destructive`: a record leaves every list and only a person can bring it back, and every link between two records goes, its labels with it.

## Handle errors

| Kind | What it means for HubSpot | What to do |
| --- | --- | --- |
| `ReconnectRequired` | HubSpot answered 401, or no longer accepts the refresh token | Socket renews the token once by itself; if that fails, connect again |
| `AccessDenied` | A scope is missing, and the message names the scopes HubSpot does; or HubSpot refused for a reason the message carries | For an OAuth connection, connect again asking for the scope. For a private app's token, add the scope to the app in HubSpot |
| `NotFound` | No such record, property, pipeline or owner; or a link to a record in another account | Check the object type and the id |
| `InvalidInput` | HubSpot refused the request (400, 409 or 422) and the message carries its reason; or a search was asked to go past its 10,000th match; or Socket refused the input before sending it | Fix the input |
| `RateLimited` | One of HubSpot's limits was reached; the message says which | See below |
| `Unexpected` | HubSpot failed; or has locked the records for two seconds while a large change is applied (423), and `retry()` says two seconds; or is moving the account between data centres (477), and `retry()` carries the wait HubSpot states | Try again later. After a 423 on a write, check before sending it again |
| `Decode` | HubSpot answered success without what was asked for, or with something that could not be read | Report it; this should not happen |

**HubSpot's limits, and how each is reported.** All are `RateLimited`.

| Limit | What HubSpot documents | `retry()` |
| --- | --- | --- |
| Ten seconds (`TEN_SECONDLY_ROLLING`) | 100 to 250 requests in any ten seconds for a private app, by plan; 110 for each account that installed a marketplace app | The wait HubSpot states, or `Later`. Socket tries the call again itself, a write included: a request that was limited was not carried out |
| Daily (`DAILY`) | 250,000 to 1,000,000 requests a day for the account, by plan, shared by all its private apps. It starts again at midnight in the account's time zone | The wait HubSpot states, or `Never`: until midnight every call is refused, and each refusal counts against the account, so Socket does not try again |
| Search (`SECONDLY` on a search) | Five searches a second for the account | As for ten seconds; the message names the search limit |
| Not named | A 429 that names no limit, or one Socket does not know | The wait HubSpot states, or `Later`. On a search the message adds that searches are held to five a second |

HubSpot's documentation does not say that a 429 carries a `Retry-After` header. When one is there, in seconds or as a date, Socket passes it on for every limit.

An input error never repeats the value you sent. It names the field when a required one is missing or a value has the wrong type, at any depth: `` `properties.amount` has the wrong type `` is an amount sent as a number, where HubSpot takes text.

**A field the operation does not know is refused**, at any depth, and named: `filter_groups`, `sorts[0].order`. A dropped field takes what it said with it: a filter, a sort, the properties to return. Each operation's input schema says the same, with `additionalProperties: false`. The one place this does not apply is the content of `properties`: a record's fields are the account's own, so whatever is given there is passed to HubSpot, which knows them.

**An object type can only ever be one segment of the address.** It is letters, digits, `_` and `-`, which covers every name, type id and custom object name HubSpot has, and anything else is refused before a request is made. An id is percent-encoded as one segment, so it cannot start a query or a fragment; one that holds a slash, or is only dots, is refused. Space around an id is not part of it and is not sent. A unique value that holds a slash can still be read with `objects.batch_read`, where it travels in the body.

An answer that cannot be read is reported with the place of the unreadable value and nothing from the record.

A request sent as GET is tried again on a throttle or a server error. Creating and changing records (a POST or a PATCH) are sent again in only two cases, both of which mean HubSpot did not carry them out: HubSpot limited the request with a 429, or rejected the access token and Socket renewed it to a different one. **If a create fails with a server error, check before sending it again**: a record created twice is two records. The two reads sent as POST are not tried again after a server error either.

**Locked records (423) are not one of those cases.** HubSpot says to leave two seconds between requests, and does not say that nothing of the locked request was done. So a 423 is `Unexpected` with a wait of two seconds, a create or an update that meets it is sent once, and the caller decides whether to send it again.

**A PUT or a DELETE is sent again by the transport after a server error or a 423**, as a read is: `associations.create`, `associations.remove` and `objects.archive`. Associating two records twice, or removing what is already removed, ends as doing it once would. One case shows: if the first try did archive the record before failing, the second is answered "not found", and the call reports `NotFound` for an archive that worked. Treat `NotFound` from `archive` as "it is gone".

## Confirmed against HubSpot's documentation, and not

Everything here was read from developers.hubspot.com in October 2026: the guides, and the reference page of each endpoint, which carries its OpenAPI description. Nothing was run against a live account.

Where the documentation differs from the issue that asked for this provider, the documentation was followed:

- **Addresses carry a date, not `v3`.** Since March 2026 HubSpot versions its API by date: `/crm/objects/2026-09/contacts` where it was `/crm/v3/objects/contacts`, and `/crm/objects/2026-09/…/associations/…` where it was `/crm/v4/objects/…`. The numbered versions still answer, and HubSpot says they stop being supported in September 2027. Socket calls `2026-09`, the current version.
- **The token endpoint is `https://api.hubapi.com/oauth/2026-09/token`**, not `/oauth/v1/token`. The request is the same form. The answer differs: the granted scopes come back as a list under `scopes`, with `hub_id` beside them.
- **Tickets have their own scopes**, `crm.objects.tickets.read` and `crm.objects.tickets.write`. The older `tickets` scope is marked legacy.
- **Only `api.hubapi.com` may receive a credential.** The issue lists `app.hubspot.com` as a host too. It serves the page a person approves on, which their browser opens and which is never sent a token or the client secret, so it is not among the allowed hosts.
- **A search page is at most 200 records**, and a search past the 10,000th match is answered with a 400.

Confirmed:

- The authorise address, its parameters `client_id`, `redirect_uri`, `scope`, `optional_scope` and `state`, that scopes are separated by spaces, that a required scope the account lacks fails the installation, and that an optional one is dropped.
- The token request: a form with `grant_type`, `code`, `redirect_uri`, `client_id` and `client_secret`, or `refresh_token` in place of the code. The answer's fields, and that an access token lasts 30 minutes (`expires_in: 1800`).
- The token endpoint's error shape: `error` and `error_description`, with HubSpot's older `status` and `message` beside them.
- `GET /account-info/2026-09/details`, its `portalId`, and that it needs `oauth`. That `GET /oauth/v1/access-tokens/{token}` writes the token in the address, and that introspection needs the client secret.
- Every CRM endpoint above, with its verb, parameters, body and status: `GET /crm/objects/2026-09/{objectType}` (`limit`, `after`, `properties`, `associations`, `archived`) and `/{objectId}` (also `propertiesWithHistory`, `idProperty`); `POST …/batch/read` (`archived`), `/batch/create` (201), `/batch/update` and `/search`; `POST /crm/objects/2026-09/{objectType}` (201); `PATCH …/{objectId}` (`idProperty`); `DELETE …/{objectId}` (204).
- `PUT /crm/objects/2026-09/{type}/{id}/associations/default/{toType}/{toId}`, `PUT …/associations/{toType}/{toId}` with a list of labels (201), and `DELETE` of the same (204). That labels given replace those there.
- `GET /crm/properties/2026-09/{objectType}` and `/{propertyName}`; `GET /crm/pipelines/2026-09/{objectType}` and `/{pipelineId}`; `GET /crm/owners/2026-09` (`email`, `after`, `limit`, `archived`) and `/{ownerId}` (`idProperty` of `id` or `userId`).
- The fields of a record, a property and its options, a pipeline and its stages, and an owner, with their spelling. That every property value is a string or null.
- Paging by `paging.next.after`; 10 records a page by default and at most 100; 100 in a batch.
- Search: the body, the thirteen operators, `value`, `highValue` and `values`, one sort written as `propertyName` and `direction`, `associations.{objectType}`, the default searchable properties, 200 a page, 3,000 characters, five requests a second, no rate-limit headers, the 10,000 limit, and that `after` is a number.
- The 429 body with `policyName`, the names `DAILY` and `TEN_SECONDLY_ROLLING`, the limits by plan, that the day starts at midnight in the account's time zone, and the `X-HubSpot-RateLimit-*` headers. 423 with its two seconds, and 477 with `Retry-After` in seconds.
- The error shape `status`, `message`, `category`, `correlationId`, `errors`, `context`, and HubSpot's warning that any of them may be missing.
- The scopes in the table above, which plans have them, and that the engagements go by the contacts scope.
- Object type ids (`0-1` contacts, `0-2` companies, `0-3` deals, `0-5` tickets, `0-46` notes, `0-47` meetings, `0-48` calls, `0-49` emails, `0-27` tasks, `2-…` custom objects), and that a record's link is `https://app.hubspot.com/contacts/{account}/record/{type id}/{record id}`.

Not confirmed:

- **That the refresh token never changes.** The issue says so, and HubSpot's current pages do not say either way. Socket handles both: a new one is saved, and an answer without one keeps the stored one.
- **What a 429 looks like on a search**, and whether any 429 carries `Retry-After`. No page shows a search's 429 or names its policy. HubSpot's pages say a `policyName` is the daily limit or a "secondly" one, and that ordinary calls are no longer held to a limit by the second; so Socket reads `SECONDLY` on a search as the five-a-second search limit. Any other policy keeps its own meaning, and a 429 that names none is reported as a limit that was not named. A wait is passed on when there is one.
- **The body of the 400 for a search past 10,000**, and whether HubSpot refuses a page that only ends past it. Socket never asks: a cursor past it is refused, a page that would end past it is shortened to end at it, and a next cursor at it is not handed out. If HubSpot still answers 400 to the page that reaches the 10,000th match, its own reason is kept and the limit is named beside it.
- **That nothing is written when HubSpot answers 423.** Its page says only how long the lock lasts. So Socket does not send a locked create or update again.
- **The body of a 403 for a missing scope.** The reference's examples name the scopes under `missingScopes`; Socket also reads `requiredScopes` and `requiredGranularScopes`, in the error and in each of its details, and recognises the refusal by the category `MISSING_SCOPES`. If HubSpot writes it another way the error is still `AccessDenied`, with HubSpot's own message.
- **What exactly HubSpot withholds from an email without `sales-email-read`.** The current pages list the scope for the emails API without saying; the older engagements reference says the details of an email are not returned.
- **Which scope reads the properties of tickets.** HubSpot's scope table has `crm.schemas.*.read` for contacts, companies and deals and none for tickets, though one page names `crm.schemas.tickets.read`. The operations list `crm.objects.tickets.read`, which is in the table.
- **Whether the scopes that read records also read their properties and pipelines.** The reference pages list both kinds as alternatives. The operations list the schema scopes for properties, which certainly do.
- **That a private app's token may call the account-details endpoint.** The page names the scope `oauth` and does not say which kinds of token have it.
- **`portalName`.** The endpoint's description lists it as optional and the guide's example leaves it out. When it is absent the account is named by its id.
- **The reference page for listing one record's associations.** The associations guide gives the address at `2026-09`; the parameters (`after`, `limit`, at most 500) are from the reference page of the version before.
- **Whether `associations` must be sent when creating a record.** The reference marks the list required and the guides' examples leave it out. Socket sends it, empty when there is nothing to associate.
- **What the plain association answers.** The reference describes a batch result. Socket takes the association as made only when the pair asked for is among the results, in either direction.
- **How many filter groups a search takes.** The guide says five, the reference says six. Socket leaves the count to HubSpot.
- **The largest page of owners.** The reference gives only the default, 100. Socket sends whatever is asked and leaves the limit to HubSpot.
- **How HubSpot reads an encoded slash in an id.** Socket refuses such an id; `objects.batch_read` takes the value in its body.
- **PKCE.** The token endpoint lists a `code_verifier`, and the sign-in page documents no challenge, so it is off.
- **Accounts hosted in the EU.** The pages read give `api.hubapi.com` as the address for every account, and a `dataHostingLocation` for each; that one host serves all of them was not confirmed.

## Not supported yet

- **Custom object definitions.** Records of a custom object work through `objects` by type id; finding the type ids of an account's custom objects (`/crm-object-schemas`) has no method.
- **Upsert, merge and batch archive**, and **GDPR deletion**.
- **Partial success of a batch create** (`objectWriteTraceId`, answered with 207).
- **Batch association calls**, **association labels and limits** (the schema endpoints), and **removing one label** while keeping the association.
- **Creating and changing properties, property groups, pipelines and stages.** They are read-only here.
- **Sensitive data properties**, which need scopes of their own.
- **Fetching an attachment or a call recording** an engagement points to.
- **A link to one account's sign-in page** (`/oauth/{account id}/authorize`).
- **Marketing, CMS, the conversations inbox and workflows.**
- **Webhooks**, and any other incoming event.

Anything HubSpot offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("crm-object-schemas/2026-09/schemas")).await?;
```
