//! What the Notion tests share: a local server that answers as Notion does,
//! and ways to read what reached it.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_notion::{Notion, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// The API version every request has to declare.
pub const VERSION: &str = "2026-03-11";

pub const PAGE: &str = "0123abcd-4567-89ab-cdef-0123456789ab";
/// The same page as a person copies it from the address bar.
pub const PAGE_URL: &str = "https://www.notion.so/acme/Roadmap-0123abcd456789abcdef0123456789ab?v=1";
pub const BLOCK: &str = "b10c0000-0000-4000-8000-000000000001";
pub const DATABASE: &str = "da7aba5e-0000-4000-8000-000000000002";
pub const SOURCE: &str = "50c5c000-0000-4000-8000-000000000003";
pub const USER: &str = "a11ce000-0000-4000-8000-000000000004";
pub const COMMENT: &str = "c0ffee00-0000-4000-8000-000000000005";
pub const THREAD: &str = "d15c0000-0000-4000-8000-000000000006";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below Notion's `v1`.
    pub path: String,
    /// Exactly the query parameters that reach Notion.
    pub query: Value,
    /// Exactly the JSON body that reaches Notion; `null` when there is none.
    pub body: Value,
    pub response: Value,
    /// What the operation returns. Checked as a subset, so models may carry more fields.
    pub returns: Value,
}

/// True when every part of `expected` is present in `actual`.
pub fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => e.iter().all(|(k, v)| a.get(k).is_some_and(|av| contains(av, v))),
        (Value::Array(a), Value::Array(e)) => a.len() == e.len() && a.iter().zip(e).all(|(av, ev)| contains(av, ev)),
        _ => actual == expected,
    }
}

pub fn notion_error(status: u16, code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_json(json!({ "object": "error", "status": status, "code": code, "message": message }))
}

pub fn query_of(request: &Request) -> Value {
    Value::Object(
        request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
            .collect(),
    )
}

pub fn body_of(request: &Request) -> Value {
    if request.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&request.body).unwrap()
    }
}

pub async fn notion() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Notion::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, "ntn-good").await;
    (server, socket, key)
}

/// A server that answers every request with `status` and `body`.
pub async fn answering(status: u16, body: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = notion().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(status).set_body_json(body))
        .mount(&server)
        .await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("notion.{name}"), input).await
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "one call to Notion");
    received.remove(0)
}

/// Text as Notion returns it: one run, with what it says beside what it is.
pub fn said(text: &str) -> Value {
    json!([{
        "type": "text", "text": { "content": text, "link": null }, "plain_text": text, "href": null,
        "annotations": { "bold": false, "italic": false, "strikethrough": false, "underline": false, "code": false, "color": "default" }
    }])
}

/// A list as Notion returns one.
pub fn list(kind: &str, results: Value, next_cursor: Option<&str>) -> Value {
    json!({ "object": "list", "results": results, "next_cursor": next_cursor, "has_more": next_cursor.is_some(), "type": kind, kind: {} })
}

/// A row of a data source as Notion returns it.
pub fn page() -> Value {
    json!({
        "object": "page", "id": PAGE,
        "created_time": "2026-10-01T09:00:00.000Z", "last_edited_time": "2026-10-09T08:15:00.000Z",
        "created_by": { "object": "user", "id": USER }, "last_edited_by": { "object": "user", "id": USER },
        "cover": null, "icon": { "type": "emoji", "emoji": "🗺️" },
        "parent": { "type": "data_source_id", "data_source_id": SOURCE, "database_id": DATABASE },
        "in_trash": false, "is_archived": false, "is_locked": false,
        "properties": {
            "Name": { "id": "title", "type": "title", "title": said("Roadmap") },
            "Status": { "id": "%3EfC", "type": "status", "status": { "id": "s-1", "name": "In progress", "color": "blue" } },
            "Tasks": { "id": "rel1", "type": "relation", "relation": [{ "id": BLOCK }], "has_more": true }
        },
        "url": "https://www.notion.so/Roadmap-0123abcd456789abcdef0123456789ab", "public_url": null
    })
}

/// What an operation that returns `page()` must pass on.
pub fn page_returned() -> Value {
    json!({
        "id": PAGE, "last_edited_time": "2026-10-09T08:15:00.000Z", "in_trash": false,
        "created_by": { "id": USER },
        "icon": { "type": "emoji", "emoji": "🗺️" },
        "parent": { "type": "data_source_id", "data_source_id": SOURCE, "database_id": DATABASE },
        "properties": {
            "Name": { "id": "title", "type": "title", "title": [{ "plain_text": "Roadmap" }] },
            "Status": { "id": "%3EfC", "type": "status", "status": { "name": "In progress" } },
            "Tasks": { "type": "relation", "relation": [{ "id": BLOCK }], "has_more": true }
        },
        "url": "https://www.notion.so/Roadmap-0123abcd456789abcdef0123456789ab"
    })
}

/// A block of `kind` holding `held`, as Notion returns it.
pub fn block(id: &str, kind: &str, held: Value, has_children: bool) -> Value {
    json!({
        "object": "block", "id": id, "type": kind, kind: held, "has_children": has_children, "in_trash": false,
        "parent": { "type": "page_id", "page_id": PAGE },
        "created_time": "2026-10-01T09:00:00.000Z", "last_edited_time": "2026-10-09T08:15:00.000Z",
        "created_by": { "object": "user", "id": USER }, "last_edited_by": { "object": "user", "id": USER }
    })
}

pub fn to_do() -> Value {
    block(
        BLOCK,
        "to_do",
        json!({ "rich_text": said("Ship it"), "checked": false, "color": "default" }),
        false,
    )
}

pub fn database() -> Value {
    json!({
        "object": "database", "id": DATABASE, "title": said("Tasks"), "description": [],
        "parent": { "type": "page_id", "page_id": PAGE }, "is_inline": false, "in_trash": false, "is_locked": false,
        "created_time": "2026-10-01T09:00:00.000Z", "last_edited_time": "2026-10-09T08:15:00.000Z",
        "data_sources": [{ "id": SOURCE, "name": "Tasks" }], "icon": null, "cover": null,
        "url": "https://www.notion.so/da7aba5e000040008000000000000002", "public_url": null
    })
}

pub fn data_source() -> Value {
    json!({
        "object": "data_source", "id": SOURCE, "title": said("Tasks"), "description": [],
        "parent": { "type": "database_id", "database_id": DATABASE }, "database_parent": { "type": "page_id", "page_id": PAGE },
        "is_inline": false, "in_trash": false, "created_time": "2026-10-01T09:00:00.000Z", "last_edited_time": "2026-10-09T08:15:00.000Z",
        "created_by": { "object": "user", "id": USER }, "last_edited_by": { "object": "user", "id": USER },
        "properties": {
            "Name": { "id": "title", "name": "Name", "type": "title", "title": {} },
            "Status": { "id": "%3EfC", "name": "Status", "type": "status", "status": { "options": [{ "id": "s-1", "name": "Done", "color": "green" }], "groups": [] } }
        },
        "icon": null, "cover": null, "url": "https://www.notion.so/50c5c000000040008000000000000003", "public_url": null
    })
}

pub fn user() -> Value {
    json!({ "object": "user", "id": USER, "type": "person", "name": "Ada Lovelace", "avatar_url": null,
            "person": { "email": "ada@example.test" } })
}

pub fn comment() -> Value {
    json!({
        "object": "comment", "id": COMMENT, "parent": { "type": "page_id", "page_id": PAGE }, "discussion_id": THREAD,
        "created_time": "2026-10-09T08:15:00.000Z", "last_edited_time": "2026-10-09T08:15:00.000Z",
        "created_by": { "object": "user", "id": USER }, "rich_text": said("Looks good"),
        "display_name": { "type": "integration", "resolved_name": "Acme bot" }
    })
}
