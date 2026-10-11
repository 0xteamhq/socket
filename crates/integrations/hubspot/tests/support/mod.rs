//! What the HubSpot tests share: a local server that answers as HubSpot
//! does, and ways to read what reached it.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_hubspot::{HubSpot, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// Where the records of every object type live, in the version of the API the crate calls.
pub const OBJECTS: &str = "/crm/objects/2026-09";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below `https://api.hubapi.com`.
    pub path: &'static str,
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

/// An error as HubSpot writes one.
pub fn hubspot_error(status: u16, category: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({
        "status": "error",
        "message": message,
        "correlationId": "c033cdaa-2c40-4a64-ae48-b4cec88dad24",
        "category": category
    }))
}

/// A 429 as HubSpot writes one, naming the limit that was met.
pub fn rate_limited(policy: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(429).set_body_json(json!({
        "status": "error",
        "message": message,
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
    let (socket, key) = connect(integration, "pat-na1-good").await;
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

/// A server that answers every request with `response`.
pub async fn refusing(response: ResponseTemplate) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = hubspot().await;
    Mock::given(any()).respond_with(response).mount(&server).await;
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

/// A contact as HubSpot returns it: every value a string, and `null` where
/// a property that was asked for has no value.
pub fn contact() -> Value {
    json!({
        "id": "512",
        "properties": {
            "createdate": "2026-10-01T09:00:00.000Z",
            "email": "ada@example.test",
            "firstname": "Ada",
            "lastname": null,
            "hs_object_id": "512",
            "lastmodifieddate": "2026-10-09T08:00:00.000Z"
        },
        "createdAt": "2026-10-01T09:00:00.000Z",
        "updatedAt": "2026-10-09T08:00:00.000Z",
        "archived": false,
        "url": "https://app.hubspot.com/contacts/1234567/record/0-1/512"
    })
}

/// What an operation that returns `contact()` must pass on.
pub fn contact_returned() -> Value {
    json!({
        "id": "512",
        "properties": { "email": "ada@example.test", "firstname": "Ada", "lastname": null, "hs_object_id": "512" },
        "createdAt": "2026-10-01T09:00:00.000Z",
        "updatedAt": "2026-10-09T08:00:00.000Z",
        "archived": false
    })
}

/// A deal with the contacts it is associated with.
pub fn deal() -> Value {
    json!({
        "id": "9001",
        "properties": { "dealname": "Analytical Engine", "amount": "1500", "dealstage": "contractsent", "pipeline": "default", "hs_object_id": "9001" },
        "createdAt": "2026-09-01T09:00:00.000Z",
        "updatedAt": "2026-10-09T08:00:00.000Z",
        "archived": false,
        "associations": {
            "contacts": {
                "results": [{ "id": "512", "type": "deal_to_contact" }, { "id": "513", "type": "deal_to_contact" }],
                "paging": { "next": { "after": "513", "link": "https://api.hubapi.com/crm/objects/2026-09/deals/9001/associations/contacts?after=513" } }
            }
        }
    })
}

/// What HubSpot answers to a batch it carried out in full.
pub fn batch_of(results: Value) -> Value {
    json!({
        "status": "COMPLETE",
        "results": results,
        "startedAt": "2026-10-09T08:00:00.000Z",
        "completedAt": "2026-10-09T08:00:00.120Z"
    })
}

/// One kind of association, as it is sent.
pub fn kind(id: i64) -> Value {
    json!({ "associationCategory": "HUBSPOT_DEFINED", "associationTypeId": id })
}

pub fn property() -> Value {
    json!({
        "name": "lifecyclestage", "label": "Lifecycle Stage", "type": "enumeration", "fieldType": "radio",
        "description": "The qualification of contacts to sales readiness.", "groupName": "contactinformation",
        "options": [
            { "label": "Lead", "value": "lead", "displayOrder": 1, "hidden": false },
            { "label": "Customer", "value": "customer", "displayOrder": 5, "hidden": false, "description": null }
        ],
        "displayOrder": -1, "calculated": false, "externalOptions": false, "hasUniqueValue": false, "hidden": false,
        "hubspotDefined": true, "formField": true, "dataSensitivity": "non_sensitive",
        "modificationMetadata": { "archivable": false, "readOnlyDefinition": true, "readOnlyValue": false },
        "createdAt": "2019-10-30T03:30:17.883Z", "updatedAt": "2026-01-05T09:00:00.000Z"
    })
}

pub fn pipeline() -> Value {
    json!({
        "id": "default", "label": "Sales Pipeline", "displayOrder": 0, "archived": false,
        "createdAt": "1970-01-01T00:00:00Z", "updatedAt": "2026-01-05T09:00:00.000Z",
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
        "id": "41", "email": "ada@example.test", "firstName": "Ada", "lastName": "Lovelace", "type": "PERSON",
        "userId": 2620022, "userIdIncludingInactive": 2620022, "archived": false,
        "teams": [{ "id": "178588", "name": "West", "primary": true }],
        "createdAt": "2019-10-30T03:30:17.883Z", "updatedAt": "2026-01-05T09:00:00.000Z"
    })
}
