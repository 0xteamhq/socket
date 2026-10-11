//! What the Google tests share: a local server that answers as Google does,
//! and ways to read what reached it.

#![allow(dead_code)] // Each test file uses its own part of this.

// ── gmail: fixtures ──
pub mod gmail;

// ── calendar: fixtures ──
pub mod calendar;

// ── meet: fixtures ──
pub mod meet;

// ── drive: fixtures ──
pub mod drive;

// ── docs and sheets: fixtures ──
pub mod docs;

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// The token every test connection holds.
pub const TOKEN: &str = "ya29.good";

/// One operation's expected behaviour: the request that must reach Google,
/// and what the operation returns for Google's answer.
///
/// Built with [`Case::new`] and the methods after it, so a case names only
/// what it needs: `Case::new("gmail_labels.get", json!({ "label": "INBOX" }),
/// "GET", "/gmail/v1/users/me/labels/INBOX").answers(200, label()).returns(…)`.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The whole path as it reaches Google, from the first `/`.
    pub path: String,
    /// Exactly the query parameters that reach Google. A parameter sent more
    /// than once is a list of its values, in the order sent.
    pub query: Value,
    /// Exactly the JSON body that reaches Google; `null` when there is none.
    pub body: Value,
    pub status: u16,
    pub response: Value,
    /// When Google answers with text and not JSON, the content type it
    /// answers with. `response` is then a string and is sent as that text.
    pub text: Option<&'static str>,
    /// What the operation returns. Checked as a subset, so models may carry more fields.
    pub returns: Value,
    /// The other requests the operation makes, and what each is answered
    /// with: verb, path, answer. See [`Case::also`].
    pub also: Vec<(&'static str, String, Value)>,
}

impl Case {
    /// An operation that sends `verb` to `path` with no query and no body,
    /// is answered `200` with `{}`, and returns anything.
    pub fn new(name: &'static str, input: Value, verb: &'static str, path: impl Into<String>) -> Self {
        Self {
            name,
            input,
            verb,
            path: path.into(),
            query: json!({}),
            body: Value::Null,
            status: 200,
            response: json!({}),
            text: None,
            returns: json!({}),
            also: Vec::new(),
        }
    }

    pub fn query(mut self, query: Value) -> Self {
        self.query = query;
        self
    }

    pub fn body(mut self, body: Value) -> Self {
        self.body = body;
        self
    }

    /// What Google answers. A `null` response is an answer with no content.
    pub fn answers(mut self, status: u16, response: Value) -> Self {
        self.status = status;
        self.response = response;
        self
    }

    /// Google answers with `text`, sent with this content type.
    pub fn answers_text(mut self, content_type: &'static str, text: &str) -> Self {
        self.text = Some(content_type);
        self.response = Value::String(text.to_owned());
        self
    }

    pub fn returns(mut self, returns: Value) -> Self {
        self.returns = returns;
        self
    }

    /// Another request the operation makes besides the one this case
    /// describes, such as the read a reply makes of the message it answers.
    /// It is answered `200` with `response`. What it must carry is for a
    /// test of its own; here it only has to be made, once.
    pub fn also(mut self, verb: &'static str, path: impl Into<String>, response: Value) -> Self {
        self.also.push((verb, path.into(), response));
        self
    }
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

/// An error as Google's APIs write one. `reason` is the word Google gives
/// for it, such as `rateLimitExceeded` or `notFound`.
pub fn google_error(status: u16, reason: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "error": {
            "code": status,
            "message": message,
            "errors": [{ "domain": "global", "reason": reason, "message": message }]
        }
    }))
}

/// The query of a request. A parameter sent more than once is a list of its
/// values, in the order sent.
pub fn query_of(request: &Request) -> Value {
    let mut query = serde_json::Map::new();
    for (name, value) in request.url.query_pairs() {
        let value = Value::String(value.into_owned());
        match query.get_mut(name.as_ref()) {
            None => {
                query.insert(name.into_owned(), value);
            }
            Some(Value::Array(values)) => values.push(value),
            Some(first) => *first = Value::Array(vec![first.take(), value]),
        }
    }
    Value::Object(query)
}

pub fn body_of(request: &Request) -> Value {
    if request.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&request.body).unwrap()
    }
}

pub async fn google() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Google::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, TOKEN).await;
    (server, socket, key)
}

/// A server that answers every request with `status` and `body`.
pub async fn answering(status: u16, body: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = google().await;
    Mock::given(any())
        .respond_with(answer(status, &body))
        .mount(&server)
        .await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("google.{name}"), input).await
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "one call to Google");
    received.remove(0)
}
