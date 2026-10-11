# Notion

**Status:** built and tested against a local server that answers as Notion's documentation says. Not yet run against the real Notion.

Socket's Notion integration gives a program search, pages, blocks, databases, users and comments as 19 typed methods, and the same 19 as operations callable by name with JSON, plus identity and lookup of a page or a database. One of them, `pages.read`, returns a page's whole content as Markdown. This page shows how to connect, lists everything that is supported, and says what was and was not confirmed against Notion's documentation.

## Connect

Add the crate with the Notion feature:

```sh
cargo add socketkit --features notion
```

### With a token you already hold

```rust
use std::sync::Arc;
use socketkit::notion::Notion;
use socketkit::{ConnectionKey, ProviderId, Socket};

let notion = Notion::with_token("ntn_…");
let socket = Socket::in_memory().integration(Arc::new(notion.clone())).build()?;
let connection = socket.connection(ConnectionKey::new(ProviderId::new("notion")?, "me")).await?;
```

Every call uses that token. This is how an internal integration connects: its secret is the token.

### With your own public integration, to connect your users

```rust
use socketkit::notion::Notion;
use socketkit::{OAuthClient, SecretString};

let notion = Notion::with_oauth(OAuthClient {
    client_id: config.notion_client_id,
    client_secret: SecretString::new(config.notion_client_secret),
    redirect_uri: "https://yourapp.example/oauth/notion/callback".parse()?,
});
let socket = Socket::builder(Arc::new(my_token_store)).integration(Arc::new(notion.clone())).build()?;
```

`begin_authorization` and `complete_authorization` then work as for every provider.

### What an integration may do

**Notion has no scopes.** Nothing is asked for at sign-in, and every operation's `required_scopes` is empty. Two other things decide what a call may do:

- **Capabilities**, set on the integration in Notion's developer portal: read content, update content, insert content, read comments, insert comments, and user information (none, without email addresses, or with them). The comment capabilities are off by default. Each operation's description names the one it needs; without it Notion answers `403`, which Socket reports as `AccessDenied` with Notion's own words.
- **Which pages were shared with it.** A person chooses pages when they approve the integration, or adds it to a page later. A page that was shared brings the pages inside it.

A page that was not shared is answered exactly as a page that does not exist: `404 object_not_found`. Socket cannot tell the two apart and neither can a caller, so every `NotFound` from Notion says so: "notion has nothing with that id, or it was not shared with this integration".

### The API version

Every request declares `Notion-Version: 2026-03-11`, the current version, and the typed methods are written for it. It differs from the versions before it in ways a caller meets:

- **A database's rows belong to its data sources** (since `2025-09-03`). `databases.get` returns the database and names its data sources; the schema is read with `databases.data_source` and the rows with `databases.query`, both by a data source's id. A search finds data sources, not databases. A new row's parent is a `data_source_id`.
- **What is trashed is marked `in_trash`.** `archived` is gone from requests and responses.
- **New blocks are placed with `position`**, in place of `after`.

`Notion::version("…")` and `NotionToken::version` declare another version. Identity and lookup work under any; the typed methods may be refused or return less under an older one.

## Identity and lookup

```rust
let me = notion.identity(&connection).await?;         // Account { id, name, email }
let page = notion.resolve(&connection, input).await?; // Resource { id, label, description }
```

`identity` reads `GET /v1/users/me`, the integration's bot user, and names it by the person who owns it, or by its workspace when the workspace does.

`resolve` accepts the address of a page or a database, or its id with or without dashes, and confirms it exists and was shared. The id is asked for as a page first and then as a database. Notion refuses a database's id asked for as a page with a `400`, the status it also gives a request that is wrong in itself, so a `400` on the page is not read as "not found": the database is asked for, and when there is none either, the `400` is reported as `InvalidInput` with Notion's reason. Only a `404` on both is `NotFound`.

## Use the typed methods

Methods are grouped by area. Each group is reached through the `Notion` value and a connection: `notion.pages(&connection)`.

**How arguments are split.** What identifies the thing acted on is a plain argument: a page id, a block id. It may be given with or without dashes, or as the address of a page, and is refused before any request when it is neither. Content and optional filters are structs from `socketkit::notion::models`.

**Names and shapes are Notion's own.** Fields are written as Notion writes them (`in_trash`, `has_children`, `rich_text`). What Notion writes in many forms is kept as the JSON Notion documents and is sent as it was written, a `null` included, since `null` is how Notion is told to empty a value: a property's value, a block's content, a filter, an icon.

```rust
use socketkit::notion::models::{CreateComment, Parent, ReadPage, RichText, SearchQuery};

let found = notion.search(&connection).run(SearchQuery {
    query: Some("roadmap".into()),
    ..SearchQuery::default()
}).await?;

let content = notion.pages(&connection).read(page_id, ReadPage::default()).await?;
println!("{}", content.markdown);

notion.comments(&connection).create(CreateComment {
    parent: Some(Parent { page_id: Some(page_id.into()), ..Parent::default() }),
    rich_text: Some(vec![RichText::plain("Read and approved.")]),
    ..CreateComment::default()
}).await?;
```

| Group | Method | Returns | Effect |
| --- | --- | --- | --- |
| `search` | `run(SearchQuery)` | `Page<PageOrDataSource>`: pages and data sources whose title matches | read |
| `pages` | `get(page)` | `Page`: its properties and where it lives, without its content | read |
| `pages` | `property(page, property, Paging)` | `PropertyItems`: one property in full | read |
| `pages` | `read(page, ReadPage)` | `PageContent`: the whole content as Markdown | read |
| `pages` | `create(CreatePage)` | `Page` | write |
| `pages` | `update(page, UpdatePage)` | `Page` | write |
| `pages` | `archive(page)` | `Page`, in the trash | destructive |
| `blocks` | `get(block)` | `Block` | read |
| `blocks` | `children(block, Paging)` | `Page<Block>`: the blocks directly inside a block or a page | read |
| `blocks` | `append(block, AppendBlocks)` | `Page<Block>`: the blocks as Notion made them | write |
| `blocks` | `update(block, UpdateBlock)` | `Block` | write |
| `blocks` | `delete(block)` | `Block`, in the trash | destructive |
| `databases` | `get(database)` | `Database`, naming its data sources | read |
| `databases` | `data_source(data_source)` | `DataSource`: the schema its rows follow | read |
| `databases` | `query(data_source, QueryDataSource)` | `Page<PageOrDataSource>`: the rows that pass a filter, in order | read |
| `users` | `list(Paging)` | `Page<User>`: members and integrations, without guests | read |
| `users` | `get(user)` | `User` | read |
| `comments` | `list(block, Paging)` | `Page<Comment>`: the comments that are not resolved | read |
| `comments` | `create(CreateComment)` | `Comment` | write |

Notes on some of them:

- **`search.run` searches titles only**, never content. `filter.value` is `page` or `data_source`; `filter.in_trash` searches the trash; `sort.direction` orders by the time of the last edit, which is the only order Notion offers besides relevance.
- **`pages.property`** is for the properties `pages.get` cuts short at 25 references: a relation, a list of people, a title or a text with many mentions. `property` is the property's id as the page gives it (`%3EfC`, kept as it is) or its name. Whatever the property, the answer has the same shape: `items`, and `next_cursor` when there are more.
- **`pages.create`** takes a `parent` with a `page_id` or a `data_source_id`, and at most 100 blocks in `children`. More are added with `blocks.append`.
- **`pages.update`** changes properties, the icon and the cover, and nothing else: it cannot trash a page, which is `pages.archive`.
- **`pages.archive` and `blocks.delete` move to the trash.** Notion's API cannot delete for good. A person can restore from the trash in Notion; no operation here restores.
- **`blocks.update`** takes `content` with one kind of block and what to set in it: `{ "to_do": { "checked": true } }`. A field that is given is replaced whole. It refuses `in_trash`, so a write cannot trash a block.
- **`blocks.append`** takes from 1 to 100 blocks, each with its own `children` two levels deep at most, and an optional `position`: `{ "type": "start" }`, `{ "type": "end" }` or `{ "type": "after_block", "after_block": { "id": "…" } }`.
- **`comments.create`** goes on a page (`parent.page_id`), on a block (`parent.block_id`), or into a thread (`discussion_id`): exactly one. It says its text as `rich_text` or as `markdown`: exactly one.
- **`users`**: what a user carries depends on the user information capability. Without it there is no name; without the email one there is no email.

## Read a page as Markdown

A page's content is a tree of blocks. Notion returns one list of blocks at a time, a hundred to a request, and each block with others inside it is a list of its own. `pages.read` walks the tree and writes it as Markdown:

```rust
let content = notion.pages(&connection).read(page_id, ReadPage { max_depth: Some(3), max_requests: None }).await?;
// content.title, content.url, content.markdown, content.truncated, content.truncation, content.blocks, content.requests
```

| Block | Written as |
| --- | --- |
| Paragraph | Its text. An empty one is left out |
| Heading 1 to 4 | `#` to `####` |
| Bulleted, numbered and to-do items, toggles | `- …`, `1. …`, `- [ ]` and `- [x]`, `- …`, with what is nested indented under them |
| Quote, callout | `> …`, a callout behind its emoji |
| Code | A fenced block with its language, then its caption |
| Table | A Markdown table. One without a heading row gets an empty one |
| Divider, equation | `---`, `$$ … $$` |
| A page or database inside the page, a link to a page | A link to it in Notion. It is not read |
| Bookmark, embed, link preview | A link to its address |
| Image, video, audio, file, PDF | A link when the file is kept elsewhere; `[image: caption]` when Notion keeps it, since that address expires within the hour |
| Columns, synced blocks, tabs | Nothing of their own: what is inside them is written in their place |
| Anything else | A line naming its kind, `[table_of_contents]`, with its text if it has any |

Text keeps bold, italic, strikethrough, inline code and links. A mention of a page is a link to it. Underline and colour have no mark in Markdown and are dropped.

**Limits.** `max_depth` (10 unless given, 50 at most) is how many levels of nesting are read. `max_requests` (50 unless given, 500 at most) is how many requests are spent: one for the page, and one for every hundred blocks of each list. The top of the page is read first and whole, then what is nested in it, a level at a time, so a limit cuts off detail and not the end of the page. When a limit stopped the reading, `truncated` is set, `truncation` says which limit, and a line such as `[not read: more blocks, the request limit was reached]` stands in the Markdown at each place left unread. The same is done for a nested block whose insides Notion will not give, such as a block synced from a page that was not shared.

A page with many nested blocks takes many requests, and Notion allows about three a second. A throttled request is waited out and sent again when the wait is short; when it is not, the read fails with `RateLimited` and returns nothing, since half a page should not pass for the page.

## Page through a list

A list takes `cursor` and `limit` and returns `items` and `next_cursor`:

```rust
use socketkit::notion::models::Paging;

let mut paging = Paging { cursor: None, limit: Some(100) };
loop {
    let page = notion.users(&connection).list(paging.clone()).await?;
    // use page.items
    let Some(next) = page.next_cursor else { break };
    paging.cursor = Some(next);
}
```

`limit` is from 1 to 100. `next_cursor` is passed back unchanged, and is absent on the last page. `SearchQuery` and `QueryDataSource` carry `cursor` and `limit` themselves. A query returns at most 10,000 rows in all, however it is paged; narrow the filter to reach others.

## Call an operation by name

Every typed method is also an operation, named `notion.<group>.<method>`, with its plain arguments and its options side by side in one JSON object:

```rust
let content = socket.invoke(key, "notion.pages.read".into(), json!({ "page": page_url, "max_depth": 3 })).await?;
let rows = socket.invoke(key, "notion.databases.query".into(), json!({
    "data_source": data_source_id,
    "filter": { "property": "Status", "status": { "equals": "Done" } },
    "sorts": [{ "timestamp": "last_edited_time", "direction": "descending" }],
    "limit": 25
})).await?;
```

Each operation says what it does to the workspace, and a host uses that to decide which calls need a person's approval. The table above gives each one's effect: twelve read, five write, and two, `pages.archive` and `blocks.delete`, are destructive. `search.run` and `databases.query` are reads that Notion offers only as `POST`; they are marked `read`.

**A field the operation does not know is refused** and named, before anything is sent. A misspelt `in_trash` or `filter` would otherwise be dropped in silence.

## Handle errors

| Notion answers | Socket reports |
| --- | --- |
| `401 unauthorized` | `ReconnectRequired` |
| `403 restricted_resource` | `AccessDenied`, with Notion's reason: usually a capability the integration lacks |
| `404 object_not_found` | `NotFound`: "nothing with that id, or it was not shared with this integration" |
| `400 validation_error` and the other `400`s, `409 conflict_error` | `InvalidInput`, with Notion's reason |
| `429 rate_limited`, `529 service_overload` | `RateLimited`, with the wait from `Retry-After` |
| `500`, `502`, `503`, `504` | `Unexpected`, to try later |

Notion allows an integration about three requests a second on most plans, counted over a minute. A throttled request was not carried out, so it is sent again after the wait Notion states, whatever it was, when that wait is short.

A request sent as GET is also retried after a server error. Nothing else is: **when creating, changing, appending or commenting fails with a server error, it may have happened**, and Notion says as much of its `503`. Check before sending it again. The two reads sent as `POST` are not retried after a server error either.

**Deleting a block is the exception today.** The transport still repeats a DELETE after a server error. Trashing twice does no harm, but if the first try did trash the block, the second may be answered "not found". Treat `NotFound` from `blocks.delete` as "it is gone".

## Confirmed against Notion's documentation, and not

Everything here was read from developers.notion.com in October 2026. Nothing was run against a live workspace.

Confirmed:

- `2026-03-11` is the current version, and its three changes: `in_trash` for `archived`, `position` for `after`, and the `meeting_notes` block for `transcription`. The change of `2025-09-03`: data sources.
- Every endpoint above, with its verb: `POST /v1/search`; `GET /v1/pages/{id}`, `GET /v1/pages/{id}/properties/{property}`, `POST /v1/pages`, `PATCH /v1/pages/{id}`; `GET /v1/blocks/{id}`, `GET` and `PATCH /v1/blocks/{id}/children`, `PATCH` and `DELETE /v1/blocks/{id}`; `GET /v1/databases/{id}`, `GET /v1/data_sources/{id}`, `POST /v1/data_sources/{id}/query`; `GET /v1/users` and `/v1/users/{id}`; `GET` and `POST /v1/comments`.
- A search's `query`, `filter` (`property: "object"` with `page` or `data_source`, and `in_trash`) and `sort` (`last_edited_time`), and that it matches titles only and returns pages and data sources.
- A page's fields; that a relation, people, a title or a text is cut short at 25 references; the two shapes the property endpoint answers in.
- A new page's `parent` (`page_id`, `data_source_id`), `properties`, `children` (100 at most), `icon` and `cover`. A page is trashed with `in_trash: true`, and the API cannot delete one for good.
- A block's fields, the kinds of block and what each holds; that a list of children is one level only and `has_children` says whether there is another; appending 100 blocks at most, two levels deep, with the three forms of `position`; that a change replaces each field given; that deleting sets `in_trash` and works on a page too.
- A database returns `data_sources` and no schema; a data source returns the schema; a query's `filter`, `sorts`, `start_cursor` and `page_size`, that it may return data sources among the rows of a wiki, and its 10,000 row limit.
- Listing users leaves out guests; the capabilities and what each allows; that the comment capabilities are off by default.
- Listing comments returns only those not resolved; a comment's target (`parent.page_id`, `parent.block_id` or `discussion_id`) and content (`rich_text` or `markdown`).
- The error codes above, that `object_not_found` also means "not shared", and that `529` is to be treated as `429`. `Retry-After` in whole seconds.
- The rate limit: 180 requests a minute for an integration, 600 on Business and Enterprise plans, and a separate limit for a workspace. The size limits: 100 blocks and 100 runs of text in a list, 2000 characters in a run, 1000 blocks and 500 KB in a request.
- Paging: `start_cursor` and `page_size` in the query of a GET and in the body of a POST, 100 at most, `has_more` and `next_cursor`.

Not confirmed:

- **What Notion answers when an id belongs to another kind of object.** The documentation does not say. Reports of the live service give `400 validation_error` with "Provided ID … is a database, not a page". Lookup does not depend on the words: after any `400` on the page it asks for the database. A typed method passes the `400` on as `InvalidInput` with Notion's message.
- **The `link_to_page` block.** It is not in the block reference today. Socket writes a link from its `page_id` or `database_id`, as earlier versions of the reference describe it, and names the block when it finds neither.
- **That `null` removes an icon or a cover.** The schema allows it; no page says what it does.
- **Whether a request may carry `plain_text` and `href` in a run of text.** They are Notion's to write. Socket sends a run as it was given.
- **The default page size.** One page of the documentation says 100 and another 10. Pass `limit` to choose one.
- **How `filter_properties` is written in a query string.** It is not offered.
- **Whether both parts of a search filter may be sent together.** Socket sends what was given.
- **What a deleted block's answer carries.** Socket returns what Notion sends.
- **The order of a list of comments**, and whether a user's email is absent or `null` without the capability.
- **Refresh tokens** for a public integration. The provider follows the standard flow, unchanged by this work.

One thing the issue that asked for this assumed is no longer so: **Notion now has its own endpoint for a page as Markdown**, `GET /v1/pages/{id}/markdown`. `pages.read` does not use it. It walks the blocks, as asked for, which keeps the limits and the "nothing goes missing" rule in Socket's hands: Notion's endpoint writes bookmarks, embeds and link previews as `<unknown>` tags and uses tags of its own. It is one request where `pages.read` may take dozens, so a method over it is worth adding; it is not here.

## Not supported yet

- **Notion's Markdown endpoints**: reading a page as Markdown in one request, and writing one from Markdown.
- **Restoring from the trash**, **locking a page**, **moving a page**, and **erasing a page's content**.
- **Creating and changing databases and data sources**, and **views**.
- **Choosing which properties come back** (`filter_properties`), and **a query that reaches past 10,000 rows**.
- **Editing and deleting comments**, and **attaching a file to a comment**.
- **File uploads.**
- **Webhooks.**
- **Pacing requests** to stay under the rate limit. Socket waits when Notion says to; it does not slow down beforehand.

Anything Notion offers that has no method here can still be called through the generic request, with the token, retries and error handling applied:

```rust
let response = socket.request(key, RawRequest::get("pages/0123abcd-4567-89ab-cdef-0123456789ab/markdown").with_header("Notion-Version", "2026-03-11")).await?;
```
