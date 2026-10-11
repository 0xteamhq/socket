# Salesforce

**Status:** built and tested against a local server that answers as Salesforce's documentation says. Not yet run against a real Salesforce organisation.

Socket's Salesforce integration gives a program an organisation's records through Salesforce's REST API. Every organisation defines objects and fields of its own, so the client is generic: queries in SOQL, text search, records of any object type, and the description of the organisation's objects and fields, which is how a caller learns what there is to ask for. That is 13 typed methods, and the same 13 as operations callable by name with JSON, plus identity and lookup of a record. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against Salesforce's documentation.

## Connect

Add the crate with the Salesforce feature:

```sh
cargo add socketkit --features salesforce
```

### Every organisation has its own address

Salesforce has no one API host. Each organisation is served from its own, such as `https://acme.my.salesforce.com`, and Salesforce names it in `instance_url` when a person authorises. Socket keeps that address with the connection's tokens, calls it and nothing else, and keeps it through every renewal.

The definition admits one family of hosts as an organisation's own: anything under **`my.salesforce.com`**. That covers production (`acme.my.salesforce.com`), sandboxes (`acme--uat.sandbox.my.salesforce.com`), Developer Edition, scratch and trial organisations (`acme-dev-ed.develop.my.salesforce.com` and the like), and Government Cloud, which signs in through My Domain. An address Salesforce names outside that family is refused when the person connects, and nothing is stored. Not admitted:

- **The older instance hosts** such as `na1.salesforce.com`. Salesforce has enforced My Domain for every organisation since Winter '24 and no longer redirects the old names.
- **Clouds with a domain of their own**, such as `salesforce.mil` and Salesforce on Alibaba Cloud. Their domains were not confirmed, so they are left out.
- **Other products' hosts**: `lightning.force.com`, Experience Cloud sites, Marketing Cloud, Commerce Cloud.

A host is admitted only as the address of the connection whose authorisation named it. One customer's token is never sent to another customer's host.

### With a token you already hold

```rust
use std::sync::Arc;
use socketkit::salesforce::Salesforce;
use socketkit::{ConnectionKey, ProviderId, Socket};

let salesforce = Salesforce::with_token("00Dxx0000001gPL!AR8…", "https://acme.my.salesforce.com");
let socket = Socket::in_memory().integration(Arc::new(salesforce.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("salesforce")?, "me")).await?;
```

The second argument is the organisation's address, as Salesforce gave it in `instance_url`. It is not optional: a token is good for one organisation, and only the address says which. One that is not an organisation's own host is reported when the `Socket` is built, and the token is not kept. An access token lasts as long as the organisation's session policy allows, often two hours, and this way has no refresh token, so it suits a script.

### With your own connected app, to connect your users

Create a connected app (or an external client app) in Salesforce Setup, enable OAuth, add your callback route as a callback URL, and select the scopes `api`, `refresh_token` and `id`.

```rust
use socketkit::salesforce::{LoginHost, Salesforce, SalesforceOAuth};
use socketkit::{OAuthClient, SecretString};

let salesforce = Salesforce::with_oauth(SalesforceOAuth {
    client: OAuthClient {
        client_id: config.salesforce_consumer_key,
        client_secret: SecretString::new(config.salesforce_consumer_secret),
        redirect_uri: "https://yourapp.example/oauth/salesforce/callback".parse()?,
    },
    // `None` asks for the defaults: api, refresh_token and id.
    scopes: None,
    // Where people sign in. `None` is production.
    login: Some(LoginHost::Sandbox),
    // The REST API version. `None` is 67.0.
    api_version: None,
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(salesforce.clone())).build()?;

// When a user clicks "Connect Salesforce":
let key = ConnectionKey::new(ProviderId::new("salesforce")?, user.id);
let authorization = socket.begin_authorization(key.clone(), None)?;
// Keep `authorization.pending` in that user's session, then redirect them to `authorization.url`.

// At your callback route:
socket.complete_authorization(pending, code, state).await?;
let connection = socket.connection(key).await?;
```

`Salesforce::with_oauth(client)` with a plain `OAuthClient` also works when the defaults are enough.

| Setting | What it does |
| --- | --- |
| `scopes` | The scopes to ask for, in place of the defaults. Salesforce has one scope for the data API, `api`, for reading and writing alike: what a connection may read or change is decided by the permissions of the person who authorised it, not by a scope. |
| `login` | `LoginHost::Production` (`login.salesforce.com`, the default), `LoginHost::Sandbox` (`test.salesforce.com`), or `LoginHost::MyDomain("acme.my.salesforce.com".into())` for an organisation that lets its people sign in only at its own address, as Government Cloud does. |
| `api_version` | The version of the REST API to call, such as `"66.0"`. The default is `67.0` (Summer '26). An organisation answers every version up to its own release; `GET /services/data/` on its host lists them. Nothing older than `46.0` is taken. |

The same settings can be given one at a time: `Salesforce::new().login(LoginHost::Sandbox).api_version("66.0").oauth(client)`.

Four things Socket does for every Salesforce connection:

- **A refresh token is always asked for.** `refresh_token` is added to whatever scopes are given, in the settings or at `begin_authorization`, unless `offline_access` is among them. Without either Salesforce issues no refresh token. The connected app has to have that scope selected.
- **PKCE is on.** The code challenge (`S256`) is sent with the sign-in and the verifier with the code, beside the client secret.
- **A token has no expiry to go by.** Salesforce's token response states no lifetime: an access token stops working when the organisation's session policy says so. Socket uses it until Salesforce answers `INVALID_SESSION_ID`, then renews it once and sends the call again. A refresh token is kept through a renewal unless Salesforce sends a new one, which it does only when the app rotates them.
- **The organisation's address is read from the token response** and checked against the hosts above before anything is stored. An authorisation that names none is refused.

A My Domain host is checked against the same rule as an organisation's API host, by the URL parser and not by joining text. One that is not such a host (`evil.example`, `acme.my.salesforce.com.evil.example`, a host with a path, a port or a user name) is reported when the `Socket` is built and never reaches the address of the sign-in page or of the token endpoint. A host that passes is added to the hosts the client secret may be sent to, for that `Salesforce` value only.

### A connection that does not know its organisation

The definition's own address is the sign-in host, because a definition has to have one and no organisation's would be right. Nothing is read from it. A connection stored without the address Salesforce gave, such as a bare token saved by hand, is refused with `ReconnectRequired` before any request is made, and its token is not sent to the sign-in host. Connect it again through OAuth, or use `with_token` with the instance URL.

This holds for the typed methods and the named operations. A raw `socket.request(…)` with a relative path does not pass through the integration, and would still resolve against the definition's address; see "Not supported yet".

A connection keeps the API version it was made with until its token is next renewed.

## Identity and lookup

```rust
let me = salesforce.identity(&connection).await?;          // Account { id, name, email }
let record = salesforce.resolve(&connection, input).await?; // Resource { id, label, description }
```

`identity` reads `GET /services/oauth2/userinfo` on the organisation's own host and needs the `id` scope, which every other scope includes. A user id is unique only within its organisation, and a sandbox copies those of production, so the account's `id` is both: `{organisation id}/{user id}`, as Salesforce's own identity address ends. The name is the person's display name, or their username when there is none.

`resolve` accepts a record's object type and id as `Account/001xx000003DGb2AAG`, or the link to the record in Lightning (`https://acme.lightning.force.com/lightning/r/Account/001xx000003DGb2AAG/view`), and confirms the record exists and the account can read it. The resource's `id` is `{object type}/{record id}` with the id in the 18 characters Salesforce writes; the label is the record's name, or its subject, title or number for an object that has no name. It reads the whole record once.

## Use the typed methods

Methods are grouped by area. Each group is reached through the `Salesforce` value and a connection: `salesforce.query(&connection)`.

**How arguments are split.** What identifies the thing acted on is a plain argument: an object type, a record id, a field name. Content and options are structs from `socketkit::salesforce::models`.

**Names and ids are checked.** An object type and a field name are letters, digits and underscores, starting with a letter; a record id is 15 or 18 letters and digits. Anything else is refused before a request is made, so none of them can add a segment or a query to an address. The value of an external id is another system's text and may hold anything: it is percent-encoded into one segment.

```rust
use socketkit::salesforce::models::{GetRecord, QueryOptions, RecordFields};
use socketkit::salesforce::quote_soql;

let soql = format!("SELECT Id, Name, Owner.Name FROM Account WHERE Industry = {} LIMIT 20", quote_soql(industry));
let accounts = salesforce.query(&connection).run(&soql, QueryOptions::default()).await?;
for account in &accounts.records {
    let owner = account.parent("Owner").and_then(|owner| owner.fields.get("Name").cloned());
}

let mut fields = serde_json::Map::new();
fields.insert("Status".into(), "Completed".into());
salesforce.records(&connection).update("Task", task_id, RecordFields { fields }).await?;
```

### `salesforce.query(&connection)`

| Method | Returns |
| --- | --- |
| `run(soql, QueryOptions)` | `QueryResult`: `totalSize`, `done`, `records` and `next_cursor` |
| `run_all(soql, QueryOptions)` | the same, including deleted records still in the recycle bin and archived tasks and events |

A query only reads: SOQL has no statement that changes a record, so both are `read` however the query is written. `totalSize` is how many records the whole query matches; for `SELECT COUNT() FROM …` it is the answer and `records` is empty.

**Keep a query small.** A batch holds up to 2000 records, each with every field the query selected. Select the fields that are needed and end with `LIMIT`. `QueryOptions.batch_size` is from 200 to 2000 (Salesforce's `Sforce-Query-Options` header), and Salesforce may return fewer than asked.

### `salesforce.search(&connection)`

| Method | Returns |
| --- | --- |
| `run(sosl)` | `SearchResult`: the records found, of every type searched |
| `find(Find)` | the same, from options and not from SOSL |

`find` takes the `text` to look for, the `objects` to look in (each a `FindIn` with its `name`, and optionally its own `fields` and `limit`), the `fields` to return of every type that names none, `within` (`all`, `name`, `email`, `phone` or `sidebar`), and `limit` and `overall_limit` from 1 to 2000. Without `objects` only the ids of what is found come back. Salesforce takes this search as a POST; it changes nothing and is marked `read`.

The text given to `find` is searched for as it stands: every character SOSL gives a meaning to is escaped. The words `AND`, `OR` and `AND NOT` between other words are still read as operators. For wildcards and phrases, write SOSL and use `run`.

### `salesforce.sobjects(&connection)`

| Method | Returns |
| --- | --- |
| `list(ListSObjects)` | `Vec<SObjectSummary>`: every object type the account can see, in a few words each |
| `describe(object)` | `Describe`: the object's fields, the types that refer to it, and its record types |

This is how fields are discovered. `list` is one call that Salesforce answers with every type, hundreds in most organisations; `ListSObjects.contains` keeps those whose API name or label contains some text, and `custom` keeps the organisation's own types or Salesforce's. A row is the name, the labels, the key prefix, and whether the account may query, search, create, change and delete.

A `Field` carries its `name`, `label` and `type`; `length`, `precision` and `scale`; `nillable` and `defaultedOnCreate` (a field that is createable, not nillable and not defaulted is required on a new record); `createable`, `updateable`, `filterable`, `sortable`; `custom`, `calculated`, `externalId`, `unique`, `idLookup`, `nameField`; `referenceTo` and `relationshipName` for a lookup; `picklistValues`, each with `value`, `label`, `active` and `defaultValue`; and `inlineHelpText`. A field hidden from the account by field-level security is not listed.

### `salesforce.records(&connection)`

| Method | Returns |
| --- | --- |
| `get(object, id, GetRecord)` | `Record` |
| `get_by_external_id(object, field, value, GetRecord)` | `Record` |
| `create(object, RecordFields)` | `Saved`: the new record's `id`, and `created: true` |
| `update(object, id, RecordFields)` | nothing |
| `upsert(object, field, value, RecordFields)` | `Saved`: the `id`, and whether the record was `created` |
| `delete(object, id)` | nothing |

**A `Record`** is the object `type`, the `id`, and `fields`: a map of the fields that were asked for, by API name, exactly as Salesforce wrote them. Relationships stay nested in it. The record a lookup leads to is under the relationship's name (`Owner`), with Salesforce's own `attributes`; the records of a subquery are under theirs (`Contacts`), as `{ totalSize, done, records }`. In Rust, `record.parent("Owner")` and `record.children("Contacts")` read them as records. A row of a query that counts or groups is an `AggregateResult` with no id.

**Name the fields to read.** `GetRecord.fields` is the list of API names to return. Without it Salesforce returns every field the account can see.

**Writing.** `RecordFields.fields` maps API names to values; `null` clears a field. `update` changes the fields that are named and leaves the rest. `upsert` creates the record whose external id `field` holds `value`, or changes it when it exists; the external id is given as an argument and not among the fields. When more than one record holds the value, Salesforce refuses and writes nothing. The organisation's validation rules, triggers and flows run as they do for a person.

**Deleting** moves a record to the recycle bin, where Salesforce keeps it for a time, and records that depend on it may go with it. It is the one `destructive` operation.

### `salesforce.limits(&connection)`

| Method | Returns |
| --- | --- |
| `get()` | `Limits`: `DailyApiRequests` with `Max` and `Remaining`, and every other allowance Salesforce reports by its own name |

## Write a value into a query

SOQL and SOSL are text, and a value placed in one is text inside text. `O'Brien` ends a SOQL string early, and what follows it is read as query. Three public functions write a value so that every character of it is taken as itself:

| Function | For |
| --- | --- |
| `escape_soql(value)` | A value between the single quotes of a SOQL string. Escapes the backslash, both quotes, line breaks, tabs and other control characters. `quote_soql(value)` adds the quotes. |
| `escape_soql_like(value)` | A value inside the string of a `LIKE`. As above, and `%` and `_` are written to mean themselves. Add the wildcards you want around the result. |
| `escape_sosl(text)` | Text between the braces of `FIND {…}`. Escapes `? & \| ! { } [ ] ( ) ^ ~ * : \ " ' + -`. |

```rust
use socketkit::salesforce::{escape_soql_like, quote_soql};

let by_name = format!("SELECT Id FROM Contact WHERE LastName = {}", quote_soql("O'Brien"));
let starting = format!("SELECT Id FROM Account WHERE Name LIKE '{}%'", escape_soql_like("100%_sure"));
```

They make a quoted value safe. A number, a date, a field name or an object name is not written in quotes: check it for what it is. No method of this integration builds SOQL itself; `search.find` uses `escape_sosl` for its text.

## Page through a query

```rust
let mut options = QueryOptions::default();
loop {
    let batch = salesforce.query(&connection).run(soql, options.clone()).await?;
    for record in &batch.records { /* … */ }
    match batch.next_cursor {
        Some(cursor) => options.cursor = Some(cursor),
        None => break,
    }
}
```

The cursor is the address Salesforce gave for the next batch (`nextRecordsUrl`), a path such as `/services/data/v67.0/query/01gxx0000004RpzAAE-2000`. Pass it back unchanged. With a cursor the query is not sent again, and neither is a batch size: the cursor is Salesforce's place in the results of the query that made it. The typed methods take an empty query with a cursor, and the operations take none: `{ "cursor": "…" }` is a whole input. With neither a query nor a cursor the call is refused.

A cursor comes back from the caller, so Socket does not trust it to be what Salesforce sent. **Nothing in a cursor is used as a host or a path.** Only the locator at its end is taken, and only when the cursor has the shape of a next batch and the locator is letters, digits, `-` and `_`. The batch is then asked for at the address Socket builds under the connection's own API. A cursor that names another host, another resource, a path with `..`, a query, or a bare locator is refused before any request is made.

## Call an operation by name

Every method is also an operation an agent, an MCP server or another language can call with JSON. The plain arguments and the options sit side by side in one object.

```rust
let output = socket
    .invoke(key, "salesforce.records.get".into(), serde_json::json!({
        "object": "Case",
        "id": "500xx000000bcdeAAA",
        "fields": ["CaseNumber", "Subject", "Status"]
    }))
    .await?;
```

| Operation | Effect | Scope | What it does |
| --- | --- | --- | --- |
| `salesforce.identity.get` | read | id | Return the account this connection is authorised as. |
| `salesforce.resource.resolve` | read | api | Confirm a record exists and the account can read it. |
| `salesforce.query.run` | read | api | Run a query in SOQL and return the records it matches, a batch at a time. |
| `salesforce.query.run_all` | read | api | The same, including deleted records still in the recycle bin and archived tasks and events. |
| `salesforce.search.run` | read | api | Run a text search written in SOSL across object types. |
| `salesforce.search.find` | read | api | Search records for some text, naming the object types to look in and the fields to return. |
| `salesforce.sobjects.list` | read | api | List the organisation's object types, each with its API name and label. |
| `salesforce.sobjects.describe` | read | api | Describe one object type: fields, picklist values, relationships, record types. |
| `salesforce.records.get` | read | api | Get one record by its id, with the fields named. |
| `salesforce.records.get_by_external_id` | read | api | Get one record by the value of an external id field. |
| `salesforce.records.create` | write | api | Create a record and return its id. |
| `salesforce.records.update` | write | api | Change the fields given on one record. |
| `salesforce.records.upsert` | write | api | Create the record that holds a value in an external id field, or change it when it exists. |
| `salesforce.records.delete` | destructive | api | Delete one record, to the recycle bin. |
| `salesforce.limits.get` | read | api | Report what is left of the organisation's allowances. |

`update` and `upsert` are `write`, as the specification of this integration has them, although an update overwrites what a field held. A host that asks a person only before a destructive operation does not ask before them.

A query is a `read` whatever it says. SOQL and SOSL can carry clauses that record that a record or an article was viewed (`FOR VIEW`, `FOR REFERENCE`, `UPDATE VIEWSTAT`, `UPDATE TRACKING`); those touch view statistics, not the data.

## Handle errors

Salesforce answers a refusal with a list of `{ errorCode, message }`. The code decides the kind where the status would mislead, and both are in the message: `salesforce denied the request (INSUFFICIENT_ACCESS_OR_READONLY): insufficient access rights on object id`.

| Kind | What it means for Salesforce | What to do |
| --- | --- | --- |
| `ReconnectRequired` | `INVALID_SESSION_ID` or any 401; a 403 from the identity service that is plain text and states no wait, which is how it refuses a token; a refresh token no longer accepted; or a connection stored without its organisation's address | Socket renews the token once by itself; if that fails, connect again |
| `RateLimited` | `REQUEST_LIMIT_EXCEEDED`, which Salesforce sends as a 403: the organisation's API requests for the last 24 hours are used up, or too many long requests are running at once. Also a 429, and any 403 that carries `Retry-After`, whatever else it says | Wait. For `REQUEST_LIMIT_EXCEEDED` Salesforce does not say for how long, so `retry()` is `Later`; `limits.get` shows the allowance once calls are accepted again |
| `AccessDenied` | Any `INSUFFICIENT_ACCESS…` code, whatever the status; `API_DISABLED_FOR_ORG` or `API_CURRENTLY_DISABLED` (an edition or a setting without API access); or any other 403, one with no body or with a page from a proxy or a network rule included. Such a 403 does not renew the token | Give the person the permission, use an organisation whose edition has the API, or look at what stands between you and Salesforce |
| `NotFound` | No such record, object type or address | Check the type and the id |
| `InvalidInput` | Salesforce refused the request: a malformed query, a missing required field, a validation rule, a duplicate, a field that cannot be written (`INVALID_FIELD_FOR_INSERT_UPDATE`). Also more than one record for an external id, and a request too long to send | Fix the input |
| `Unexpected` | Salesforce failed | Try again later |
| `Decode` | Salesforce answered success without what was asked for, or with something that could not be read | Report it; this should not happen |

**Limits and permissions, said out loud:**

- **The daily allowance.** Every call counts against the organisation's API requests for the last 24 hours, shared with every other integration it runs. When it is used up, every call is `RateLimited` until earlier ones are a day old.
- **Editions.** Some editions have no API access at all; every call is then `AccessDenied` with `API_DISABLED_FOR_ORG`.
- **The person's own permissions** decide what a connection sees. An object the person may not read looks as if it did not exist, and a field hidden by field-level security is not in `describe` and is refused in a query as an unknown column, which arrives as `InvalidInput` and cannot be told from a misspelt name. A write to a field the person may not write is answered with `INVALID_FIELD_FOR_INSERT_UPDATE`, which Salesforce also sends for a field nobody may write, such as an id, a formula, or the external id repeated among the fields of an upsert. The code does not say which, so it is `InvalidInput`, and Salesforce's words in the message say to check the field's security settings.
- **A query has to fit in an address.** The query and the headers have about 16,000 bytes between them; a longer one is `InvalidInput`.
- **Salesforce's own words are passed on**, cut to 300 characters, as every Socket integration passes on its provider's reason. Salesforce writes some of them with a value from the request in them (a value too long for its field, a query it could not parse), with a record id (a duplicate), or with text the organisation's administrator wrote (a validation rule). Treat an error message from a write as you treat the record.

An input error never repeats the value you sent. It names the field when one is missing, of the wrong type, or not known to the operation: `objects[1].where`. Each operation's input schema says the same, with `additionalProperties: false`. A record's own `fields` are the organisation's to define and are passed on whatever they are called.

A request sent as GET is retried on a throttle or a server error. `create`, `update`, `upsert` and `search.find` are sent again only when Salesforce did not carry them out: it refused the request for the allowance, or rejected the access token and Socket renewed it. **If a write fails with a server error, check before sending it again**; `upsert` is the write that is safe to repeat.

**Deleting is the exception today.** The transport still repeats a DELETE after a server error. If the first try did delete the record, the second is answered "not found", and the call reports `NotFound` for a delete that worked. Treat `NotFound` from `delete` as "it is gone".

## Confirmed against Salesforce's documentation, and not

The pages of developer.salesforce.com and help.salesforce.com could not be opened directly when this was written, in October 2026: they answered a plain fetch with an error. What follows was confirmed from search results that quote those pages, which show parts of a page and not the whole. Nothing was run against a live organisation.

Confirmed:

- The web server flow at `/services/oauth2/authorize` and `/services/oauth2/token`; PKCE with `code_challenge` and `code_verifier`, and `S256` as the method.
- The token response: `access_token`, `refresh_token`, `signature`, `scope`, `instance_url`, `id`, `token_type`, `issued_at`, with no `expires_in` among them: the app's session timeout decides when a token ends. The same for a refresh, which returns a new `refresh_token` only when the app rotates them. Should Salesforce ever state a lifetime, Socket honours it.
- A refresh token is issued only with the `refresh_token` or `offline_access` scope. `full` does not include it. Every scope includes `id`.
- `login.salesforce.com` and `test.salesforce.com`; My Domain hosts as `name.my.salesforce.com` and `name--sandbox.sandbox.my.salesforce.com`; that Government Cloud signs in through My Domain and not the generic hosts; that instance hosts are no longer redirected.
- `GET /services/oauth2/userinfo` and its fields `user_id`, `organization_id`, `preferred_username`, `name`, `email`; that it answers a token it does not accept with 403 and `Bad_OAuth_Token`.
- `GET /services/data/` lists the versions an organisation answers, without a token. Version 67.0 is Summer '26.
- `GET …/query?q=` and `…/queryAll?q=`; `nextRecordsUrl`; that the next batch of `queryAll` is addressed under `query`; the `Sforce-Query-Options: batchSize=` header from 200 to 2000.
- `…/parameterizedSearch` as GET and as POST, with `q`, `fields`, `in`, `defaultLimit` and `overallLimit` up to 2000, and a name, fields and a limit for each object.
- `…/limits` and `DailyApiRequests` with `Max` and `Remaining`.
- Upsert by `PATCH …/sobjects/{type}/{field}/{value}`: 201 for a record created and 200 for one changed, with `created` in the answer, from version 46.0; 204 and no answer before that; 300 when more than one record holds the value.
- `DELETE` answers 204. A create answers 201 with the new id.
- Errors as a list with `message` and `errorCode`; 401 for an expired session; 403 with `REQUEST_LIMIT_EXCEEDED` for a used-up allowance, of requests in 24 hours or of long requests at once.
- SOQL's escapes: `\\`, `\'`, `\"`, `\n`, `\r`, `\t`, `\f`, `\uXXXX`, and `\%` and `\_` inside `LIKE` only. That SOSL's reserved characters need a backslash.
- An address and its headers may total 16,384 bytes; 414 and 431 beyond that.

Different from what the specification of this integration expected:

- **`search.find` is sent as a POST**, with its options in a JSON body. Salesforce offers the search both ways; the body states several object types, each with its own fields, without ambiguity.
- **A refused token is not always a 401.** The identity service answers 403 with one word of plain text. Socket's transport keeps no error body that is not JSON, so the word is not read: a 403 whose content type is `text/plain`, with nothing readable in it and no `Retry-After`, is taken for that refusal and the token is renewed. Every other 403 without Salesforce's own error is `AccessDenied`, and the token is left alone.
- **A write refused by field-level security is `InvalidInput`, not `AccessDenied`.** The specification asked for `AccessDenied`. Salesforce has no code for it alone: `INVALID_FIELD_FOR_INSERT_UPDATE` is also what it answers for any field that cannot be written, which is the caller's to correct.
- **An upsert has three answers**, and one that changes a record did not say so before version 46.0. Socket takes no version older than that.
- **A batch has a size that can be asked for**, and a query has a length it cannot pass. Neither was in the specification; both are here.

Not confirmed:

- **The names in the body of `search.find`** (`sobjects`, and `name`, `fields`, `limit` within it). The excerpts name the parameters but not the array.
- **The answer of a search**: `searchRecords`, each a record with `attributes`. The page was not shown whole.
- **The exact list of SOSL's reserved characters.** The page says there is one, and the excerpts cut it off; the list used here was written from memory of that page.
- **The properties of an object type and of a field** in `describe` and in the list of types, beyond `fields`, `childRelationships`, `relationshipName`, `referenceTo` and `picklistValues`.
- **That an update answers 204**, and the `id`, `success` and `errors` of a create's answer. Socket accepts any success for an update.
- **The shape of a query locator**, and of a record id as 15 or 18 letters and digits. Both are checked as that.
- **Whether `fields` on a record may reach through a relationship** (`Account.Name`). Socket allows it and Salesforce decides.
- **How field-level security shows in a query**, and the status that carries each `INSUFFICIENT_ACCESS…` code.
- **The scope `userinfo` needs.** `id` is assumed, since every scope includes it.
- **The content type of the identity service's refusal.** `text/plain` is assumed. If it is sent as anything else, a refused token is reported by `identity` as `AccessDenied` and is not renewed there; a call to the data API still renews it, on its 401.
- **Whether version 68.0 is released everywhere.** The default stays at 67.0.
- **The domains of the clouds outside `salesforce.com`.**

## Not supported yet

- **Bulk API and Composite API**, so a write is one record at a time and a query has to fit in an address.
- **Apex, the Metadata and Tooling APIs, streaming and change data capture.**
- **Other sign-in flows**: JWT bearer and client credentials, where no person signs in.
- **Clouds with a domain of their own**, and the older instance hosts.
- **Files and attachments' content**, record layouts, the UI API, reports, approvals and Chatter.
- **Listing the API versions** an organisation answers.
- **`login_hint` and `prompt`** on the sign-in page.
- **A definition without an address.** The core asks every definition for one API address, so Salesforce's carries the sign-in host, and a raw `socket.request` with a relative path on a connection that has no address of its own would be sent there. The typed methods and operations refuse such a connection first.

Anything Salesforce offers that has no method here can still be called through the generic request, with the token, retries and error handling applied. A relative path is resolved against the connection's own organisation and API version:

```rust
let response = socket.request(key, RawRequest::get("sobjects/Account/describe/layouts")).await?;
```
