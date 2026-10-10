//! What the Microsoft tests share: a local server that answers as Graph does,
//! and ways to read what reached it.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// The header that asks Graph to write every time in UTC.
pub const IN_UTC: &str = "outlook.timezone=\"UTC\"";

/// The header that asks Graph for the body of a message as plain text, and as HTML.
pub const AS_TEXT: &str = "outlook.body-content-type=\"text\"";
pub const AS_HTML: &str = "outlook.body-content-type=\"html\"";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below Graph's `v1.0`.
    pub path: &'static str,
    /// Exactly the query parameters that reach Graph.
    pub query: Value,
    /// Exactly the JSON body that reaches Graph; `null` when there is none.
    pub body: Value,
    /// Exactly the `Prefer` header that reaches Graph, if any.
    pub prefer: Option<&'static str>,
    pub status: u16,
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

pub fn answer(status: u16, body: &Value) -> ResponseTemplate {
    if body.is_null() {
        ResponseTemplate::new(status)
    } else {
        ResponseTemplate::new(status).set_body_json(body.clone())
    }
}

pub fn graph_error(status: u16, code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({ "error": { "code": code, "message": message } }))
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

pub fn prefer(request: &Request) -> Option<&str> {
    request.headers.get("prefer").map(|value| value.to_str().unwrap())
}

pub async fn microsoft() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Microsoft::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, "eyJ.good").await;
    (server, socket, key)
}

/// A server that answers every request with `status` and `body`.
pub async fn answering(status: u16, body: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(answer(status, &body))
        .mount(&server)
        .await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("microsoft.{name}"), input).await
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "one call to Graph");
    received.remove(0)
}

/// A received message as Graph returns it when asked for the body as text.
pub fn message() -> Value {
    json!({
        "id": "msg-1",
        "conversationId": "conv-1",
        "subject": "Q3 plan",
        "bodyPreview": "Attached is the plan",
        "body": { "contentType": "text", "content": "Attached is the plan for Q3." },
        "from": { "emailAddress": { "name": "Grace Hopper", "address": "grace@contoso.example" } },
        "sender": { "emailAddress": { "name": "Grace Hopper", "address": "grace@contoso.example" } },
        "toRecipients": [{ "emailAddress": { "name": "Ada Lovelace", "address": "ada@contoso.example" } }],
        "ccRecipients": [{ "emailAddress": { "name": "Alan Turing", "address": "alan@contoso.example" } }],
        "bccRecipients": [],
        "replyTo": [],
        "receivedDateTime": "2026-10-09T08:15:00Z",
        "sentDateTime": "2026-10-09T08:14:58Z",
        "isRead": false,
        "isDraft": false,
        "hasAttachments": true,
        "importance": "normal",
        "categories": ["Customer"],
        "flag": { "flagStatus": "notFlagged" },
        "parentFolderId": "folder-inbox",
        "internetMessageId": "<abc@contoso.example>",
        "isDeliveryReceiptRequested": null,
        "webLink": "https://outlook.office365.com/owa/?ItemID=msg-1"
    })
}

/// What an operation that returns `message()` must pass on.
pub fn message_returned() -> Value {
    json!({
        "id": "msg-1",
        "conversationId": "conv-1",
        "subject": "Q3 plan",
        "from": { "emailAddress": { "address": "grace@contoso.example" } },
        "toRecipients": [{ "emailAddress": { "address": "ada@contoso.example" } }],
        "ccRecipients": [{ "emailAddress": { "address": "alan@contoso.example" } }],
        "receivedDateTime": "2026-10-09T08:15:00Z",
        "bodyPreview": "Attached is the plan",
        "body": { "contentType": "text", "content": "Attached is the plan for Q3." },
        "isRead": false,
        "hasAttachments": true,
        "webLink": "https://outlook.office365.com/owa/?ItemID=msg-1"
    })
}

pub fn to(address: &str) -> Value {
    json!([{ "emailAddress": { "address": address } }])
}
