//! What the Attio tests share: a local server that answers as Attio does,
//! ways to read what reached it, and things as Attio's documentation shows them.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_attio::{Attio, provider};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

pub const WORKSPACE: &str = "14beef7a-99f7-4534-a87e-70b564330a4c";
pub const OBJECT: &str = "97052eb9-e65e-443f-a297-f2d9a4a7f795";
pub const RECORD: &str = "bf071e1f-6035-429d-b874-d83ea64ea13b";
pub const COMPANY: &str = "99a03ff3-0435-47da-95cc-76b2caeb4dab";
pub const LIST: &str = "33ebdbe9-e529-47c9-b894-0ba25e9c15c0";
pub const ENTRY: &str = "2e6e29ea-c4e0-4f44-842d-78a891f8c156";
pub const ATTRIBUTE: &str = "41252299-f8c7-4b5e-99c9-4ff8321d2f96";
pub const NOTE: &str = "ff3f3bd4-40f4-4f80-8187-cd02385af424";
pub const TASK: &str = "649e34f4-c39a-4f4d-99ef-48a36bef8f04";
pub const THREAD: &str = "a649e4d9-435c-43fb-83ba-847b4876f27a";
pub const COMMENT: &str = "aa1dc1d9-93ac-4c6c-987e-16b6eea9aab2";
pub const MEMBER: &str = "50cf242c-7fa3-4cad-87d0-75b1af71c57b";
pub const MEETING: &str = "cb59ab17-ad15-460c-a126-0715617c0853";
pub const RECORDING: &str = "e8f2a3b7-9b4d-4c5e-8a1f-3d7b2c5e8f9a";
pub const THEN: &str = "2023-01-01T15:00:00.000000000Z";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below Attio's `v2`.
    pub path: String,
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

/// An error as Attio writes one.
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
    let (socket, key) = connect(build(point_at(provider(), &server)), "at-good").await;
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

fn by_member() -> Value {
    json!({ "type": "workspace-member", "id": MEMBER })
}

/// One value of an attribute as Attio writes it: when it became active,
/// when it stopped being so, who set it, its type, and the type's own fields.
pub fn value(kind: &str, from: &str, until: Option<&str>, fields: Value) -> Value {
    let mut value = json!({
        "active_from": from,
        "active_until": until,
        "created_by_actor": by_member(),
        "attribute_type": kind,
    });
    value
        .as_object_mut()
        .unwrap()
        .extend(fields.as_object().unwrap().clone());
    value
}

/// A value that is active now.
pub fn active(kind: &str, fields: Value) -> Value {
    value(kind, THEN, None, fields)
}

fn email(address: &str) -> Value {
    let (local, domain) = address.split_once('@').unwrap();
    active(
        "email-address",
        json!({ "original_email_address": address, "email_address": address, "email_domain": domain,
                "email_root_domain": domain, "email_local_specifier": local }),
    )
}

/// What `GET /v2/self` answers for a workspace's own token.
pub fn token() -> Value {
    json!({
        "active": true,
        "scope": "record_permission:read object_configuration:read note:read-write",
        "client_id": "c7f2a9d1-0000-4000-8000-000000000001",
        "token_type": "Bearer",
        "exp": null,
        "iat": 1_700_000_000,
        "sub": WORKSPACE,
        "aud": "c7f2a9d1-0000-4000-8000-000000000001",
        "iss": "attio.com",
        "token_level": "workspace",
        "authorized_by_workspace_member_id": MEMBER,
        "workspace_id": WORKSPACE,
        "workspace_name": "Acme",
        "workspace_slug": "acme",
        "workspace_logo_url": null
    })
}

pub fn object() -> Value {
    json!({ "id": { "workspace_id": WORKSPACE, "object_id": OBJECT }, "api_slug": "people",
            "singular_noun": "Person", "plural_noun": "People", "created_at": "2022-11-21T13:22:49.061281000Z" })
}

pub fn attribute() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "object_id": OBJECT, "attribute_id": ATTRIBUTE },
        "title": "Company", "description": "Where the person works", "api_slug": "company", "type": "record-reference",
        "is_system_attribute": false, "is_writable": true, "is_required": false, "is_unique": false,
        "is_multiselect": false, "is_default_value_enabled": false, "is_archived": false,
        "default_value": null,
        "relationship": { "id": { "workspace_id": WORKSPACE, "object_id": OBJECT, "attribute_id": ATTRIBUTE },
                          "object_slug": "companies", "title": "Team members", "api_slug": "team", "is_multiselect": true },
        "created_at": "2021-11-21T13:22:49.061Z",
        "config": { "currency": { "default_currency_code": null, "display_type": null },
                    "record_reference": { "allowed_object_ids": [COMPANY] } }
    })
}

/// A person with a name, two email addresses, no job title and a company.
pub fn record() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "object_id": OBJECT, "record_id": RECORD },
        "created_at": "2022-11-21T13:22:49.061281000Z",
        "web_url": format!("https://app.attio.com/acme/person/{RECORD}"),
        "values": {
            "name": [active("personal-name", json!({ "first_name": "Ada", "last_name": "Lovelace", "full_name": "Ada Lovelace" }))],
            "email_addresses": [email("ada@example.com"), email("countess@example.org")],
            "job_title": [],
            "company": [active("record-reference", json!({ "target_object": "companies", "target_record_id": COMPANY }))]
        }
    })
}

/// Each attribute of `record()` as it is now.
pub fn record_current() -> Value {
    json!({
        "name": "Ada Lovelace",
        "email_addresses": ["ada@example.com", "countess@example.org"],
        "job_title": null,
        "company": { "target_object": "companies", "target_record_id": COMPANY }
    })
}

pub fn list() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "list_id": LIST },
        "api_slug": "enterprise_sales", "name": "Enterprise sales", "parent_object": ["people"],
        "workspace_access": "read-and-write",
        "workspace_member_access": [{ "workspace_member_id": MEMBER, "level": "full-access" }],
        "created_by_actor": by_member(), "created_at": "2022-11-21T13:22:49.061281000Z"
    })
}

fn stage(title: &str) -> Value {
    json!({ "status": { "id": { "workspace_id": WORKSPACE, "object_id": LIST, "attribute_id": ATTRIBUTE, "status_id": "11f07f01-c10f-4e05-a522-33e050bc52ee" },
                        "title": title, "is_archived": false, "celebration_enabled": true, "target_time_in_status": null } })
}

/// An entry of a sales list: a stage and a deal value.
pub fn entry() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "list_id": LIST, "entry_id": ENTRY },
        "parent_record_id": RECORD, "parent_object": "people", "created_at": "2022-11-21T13:22:49.061281000Z",
        "entry_values": {
            "stage": [active("status", stage("Won"))],
            "deal_value": [active("currency", json!({ "currency_value": 12000, "currency_code": "USD" }))]
        }
    })
}

pub fn entry_current() -> Value {
    json!({ "stage": "Won", "deal_value": { "currency_value": 12000, "currency_code": "USD" } })
}

pub fn note() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "note_id": NOTE },
        "parent_object": "people", "parent_record_id": RECORD, "title": "Initial call", "meeting_id": null,
        "content_plaintext": "Introduction\nBudget agreed", "content_markdown": "# Introduction\nBudget agreed",
        "tags": [{ "type": "workspace-member", "workspace_member_id": MEMBER }, { "type": "record", "object": "people", "record_id": RECORD }],
        "created_by_actor": by_member(), "created_at": "2022-11-21T13:22:49.061281000Z"
    })
}

pub fn task() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "task_id": TASK },
        "content_plaintext": "Follow up on the contract", "deadline_at": "2026-11-01T15:00:00.000000000Z",
        "is_completed": false, "completed_at": null,
        "linked_records": [{ "target_object_id": "people", "target_record_id": RECORD }],
        "assignees": [{ "referenced_actor_type": "workspace-member", "referenced_actor_id": MEMBER }],
        "created_by_actor": by_member(), "created_at": "2022-11-21T13:22:49.061281000Z"
    })
}

pub fn comment(text: &str) -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "comment_id": COMMENT }, "thread_id": THREAD, "content_plaintext": text,
        "entry": null, "record": { "record_id": RECORD, "object_id": OBJECT },
        "resolved_at": null, "resolved_by": { "id": null, "type": null },
        "created_at": THEN, "author": by_member()
    })
}

/// A thread of two comments on `record()`.
pub fn thread() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "thread_id": THREAD }, "created_at": THEN,
        "comments": [comment("Let's close this deal."), comment("Agreed.")]
    })
}

pub fn member() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "workspace_member_id": MEMBER },
        "first_name": "Susan", "last_name": "Kare", "avatar_url": null, "email_address": "susan@example.com",
        "created_at": "2022-11-21T13:22:49.061281000Z", "access_level": "admin"
    })
}

pub fn meeting() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "meeting_id": MEETING },
        "title": "Renewal call", "description": "Terms for next year", "is_all_day": false,
        "start": { "datetime": "2026-10-12T16:00:00.000000000Z", "timezone": "Europe/London" },
        "end": { "datetime": "2026-10-12T17:00:00.000000000Z", "timezone": "Europe/London" },
        "participants": [{ "status": "accepted", "is_organizer": true, "email_address": "ada@example.com", "name": "Ada Lovelace" }],
        "linked_records": [{ "object_slug": "people", "object_id": OBJECT, "record_id": RECORD }],
        "created_at": THEN, "created_by_actor": { "type": "system", "id": null }
    })
}

/// A recording as a list returns it.
pub fn recording_row() -> Value {
    json!({
        "id": { "workspace_id": WORKSPACE, "meeting_id": MEETING, "call_recording_id": RECORDING },
        "status": "completed", "web_url": format!("https://app.attio.com/acme/calls/{MEETING}/{RECORDING}"),
        "created_by_actor": by_member(), "created_at": THEN
    })
}

/// A recording as Attio returns one by itself, with the transcript in Attio's own shape.
pub fn recording() -> Value {
    let mut recording = recording_row();
    recording["video_url"] = json!(null);
    recording["transcript"] = json!({
        "segments": [
            { "speech": "Hello,", "start_time": 0.51, "end_time": 0.81, "speaker": { "name": "Alex Bell" } },
            { "speech": "Mr Watson, come here.", "start_time": 0.81, "end_time": 2.11, "speaker": { "name": "Alex Bell" } },
            { "speech": "I'm here.", "start_time": 4.21, "end_time": 4.91, "speaker": { "name": "Tom Watson" } }
        ],
        "raw_transcript": "[00:00] Alex Bell: Hello, Mr Watson, come here.\n[00:04] Tom Watson: I'm here."
    });
    recording
}
