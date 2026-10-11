//! What the Attio tests share: a local server that answers as Attio does,
//! ways to read what reached it, and things as Attio returns them.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_attio::{Attio, provider};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// The token every test connection carries.
pub const TOKEN: &str = "at-good";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below Attio's `v2`.
    pub path: &'static str,
    /// Exactly the query parameters that reach Attio.
    pub query: Value,
    /// Exactly the JSON body that reaches Attio; `null` when there is none.
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

pub fn answer(status: u16, body: &Value) -> ResponseTemplate {
    if body.is_null() {
        ResponseTemplate::new(status)
    } else {
        ResponseTemplate::new(status).set_body_json(body.clone())
    }
}

/// A refusal in the shape Attio gives every one of them.
pub fn attio_error(status: u16, kind: &str, code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status)
        .set_body_json(json!({ "status_code": status, "type": kind, "code": code, "message": message }))
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

pub fn build(spec: socketkit_core::ProviderSpec) -> Arc<dyn Integration> {
    Arc::new(Attio::with_spec(spec))
}

pub async fn attio() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let (socket, key) = connect(build(point_at(provider(), &server)), TOKEN).await;
    (server, socket, key)
}

/// A server that answers every request with `status` and `body`.
pub async fn answering(status: u16, body: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(answer(status, &body))
        .mount(&server)
        .await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("attio.{name}"), input).await
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "one call to Attio");
    received.remove(0)
}

const SINCE: &str = "2026-10-01T09:00:00.000000000Z";

/// One value as Attio writes it: what it holds, and the bookkeeping around it.
pub fn value(kind: &str, holds: Value) -> Value {
    let mut value = json!({
        "active_from": SINCE,
        "active_until": null,
        "created_by_actor": { "type": "workspace-member", "id": "mem-1" },
        "attribute_type": kind,
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(holds.as_object().unwrap().clone());
    value
}

pub fn object() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "object_id": "obj-people" },
        "api_slug": "people", "singular_noun": "Person", "plural_noun": "People", "created_at": SINCE
    })
}

pub fn attribute() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "object_id": "obj-people", "attribute_id": "attr-email" },
        "title": "Email addresses", "description": null, "api_slug": "email_addresses", "type": "email-address",
        "is_system_attribute": true, "is_writable": true, "is_required": false, "is_unique": true,
        "is_multiselect": true, "is_default_value_enabled": false, "is_archived": false,
        "default_value": null, "relationship": null, "created_at": SINCE,
        "config": { "currency": { "default_currency_code": null, "display_type": null }, "record_reference": { "allowed_object_ids": null } }
    })
}

/// A person as Attio returns one.
pub fn record() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "object_id": "obj-people", "record_id": "rec-1" },
        "created_at": SINCE,
        "web_url": "https://app.attio.com/acme/person/rec-1",
        "values": {
            "name": [value("personal-name", json!({ "first_name": "Ada", "last_name": "Lovelace", "full_name": "Ada Lovelace" }))],
            "email_addresses": [value("email-address", json!({
                "original_email_address": "ada@example.test", "email_address": "ada@example.test",
                "email_domain": "example.test", "email_root_domain": "example.test", "email_local_specifier": "ada"
            }))],
            "company": [value("record-reference", json!({ "target_object": "companies", "target_record_id": "rec-2" }))],
            "job_title": []
        }
    })
}

/// What an operation that returns `record()` must pass on: Attio's own
/// shape, and beside it what each attribute holds now.
pub fn record_returned() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "object_id": "obj-people", "record_id": "rec-1" },
        "web_url": "https://app.attio.com/acme/person/rec-1",
        "values": {
            "name": [{ "attribute_type": "personal-name", "full_name": "Ada Lovelace", "active_from": SINCE, "active_until": null,
                       "created_by_actor": { "type": "workspace-member", "id": "mem-1" } }],
            "email_addresses": [{ "attribute_type": "email-address", "email_address": "ada@example.test" }],
            "company": [{ "target_object": "companies", "target_record_id": "rec-2" }],
            "job_title": []
        },
        "current": {
            "name": "Ada Lovelace",
            "email_addresses": "ada@example.test",
            "company": { "target_object": "companies", "target_record_id": "rec-2" },
            "job_title": null
        }
    })
}

pub fn list() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "list_id": "list-1" },
        "api_slug": "sales", "name": "Sales", "parent_object": ["companies"],
        "workspace_access": "read-and-write",
        "workspace_member_access": [{ "workspace_member_id": "mem-1", "level": "full-access" }],
        "created_by_actor": { "type": "workspace-member", "id": "mem-1" }, "created_at": SINCE
    })
}

pub fn entry() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "list_id": "list-1", "entry_id": "ent-1" },
        "parent_record_id": "rec-2", "parent_object": "companies", "created_at": SINCE,
        "entry_values": {
            "stage": [value("status", json!({ "status": {
                "id": { "workspace_id": "ws-1", "object_id": "list-1", "attribute_id": "attr-stage", "status_id": "st-1" },
                "title": "In progress", "is_archived": false, "celebration_enabled": false, "target_time_in_status": null
            } }))],
            "value": [value("currency", json!({ "currency_value": 12000.0, "currency_code": "USD" }))]
        }
    })
}

pub fn entry_returned() -> Value {
    json!({
        "id": { "list_id": "list-1", "entry_id": "ent-1" },
        "parent_record_id": "rec-2", "parent_object": "companies",
        "entry_values": { "stage": [{ "attribute_type": "status", "status": { "title": "In progress" } }] },
        "current": { "stage": "In progress", "value": 12000.0 }
    })
}

pub fn note() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "note_id": "note-1" },
        "parent_object": "people", "parent_record_id": "rec-1", "title": "Call", "meeting_id": null,
        "content_plaintext": "Asked about pricing.", "content_markdown": "Asked about **pricing**.",
        "tags": [{ "type": "workspace-member", "workspace_member_id": "mem-1" }],
        "created_by_actor": { "type": "workspace-member", "id": "mem-1" }, "created_at": SINCE
    })
}

pub fn task() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "task_id": "task-1" },
        "content_plaintext": "Send the contract", "deadline_at": "2026-10-20T17:00:00.000000000Z",
        "is_completed": false, "completed_at": null,
        "linked_records": [{ "target_object_id": "people", "target_record_id": "rec-1" }],
        "assignees": [{ "referenced_actor_type": "workspace-member", "referenced_actor_id": "mem-1" }],
        "created_by_actor": { "type": "workspace-member", "id": "mem-1" }, "created_at": SINCE
    })
}

pub fn comment() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "comment_id": "com-1" },
        "thread_id": "thr-1", "content_plaintext": "Looks good",
        "entry": null, "record": { "record_id": "rec-1", "object_id": "obj-people" },
        "resolved_at": null, "resolved_by": null, "created_at": SINCE,
        "author": { "type": "workspace-member", "id": "mem-1" }
    })
}

pub fn thread() -> Value {
    json!({ "id": { "workspace_id": "ws-1", "thread_id": "thr-1" }, "created_at": SINCE, "comments": [comment()] })
}

pub fn member() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "workspace_member_id": "mem-1" },
        "first_name": "Grace", "last_name": "Hopper", "avatar_url": null,
        "email_address": "grace@example.test", "created_at": SINCE, "access_level": "admin"
    })
}

pub fn meeting() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "meeting_id": "meet-1" },
        "title": "Pricing review", "description": "Quarterly", "is_all_day": false,
        "start": { "datetime": "2026-10-12T16:00:00.000Z", "timezone": "Europe/London" },
        "end": { "datetime": "2026-10-12T17:00:00.000Z", "timezone": "Europe/London" },
        "participants": [{ "status": "accepted", "is_organizer": true, "email_address": "grace@example.test", "name": "Grace Hopper" }],
        "linked_records": [{ "object_slug": "people", "object_id": "obj-people", "record_id": "rec-1" }],
        "created_at": SINCE, "created_by_actor": { "type": "system", "id": null }
    })
}

/// A recording as a list carries it: without what was said.
pub fn call_recording() -> Value {
    json!({
        "id": { "workspace_id": "ws-1", "meeting_id": "meet-1", "call_recording_id": "cr-1" },
        "status": "completed", "web_url": "https://app.attio.com/acme/calls/meet-1/cr-1",
        "created_by_actor": { "type": "workspace-member", "id": "mem-1" }, "created_at": SINCE
    })
}

/// A recording read on its own, with what was said.
pub fn call_recording_with_transcript() -> Value {
    let mut recording = call_recording();
    recording["video_url"] = json!("https://files.attio.example/cr-1.mp4");
    recording["transcript"] = json!({
        "segments": [
            { "speech": "Hello,", "start_time": 0.51, "end_time": 0.81, "speaker": { "name": "Alex Bell" } },
            { "speech": "I'm here.", "start_time": 4.21, "end_time": 4.91, "speaker": { "name": "Tom Watson" } }
        ],
        "raw_transcript": "[00:00] Alex Bell: Hello,\n[00:04] Tom Watson: I'm here."
    });
    recording
}
