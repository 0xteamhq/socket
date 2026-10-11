//! Every HubSpot operation, called by name against a local server that answers as HubSpot's CRM API does.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{Effect, ErrorKind, Integration};
use socketkit_hubspot::models::{
    AssociationCategory, AssociationSpec, CreateAssociation, CreateObject, Filter, FilterGroup, FilterOperator,
    GetObject, ListObjects, NewAssociation, ObjectId, Paging, Search, UpdateObject,
};
use socketkit_hubspot::{HubSpot, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    Case, OBJECTS, OWNERS, PIPELINES, PROPERTIES, TOKEN, answer, answering, association, batch_of, body_of, contact,
    contact_returned, contains, hubspot, hubspot_error, invoke, note, only_request, owner, pipeline, property,
    query_of,
};

fn deal() -> Value {
    json!({
        "id": "777",
        "properties": { "amount": "4800", "closedate": "2026-11-30T00:00:00.000Z", "dealname": "Renewal", "dealstage": "closedwon", "hs_object_id": "777", "pipeline": "default" },
        "createdAt": "2026-08-01T09:00:00.000Z", "updatedAt": "2026-10-10T11:00:00.000Z", "archived": false
    })
}

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, path: String, query, body, status, response, returns| Case { name, input, verb, path, query, body, status, response, returns };
    let to_contact = json!([{ "to": { "id": "12345" }, "types": [{ "associationCategory": "HUBSPOT_DEFINED", "associationTypeId": 202 }] }]);
    let filters = json!([{ "filters": [{ "propertyName": "amount", "operator": "GT", "value": "1000" }] }]);
    let newest = json!([{ "propertyName": "closedate", "direction": "DESCENDING" }]);
    let with_history = {
        let mut record = contact();
        record["propertiesWithHistory"] = json!({ "lifecyclestage": [{ "value": "lead", "timestamp": "2026-09-01T09:00:00.000Z", "sourceType": "CRM_UI", "sourceId": "userId:9586504", "updatedByUserId": 9586504 }] });
        record["associations"] = json!({ "deals": { "results": [{ "id": "777", "type": "contact_to_deal" }] } });
        record["url"] = json!("https://app.hubspot.com/contacts/8675309/record/0-1/12345");
        record
    };
    vec![
        // objects: reading
        case("objects.list", json!({ "object_type": "contacts", "properties": ["email", "firstname"], "associations": ["companies"], "limit": 2, "cursor": "12344" }),
            "GET", format!("{OBJECTS}/contacts"), json!({ "properties": "email,firstname", "associations": "companies", "limit": "2", "after": "12344" }), json!(null), 200,
            json!({ "results": [contact()], "paging": { "next": { "after": "12346", "link": "https://api.hubapi.com/crm/objects/2026-09/contacts?after=12346" } } }),
            json!({ "items": [contact_returned()], "next_cursor": "12346" })),
        case("objects.get", json!({ "object_type": "contacts", "id": "ada@example.com", "idProperty": "email", "properties": ["email"], "propertiesWithHistory": ["lifecyclestage"], "associations": ["deals"] }),
            "GET", format!("{OBJECTS}/contacts/ada%40example.com"), json!({ "idProperty": "email", "properties": "email", "propertiesWithHistory": "lifecyclestage", "associations": "deals" }), json!(null), 200,
            with_history,
            json!({ "id": "12345", "properties": { "email": "ada@example.com" },
                    "propertiesWithHistory": { "lifecyclestage": [{ "value": "lead", "timestamp": "2026-09-01T09:00:00.000Z", "sourceType": "CRM_UI", "updatedByUserId": 9586504 }] },
                    "associations": { "deals": { "results": [{ "id": "777", "type": "contact_to_deal" }] } },
                    "url": "https://app.hubspot.com/contacts/8675309/record/0-1/12345" })),
        // HubSpot offers these two only as POST; they change nothing.
        case("objects.batch_read", json!({ "object_type": "companies", "ids": ["1", "2"], "properties": ["name"] }),
            "POST", format!("{OBJECTS}/companies/batch/read"), json!({}),
            json!({ "inputs": [{ "id": "1" }, { "id": "2" }], "properties": ["name"], "propertiesWithHistory": [] }), 200,
            batch_of(json!([{ "id": "1", "properties": { "name": "Analytical Engines" }, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z", "archived": false }])),
            json!({ "status": "COMPLETE", "results": [{ "id": "1", "properties": { "name": "Analytical Engines" } }], "numErrors": 0, "errors": [] })),
        case("objects.search", json!({ "object_type": "deals", "query": "renewal", "filterGroups": filters.clone(), "sorts": newest.clone(), "properties": ["dealname", "amount"], "limit": 20, "cursor": "20" }),
            "POST", format!("{OBJECTS}/deals/search"), json!({}),
            json!({ "query": "renewal", "filterGroups": filters, "sorts": newest, "properties": ["dealname", "amount"], "limit": 20, "after": "20" }), 200,
            json!({ "total": 41, "results": [deal()], "paging": { "next": { "after": "40" } } }),
            json!({ "total": 41, "items": [{ "id": "777", "properties": { "dealname": "Renewal", "amount": "4800" } }], "next_cursor": "40" })),

        // objects: writing
        case("objects.create", json!({ "object_type": "notes", "properties": { "hs_timestamp": "2026-10-10T10:00:00Z", "hs_note_body": "Agreed to renew in November." }, "associations": to_contact.clone() }),
            "POST", format!("{OBJECTS}/notes"), json!({}),
            json!({ "properties": { "hs_timestamp": "2026-10-10T10:00:00Z", "hs_note_body": "Agreed to renew in November." }, "associations": to_contact }), 201,
            note(), json!({ "id": "9001", "properties": { "hs_note_body": "Agreed to renew in November." }, "createdAt": "2026-10-10T10:00:00.000Z" })),
        case("objects.update", json!({ "object_type": "deals", "id": "777", "properties": { "dealstage": "closedwon" } }),
            "PATCH", format!("{OBJECTS}/deals/777"), json!({}), json!({ "properties": { "dealstage": "closedwon" } }), 200,
            deal(), json!({ "id": "777", "properties": { "dealstage": "closedwon" } })),
        case("objects.batch_create", json!({ "object_type": "contacts", "inputs": [{ "properties": { "email": "ada@example.com" } }, { "properties": { "email": "grace@example.com" } }] }),
            "POST", format!("{OBJECTS}/contacts/batch/create"), json!({}),
            json!({ "inputs": [{ "properties": { "email": "ada@example.com" }, "associations": [] }, { "properties": { "email": "grace@example.com" }, "associations": [] }] }), 201,
            batch_of(json!([contact()])), json!({ "status": "COMPLETE", "results": [contact_returned()] })),
        case("objects.batch_update", json!({ "object_type": "tickets", "inputs": [{ "id": "55", "properties": { "hs_pipeline_stage": "4" } }, { "id": "t-9", "idProperty": "external_id", "properties": { "hs_pipeline_stage": "4" } }] }),
            "POST", format!("{OBJECTS}/tickets/batch/update"), json!({}),
            json!({ "inputs": [{ "id": "55", "properties": { "hs_pipeline_stage": "4" } }, { "id": "t-9", "idProperty": "external_id", "properties": { "hs_pipeline_stage": "4" } }] }), 200,
            batch_of(json!([{ "id": "55", "properties": { "hs_pipeline_stage": "4" } }])), json!({ "results": [{ "id": "55", "properties": { "hs_pipeline_stage": "4" } }] })),
        case("objects.archive", json!({ "object_type": "tasks", "id": "31" }),
            "DELETE", format!("{OBJECTS}/tasks/31"), json!({}), json!(null), 204, json!(null), json!(null)),

        // associations
        case("associations.list", json!({ "from_object_type": "contacts", "from_id": "12345", "to_object_type": "companies", "limit": 100 }),
            "GET", format!("{OBJECTS}/contacts/12345/associations/companies"), json!({ "limit": "100" }), json!(null), 200,
            json!({ "results": [association()], "paging": { "next": { "after": "5790939451" } } }),
            // HubSpot writes the id as a number here; it is the same id as everywhere else.
            json!({ "items": [{ "toObjectId": "5790939450", "associationTypes": [
                { "category": "HUBSPOT_DEFINED", "typeId": 1, "label": "Primary" }, { "typeId": 279, "label": null }, { "category": "USER_DEFINED", "typeId": 28, "label": "Billing contact" }] }],
                "next_cursor": "5790939451" })),
        case("associations.create", json!({ "from_object_type": "contacts", "from_id": "12345", "to_object_type": "companies", "to_id": "67891" }),
            "PUT", format!("{OBJECTS}/contacts/12345/associations/default/companies/67891"), json!({}), json!(null), 200,
            batch_of(json!([
                { "from": { "id": "12345" }, "to": { "id": "67891" }, "associationSpec": { "associationCategory": "HUBSPOT_DEFINED", "associationTypeId": 279 } },
                { "from": { "id": "67891" }, "to": { "id": "12345" }, "associationSpec": { "associationCategory": "HUBSPOT_DEFINED", "associationTypeId": 280 } }])),
            json!({ "fromObjectId": "12345", "toObjectId": "67891", "labels": [] })),
        case("associations.remove", json!({ "from_object_type": "contacts", "from_id": "12345", "to_object_type": "companies", "to_id": "67891" }),
            "DELETE", format!("{OBJECTS}/contacts/12345/associations/companies/67891"), json!({}), json!(null), 204, json!(null), json!(null)),

        // properties
        case("properties.list", json!({ "object_type": "contacts" }),
            "GET", format!("{PROPERTIES}/contacts"), json!({}), json!(null), 200,
            json!({ "results": [property()] }),
            json!([{ "name": "hs_lead_status", "label": "Lead Status", "type": "enumeration", "fieldType": "radio", "groupName": "sales_properties", "calculated": false, "hasUniqueValue": false }])),
        case("properties.get", json!({ "object_type": "contacts", "name": "hs_lead_status" }),
            "GET", format!("{PROPERTIES}/contacts/hs_lead_status"), json!({}), json!(null), 200,
            property(),
            json!({ "name": "hs_lead_status", "label": "Lead Status", "type": "enumeration", "fieldType": "radio",
                    "description": "The contact's sales, prospecting or outreach status",
                    "options": [{ "label": "New", "value": "NEW" }, { "label": "In Progress", "value": "IN_PROGRESS" }],
                    "modificationMetadata": { "readOnlyValue": false }, "hubspotDefined": true })),

        // pipelines
        case("pipelines.list", json!({ "object_type": "deals" }),
            "GET", format!("{PIPELINES}/deals"), json!({}), json!(null), 200,
            json!({ "results": [pipeline()] }),
            json!([{ "id": "default", "label": "Sales Pipeline", "stages": [
                { "id": "appointmentscheduled", "label": "Appointment Scheduled", "metadata": { "isClosed": "false", "probability": "0.2" } },
                { "id": "closedwon", "label": "Closed Won", "metadata": { "isClosed": "true" } }] }])),
        case("pipelines.get", json!({ "object_type": "tickets", "pipeline": "0" }),
            "GET", format!("{PIPELINES}/tickets/0"), json!({}), json!(null), 200,
            json!({ "id": "0", "label": "Support Pipeline", "displayOrder": 0, "archived": false,
                    "stages": [{ "id": "1", "label": "New", "displayOrder": 0, "metadata": { "ticketState": "OPEN", "isClosed": "false" } }] }),
            json!({ "id": "0", "label": "Support Pipeline", "stages": [{ "id": "1", "label": "New", "metadata": { "ticketState": "OPEN" } }] })),

        // owners
        case("owners.list", json!({ "email": "grace@example.com", "limit": 50 }),
            "GET", OWNERS.to_owned(), json!({ "email": "grace@example.com", "limit": "50" }), json!(null), 200,
            json!({ "results": [owner()] }),
            json!({ "items": [{ "id": "41629779", "email": "grace@example.com", "firstName": "Grace", "lastName": "Hopper", "type": "PERSON", "userId": 9586504,
                                "teams": [{ "id": "368389", "name": "Sales Team", "primary": true }] }], "next_cursor": null })),
        case("owners.get", json!({ "owner": "9586504", "idProperty": "userId" }),
            "GET", format!("{OWNERS}/9586504"), json!({ "idProperty": "userId" }), json!(null), 200,
            owner(), json!({ "id": "41629779", "email": "grace@example.com", "type": "PERSON" })),
    ]
}

/// Every operation, with what it does to the account's data and the scopes
/// it lists. Stated here and not derived from the code under test: a host
/// lets a read run freely and asks a person before anything else.
fn expected() -> Vec<(&'static str, Effect, Vec<&'static str>)> {
    let reading = || {
        vec![
            "crm.objects.contacts.read",
            "crm.objects.companies.read",
            "crm.objects.deals.read",
            "crm.objects.tickets.read",
        ]
    };
    let writing = || {
        vec![
            "crm.objects.contacts.write",
            "crm.objects.companies.write",
            "crm.objects.deals.write",
            "crm.objects.tickets.write",
        ]
    };
    let fields = || {
        vec![
            "crm.schemas.contacts.read",
            "crm.schemas.companies.read",
            "crm.schemas.deals.read",
            "crm.objects.tickets.read",
        ]
    };
    let stages = || vec!["crm.objects.deals.read", "crm.objects.tickets.read"];
    vec![
        ("objects.list", Effect::Read, reading()),
        ("objects.get", Effect::Read, reading()),
        // Reads that HubSpot takes as POST.
        ("objects.batch_read", Effect::Read, reading()),
        ("objects.search", Effect::Read, reading()),
        ("objects.create", Effect::Write, writing()),
        ("objects.update", Effect::Write, writing()),
        ("objects.batch_create", Effect::Write, writing()),
        ("objects.batch_update", Effect::Write, writing()),
        // The record leaves the account's lists, and only a person can bring it back.
        ("objects.archive", Effect::Destructive, writing()),
        ("associations.list", Effect::Read, reading()),
        ("associations.create", Effect::Write, writing()),
        // Every link between the two records goes, labels included.
        ("associations.remove", Effect::Destructive, writing()),
        ("properties.list", Effect::Read, fields()),
        ("properties.get", Effect::Read, fields()),
        ("pipelines.list", Effect::Read, stages()),
        ("pipelines.get", Effect::Read, stages()),
        ("owners.list", Effect::Read, vec!["crm.objects.owners.read"]),
        ("owners.get", Effect::Read, vec!["crm.objects.owners.read"]),
    ]
}

#[tokio::test]
async fn the_operations_hubspot_offers_are_exactly_these() {
    let mut listed: Vec<String> = HubSpot::new().operations().into_iter().map(|o| o.name).collect();
    let mut named: Vec<String> = expected()
        .iter()
        .map(|(name, _, _)| format!("hubspot.{name}"))
        .collect();
    named.extend(["hubspot.identity.get".to_owned(), "hubspot.resource.resolve".to_owned()]);
    listed.sort();
    named.sort();
    assert_eq!(
        listed, named,
        "an operation was added or removed without being labelled here"
    );
    assert_eq!(listed.len(), 20);

    // Every one of them has a row in the table of requests below.
    let mut tested: Vec<&str> = cases().iter().map(|case| case.name).collect();
    let mut labelled: Vec<&str> = expected().iter().map(|(name, _, _)| *name).collect();
    tested.sort_unstable();
    labelled.sort_unstable();
    assert_eq!(tested, labelled);
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_hubspot_sent() {
    for case in cases() {
        let (server, socket, key) = hubspot().await;
        Mock::given(method(case.verb))
            .and(path(case.path.as_str()))
            .respond_with(answer(case.status, &case.response))
            .mount(&server)
            .await;

        let output = invoke(&socket, &key, case.name, case.input.clone())
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(
            contains(&output, &case.returns),
            "{}: returned {output}, expected {}",
            case.name,
            case.returns
        );

        let request = only_request(&server).await;
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            format!("Bearer {TOKEN}").as_str(),
            "{}",
            case.name
        );
        assert_eq!(
            query_of(&request),
            case.query,
            "{}: exactly these parameters reach HubSpot",
            case.name
        );
        assert_eq!(
            body_of(&request),
            case.body,
            "{}: exactly this body reaches HubSpot",
            case.name
        );
        assert!(
            !request.url.as_str().contains(TOKEN),
            "{}: the token is never part of an address",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_says_what_it_changes_and_which_scopes_it_needs() {
    let operations = HubSpot::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == format!("hubspot.{name}"))
            .unwrap_or_else(|| panic!("{name}"))
    };
    for (name, effect, scopes) in expected() {
        let operation = find(name);
        assert_eq!(operation.effect, effect, "{name}");
        assert_eq!(operation.required_scopes, scopes, "{name}");
    }

    // A read that HubSpot happens to take as POST is still a read, and
    // nothing that changes a record is ever called one. The verb is taken
    // from the request each operation was seen to send.
    for case in cases() {
        let effect = find(case.name).effect;
        let posted_read = matches!(case.name, "objects.batch_read" | "objects.search");
        match (case.verb, posted_read) {
            ("GET", false) => assert_eq!(effect, Effect::Read, "{}", case.name),
            ("POST", true) => assert_eq!(effect, Effect::Read, "{}: reads, though it is posted", case.name),
            (verb, _) => assert_ne!(effect, Effect::Read, "{}: a {verb} that changes the account", case.name),
        }
    }
    for write in [
        "objects.create",
        "objects.update",
        "objects.batch_create",
        "objects.batch_update",
        "associations.create",
    ] {
        assert_eq!(find(write).effect, Effect::Write, "{write}");
    }
    for destructive in ["objects.archive", "associations.remove"] {
        assert_eq!(find(destructive).effect, Effect::Destructive, "{destructive}");
    }
    // No operation that writes lists only a scope that reads, or the other way round.
    for operation in operations.iter().filter(|o| o.name.starts_with("hubspot.objects.")) {
        let access = if operation.effect == Effect::Read {
            ".read"
        } else {
            ".write"
        };
        assert!(
            operation.required_scopes.iter().all(|scope| scope.ends_with(access)),
            "{}: {:?}",
            operation.name,
            operation.required_scopes
        );
    }

    // The two every integration offers say what they need too.
    assert_eq!(find("identity.get").required_scopes, ["oauth"]);
    assert_eq!(find("identity.get").effect, Effect::Read);
    assert_eq!(find("resource.resolve").effect, Effect::Read);
    // A link is checked against the account, which is what `oauth` reads;
    // the record is then read like any other.
    let mut to_resolve = vec!["oauth".to_owned()];
    to_resolve.extend(find("objects.get").required_scopes.iter().cloned());
    assert_eq!(find("resource.resolve").required_scopes, to_resolve);
}

#[tokio::test]
async fn every_operation_describes_what_it_takes_and_what_it_returns() {
    let operations = HubSpot::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == format!("hubspot.{name}"))
            .unwrap_or_else(|| panic!("{name}"))
    };
    let required = |name: &str| -> Vec<String> {
        find(name).input_schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    for operation in &operations {
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert!(!operation.description.is_empty(), "{}", operation.name);
        assert!(!operation.output_schema.is_null(), "{}", operation.name);
        assert_eq!(
            operation.input_schema["additionalProperties"], false,
            "{}: a field the operation does not know is not allowed",
            operation.name
        );
    }

    assert_eq!(required("objects.list"), ["object_type"]);
    assert_eq!(required("objects.get"), ["object_type", "id"]);
    assert_eq!(required("objects.create"), ["object_type", "properties"]);
    assert_eq!(required("objects.update"), ["object_type", "id", "properties"]);
    assert_eq!(required("objects.batch_read"), ["object_type", "ids"]);
    assert_eq!(
        required("objects.search"),
        ["object_type"],
        "a search needs no condition"
    );
    assert_eq!(
        required("associations.create"),
        ["from_object_type", "from_id", "to_object_type", "to_id"]
    );
    assert_eq!(required("owners.list"), Vec::<String>::new());

    let create = find("objects.create");
    let fields = &create.input_schema["properties"];
    for field in ["object_type", "properties", "associations"] {
        assert!(fields.get(field).is_some(), "{field} is described");
    }
    // A record's own fields are the account's, so the schema names none of
    // them and does not forbid any: what closes every other object must not
    // close this one, nor be mistaken for a field of the operation.
    assert!(fields.get("additionalProperties").is_none(), "{fields}");
    assert_eq!(fields["properties"]["type"], "object");
    assert_eq!(fields["properties"]["additionalProperties"]["type"], "string");

    let search = &find("objects.search").input_schema["properties"];
    for field in ["query", "filterGroups", "sorts", "properties", "cursor", "limit"] {
        assert!(search.get(field).is_some(), "{field} is described");
    }
    let found = &find("objects.search").output_schema["properties"];
    for field in ["total", "items", "next_cursor"] {
        assert!(found.get(field).is_some(), "{field} is described");
    }
    let record = &find("objects.get").output_schema["properties"];
    for field in [
        "id",
        "properties",
        "propertiesWithHistory",
        "associations",
        "createdAt",
        "url",
    ] {
        assert!(record.get(field).is_some(), "{field} is described");
    }
}

#[tokio::test]
async fn every_object_type_goes_through_the_same_methods() {
    // The standard objects, the engagements, and a custom object by its type
    // id and by its full name.
    for object_type in [
        "contacts",
        "companies",
        "deals",
        "tickets",
        "notes",
        "calls",
        "meetings",
        "emails",
        "tasks",
        "2-3465404",
        "p8675309_cars",
    ] {
        let (server, socket, key) = hubspot().await;
        let record = json!({ "id": "1", "properties": { "hs_object_id": "1" }, "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z", "archived": false });
        Mock::given(method("GET"))
            .and(path(format!("{OBJECTS}/{object_type}")))
            .respond_with(answer(200, &json!({ "results": [record.clone()] })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{OBJECTS}/{object_type}/1")))
            .respond_with(answer(200, &record))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{OBJECTS}/{object_type}/search")))
            .respond_with(answer(200, &json!({ "total": 1, "results": [record.clone()] })))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path(format!("{OBJECTS}/{object_type}")))
            .respond_with(answer(201, &record))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("PATCH"))
            .and(path(format!("{OBJECTS}/{object_type}/1")))
            .respond_with(answer(200, &record))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path(format!("{OBJECTS}/{object_type}/1")))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(format!("{PROPERTIES}/{object_type}")))
            .respond_with(answer(200, &json!({ "results": [property()] })))
            .expect(1)
            .mount(&server)
            .await;

        let of_type = |mut input: Value| {
            input["object_type"] = json!(object_type);
            input
        };
        let listed = invoke(&socket, &key, "objects.list", of_type(json!({}))).await.unwrap();
        assert_eq!(listed["items"][0]["id"], "1", "{object_type}");
        let one = invoke(&socket, &key, "objects.get", of_type(json!({ "id": "1" })))
            .await
            .unwrap();
        assert_eq!(one["id"], "1", "{object_type}");
        let found = invoke(&socket, &key, "objects.search", of_type(json!({})))
            .await
            .unwrap();
        assert_eq!(found["total"], 1, "{object_type}");
        let content = json!({ "properties": { "hs_timestamp": "2026-10-10T10:00:00Z" } });
        invoke(&socket, &key, "objects.create", of_type(content.clone()))
            .await
            .unwrap();
        let mut change = of_type(content);
        change["id"] = json!("1");
        invoke(&socket, &key, "objects.update", change).await.unwrap();
        invoke(&socket, &key, "objects.archive", of_type(json!({ "id": "1" })))
            .await
            .unwrap();
        let fields = invoke(&socket, &key, "properties.list", of_type(json!({})))
            .await
            .unwrap();
        assert_eq!(fields[0]["name"], "hs_lead_status", "{object_type}");
    }
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent() {
    let (server, socket, key) = answering(200, json!({ "results": [], "total": 0 })).await;
    invoke(&socket, &key, "objects.list", json!({ "object_type": "contacts" }))
        .await
        .unwrap();
    invoke(&socket, &key, "objects.search", json!({ "object_type": "contacts" }))
        .await
        .unwrap();
    // Names that say nothing are not sent as a list of nothing.
    invoke(
        &socket,
        &key,
        "objects.list",
        json!({ "object_type": "contacts", "properties": [], "associations": [" "], "archived": false }),
    )
    .await
    .unwrap();
    let received = server.received_requests().await.unwrap();
    assert_eq!(received[0].url.query(), None);
    assert_eq!(body_of(&received[1]), json!({}), "a search of everything");
    assert_eq!(query_of(&received[2]), json!({ "archived": "false" }));
}

// ── Paging ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_token_of_the_next_page_is_the_cursor_and_goes_back_as_after() {
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(answer(
            200,
            &json!({ "results": [contact()], "paging": { "next": { "after": "12346", "link": "https://evil.test/next" } } }),
        ))
        .mount(&server)
        .await;
    let first = invoke(
        &socket,
        &key,
        "objects.list",
        json!({ "object_type": "contacts", "limit": 1 }),
    )
    .await
    .unwrap();
    assert_eq!(first["next_cursor"], "12346");
    invoke(
        &socket,
        &key,
        "objects.list",
        json!({ "object_type": "contacts", "limit": 1, "cursor": first["next_cursor"] }),
    )
    .await
    .unwrap();
    let received = server.received_requests().await.unwrap();
    assert_eq!(query_of(&received[0]), json!({ "limit": "1" }));
    assert_eq!(query_of(&received[1]), json!({ "limit": "1", "after": "12346" }));
    // The link HubSpot sends beside the token is never followed.
    assert!(received.iter().all(|r| r.url.path() == format!("{OBJECTS}/contacts")));
}

#[tokio::test]
async fn a_cursor_can_only_say_where_to_go_on() {
    // A cursor comes back from the caller. Whatever it holds, it is one
    // value of one parameter of the same list.
    let (server, socket, key) = answering(200, json!({ "results": [] })).await;
    let hostile = "1&archived=true#x/../../owners?limit=500";
    invoke(
        &socket,
        &key,
        "objects.list",
        json!({ "object_type": "contacts", "cursor": hostile }),
    )
    .await
    .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), format!("{OBJECTS}/contacts"));
    assert_eq!(query_of(&request), json!({ "after": hostile }));
    assert_eq!(request.url.fragment(), None);
}

#[tokio::test]
async fn the_last_page_has_no_cursor_however_hubspot_ends_it() {
    for last in [
        json!({ "results": [contact()] }),
        json!({ "results": [contact()], "paging": null }),
        json!({ "results": [contact()], "paging": {} }),
        json!({ "results": [contact()], "paging": { "next": { "after": "" } } }),
        json!({ "results": [], "paging": { "prev": { "before": "9" } } }),
    ] {
        let (_server, socket, key) = answering(200, last.clone()).await;
        let page = invoke(&socket, &key, "objects.list", json!({ "object_type": "contacts" }))
            .await
            .unwrap();
        assert_eq!(page["next_cursor"], json!(null), "{last}");
    }
}

#[tokio::test]
async fn a_blank_cursor_is_the_first_page() {
    let (server, socket, key) = answering(200, json!({ "results": [], "total": 0 })).await;
    for name in ["objects.list", "objects.search"] {
        for blank in ["", "   "] {
            invoke(
                &socket,
                &key,
                name,
                json!({ "object_type": "contacts", "cursor": blank }),
            )
            .await
            .unwrap();
        }
    }
    for request in server.received_requests().await.unwrap() {
        assert_eq!(request.url.query(), None);
        assert!(body_of(&request).get("after").is_none());
    }
}

#[tokio::test]
async fn a_page_size_hubspot_would_refuse_is_refused_first() {
    let (server, socket, key) = hubspot().await;
    let contacts = json!({ "object_type": "contacts" });
    let pair = json!({ "from_object_type": "contacts", "from_id": "1", "to_object_type": "companies" });
    for (name, input, limit, message) in [
        ("objects.list", contacts.clone(), 0, "`limit` is from 1 to 100"),
        ("objects.list", contacts.clone(), 101, "`limit` is from 1 to 100"),
        ("objects.search", contacts.clone(), 0, "`limit` is from 1 to 200"),
        ("objects.search", contacts.clone(), 201, "`limit` is from 1 to 200"),
        ("associations.list", pair.clone(), 501, "`limit` is from 1 to 500"),
        ("owners.list", json!({}), 0, "`limit` is at least 1"),
    ] {
        let mut input = input;
        input["limit"] = json!(limit);
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (ErrorKind::InvalidInput, message),
            "{name}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The largest each takes is sent as it is.
    let (server, socket, key) = answering(200, json!({ "results": [], "total": 0 })).await;
    for (name, mut input, limit) in [
        ("objects.list", contacts.clone(), 100),
        ("objects.search", contacts, 200),
        ("associations.list", pair, 500),
        ("owners.list", json!({}), 500),
    ] {
        input["limit"] = json!(limit);
        invoke(&socket, &key, name, input).await.unwrap();
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 4);
}

// ── Search ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_search_stops_at_ten_thousand_matches_and_says_so() {
    // A cursor past the last match a search returns is refused before
    // HubSpot is asked.
    let (server, socket, key) = hubspot().await;
    for past in ["10000", "10200", "18446744073709551615"] {
        let err = invoke(
            &socket,
            &key,
            "objects.search",
            json!({ "object_type": "contacts", "cursor": past }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{past}");
        assert!(err.message().contains("first 10,000 matches"), "{}", err.message());
        assert!(err.message().contains("narrow the filters"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The page that reaches the last match is asked for only as far as it,
    // since HubSpot refuses a page that would end past it. It comes back as
    // a last page does, with no cursor, though HubSpot names a next place;
    // the total says there was more than a search can reach.
    for (cursor, limit, asked_for) in [
        ("9900", Some(200), json!(100)),
        ("9995", None, json!(5)),
        ("9999", Some(1), json!(1)),
        ("9990", Some(10), json!(10)),
    ] {
        let (server, socket, key) = answering(
            200,
            json!({ "total": 25_000, "results": [contact()], "paging": { "next": { "after": "10000" } } }),
        )
        .await;
        let mut input = json!({ "object_type": "contacts", "cursor": cursor });
        if let Some(limit) = limit {
            input["limit"] = json!(limit);
        }
        let last = invoke(&socket, &key, "objects.search", input).await.unwrap();
        assert_eq!(last["total"], 25_000, "{cursor}");
        assert_eq!(last["items"].as_array().map(Vec::len), Some(1), "{cursor}");
        assert_eq!(
            last["next_cursor"],
            json!(null),
            "{cursor}: a cursor that would be refused is not handed out"
        );
        let sent = body_of(&only_request(&server).await);
        assert_eq!(
            (&sent["after"], &sent["limit"]),
            (&json!(cursor), &asked_for),
            "{cursor}"
        );
    }

    // Before that, a page is asked for as the caller sized it, and
    // HubSpot's cursor is passed on.
    let (server, socket, key) = answering(
        200,
        json!({ "total": 25_000, "results": [contact()], "paging": { "next": { "after": "9200" } } }),
    )
    .await;
    let middle = invoke(
        &socket,
        &key,
        "objects.search",
        json!({ "object_type": "contacts", "cursor": "9000", "limit": 200 }),
    )
    .await
    .unwrap();
    assert_eq!(middle["next_cursor"], "9200");
    assert_eq!(body_of(&only_request(&server).await)["limit"], 200);
}

#[tokio::test]
async fn paging_a_search_until_there_is_no_cursor_ends_without_an_error() {
    // What a caller does: go on while there is a cursor. With more matches
    // than a search returns, that has to end on a last page, not on a refusal.
    let (server, socket, key) = hubspot().await;
    let page =
        |after: &str| json!({ "total": 25_000, "results": [contact()], "paging": { "next": { "after": after } } });
    Mock::given(any())
        .respond_with(answer(200, &page("9900")))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(answer(200, &page("10000")))
        .mount(&server)
        .await;
    let mut input = json!({ "object_type": "contacts", "cursor": "9800", "limit": 100 });
    let mut pages = 0;
    loop {
        let found = invoke(&socket, &key, "objects.search", input.clone()).await.unwrap();
        pages += 1;
        match found["next_cursor"].as_str() {
            Some(cursor) => input["cursor"] = json!(cursor),
            None => break,
        }
    }
    assert_eq!(pages, 2);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2);
    assert_eq!(body_of(&received[1])["after"], "9900");
}

#[tokio::test]
async fn a_refusal_of_a_search_keeps_hubspots_own_reason() {
    let refusing = |message: &'static str| async move {
        let (server, socket, key) = hubspot().await;
        Mock::given(any())
            .respond_with(hubspot_error(400, "VALIDATION_ERROR", message))
            .mount(&server)
            .await;
        (server, socket, key)
    };
    let (_server, socket, key) = refusing("Property `nope` does not exist").await;
    let err = invoke(
        &socket,
        &key,
        "objects.search",
        json!({ "object_type": "contacts", "cursor": "20", "limit": 20 }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.message(),
        "hubspot rejected the request: Property `nope` does not exist"
    );

    // On the page that reaches the 10,000th match a refusal may be about
    // that, and HubSpot does not document what it then says. So its own
    // reason is kept, a mistake in the request with it, and the limit is
    // named beside it.
    for (cursor, limit) in [("9900", 200), ("9990", 10)] {
        let (_server, socket, key) = refusing("Property `nope` does not exist.").await;
        let err = invoke(
            &socket,
            &key,
            "objects.search",
            json!({ "object_type": "contacts", "cursor": cursor, "limit": limit }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(
            err.message(),
            "hubspot rejected the request: Property `nope` does not exist; \
             this page also reaches the 10,000th match, which is the last a search returns",
            "{cursor}"
        );
        assert_eq!(err.provider().map(|p| p.as_str()), Some("hubspot"));
    }
}

#[tokio::test]
async fn the_limit_on_searches_is_told_apart_from_hubspots_other_limits() {
    use socketkit_core::Retry;
    use std::time::Duration;
    let limited = |policy: &'static str, wait: Option<&'static str>| async move {
        let (server, socket, key) = hubspot().await;
        let response = match wait {
            Some(wait) => support::rate_limited(policy).insert_header("retry-after", wait),
            None => support::rate_limited(policy),
        };
        Mock::given(any()).respond_with(response).mount(&server).await;
        (server, socket, key)
    };
    let contacts = json!({ "object_type": "contacts" });

    // A limit by the second on a search is the search's own, five a second:
    // HubSpot no longer holds its other calls to one.
    let (_server, socket, key) = limited("SECONDLY", Some("1")).await;
    let err = invoke(&socket, &key, "objects.search", contacts.clone())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert!(err.message().contains("five searches a second"), "{}", err.message());
    assert!(
        err.message()
            .contains("listing and reading records are not held to that limit")
    );
    assert_eq!(err.retry(), Retry::After(Duration::from_secs(1)), "the wait is kept");
    assert_eq!(err.provider().map(|p| p.as_str()), Some("hubspot"));

    // The ten-second limit covers every call. A search that reaches it must
    // not be told that listing would get through.
    let (_server, socket, key) = limited("TEN_SECONDLY_ROLLING", Some("4")).await;
    let err = invoke(&socket, &key, "objects.search", contacts.clone())
        .await
        .unwrap_err();
    assert_eq!(
        (err.kind(), err.retry()),
        (ErrorKind::RateLimited, Retry::After(Duration::from_secs(4)))
    );
    assert_eq!(
        err.message(),
        "hubspot is rate limiting requests: its limit for any ten seconds was reached"
    );

    // When HubSpot names no limit, or one that is not known, nothing is
    // claimed about which it was.
    for policy in ["", "SOMETHING_NEW"] {
        let (_server, socket, key) = limited(policy, Some("2")).await;
        let err = invoke(&socket, &key, "objects.search", contacts.clone())
            .await
            .unwrap_err();
        assert_eq!(
            (err.kind(), err.retry()),
            (ErrorKind::RateLimited, Retry::After(Duration::from_secs(2))),
            "{policy}"
        );
        assert_eq!(
            err.message(),
            "hubspot is rate limiting requests; HubSpot did not say which limit, \
             and allows five searches a second for the whole account",
            "{policy}"
        );
        assert!(!err.message().contains("not held"), "{}", err.message());
    }

    // The day's requests running out is not the search limit.
    let (_server, socket, key) = limited("DAILY", None).await;
    let err = invoke(&socket, &key, "objects.search", contacts.clone())
        .await
        .unwrap_err();
    assert_eq!((err.kind(), err.retry()), (ErrorKind::RateLimited, Retry::Never));
    assert!(err.message().contains("daily request limit"), "{}", err.message());
    assert!(!err.message().contains("five searches"), "{}", err.message());

    // And a call that is not a search is never told about searches,
    // whatever the policy.
    for (policy, says) in [
        ("SECONDLY", "its limit for one second was reached"),
        ("TEN_SECONDLY_ROLLING", "its limit for any ten seconds was reached"),
        ("", "is rate limiting requests"),
    ] {
        for name in ["objects.list", "objects.batch_read"] {
            let (_server, socket, key) = limited(policy, None).await;
            let mut input = contacts.clone();
            input["ids"] = json!(["1"]);
            if name == "objects.list" {
                input = contacts.clone();
            }
            let err = invoke(&socket, &key, name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::RateLimited, "{name} {policy}");
            assert!(err.message().ends_with(says), "{name} {policy}: {}", err.message());
            assert!(!err.message().contains("searches"), "{}", err.message());
        }
    }
}

#[tokio::test]
async fn a_search_hubspot_cannot_run_is_refused_before_it_is_sent() {
    let (server, socket, key) = hubspot().await;
    let by = |property: &str| json!({ "propertyName": property });
    for (input, message) in [
        (json!({ "cursor": "abc" }), "`cursor` is not a place in a search"),
        (json!({ "cursor": "-1" }), "`cursor` is not a place in a search"),
        (
            json!({ "cursor": "20&limit=200" }),
            "`cursor` is not a place in a search",
        ),
        (
            json!({ "query": "x".repeat(3001) }),
            "`query` is at most 3,000 characters",
        ),
        (
            json!({ "sorts": [by("createdate"), by("amount")] }),
            "`sorts` takes one sort",
        ),
    ] {
        let mut input = input;
        input["object_type"] = json!("contacts");
        let err = invoke(&socket, &key, "objects.search", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{message}");
        assert!(err.message().contains(message), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The longest query HubSpot takes is counted in characters, not bytes.
    let (server, socket, key) = answering(200, json!({ "total": 0, "results": [] })).await;
    invoke(
        &socket,
        &key,
        "objects.search",
        json!({ "object_type": "contacts", "query": "é".repeat(3000) }),
    )
    .await
    .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn every_kind_of_filter_is_written_as_hubspot_names_it() {
    let (server, socket, key) = answering(200, json!({ "total": 0, "results": [] })).await;
    let filters = json!([
        { "propertyName": "amount", "operator": "BETWEEN", "value": "100", "highValue": "500" },
        { "propertyName": "dealstage", "operator": "IN", "values": ["closedwon", "closedlost"] },
        { "propertyName": "dealstage", "operator": "NOT_IN", "values": ["qualifiedtobuy"] },
        { "propertyName": "closedate", "operator": "HAS_PROPERTY" },
        { "propertyName": "hubspot_owner_id", "operator": "NOT_HAS_PROPERTY" },
        { "propertyName": "dealname", "operator": "CONTAINS_TOKEN", "value": "renew*" },
        { "propertyName": "dealname", "operator": "NOT_CONTAINS_TOKEN", "value": "test" },
        { "propertyName": "amount", "operator": "LTE", "value": "500" },
        { "propertyName": "amount", "operator": "GTE", "value": "100" },
        { "propertyName": "amount", "operator": "LT", "value": "501" },
        { "propertyName": "pipeline", "operator": "EQ", "value": "default" },
        { "propertyName": "pipeline", "operator": "NEQ", "value": "other" },
        { "propertyName": "associations.contact", "operator": "EQ", "value": "12345" }
    ]);
    let groups = json!([{ "filters": filters }]);
    invoke(
        &socket,
        &key,
        "objects.search",
        json!({ "object_type": "deals", "filterGroups": groups, "sorts": [{ "propertyName": "amount" }] }),
    )
    .await
    .unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "filterGroups": groups, "sorts": [{ "propertyName": "amount" }] })
    );

    let err = invoke(
        &socket,
        &key,
        "objects.search",
        json!({ "object_type": "deals", "filterGroups": [{ "filters": [{ "propertyName": "amount", "operator": "LIKE", "value": "secret-value" }] }] }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert_eq!(
        err.message(),
        "`filterGroups[0].filters[0].operator` has the wrong type"
    );
}

// ── Addresses ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn an_object_type_that_is_more_than_one_segment_is_refused() {
    let (server, socket, key) = hubspot().await;
    let pair =
        |from: &str, to: &str| json!({ "from_object_type": from, "from_id": "1", "to_object_type": to, "to_id": "2" });
    for bad in [
        "contacts/12345",
        "contacts/../owners",
        "../../oauth/2026-09/token",
        "..",
        ".",
        "contacts?archived=true",
        "contacts#frag",
        "contacts%2F12345",
        "contacts%2e%2e",
        "contacts\\12345",
        "contacts 1",
        " contacts",
        "",
        "   ",
    ] {
        let of_type = json!({ "object_type": bad });
        let mut record = of_type.clone();
        record["id"] = json!("1");
        let mut content = record.clone();
        content["properties"] = json!({ "email": "ada@example.com" });
        let mut field = of_type.clone();
        field["name"] = json!("email");
        let mut stage = of_type.clone();
        stage["pipeline"] = json!("default");
        for (name, input) in [
            ("objects.list", of_type.clone()),
            ("objects.get", record.clone()),
            ("objects.search", of_type.clone()),
            ("objects.batch_read", json!({ "object_type": bad, "ids": ["1"] })),
            ("objects.create", content.clone()),
            ("objects.update", content.clone()),
            (
                "objects.batch_create",
                json!({ "object_type": bad, "inputs": [{ "properties": { "a": "b" } }] }),
            ),
            (
                "objects.batch_update",
                json!({ "object_type": bad, "inputs": [{ "id": "1", "properties": { "a": "b" } }] }),
            ),
            ("objects.archive", record.clone()),
            (
                "associations.list",
                json!({ "from_object_type": bad, "from_id": "1", "to_object_type": "companies" }),
            ),
            (
                "associations.list",
                json!({ "from_object_type": "contacts", "from_id": "1", "to_object_type": bad }),
            ),
            ("associations.create", pair(bad, "companies")),
            ("associations.create", pair("contacts", bad)),
            ("associations.remove", pair(bad, "companies")),
            ("associations.remove", pair("contacts", bad)),
            ("properties.list", of_type.clone()),
            ("properties.get", field.clone()),
            ("pipelines.list", of_type.clone()),
            ("pipelines.get", stage.clone()),
        ] {
            let err = invoke(&socket, &key, name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {bad:?}");
            // What was refused is not repeated.
            assert!(
                bad.len() < 3 || !err.message().contains(bad),
                "{name}: {}",
                err.message()
            );
        }
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing reached HubSpot"
    );
}

#[tokio::test]
async fn an_id_that_would_add_a_segment_is_refused_and_anything_else_is_one_segment() {
    let (server, socket, key) = hubspot().await;
    for bad in ["1/associations/companies", "../2", "..", ".", "1\\2", "1\n2", "", "  "] {
        for (name, input) in [
            ("objects.get", json!({ "object_type": "contacts", "id": bad })),
            (
                "objects.update",
                json!({ "object_type": "contacts", "id": bad, "properties": { "a": "b" } }),
            ),
            ("objects.archive", json!({ "object_type": "contacts", "id": bad })),
            (
                "associations.list",
                json!({ "from_object_type": "contacts", "from_id": bad, "to_object_type": "companies" }),
            ),
            (
                "associations.create",
                json!({ "from_object_type": "contacts", "from_id": "1", "to_object_type": "companies", "to_id": bad }),
            ),
            (
                "associations.remove",
                json!({ "from_object_type": "contacts", "from_id": bad, "to_object_type": "companies", "to_id": "2" }),
            ),
            ("properties.get", json!({ "object_type": "contacts", "name": bad })),
            ("pipelines.get", json!({ "object_type": "deals", "pipeline": bad })),
            ("owners.get", json!({ "owner": bad })),
        ] {
            let err = invoke(&socket, &key, name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {bad:?}");
        }
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing reached HubSpot"
    );

    // With `idProperty` an id is a value the account chose. Whatever it
    // holds stays in its own segment and cannot start a query or a fragment.
    let (server, socket, key) = answering(200, contact()).await;
    for (id, written) in [
        ("ada@example.com", "ada%40example.com"),
        ("a?archived=true", "a%3Farchived%3Dtrue"),
        ("a#b", "a%23b"),
        ("a b+c", "a%20b%2Bc"),
        ("50%2F50", "50%252F50"),
        ("...", "..."),
    ] {
        invoke(
            &socket,
            &key,
            "objects.get",
            json!({ "object_type": "contacts", "id": id, "idProperty": "external_id" }),
        )
        .await
        .unwrap();
        let request = server.received_requests().await.unwrap().pop().unwrap();
        assert_eq!(request.url.path(), format!("{OBJECTS}/contacts/{written}"), "{id}");
        assert_eq!(query_of(&request), json!({ "idProperty": "external_id" }), "{id}");
    }
}

// ── What is refused before HubSpot is called ─────────────────────────────────

#[tokio::test]
async fn input_of_the_wrong_shape_is_refused_by_name_without_calling_hubspot() {
    let (server, socket, key) = hubspot().await;
    for (name, input, message) in [
        ("objects.list", json!({}), "missing field `object_type`"),
        (
            "objects.get",
            json!({ "object_type": "contacts" }),
            "missing field `id`",
        ),
        (
            "objects.create",
            json!({ "object_type": "contacts" }),
            "missing field `properties`",
        ),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts" }),
            "missing field `ids`",
        ),
        (
            "objects.list",
            json!({ "object_type": 7 }),
            "`object_type` has the wrong type",
        ),
        (
            "objects.list",
            json!({ "object_type": "contacts", "limit": "ten" }),
            "`limit` has the wrong type",
        ),
        (
            "objects.list",
            json!({ "object_type": "contacts", "properties": "email" }),
            "`properties` has the wrong type",
        ),
        // HubSpot writes every value as text, and takes it as text.
        (
            "objects.create",
            json!({ "object_type": "deals", "properties": { "amount": 4800 } }),
            "`properties.amount` has the wrong type",
        ),
        (
            "owners.get",
            json!({ "owner": "1", "idProperty": "email" }),
            "`idProperty` has the wrong type",
        ),
        (
            "associations.create",
            json!({ "from_object_type": "contacts", "from_id": "1", "to_object_type": "companies", "to_id": "2",
            "types": [{ "associationCategory": "MINE", "associationTypeId": 1 }] }),
            "`types[0].associationCategory` has the wrong type",
        ),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (ErrorKind::InvalidInput, message),
            "{name}"
        );
        assert_eq!(err.provider().map(|p| p.as_str()), Some("hubspot"));
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_field_the_operation_does_not_know_is_refused_and_named() {
    let (server, socket, key) = hubspot().await;
    for (name, input, message) in [
        // A misspelt option would otherwise be dropped, and the call made without it.
        (
            "objects.list",
            json!({ "object_type": "contacts", "property": ["email"] }),
            "`property` is not a field of this operation; check its spelling",
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "filter_groups": [] }),
            "`filter_groups` is not a field of this operation; check its spelling",
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "EQ", "val": "x" }] }] }),
            "`filterGroups[0].filters[0].val` is not a field of this operation; check its spelling",
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "sorts": [{ "propertyName": "email", "order": "DESCENDING" }] }),
            "`sorts[0].order` is not a field of this operation; check its spelling",
        ),
        (
            "objects.create",
            json!({ "object_type": "contacts", "properties": {}, "association": [] }),
            "`association` is not a field of this operation; check its spelling",
        ),
        (
            "objects.create",
            json!({ "object_type": "contacts", "properties": {}, "associations": [{ "to": { "id": "1", "type": "deal" }, "types": [] }] }),
            "`associations[0].to.type` is not a field of this operation; check its spelling",
        ),
        (
            "objects.batch_update",
            json!({ "object_type": "contacts", "inputs": [{ "id": "1", "properties": {}, "idproperty": "email" }] }),
            "`inputs[0].idproperty` is not a field of this operation; check its spelling",
        ),
        (
            "objects.archive",
            json!({ "object_type": "contacts", "id": "1", "permanent": true }),
            "`permanent` is not a field of this operation; check its spelling",
        ),
        (
            "owners.list",
            json!({ "after": "5" }),
            "`after` is not a field of this operation; check its spelling",
        ),
        // A name that does not look like one is the caller's own text, and is not repeated.
        (
            "objects.list",
            json!({ "object_type": "contacts", "ada@example.com wrote: hello": 1 }),
            "the input has a field this operation does not know",
        ),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (ErrorKind::InvalidInput, message),
            "{name}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_records_own_fields_are_the_accounts_and_none_is_refused() {
    // The schema names the options of an operation. It does not name the
    // properties of a record: every account adds its own, a field called
    // `properties` or `additionalProperties` among them if it likes.
    let (server, socket, key) = answering(201, note()).await;
    let properties = json!({
        "hs_note_body": "x", "my_custom_field__c": "y", "properties": "z", "additionalProperties": "w", "type": "v"
    });
    invoke(
        &socket,
        &key,
        "objects.create",
        json!({ "object_type": "notes", "properties": properties }),
    )
    .await
    .unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "properties": properties, "associations": [] })
    );
}

#[tokio::test]
async fn values_that_cannot_work_are_refused_before_hubspot_is_called() {
    let (server, socket, key) = hubspot().await;
    let many: Vec<Value> = (0..101)
        .map(|n| json!({ "properties": { "email": format!("p{n}@example.com") } }))
        .collect();
    let many_ids: Vec<String> = (0..101).map(|n| n.to_string()).collect();
    let many_changes: Vec<Value> = (0..101)
        .map(|n| json!({ "id": n.to_string(), "properties": { "a": "b" } }))
        .collect();
    let to_nobody = json!([{ "to": { "id": " " }, "types": [{ "associationCategory": "HUBSPOT_DEFINED", "associationTypeId": 202 }] }]);
    let no_kind = json!([{ "to": { "id": "1" }, "types": [] }]);
    for (name, input, message) in [
        (
            "objects.create",
            json!({ "properties": {} }),
            "a record needs `properties`",
        ),
        (
            "objects.create",
            json!({ "properties": { "a": "b" }, "associations": to_nobody }),
            "every association needs `to.id`",
        ),
        (
            "objects.create",
            json!({ "properties": { "a": "b" }, "associations": no_kind }),
            "every association needs one of `types`",
        ),
        (
            "objects.update",
            json!({ "id": "1", "properties": {} }),
            "`properties` names nothing to change",
        ),
        (
            "objects.batch_read",
            json!({ "ids": [] }),
            "`ids` takes from 1 to 100 records",
        ),
        (
            "objects.batch_read",
            json!({ "ids": many_ids }),
            "`ids` takes from 1 to 100 records",
        ),
        (
            "objects.batch_read",
            json!({ "ids": ["1", " "] }),
            "every one of `ids` needs a value",
        ),
        (
            "objects.batch_create",
            json!({ "inputs": [] }),
            "`inputs` takes from 1 to 100 records",
        ),
        (
            "objects.batch_create",
            json!({ "inputs": many }),
            "`inputs` takes from 1 to 100 records",
        ),
        (
            "objects.batch_create",
            json!({ "inputs": [{ "properties": { "a": "b" } }, { "properties": {} }] }),
            "a record needs `properties`",
        ),
        (
            "objects.batch_update",
            json!({ "inputs": [] }),
            "`inputs` takes from 1 to 100 records",
        ),
        (
            "objects.batch_update",
            json!({ "inputs": many_changes }),
            "`inputs` takes from 1 to 100 records",
        ),
        (
            "objects.batch_update",
            json!({ "inputs": [{ "id": "", "properties": { "a": "b" } }] }),
            "every one of `inputs` needs an `id`",
        ),
        (
            "objects.batch_update",
            json!({ "inputs": [{ "id": "1", "properties": {} }] }),
            "every one of `inputs` needs `properties` to change",
        ),
    ] {
        let mut input = input;
        input["object_type"] = json!("contacts");
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (ErrorKind::InvalidInput, message),
            "{name}"
        );
    }
    let err = invoke(
        &socket,
        &key,
        "associations.create",
        json!({ "from_object_type": "contacts", "from_id": "1", "to_object_type": "companies", "to_id": "2", "types": [] }),
    )
    .await
    .unwrap_err();
    assert_eq!(
        err.message(),
        "`types` needs a label; leave it out for the plain association"
    );
    assert!(server.received_requests().await.unwrap().is_empty());

    // A batch of exactly 100 is sent.
    let (server, socket, key) = answering(200, batch_of(json!([]))).await;
    let hundred: Vec<String> = (0..100).map(|n| n.to_string()).collect();
    invoke(
        &socket,
        &key,
        "objects.batch_read",
        json!({ "object_type": "contacts", "ids": hundred }),
    )
    .await
    .unwrap();
    assert_eq!(
        body_of(&only_request(&server).await)["inputs"].as_array().map(Vec::len),
        Some(100)
    );
}

// ── What HubSpot answers ─────────────────────────────────────────────────────

#[tokio::test]
async fn a_batch_read_returns_what_exists_and_names_what_does_not() {
    // HubSpot answers 207 when only some of the records were found.
    let partly = json!({
        "status": "COMPLETE",
        "results": [contact()],
        "numErrors": 1,
        "errors": [{ "status": "error", "category": "OBJECT_NOT_FOUND",
                     "message": "Could not get some CONTACT objects, they may be deleted or not exist. Check that ids are valid.",
                     "context": { "ids": ["99999"] } }],
        "startedAt": "2026-10-10T10:00:00.000Z", "completedAt": "2026-10-10T10:00:00.050Z"
    });
    let (server, socket, key) = answering(207, partly).await;
    let read = invoke(
        &socket,
        &key,
        "objects.batch_read",
        json!({ "object_type": "contacts", "ids": ["ada@example.com", "nobody@example.com"], "idProperty": "email", "archived": true }),
    )
    .await
    .unwrap();
    assert_eq!(read["results"].as_array().map(Vec::len), Some(1));
    assert_eq!(read["numErrors"], 1);
    assert_eq!(read["errors"][0]["category"], "OBJECT_NOT_FOUND");
    assert_eq!(read["errors"][0]["context"]["ids"], json!(["99999"]));
    let request = only_request(&server).await;
    assert_eq!(query_of(&request), json!({ "archived": "true" }));
    assert_eq!(
        body_of(&request),
        json!({ "inputs": [{ "id": "ada@example.com" }, { "id": "nobody@example.com" }], "idProperty": "email",
                "properties": [], "propertiesWithHistory": [] }),
        "a value with a slash or an at sign travels in the body, where it needs no encoding"
    );
}

#[tokio::test]
async fn labels_are_set_on_an_association_and_the_plain_one_has_to_be_confirmed() {
    let (server, socket, key) = hubspot().await;
    let labelled = format!("{OBJECTS}/contacts/12345/associations/deals/777");
    // HubSpot's own example of this answer writes the ids as numbers.
    Mock::given(method("PUT"))
        .and(path(labelled.as_str()))
        .respond_with(answer(
            201,
            &json!({ "fromObjectTypeId": "0-1", "fromObjectId": 12345, "toObjectTypeId": "0-3", "toObjectId": 777, "labels": ["Point of contact"] }),
        ))
        .mount(&server)
        .await;
    let types = json!([{ "associationCategory": "USER_DEFINED", "associationTypeId": 36 }]);
    let made = invoke(
        &socket,
        &key,
        "associations.create",
        json!({ "from_object_type": "contacts", "from_id": "12345", "to_object_type": "deals", "to_id": "777", "types": types }),
    )
    .await
    .unwrap();
    assert_eq!(
        made,
        json!({ "fromObjectId": "12345", "toObjectId": "777", "fromObjectTypeId": "0-1", "toObjectTypeId": "0-3", "labels": ["Point of contact"] })
    );
    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "PUT");
    assert_eq!(body_of(&request), types, "the labels are the whole body");

    // An answer that names no pair of records confirms nothing.
    for unconfirmed in [
        json!({ "labels": ["Point of contact"] }),
        json!({ "fromObjectId": "12345", "toObjectTypeId": "0-3", "labels": [] }),
        json!({ "fromObjectId": null, "toObjectId": 777, "labels": ["Point of contact"] }),
        json!({ "fromObjectId": "", "toObjectId": "" }),
    ] {
        let (_server, socket, key) = answering(201, unconfirmed.clone()).await;
        let err = invoke(
            &socket,
            &key,
            "associations.create",
            json!({ "from_object_type": "contacts", "from_id": "12345", "to_object_type": "deals", "to_id": "777", "types": types }),
        )
        .await
        .unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (ErrorKind::Decode, "hubspot answered without the association"),
            "{unconfirmed}"
        );
    }

    // The plain association answers as a batch does, and a batch can report
    // a failure inside an answer that succeeded.
    for unconfirmed in [
        json!({ "status": "COMPLETE", "results": [], "numErrors": 1, "errors": [{ "status": "error", "category": "VALIDATION_ERROR", "message": "nope" }] }),
        json!({ "status": "COMPLETE", "results": [{ "from": { "id": "1" }, "to": { "id": "2" } }] }),
        json!({}),
    ] {
        let (_server, socket, key) = answering(200, unconfirmed.clone()).await;
        let err = invoke(
            &socket,
            &key,
            "associations.create",
            json!({ "from_object_type": "contacts", "from_id": "12345", "to_object_type": "companies", "to_id": "67891" }),
        )
        .await
        .unwrap_err();
        assert_eq!(
            (err.kind(), err.message()),
            (ErrorKind::Decode, "hubspot answered without the association"),
            "{unconfirmed}"
        );
    }
}

#[tokio::test]
async fn what_hubspot_leaves_empty_does_not_stop_a_record_from_being_read() {
    let bare = json!({ "id": 12345, "properties": null, "createdAt": null, "updatedAt": null, "archived": null });
    let (_server, socket, key) = answering(200, bare).await;
    let record = invoke(
        &socket,
        &key,
        "objects.get",
        json!({ "object_type": "contacts", "id": "12345" }),
    )
    .await
    .unwrap();
    assert_eq!(
        record,
        json!({ "id": "12345", "properties": {}, "createdAt": null, "updatedAt": null, "archived": false }),
        "the heavy parts that were not asked for are left out, not written as null"
    );

    let sparse = json!({ "results": [{ "name": "email" }, { "name": "notes", "label": null, "type": null, "fieldType": null, "groupName": null }] });
    let (_server, socket, key) = answering(200, sparse).await;
    let fields = invoke(&socket, &key, "properties.list", json!({ "object_type": "contacts" }))
        .await
        .unwrap();
    assert_eq!(
        fields,
        json!([
            { "name": "email", "label": "", "type": "", "fieldType": "", "groupName": null },
            { "name": "notes", "label": "", "type": "", "fieldType": "", "groupName": null }
        ])
    );
}

#[tokio::test]
async fn a_list_of_properties_is_kept_to_what_is_needed_to_choose_one() {
    // An account has hundreds of properties, and HubSpot returns them all at
    // once, each with its description and every option it has.
    let (_server, socket, key) = answering(200, json!({ "results": [property()] })).await;
    let listed = invoke(
        &socket,
        &key,
        "properties.list",
        json!({ "object_type": "contacts", "archived": false }),
    )
    .await
    .unwrap();
    let row = listed[0].as_object().unwrap();
    for heavy in ["options", "description", "modificationMetadata", "createdAt"] {
        assert!(!row.contains_key(heavy), "{heavy} is what `properties.get` is for");
    }
}

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error_not_an_empty_result() {
    let contacts = json!({ "object_type": "contacts" });
    let one = json!({ "object_type": "contacts", "id": "1" });
    let pair = json!({ "from_object_type": "contacts", "from_id": "1", "to_object_type": "companies" });
    for (name, input) in [
        ("objects.list", contacts.clone()),
        ("objects.get", one.clone()),
        ("objects.search", contacts.clone()),
        ("objects.batch_read", json!({ "object_type": "contacts", "ids": ["1"] })),
        (
            "objects.create",
            json!({ "object_type": "contacts", "properties": { "a": "b" } }),
        ),
        (
            "objects.update",
            json!({ "object_type": "contacts", "id": "1", "properties": { "a": "b" } }),
        ),
        (
            "objects.batch_create",
            json!({ "object_type": "contacts", "inputs": [{ "properties": { "a": "b" } }] }),
        ),
        (
            "objects.batch_update",
            json!({ "object_type": "contacts", "inputs": [{ "id": "1", "properties": { "a": "b" } }] }),
        ),
        ("associations.list", pair),
        ("properties.list", contacts.clone()),
        ("properties.get", json!({ "object_type": "contacts", "name": "email" })),
        ("pipelines.list", json!({ "object_type": "deals" })),
        (
            "pipelines.get",
            json!({ "object_type": "deals", "pipeline": "default" }),
        ),
        ("owners.list", json!({})),
        ("owners.get", json!({ "owner": "1" })),
    ] {
        for empty in [
            json!({}),
            json!({ "status": "error", "message": "something else entirely" }),
        ] {
            let (_server, socket, key) = answering(200, empty.clone()).await;
            let err = invoke(&socket, &key, name, input.clone())
                .await
                .expect_err(&format!("{name}: {empty} is not what was asked for"));
            assert_eq!(err.kind(), ErrorKind::Decode, "{name}: {err}");
            assert!(
                err.message().starts_with("hubspot answered without"),
                "{name}: {}",
                err.message()
            );
        }
    }
    // A search that says how many matched but lists none of them, and one
    // that lists them but not how many.
    for partial in [
        json!({ "total": 3 }),
        json!({ "results": [] }),
        json!({ "results": "none", "total": 0 }),
    ] {
        let (_server, socket, key) = answering(200, partial.clone()).await;
        let err = invoke(&socket, &key, "objects.search", contacts.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{partial}");
    }
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_place_and_never_repeats_what_hubspot_sent() {
    let secret = "4111 1111 1111 1111";
    for (name, input, response, place) in [
        (
            "objects.get",
            json!({ "object_type": "contacts", "id": "1" }),
            json!({ "id": "1", "properties": { "card_number": { "value": secret } } }),
            "properties.card_number",
        ),
        (
            "objects.list",
            json!({ "object_type": "contacts" }),
            json!({ "results": [contact(), { "id": "2", "properties": [secret] }] }),
            "[1].properties",
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts" }),
            json!({ "total": 1, "results": [{ "id": { "is": secret } }] }),
            "[0].id",
        ),
        (
            "owners.get",
            json!({ "owner": "1" }),
            json!({ "id": "1", "teams": [{ "id": "t", "name": [secret] }] }),
            "teams[0].name",
        ),
        (
            "pipelines.get",
            json!({ "object_type": "deals", "pipeline": "default" }),
            json!({ "id": "default", "stages": [{ "id": "s", "metadata": { "probability": [secret] } }] }),
            "stages[0].metadata.probability",
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name}");
        assert!(
            err.message().contains(&format!("at `{place}`")),
            "{name}: {}",
            err.message()
        );
        let shown = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!shown.contains(secret) && !shown.contains("4111"), "{name}: {shown}");
    }

    // A property's name comes from HubSpot's answer too. One that does not
    // read as a name is not repeated either.
    let odd = json!({ "id": "1", "properties": { secret: { "nested": true } } });
    let (_server, socket, key) = answering(200, odd).await;
    let err = invoke(
        &socket,
        &key,
        "objects.get",
        json!({ "object_type": "contacts", "id": "1" }),
    )
    .await
    .unwrap_err();
    assert_eq!(
        (err.kind(), err.message()),
        (ErrorKind::Decode, "hubspot sent a record that could not be read")
    );
}

#[tokio::test]
async fn hubspots_refusals_reach_the_caller_as_errors_they_can_act_on() {
    for (response, kind, says) in [
        (
            hubspot_error(401, "INVALID_AUTHENTICATION", "Authentication credentials not found."),
            ErrorKind::ReconnectRequired,
            "rejected the stored authorization",
        ),
        (
            hubspot_error(404, "OBJECT_NOT_FOUND", "resource not found"),
            ErrorKind::NotFound,
            "no such resource",
        ),
        (
            hubspot_error(400, "VALIDATION_ERROR", "Property values were not valid"),
            ErrorKind::InvalidInput,
            "Property values were not valid",
        ),
        (
            hubspot_error(409, "CONFLICT", "Contact already exists. Existing ID: 12345"),
            ErrorKind::InvalidInput,
            "Existing ID: 12345",
        ),
        (
            hubspot_error(403, "FORBIDDEN", "This account does not have access to custom objects."),
            ErrorKind::AccessDenied,
            "does not have access to custom objects",
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let err = invoke(
            &socket,
            &key,
            "objects.create",
            json!({ "object_type": "contacts", "properties": { "email": "ada@example.com" } }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().contains(says), "{}", err.message());
    }
}

#[tokio::test]
async fn a_write_is_sent_once_when_hubspot_fails_and_a_read_is_tried_again() {
    let contacts = json!({ "object_type": "contacts" });
    let content = json!({ "object_type": "contacts", "properties": { "email": "ada@example.com" } });
    let mut change = content.clone();
    change["id"] = json!("1");
    for (name, input, sent) in [
        ("objects.list", contacts.clone(), 2),
        ("objects.get", json!({ "object_type": "contacts", "id": "1" }), 2),
        ("properties.list", contacts.clone(), 2),
        // A record that was created twice is two records.
        ("objects.create", content, 1),
        ("objects.update", change, 1),
        (
            "objects.batch_create",
            json!({ "object_type": "contacts", "inputs": [{ "properties": { "a": "b" } }] }),
            1,
        ),
        (
            "objects.batch_update",
            json!({ "object_type": "contacts", "inputs": [{ "id": "1", "properties": { "a": "b" } }] }),
            1,
        ),
        // The two reads that are posted are sent once too: the transport
        // cannot tell them from a write.
        ("objects.search", contacts, 1),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts", "ids": ["1"] }),
            1,
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(server.received_requests().await.unwrap().len(), sent, "{name}");
    }
}

#[tokio::test]
async fn a_put_and_a_delete_are_sent_again_after_a_server_error() {
    // The transport repeats a PUT and a DELETE after a server error, as it
    // does a read. Associating two records twice, or removing what is
    // already removed, ends as doing it once would.
    let pair = json!({ "from_object_type": "contacts", "from_id": "1", "to_object_type": "companies", "to_id": "2" });
    let mut labelled = pair.clone();
    labelled["types"] = json!([{ "associationCategory": "USER_DEFINED", "associationTypeId": 36 }]);
    for (name, input, verb) in [
        ("associations.create", pair.clone(), "PUT"),
        ("associations.create", labelled, "PUT"),
        ("associations.remove", pair, "DELETE"),
        (
            "objects.archive",
            json!({ "object_type": "contacts", "id": "1" }),
            "DELETE",
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any())
            .respond_with(ResponseTemplate::new(502))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 2, "{name}: a {verb} is tried again");
        assert!(received.iter().all(|request| request.method.as_str() == verb), "{name}");
    }

    // So an archive that failed once can still succeed without the caller
    // doing anything.
    let archive = json!({ "object_type": "contacts", "id": "1" });
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    invoke(&socket, &key, "objects.archive", archive.clone()).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // And one that did archive the record before failing is answered "not
    // found" the second time: the record is gone, and the call says so.
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(502))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(hubspot_error(404, "OBJECT_NOT_FOUND", "resource not found"))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "objects.archive", archive).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[tokio::test]
async fn locked_records_are_reported_and_a_write_is_not_sent_again() {
    use socketkit_core::Retry;
    use std::time::Duration;
    let locked = || ResponseTemplate::new(423).set_body_json(json!({ "status": "error", "message": "Locked" }));
    let content = json!({ "object_type": "companies", "properties": { "name": "Analytical Engines" } });
    let mut change = content.clone();
    change["id"] = json!("1");

    // HubSpot asks for two seconds, and that is what the caller is told.
    let (server, socket, key) = hubspot().await;
    Mock::given(any()).respond_with(locked()).mount(&server).await;
    let err = invoke(&socket, &key, "objects.create", content.clone())
        .await
        .unwrap_err();
    assert_eq!(
        (err.kind(), err.retry()),
        (ErrorKind::Unexpected, Retry::After(Duration::from_secs(2)))
    );
    assert!(err.message().contains("locked the records"), "{}", err.message());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    // HubSpot does not say that nothing of a locked request was done. So
    // even when the wait is one the transport would sit out, a write is
    // sent once and the caller decides; only what is safe to repeat is.
    for (name, input, sent) in [
        ("objects.list", json!({ "object_type": "companies" }), 2),
        ("objects.get", json!({ "object_type": "companies", "id": "1" }), 2),
        ("objects.create", content.clone(), 1),
        ("objects.update", change, 1),
        (
            "objects.batch_create",
            json!({ "object_type": "companies", "inputs": [{ "properties": { "name": "x" } }] }),
            1,
        ),
        (
            "objects.batch_update",
            json!({ "object_type": "companies", "inputs": [{ "id": "1", "properties": { "name": "x" } }] }),
            1,
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any())
            .respond_with(locked().insert_header("retry-after", "0"))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(
            (err.kind(), err.retry()),
            (ErrorKind::Unexpected, Retry::After(Duration::ZERO)),
            "{name}"
        );
        assert_eq!(server.received_requests().await.unwrap().len(), sent, "{name}");
    }

    // A request HubSpot limited was not carried out, and that one is sent
    // again, a write included.
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(support::rate_limited("TEN_SECONDLY_ROLLING"))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "objects.create", content).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn space_around_an_id_is_not_part_of_it() {
    let (server, socket, key) = hubspot().await;
    Mock::given(method("PUT"))
        .and(path(format!(
            "{OBJECTS}/contacts/12345/associations/default/companies/67891"
        )))
        .respond_with(answer(
            200,
            &batch_of(json!([{ "from": { "id": "12345" }, "to": { "id": " 67891" } }])),
        ))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{OBJECTS}/contacts/12345")))
        .respond_with(answer(200, &contact()))
        .expect(1)
        .mount(&server)
        .await;
    // The association HubSpot made is not reported as unconfirmed because
    // the id was given with a space or a line break after it.
    let made = invoke(
        &socket,
        &key,
        "associations.create",
        json!({ "from_object_type": "contacts", "from_id": " 12345 ", "to_object_type": "companies", "to_id": "67891\n" }),
    )
    .await
    .unwrap();
    assert_eq!(
        made,
        json!({ "fromObjectId": "12345", "toObjectId": "67891", "fromObjectTypeId": null, "toObjectTypeId": null, "labels": [] })
    );
    let record = invoke(
        &socket,
        &key,
        "objects.get",
        json!({ "object_type": "contacts", "id": "\t12345 " }),
    )
    .await
    .unwrap();
    assert_eq!(record["id"], "12345");
}

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let hubspot = HubSpot::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(hubspot.clone()), TOKEN).await;
    let connection = socket.connection(key).await.unwrap();
    Mock::given(method("GET"))
        .and(path(format!("{OBJECTS}/contacts")))
        .respond_with(answer(
            200,
            &json!({ "results": [contact()], "paging": { "next": { "after": "12346" } } }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{OBJECTS}/contacts/12345")))
        .respond_with(answer(200, &contact()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{OBJECTS}/deals/search")))
        .respond_with(answer(200, &json!({ "total": 1, "results": [deal()] })))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{OBJECTS}/notes")))
        .respond_with(answer(201, &note()))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .and(path(format!("{OBJECTS}/deals/777")))
        .respond_with(answer(200, &deal()))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{OBJECTS}/contacts/12345/associations/companies")))
        .respond_with(answer(200, &json!({ "results": [association()] })))
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .and(path(format!("{OBJECTS}/notes/9001/associations/default/deals/777")))
        .respond_with(answer(
            200,
            &batch_of(json!([{ "from": { "id": "9001" }, "to": { "id": "777" } }])),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("{PIPELINES}/deals")))
        .respond_with(answer(200, &json!({ "results": [pipeline()] })))
        .mount(&server)
        .await;

    let objects = hubspot.objects(&connection);
    let page = objects
        .list(
            "contacts",
            ListObjects {
                properties: Some(vec!["email".into(), "firstname".into()]),
                limit: Some(1),
                ..ListObjects::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].properties["email"].as_deref(), Some("ada@example.com"));
    assert_eq!(page.items[0].properties["phone"], None, "a property without a value");
    assert_eq!(page.next_cursor.as_deref(), Some("12346"));

    let ada = objects.get("contacts", "12345", GetObject::default()).await.unwrap();
    assert_eq!((ada.id.as_str(), ada.archived), ("12345", false));
    assert_eq!(ada.created_at.as_deref(), Some("2026-09-01T09:00:00.000Z"));

    let found = objects
        .search(
            "deals",
            Search {
                filter_groups: Some(vec![FilterGroup {
                    filters: vec![Filter {
                        property_name: "associations.contact".into(),
                        operator: FilterOperator::Eq,
                        value: Some("12345".into()),
                        high_value: None,
                        values: None,
                    }],
                }]),
                ..Search::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((found.total, found.items.len(), found.next_cursor), (1, 1, None));

    let written = objects
        .create(
            "notes",
            CreateObject {
                properties: BTreeMap::from([
                    ("hs_timestamp".to_owned(), "2026-10-10T10:00:00Z".to_owned()),
                    ("hs_note_body".to_owned(), "Agreed to renew in November.".to_owned()),
                ]),
                associations: Some(vec![NewAssociation {
                    to: ObjectId { id: "12345".into() },
                    types: vec![AssociationSpec {
                        association_category: AssociationCategory::HubspotDefined,
                        association_type_id: 202,
                    }],
                }]),
            },
        )
        .await
        .unwrap();
    assert_eq!(written.id, "9001");

    let moved = objects
        .update(
            "deals",
            "777",
            UpdateObject {
                properties: BTreeMap::from([("dealstage".to_owned(), "closedwon".to_owned())]),
                id_property: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(moved.properties["dealstage"].as_deref(), Some("closedwon"));

    let associations = hubspot.associations(&connection);
    let companies = associations
        .list("contacts", "12345", "companies", Paging::default())
        .await
        .unwrap();
    assert_eq!(companies.items[0].to_object_id, "5790939450");
    assert_eq!(
        companies.items[0].association_types[0].label.as_deref(),
        Some("Primary")
    );
    let linked = associations
        .create("notes", "9001", "deals", "777", CreateAssociation::default())
        .await
        .unwrap();
    assert_eq!(
        (linked.from_object_id.as_str(), linked.to_object_id.as_str()),
        ("9001", "777")
    );

    let pipelines = hubspot.pipelines(&connection).list("deals").await.unwrap();
    assert_eq!(pipelines[0].stages[1].id, "closedwon");
    assert_eq!(pipelines[0].stages[1].metadata["isClosed"], "true");

    let sent: Vec<Value> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method.as_str() != "GET")
        .map(body_of)
        .collect();
    assert_eq!(
        sent,
        [
            json!({ "filterGroups": [{ "filters": [{ "propertyName": "associations.contact", "operator": "EQ", "value": "12345" }] }] }),
            json!({ "properties": { "hs_timestamp": "2026-10-10T10:00:00Z", "hs_note_body": "Agreed to renew in November." },
                    "associations": [{ "to": { "id": "12345" }, "types": [{ "associationCategory": "HUBSPOT_DEFINED", "associationTypeId": 202 }] }] }),
            json!({ "properties": { "dealstage": "closedwon" } }),
            json!(null),
        ]
    );
}
