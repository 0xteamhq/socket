//! What the Pipedrive tests share: a local server that answers as Pipedrive
//! does, records as it writes them, and ways to read what reached the server.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_pipedrive::{Pipedrive, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// The access token every test connection is stored with.
pub const TOKEN: &str = "v1u:oauth-access";

/// A lead's id, and the keys of two custom fields.
pub const LEAD: &str = "adf21080-0e10-11eb-879b-05d71fb426ec";
pub const INDUSTRY: &str = "4d1d7a5b1b5a2c5b6a3e9f8d7c6b5a4f3e2d1c0b";
pub const BUDGET: &str = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below `/api`, which says which version of the API is called.
    pub path: &'static str,
    /// Exactly the query parameters that reach Pipedrive.
    pub query: Value,
    /// Exactly the JSON body that reaches Pipedrive; `null` when there is none.
    pub body: Value,
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

/// Pipedrive's answer with one record, or anything else, under `data`.
pub fn data(data: Value) -> Value {
    json!({ "success": true, "data": data })
}

/// One page of a version 2 list.
pub fn page(rows: Value, next_cursor: Option<&str>) -> Value {
    json!({ "success": true, "data": rows, "additional_data": { "next_cursor": next_cursor } })
}

/// One page of a version 2 search.
pub fn found(items: Value, next_cursor: Option<&str>) -> Value {
    json!({ "success": true, "data": { "items": items }, "additional_data": { "next_cursor": next_cursor } })
}

/// One page of a version 1 list. `next_start` is where the next page starts, if there is one.
pub fn offset_page(rows: Value, start: u64, limit: u64, next_start: Option<u64>) -> Value {
    let mut pagination = json!({ "start": start, "limit": limit, "more_items_in_collection": next_start.is_some() });
    if let Some(next) = next_start {
        pagination["next_start"] = json!(next);
    }
    json!({ "success": true, "data": rows, "additional_data": { "pagination": pagination } })
}

/// Pipedrive's error body, as version 1 writes it.
pub fn failure(status: u16, error: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "success": false, "error": error, "error_info": "Please check developers.pipedrive.com",
        "data": null, "additional_data": null
    }))
}

pub fn ok(body: Value) -> ResponseTemplate {
    ResponseTemplate::new(200).set_body_json(body)
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

/// A `Socket` with the OAuth definition aimed at a local server, and a
/// connection stored for it.
pub async fn pipedrive() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Pipedrive::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, TOKEN).await;
    (server, socket, key)
}

/// A server that answers every request with `response`.
pub async fn answering(response: ResponseTemplate) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = pipedrive().await;
    Mock::given(any()).respond_with(response).mount(&server).await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("pipedrive.{name}"), input).await
}

/// Every request the server received.
pub async fn requests(server: &MockServer) -> Vec<Request> {
    server.received_requests().await.unwrap()
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = requests(server).await;
    assert_eq!(received.len(), 1, "one call to Pipedrive");
    received.remove(0)
}

/// A deal as version 2 returns it, with two custom fields.
pub fn deal() -> Value {
    json!({
        "id": 42, "title": "Acme renewal", "owner_id": 7, "person_id": 11, "org_id": 5, "pipeline_id": 1, "stage_id": 3,
        "value": 12000.5, "currency": "EUR", "status": "open", "probability": null, "expected_close_date": "2026-11-30",
        "add_time": "2026-09-01T08:00:00Z", "update_time": "2026-10-09T10:15:00Z", "stage_change_time": "2026-10-01T09:00:00Z",
        "close_time": null, "won_time": null, "lost_time": null, "lost_reason": null, "visible_to": 3,
        "is_deleted": false, "is_archived": false, "label_ids": [2, 9], "origin": "ManuallyCreated", "origin_id": null,
        "channel": null, "channel_id": null, "arr": null, "mrr": null, "acv": null,
        "custom_fields": { INDUSTRY: 12, BUDGET: { "value": 5000, "currency": "EUR" } }
    })
}

/// What an operation that returns `deal()` whole must pass on.
pub fn deal_returned() -> Value {
    json!({
        "id": 42, "title": "Acme renewal", "owner_id": 7, "person_id": 11, "org_id": 5, "pipeline_id": 1, "stage_id": 3,
        "value": 12000.5, "currency": "EUR", "status": "open", "expected_close_date": "2026-11-30", "label_ids": [2, 9],
        "custom_fields": { INDUSTRY: 12, BUDGET: { "value": 5000, "currency": "EUR" } }
    })
}

pub fn person() -> Value {
    json!({
        "id": 11, "name": "Grace Hopper", "first_name": "Grace", "last_name": "Hopper", "owner_id": 7, "org_id": 5,
        "add_time": "2026-08-01T08:00:00Z", "update_time": null,
        "emails": [{ "value": "grace@acme.example", "primary": true, "label": "work" }],
        "phones": [{ "value": "+1 555 0100", "primary": true, "label": "mobile" }],
        "is_deleted": false, "visible_to": 3, "label_ids": [], "picture_id": null, "postal_address": null,
        "notes": null, "im": [], "birthday": null, "job_title": "CTO",
        "custom_fields": { INDUSTRY: 13 }
    })
}

pub fn organization() -> Value {
    json!({
        "id": 5, "name": "Acme Ltd", "owner_id": 7, "add_time": "2026-07-01T08:00:00Z", "update_time": "2026-10-01T08:00:00Z",
        "is_deleted": false, "visible_to": 3, "label_ids": [4],
        "address": { "value": "1 Main St, Springfield", "country": "US", "locality": "Springfield", "postal_code": "12345",
                     "admin_area_level_1": null, "admin_area_level_2": null, "sublocality": null, "route": "Main St", "street_number": "1", "subpremise": null },
        "website": "https://acme.example", "linkedin": null, "industry": 12, "annual_revenue": 2500000, "employee_count": 40,
        "custom_fields": { BUDGET: null }
    })
}

/// A lead as version 1 returns it: custom fields beside its own, under their keys.
pub fn lead() -> Value {
    json!({
        "id": LEAD, "title": "Jane Doe lead", "owner_id": 7, "creator_id": 7, "label_ids": ["f08b42a0-4e75-11ea-9643-03698ef1cfd6"],
        "person_id": 11, "organization_id": null, "source_name": "API", "origin": "API", "origin_id": null, "channel": null, "channel_id": null,
        "is_archived": false, "was_seen": false, "value": { "amount": 999, "currency": "USD" }, "expected_close_date": null,
        "next_activity_id": 1, "add_time": "2026-09-30T20:49:35.397Z", "update_time": "2026-09-30T20:49:35.397Z", "visible_to": "3",
        "cc_email": "company+1+leadntPaYKA5QRxXkh6WMNHiGh@dev.pipedrivemail.com",
        INDUSTRY: 12
    })
}

pub fn activity() -> Value {
    json!({
        "id": 8, "subject": "Renewal call", "type": "call", "owner_id": 7, "creator_user_id": 7, "is_deleted": false,
        "add_time": "2026-10-08T08:00:00Z", "update_time": "2026-10-09T08:00:00Z", "deal_id": 42, "lead_id": null, "person_id": 11, "org_id": 5,
        "project_id": null, "due_date": "2026-10-09", "due_time": "10:00", "duration": "00:30", "busy": true, "done": true,
        "marked_as_done_time": "2026-10-09T10:31:00Z", "location": null, "participants": [{ "person_id": 11, "primary": true }],
        "attendees": [{ "email": "grace@acme.example", "name": "Grace Hopper", "status": "accepted", "is_organizer": false, "person_id": 11, "user_id": null }],
        "conference_meeting_client": null, "conference_meeting_url": null, "conference_meeting_id": null,
        "public_description": "Quarterly renewal", "priority": 263, "note": "<p>They want a two-year term.</p>"
    })
}

/// A note as version 1 returns it.
pub fn note(content: &str) -> Value {
    json!({
        "id": 3, "active_flag": true, "add_time": "2026-10-09 10:40:00", "update_time": "2026-10-09 10:40:00", "content": content,
        "deal_id": 42, "lead_id": null, "person_id": null, "org_id": null, "project_id": null, "task_id": null, "user_id": 7, "last_update_user_id": null,
        "deal": { "title": "Acme renewal" }, "person": null, "organization": null, "project": null, "task": null,
        "user": { "email": "ada@example.test", "name": "Ada Lovelace", "icon_url": null, "is_you": true },
        "pinned_to_deal_flag": false, "pinned_to_person_flag": false, "pinned_to_organization_flag": false, "pinned_to_project_flag": false, "pinned_to_task_flag": false
    })
}

pub fn field() -> Value {
    json!({
        "field_name": "Industry", "field_code": INDUSTRY, "field_type": "enum", "is_custom_field": true, "is_optional_response_field": false,
        "description": null, "subfields": null,
        "options": [{ "id": 12, "label": "Software", "color": null, "add_time": "2026-01-01T00:00:00Z", "update_time": null }, { "id": 13, "label": "Retail", "color": "blue" }]
    })
}

/// The signed-in user as version 1 returns it, with their company.
pub fn me() -> Value {
    json!({
        "id": 7, "name": "Ada Lovelace", "email": "ada@example.test", "phone": null, "lang": 1, "locale": "en_US", "default_currency": "EUR",
        "timezone_name": "Europe/London", "timezone_offset": "+01:00", "activated": true, "active_flag": true, "is_admin": 1, "is_you": true, "is_deleted": false,
        "role_id": 1, "icon_url": null, "last_login": "2026-10-09 07:00:00", "created": "2025-01-05 09:00:00", "modified": "2026-10-09 07:00:00",
        "has_created_company": true, "access": [{ "app": "sales", "admin": true, "permission_set_id": "62cc4d7f" }],
        "company_id": 1001, "company_name": "Acme Ltd", "company_domain": "acme", "company_country": "GB", "company_industry": "Software",
        "language": { "language_code": "en", "country_code": "US" }
    })
}

/// A deal as a search finds it.
pub fn deal_hit() -> Value {
    json!({ "result_score": 1.22, "item": {
        "id": 42, "type": "deal", "title": "Acme renewal", "value": 12000.5, "currency": "EUR", "status": "open", "visible_to": 3,
        "owner": { "id": 7 }, "stage": { "id": 3, "name": "Negotiation" }, "person": { "id": 11, "name": "Grace Hopper" },
        "organization": { "id": 5, "name": "Acme Ltd", "address": null }, "custom_fields": [], "notes": ["They want a two-year term."], "is_archived": false
    } })
}

/// A lead as a search finds it: its id is a UUID.
pub fn lead_hit() -> Value {
    json!({ "result_score": 0.29, "item": {
        "id": LEAD, "type": "lead", "title": "Jane Doe lead", "owner": { "id": 7 }, "person": { "id": 11, "name": "Grace Hopper" }, "organization": null,
        "phones": [], "emails": ["grace@acme.example"], "custom_fields": [], "notes": [], "value": 999, "currency": "USD", "visible_to": 3, "is_archived": false
    } })
}
