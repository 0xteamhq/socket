//! What the HubSpot tests share: a local server that answers as HubSpot's
//! API does, and ways to read what reached it.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_hubspot::{HubSpot, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// The token every test connection holds.
pub const TOKEN: &str = "pat-na1-good";

/// Where each area of the API lives, at the version this crate calls.
pub const OBJECTS: &str = "/crm/objects/2026-09";
pub const PROPERTIES: &str = "/crm/properties/2026-09";
pub const PIPELINES: &str = "/crm/pipelines/2026-09";
pub const OWNERS: &str = "/crm/owners/2026-09";
pub const ACCOUNT: &str = "/account-info/2026-09/details";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The whole path that reaches HubSpot.
    pub path: String,
    /// Exactly the query parameters that reach HubSpot.
    pub query: Value,
    /// Exactly the JSON body that reaches HubSpot; `null` when there is none.
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

pub fn answer(status: u16, body: &Value) -> ResponseTemplate {
    if body.is_null() {
        ResponseTemplate::new(status)
    } else {
        ResponseTemplate::new(status).set_body_json(body.clone())
    }
}

/// An error as HubSpot's CRM writes one.
pub fn hubspot_error(status: u16, category: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "status": "error",
        "message": message,
        "correlationId": "a43683b0-5717-4ceb-80b4-104d02915d8c",
        "category": category
    }))
}

/// A 429 as HubSpot writes one, naming the limit that was reached.
pub fn rate_limited(policy: &str) -> ResponseTemplate {
    ResponseTemplate::new(429).set_body_json(json!({
        "status": "error",
        "message": "You have reached your limit.",
        "errorType": "RATE_LIMIT",
        "correlationId": "c033cdaa-2c40-4a64-ae48-b4cec88dad24",
        "policyName": policy,
        "requestId": "3d3e35b7-0dae-4b9f-a6e3-9c230cbcf8dd"
    }))
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

pub async fn hubspot() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(HubSpot::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, TOKEN).await;
    (server, socket, key)
}

/// A server that answers every request with `status` and `body`.
pub async fn answering(status: u16, body: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(answer(status, &body))
        .mount(&server)
        .await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("hubspot.{name}"), input).await
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "one call to HubSpot");
    received.remove(0)
}

/// A contact as HubSpot returns it when asked for its email and names.
pub fn contact() -> Value {
    json!({
        "id": "12345",
        "properties": {
            "createdate": "2026-09-01T09:00:00.000Z",
            "email": "ada@example.com",
            "firstname": "Ada",
            "hs_object_id": "12345",
            "lastmodifieddate": "2026-10-09T08:15:00.000Z",
            "lastname": "Lovelace",
            "phone": null
        },
        "createdAt": "2026-09-01T09:00:00.000Z",
        "updatedAt": "2026-10-09T08:15:00.000Z",
        "archived": false
    })
}

/// What an operation that returns `contact()` must pass on.
pub fn contact_returned() -> Value {
    json!({
        "id": "12345",
        "properties": { "email": "ada@example.com", "firstname": "Ada", "lastname": "Lovelace", "phone": null },
        "createdAt": "2026-09-01T09:00:00.000Z",
        "updatedAt": "2026-10-09T08:15:00.000Z",
        "archived": false
    })
}

/// A note as HubSpot returns it once created.
pub fn note() -> Value {
    json!({
        "id": "9001",
        "properties": {
            "hs_createdate": "2026-10-10T10:00:00.000Z",
            "hs_lastmodifieddate": "2026-10-10T10:00:00.000Z",
            "hs_note_body": "Agreed to renew in November.",
            "hs_object_id": "9001",
            "hs_timestamp": "2026-10-10T10:00:00.000Z"
        },
        "createdAt": "2026-10-10T10:00:00.000Z",
        "updatedAt": "2026-10-10T10:00:00.000Z",
        "archived": false
    })
}

/// What a batch call answers once it is done.
pub fn batch_of(results: Value) -> Value {
    json!({
        "status": "COMPLETE",
        "results": results,
        "startedAt": "2026-10-10T10:00:00.000Z",
        "completedAt": "2026-10-10T10:00:00.100Z"
    })
}

/// A property that takes one value from a list.
pub fn property() -> Value {
    json!({
        "name": "hs_lead_status", "label": "Lead Status", "type": "enumeration", "fieldType": "radio",
        "description": "The contact's sales, prospecting or outreach status", "groupName": "sales_properties",
        "options": [
            { "label": "New", "value": "NEW", "displayOrder": 0, "hidden": false },
            { "label": "In Progress", "value": "IN_PROGRESS", "displayOrder": 2, "hidden": false, "description": null }
        ],
        "displayOrder": 5, "calculated": false, "externalOptions": false, "hasUniqueValue": false, "hidden": false,
        "hubspotDefined": true, "formField": true, "dataSensitivity": "non_sensitive",
        "modificationMetadata": { "archivable": true, "readOnlyDefinition": true, "readOnlyOptions": false, "readOnlyValue": false },
        "createdAt": "2019-08-06T02:41:08.029Z", "updatedAt": "2024-01-10T00:00:00.000Z", "archived": false
    })
}

/// The pipeline every account has for its deals.
pub fn pipeline() -> Value {
    json!({
        "id": "default", "label": "Sales Pipeline", "displayOrder": 0, "archived": false,
        "createdAt": "1970-01-01T00:00:00Z", "updatedAt": "2026-01-10T00:00:00.000Z",
        "stages": [
            { "id": "appointmentscheduled", "label": "Appointment Scheduled", "displayOrder": 0, "archived": false,
              "metadata": { "isClosed": "false", "probability": "0.2" }, "writePermissions": "CRM_PERMISSIONS_ENFORCEMENT",
              "createdAt": "1970-01-01T00:00:00Z", "updatedAt": "1970-01-01T00:00:00Z" },
            { "id": "closedwon", "label": "Closed Won", "displayOrder": 5, "archived": false,
              "metadata": { "isClosed": "true", "probability": "1.0" }, "writePermissions": "CRM_PERMISSIONS_ENFORCEMENT",
              "createdAt": "1970-01-01T00:00:00Z", "updatedAt": "1970-01-01T00:00:00Z" }
        ]
    })
}

pub fn owner() -> Value {
    json!({
        "id": "41629779", "email": "grace@example.com", "type": "PERSON", "firstName": "Grace", "lastName": "Hopper",
        "userId": 9586504, "userIdIncludingInactive": 9586504, "archived": false,
        "createdAt": "2019-12-25T13:01:35.228Z", "updatedAt": "2023-08-22T13:40:26.790Z",
        "teams": [{ "id": "368389", "name": "Sales Team", "primary": true }]
    })
}

/// A company a contact is associated with, as its primary one and by a label.
pub fn association() -> Value {
    json!({
        "toObjectId": 5_790_939_450_u64,
        "associationTypes": [
            { "category": "HUBSPOT_DEFINED", "typeId": 1, "label": "Primary" },
            { "category": "HUBSPOT_DEFINED", "typeId": 279, "label": null },
            { "category": "USER_DEFINED", "typeId": 28, "label": "Billing contact" }
        ]
    })
}
