//! What the Salesforce tests share: a local server that answers as
//! Salesforce does, and ways to read what reached it.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_salesforce::{Salesforce, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// Where the data API is on a host, at the version the definition calls.
pub const API: &str = "/services/data/v67.0";

/// The token every test connection holds.
pub const TOKEN: &str = "00Dxx0000001gPL!AR8AQ.good";

pub const ACCOUNT: &str = "001xx000003DGb2AAG";
pub const CONTACT: &str = "003xx000004TmiQAAS";
pub const LEAD: &str = "00Qxx0000012345AAA";
pub const OPPORTUNITY: &str = "006xx000001a2b3AAA";
pub const CASE: &str = "500xx000000bcdeAAA";
pub const TASK: &str = "00Txx000003fghiAAA";
pub const EVENT: &str = "00Uxx000001jklmAAA";
pub const NOTE: &str = "002xx000000nopqAAA";
pub const EMAIL: &str = "02sxx000000rstuAAA";
pub const USER: &str = "005xx000001SvogAAC";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below the versioned data API, as it is written on the wire.
    pub path: &'static str,
    /// Exactly the query parameters that reach Salesforce.
    pub query: Value,
    /// Exactly the JSON body that reaches Salesforce; `null` when there is none.
    pub body: Value,
    /// Exactly the `Sforce-Query-Options` header that reaches Salesforce, if any.
    pub options: Option<&'static str>,
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

/// A refusal as Salesforce's data API writes one.
pub fn refusal(status: u16, code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!([{ "message": message, "errorCode": code }]))
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

pub fn header<'r>(request: &'r Request, name: &str) -> Option<&'r str> {
    request.headers.get(name).map(|value| value.to_str().unwrap())
}

/// Salesforce aimed at a local server, with a stored connection whose calls
/// go there.
pub async fn salesforce() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Salesforce::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, TOKEN).await;
    (server, socket, key)
}

/// A server that answers every request with `status` and `body`.
pub async fn answering(status: u16, body: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = salesforce().await;
    Mock::given(any())
        .respond_with(answer(status, &body))
        .mount(&server)
        .await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("salesforce.{name}"), input).await
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "one call to Salesforce");
    received.remove(0)
}

/// What Salesforce writes beside a record's fields.
pub fn attributes(object: &str, id: &str) -> Value {
    json!({ "type": object, "url": format!("{API}/sobjects/{object}/{id}") })
}

/// An account as a query returns it when asked for its owner's name.
pub fn account() -> Value {
    json!({
        "attributes": attributes("Account", ACCOUNT),
        "Id": ACCOUNT,
        "Name": "Acme",
        "Industry": "Energy",
        "AnnualRevenue": 1250000.5,
        "BillingAddress": { "city": "Paris", "country": "France", "postalCode": "75002", "street": "1 rue de la Paix" },
        "Owner": { "attributes": attributes("User", USER), "Name": "Ada Lovelace" },
        "Parent": null
    })
}

/// What an operation that returns `account()` must pass on.
pub fn account_returned() -> Value {
    json!({
        "type": "Account",
        "id": ACCOUNT,
        "fields": {
            "Name": "Acme",
            "Industry": "Energy",
            "AnnualRevenue": 1250000.5,
            "BillingAddress": { "city": "Paris" },
            "Owner": { "attributes": { "type": "User" }, "Name": "Ada Lovelace" },
            "Parent": null
        }
    })
}

/// An opportunity with the contacts that have a role in it, as a subquery returns them.
pub fn opportunity() -> Value {
    json!({
        "attributes": attributes("Opportunity", OPPORTUNITY),
        "Id": OPPORTUNITY,
        "Name": "Acme renewal",
        "StageName": "Negotiation/Review",
        "Amount": 48000.0,
        "CloseDate": "2026-11-30",
        "Account": { "attributes": attributes("Account", ACCOUNT), "Name": "Acme" },
        "OpportunityContactRoles": {
            "totalSize": 1,
            "done": true,
            "records": [{
                "attributes": attributes("OpportunityContactRole", "00Kxx000000vwxyAAA"),
                "Role": "Decision Maker",
                "Contact": { "attributes": attributes("Contact", CONTACT), "Name": "Grace Hopper", "Email": "grace@acme.example" }
            }]
        }
    })
}

pub fn case() -> Value {
    json!({
        "attributes": attributes("Case", CASE),
        "Id": CASE,
        "CaseNumber": "00001026",
        "Subject": "Pump will not start",
        "Status": "Escalated",
        "Priority": "High"
    })
}

/// An email as Salesforce keeps one on a case.
pub fn email_message() -> Value {
    json!({
        "attributes": attributes("EmailMessage", EMAIL),
        "Id": EMAIL,
        "ParentId": CASE,
        "Subject": "Re: Pump will not start",
        "FromAddress": "grace@acme.example",
        "ToAddress": "support@example.test",
        "TextBody": "It still does not start.",
        "Incoming": true,
        "MessageDate": "2026-10-09T08:15:00.000+0000"
    })
}

/// The answer to a create.
pub fn created(id: &str) -> Value {
    json!({ "id": id, "success": true, "errors": [] })
}
