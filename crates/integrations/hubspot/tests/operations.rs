//! Every HubSpot operation, called by name against a local server that answers as HubSpot does.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use socketkit_core::{Effect, ErrorKind, Integration, Retry};
use socketkit_hubspot::models::{
    AssociationType, BatchRead, CreateAssociation, CreateObject, Filter, FilterGroup, GetObject, ListObjects, Operator,
    Paging, Search, UpdateObject,
};
use socketkit_hubspot::{HubSpot, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    Case, OBJECTS, answer, answering, batch_of, body_of, contact, contact_returned, contains, deal, hubspot,
    hubspot_error, invoke, kind, only_request, owner, pipeline, property, query_of, rate_limited, refusing,
};

/// What HubSpot answers to a default association: the link, once from each end.
fn associated_by_default() -> Value {
    batch_of(json!([
        { "from": { "id": "512" }, "to": { "id": "77" }, "associationSpec": kind(279) },
        { "from": { "id": "77" }, "to": { "id": "512" }, "associationSpec": kind(280) }
    ]))
}

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, path, query, body, status, response, returns| Case { name, input, verb, path, query, body, status, response, returns };
    let to_company = json!([{ "to": { "id": "77" }, "types": [kind(279)] }]);
    let filters = json!([{ "filters": [{ "propertyName": "lifecyclestage", "operator": "EQ", "value": "customer" }] }]);
    let sorts = json!([{ "propertyName": "createdate", "direction": "DESCENDING" }]);
    let note = json!({ "properties": { "hs_note_body": "Called, will sign Friday.", "hs_timestamp": "2026-10-09T08:00:00Z" }, "associations": [{ "to": { "id": "512" }, "types": [kind(202)] }] });
    vec![
        // objects: reading
        case("objects.list", json!({ "object_type": "contacts", "properties": ["email", "firstname", "lastname"], "limit": 2 }), "GET", "/crm/objects/2026-09/contacts",
            json!({ "properties": "email,firstname,lastname", "limit": "2" }), json!(null), 200,
            json!({ "results": [contact()], "paging": { "next": { "after": "513", "link": "https://api.hubapi.com/crm/objects/2026-09/contacts?after=513" } } }),
            json!({ "items": [contact_returned()], "next_cursor": "513" })),
        case("objects.get", json!({ "object_type": "deals", "record": "9001", "properties": ["dealname", "amount"], "associations": ["contacts"] }), "GET", "/crm/objects/2026-09/deals/9001",
            json!({ "properties": "dealname,amount", "associations": "contacts" }), json!(null), 200, deal(),
            json!({ "id": "9001", "properties": { "dealname": "Analytical Engine", "amount": "1500" },
                    "associations": { "contacts": { "results": [{ "id": "512", "type": "deal_to_contact" }, { "id": "513", "type": "deal_to_contact" }], "paging": { "next": { "after": "513" } } } } })),
        // HubSpot offers these two only as POST; they change nothing.
        case("objects.batch_read", json!({ "object_type": "contacts", "ids": ["512", "513"], "properties": ["email"] }), "POST", "/crm/objects/2026-09/contacts/batch/read", json!({}),
            json!({ "inputs": [{ "id": "512" }, { "id": "513" }], "properties": ["email"] }), 200, batch_of(json!([contact()])),
            json!({ "status": "COMPLETE", "results": [contact_returned()], "errors": [] })),
        case("objects.search", json!({ "object_type": "contacts", "query": "ada", "filterGroups": filters.clone(), "sorts": sorts.clone(), "properties": ["email"], "limit": 5 }), "POST", "/crm/objects/2026-09/contacts/search", json!({}),
            json!({ "query": "ada", "filterGroups": filters.clone(), "sorts": sorts.clone(), "properties": ["email"], "limit": 5 }), 200,
            json!({ "total": 12, "results": [contact()], "paging": { "next": { "after": "5" } } }),
            json!({ "items": [contact_returned()], "next_cursor": "5" })),

        // objects: writing
        case("objects.create", json!({ "object_type": "contacts", "properties": { "email": "ada@example.test", "firstname": "Ada" }, "associations": to_company.clone() }), "POST", "/crm/objects/2026-09/contacts", json!({}),
            json!({ "properties": { "email": "ada@example.test", "firstname": "Ada" }, "associations": to_company.clone() }), 201, contact(), contact_returned()),
        case("objects.update", json!({ "object_type": "deals", "record": "9001", "properties": { "dealstage": "closedwon", "amount": "1800" } }), "PATCH", "/crm/objects/2026-09/deals/9001", json!({}),
            json!({ "properties": { "dealstage": "closedwon", "amount": "1800" } }), 200, deal(), json!({ "id": "9001", "properties": { "dealname": "Analytical Engine" } })),
        case("objects.batch_create", json!({ "object_type": "notes", "inputs": [note.clone()] }), "POST", "/crm/objects/2026-09/notes/batch/create", json!({}),
            json!({ "inputs": [note.clone()] }), 201,
            batch_of(json!([{ "id": "4401", "properties": { "hs_note_body": "Called, will sign Friday.", "hs_object_id": "4401" }, "createdAt": "2026-10-09T08:00:00.000Z", "updatedAt": "2026-10-09T08:00:00.000Z", "archived": false }])),
            json!({ "status": "COMPLETE", "results": [{ "id": "4401", "properties": { "hs_note_body": "Called, will sign Friday." } }], "errors": [] })),
        case("objects.batch_update", json!({ "object_type": "tickets", "inputs": [{ "id": "31", "properties": { "hs_pipeline_stage": "3" } }, { "id": "ada@example.test", "idProperty": "requester_email", "properties": { "hs_ticket_priority": "HIGH" } }] }), "POST", "/crm/objects/2026-09/tickets/batch/update", json!({}),
            json!({ "inputs": [{ "id": "31", "properties": { "hs_pipeline_stage": "3" } }, { "id": "ada@example.test", "idProperty": "requester_email", "properties": { "hs_ticket_priority": "HIGH" } }] }), 200,
            batch_of(json!([{ "id": "31", "properties": { "hs_pipeline_stage": "3" }, "archived": false }, { "id": "32", "properties": { "hs_ticket_priority": "HIGH" }, "archived": false }])),
            json!({ "results": [{ "id": "31" }, { "id": "32" }] })),
        case("objects.archive", json!({ "object_type": "contacts", "record": "512" }), "DELETE", "/crm/objects/2026-09/contacts/512", json!({}), json!(null), 204, json!(null), json!(null)),

        // associations. HubSpot's own examples write the id at the other end as a number.
        case("associations.list", json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "limit": 100 }), "GET", "/crm/objects/2026-09/contacts/512/associations/companies",
            json!({ "limit": "100" }), json!(null), 200,
            json!({ "results": [{ "toObjectId": 77, "associationTypes": [{ "category": "HUBSPOT_DEFINED", "typeId": 279, "label": null }, { "category": "USER_DEFINED", "typeId": 36, "label": "Billing contact" }] }] }),
            json!({ "items": [{ "toObjectId": "77", "associationTypes": [{ "category": "HUBSPOT_DEFINED", "typeId": 279, "label": null }, { "typeId": 36, "label": "Billing contact" }] }], "next_cursor": null })),
        case("associations.create", json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77" }), "POST", "/crm/associations/2026-09/contacts/companies/batch/associate/default",
            json!({}), json!({ "inputs": [{ "from": { "id": "512" }, "to": { "id": "77" } }] }), 200, associated_by_default(),
            json!({ "fromObjectId": "512", "toObjectId": "77", "labels": [], "types": [kind(279)] })),
        case("associations.remove", json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77" }), "DELETE", "/crm/objects/2026-09/contacts/512/associations/companies/77",
            json!({}), json!(null), 204, json!(null), json!(null)),

        // properties
        case("properties.list", json!({ "object_type": "contacts" }), "GET", "/crm/properties/2026-09/contacts", json!({}), json!(null), 200,
            json!({ "results": [property()] }),
            json!([{ "name": "lifecyclestage", "label": "Lifecycle Stage", "type": "enumeration", "fieldType": "radio", "groupName": "contactinformation", "hubspotDefined": true,
                     "options": [{ "label": "Lead", "value": "lead" }, { "label": "Customer", "value": "customer" }], "modificationMetadata": { "readOnlyValue": false } }])),
        case("properties.get", json!({ "object_type": "contacts", "property": "lifecyclestage" }), "GET", "/crm/properties/2026-09/contacts/lifecyclestage", json!({}), json!(null), 200,
            property(), json!({ "name": "lifecyclestage", "type": "enumeration", "options": [{ "value": "lead" }, { "value": "customer" }] })),

        // pipelines
        case("pipelines.list", json!({ "object_type": "deals" }), "GET", "/crm/pipelines/2026-09/deals", json!({}), json!(null), 200,
            json!({ "results": [pipeline()] }),
            json!([{ "id": "default", "label": "Sales Pipeline", "stages": [
                { "id": "appointmentscheduled", "label": "Appointment Scheduled", "metadata": { "isClosed": "false", "probability": "0.2" } },
                { "id": "closedwon", "label": "Closed Won", "metadata": { "isClosed": "true" } }] }])),
        case("pipelines.get", json!({ "object_type": "tickets", "pipeline": "0" }), "GET", "/crm/pipelines/2026-09/tickets/0", json!({}), json!(null), 200,
            json!({ "id": "0", "label": "Support Pipeline", "displayOrder": 0, "archived": false, "stages": [{ "id": "1", "label": "New", "displayOrder": 0, "metadata": { "ticketState": "OPEN" }, "archived": false }] }),
            json!({ "id": "0", "label": "Support Pipeline", "stages": [{ "id": "1", "label": "New", "metadata": { "ticketState": "OPEN" } }] })),

        // owners
        case("owners.list", json!({ "email": "ada@example.test", "limit": 10 }), "GET", "/crm/owners/2026-09", json!({ "email": "ada@example.test", "limit": "10" }), json!(null), 200,
            json!({ "results": [owner()] }),
            json!({ "items": [{ "id": "41", "email": "ada@example.test", "firstName": "Ada", "lastName": "Lovelace", "type": "PERSON", "userId": 2_620_022, "teams": [{ "id": "178588", "name": "West", "primary": true }] }], "next_cursor": null })),
        case("owners.get", json!({ "owner": "2620022", "idProperty": "userId" }), "GET", "/crm/owners/2026-09/2620022", json!({ "idProperty": "userId" }), json!(null), 200,
            owner(), json!({ "id": "41", "email": "ada@example.test", "userId": 2_620_022 })),
    ]
}

#[tokio::test]
async fn the_table_below_covers_every_operation_hubspot_offers() {
    let listed: Vec<String> = HubSpot::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = cases().iter().map(|c| format!("hubspot.{}", c.name)).collect();
    tested.extend(["hubspot.identity.get".to_owned(), "hubspot.resource.resolve".to_owned()]);
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist"
    );
    assert_eq!(listed.len(), 20);
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_hubspot_sent() {
    for case in cases() {
        let (server, socket, key) = hubspot().await;
        Mock::given(method(case.verb))
            .and(path(case.path))
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
            "Bearer pat-na1-good",
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
    }
}

#[tokio::test]
async fn every_operation_describes_its_input_and_marks_what_it_changes() {
    let operations = HubSpot::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    for operation in &operations {
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert!(!operation.description.is_empty(), "{}", operation.name);
    }

    // A host lets a read run freely and asks a person before anything else,
    // so each effect is stated here and not derived from the code under test.
    // The scope of most operations depends on the object type, which is an
    // argument: those list none, and say in words how the scope is formed.
    let none: &[&str] = &[];
    let expected: [(&str, Effect, &[&str], &str); 18] = [
        ("objects.list", Effect::Read, none, "crm.objects.{type}.read"),
        ("objects.get", Effect::Read, none, "crm.objects.{type}.read"),
        ("objects.batch_read", Effect::Read, none, "crm.objects.{type}.read"),
        ("objects.search", Effect::Read, none, "crm.objects.{type}.read"),
        ("objects.create", Effect::Write, none, "crm.objects.{type}.write"),
        // Sets the properties that are named. Nothing is deleted.
        ("objects.update", Effect::Write, none, "crm.objects.{type}.write"),
        ("objects.batch_create", Effect::Write, none, "crm.objects.{type}.write"),
        ("objects.batch_update", Effect::Write, none, "crm.objects.{type}.write"),
        ("objects.archive", Effect::Destructive, none, "crm.objects.{type}.write"),
        ("associations.list", Effect::Read, none, "read scope of both"),
        ("associations.create", Effect::Write, none, "write scope of both"),
        ("associations.remove", Effect::Destructive, none, "write scope of both"),
        ("properties.list", Effect::Read, none, "crm.schemas.{type}.read"),
        ("properties.get", Effect::Read, none, "crm.schemas.{type}.read"),
        ("pipelines.list", Effect::Read, none, "crm.objects.{type}.read"),
        ("pipelines.get", Effect::Read, none, "crm.objects.{type}.read"),
        ("owners.list", Effect::Read, &["crm.objects.owners.read"], "assigned"),
        ("owners.get", Effect::Read, &["crm.objects.owners.read"], "owner"),
    ];
    assert_eq!(expected.len(), cases().len());
    for (name, effect, scopes, said) in expected {
        let operation = find(&format!("hubspot.{name}"));
        assert_eq!(operation.effect, effect, "{name}");
        assert_eq!(operation.required_scopes, scopes, "{name}");
        assert!(
            operation.description.contains(said),
            "{name} says which scope it needs: {}",
            operation.description
        );
    }

    // Nothing that changes a record is sent as a GET, which the transport
    // always repeats after a server error. The two reads HubSpot offers only
    // as POST are the only reads that are not a GET.
    for case in cases() {
        let effect = find(&format!("hubspot.{}", case.name)).effect;
        let posted_read = matches!(case.name, "objects.search" | "objects.batch_read");
        match effect {
            Effect::Read if posted_read => assert_eq!(case.verb, "POST", "{}", case.name),
            Effect::Read => assert_eq!(case.verb, "GET", "{}", case.name),
            _ => assert_ne!(case.verb, "GET", "{}", case.name),
        }
    }

    assert_eq!(find("hubspot.identity.get").required_scopes, ["oauth"]);
    let resolve = find("hubspot.resource.resolve");
    assert!(resolve.required_scopes.is_empty());
    assert!(resolve.description.contains("crm.objects.contacts.read"));

    let create = find("hubspot.objects.create");
    let required: Vec<&str> = create.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(required, ["object_type", "properties"]);
    for field in ["id", "properties", "createdAt", "updatedAt", "archived", "associations"] {
        assert!(
            create.output_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    let search = &find("hubspot.objects.search").input_schema;
    for field in [
        "object_type",
        "query",
        "filterGroups",
        "sorts",
        "properties",
        "cursor",
        "limit",
    ] {
        assert!(search["properties"].get(field).is_some(), "{field} is described");
    }
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent() {
    for (name, input, response, body) in [
        (
            "objects.list",
            json!({ "object_type": "contacts" }),
            json!({ "results": [] }),
            json!(null),
        ),
        // An empty list of names is no list.
        (
            "objects.list",
            json!({ "object_type": "contacts", "properties": [], "associations": [], "cursor": " " }),
            json!({ "results": [] }),
            json!(null),
        ),
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "512" }),
            contact(),
            json!(null),
        ),
        (
            "objects.create",
            json!({ "object_type": "contacts", "properties": { "email": "ada@example.test" } }),
            contact(),
            json!({ "properties": { "email": "ada@example.test" } }),
        ),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts", "ids": ["512"] }),
            batch_of(json!([contact()])),
            json!({ "inputs": [{ "id": "512" }] }),
        ),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts", "ids": ["512"], "properties": [] }),
            batch_of(json!([contact()])),
            json!({ "inputs": [{ "id": "512" }] }),
        ),
        // A search with nothing set matches every record, and says so with an empty body.
        (
            "objects.search",
            json!({ "object_type": "contacts" }),
            json!({ "total": 0, "results": [] }),
            json!({}),
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "HAS_PROPERTY" }] }] }),
            json!({ "total": 0, "results": [] }),
            json!({ "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "HAS_PROPERTY" }] }] }),
        ),
        (
            "associations.list",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies" }),
            json!({ "results": [] }),
            json!(null),
        ),
        ("owners.list", json!({}), json!({ "results": [] }), json!(null)),
        ("owners.get", json!({ "owner": "41" }), owner(), json!(null)),
        (
            "properties.list",
            json!({ "object_type": "contacts" }),
            json!({ "results": [] }),
            json!(null),
        ),
    ] {
        let (server, socket, key) = answering(200, response).await;
        invoke(&socket, &key, name, input.clone())
            .await
            .unwrap_or_else(|e| panic!("{name} {input}: {e}"));
        let request = only_request(&server).await;
        assert_eq!(
            request.url.query(),
            None,
            "{name} {input}: HubSpot applies its own defaults"
        );
        assert_eq!(body_of(&request), body, "{name} {input}");
    }
}

#[tokio::test]
async fn options_that_are_set_reach_hubspot_under_its_own_names() {
    for (name, input, response, query, body) in [
        (
            "objects.list",
            json!({ "object_type": "contacts", "archived": true, "associations": ["companies", "deals"], "cursor": "513", "limit": 100 }),
            json!({ "results": [] }),
            json!({ "archived": "true", "associations": "companies,deals", "after": "513", "limit": "100" }),
            json!(null),
        ),
        // A record named by a property with unique values, here its email address.
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "ada@example.test", "idProperty": "email", "archived": false }),
            contact(),
            json!({ "idProperty": "email", "archived": "false" }),
            json!(null),
        ),
        (
            "objects.update",
            json!({ "object_type": "contacts", "record": "ada@example.test", "idProperty": "email", "properties": { "firstname": "" } }),
            contact(),
            json!({ "idProperty": "email" }),
            // An empty string is how HubSpot is told to clear a property.
            json!({ "properties": { "firstname": "" } }),
        ),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts", "ids": ["ada@example.test"], "idProperty": "email", "archived": true }),
            batch_of(json!([contact()])),
            json!({ "archived": "true" }),
            json!({ "inputs": [{ "id": "ada@example.test" }], "idProperty": "email" }),
        ),
        (
            "properties.list",
            json!({ "object_type": "deals", "archived": true }),
            json!({ "results": [] }),
            json!({ "archived": "true" }),
            json!(null),
        ),
        (
            "owners.get",
            json!({ "owner": "41", "idProperty": "id", "archived": true }),
            owner(),
            json!({ "idProperty": "id", "archived": "true" }),
            json!(null),
        ),
    ] {
        let (server, socket, key) = answering(200, response).await;
        invoke(&socket, &key, name, input.clone())
            .await
            .unwrap_or_else(|e| panic!("{name} {input}: {e}"));
        let request = only_request(&server).await;
        assert_eq!(query_of(&request), query, "{name} {input}");
        assert_eq!(body_of(&request), body, "{name} {input}");
    }
}

// ── Every object type ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_same_methods_work_for_every_object_type_by_its_name_or_its_type_id() {
    // One property each type is known by, and what a new record of it needs.
    for (kind, properties) in [
        ("contacts", json!({ "email": "ada@example.test" })),
        (
            "companies",
            json!({ "name": "Analytical Engines", "domain": "example.test" }),
        ),
        (
            "deals",
            json!({ "dealname": "Analytical Engine", "dealstage": "appointmentscheduled" }),
        ),
        (
            "tickets",
            json!({ "subject": "Cannot log in", "hs_pipeline_stage": "1" }),
        ),
        (
            "notes",
            json!({ "hs_timestamp": "2026-10-09T08:00:00Z", "hs_note_body": "Called." }),
        ),
        (
            "calls",
            json!({ "hs_timestamp": "2026-10-09T08:00:00Z", "hs_call_title": "Discovery" }),
        ),
        (
            "meetings",
            json!({ "hs_timestamp": "2026-10-09T08:00:00Z", "hs_meeting_title": "Kick-off" }),
        ),
        (
            "emails",
            json!({ "hs_timestamp": "2026-10-09T08:00:00Z", "hs_email_subject": "Hello", "hs_email_direction": "EMAIL" }),
        ),
        (
            "tasks",
            json!({ "hs_timestamp": "2026-10-09T08:00:00Z", "hs_task_subject": "Follow up" }),
        ),
        // A custom object, by its type id.
        ("2-12345", json!({ "model": "Mark I" })),
    ] {
        let names: Vec<&str> = properties.as_object().unwrap().keys().map(String::as_str).collect();
        let record = json!({ "id": "1", "properties": properties.clone(), "createdAt": "2026-10-09T08:00:00.000Z", "updatedAt": "2026-10-09T08:00:00.000Z", "archived": false });
        let page = json!({ "results": [record.clone()] });
        let records = format!("{OBJECTS}/{kind}");
        let one = format!("{records}/1");
        for (name, input, verb, at, status, response, body) in [
            (
                "objects.list",
                json!({ "properties": names }),
                "GET",
                records.clone(),
                200,
                page.clone(),
                json!(null),
            ),
            (
                "objects.get",
                json!({ "record": "1", "properties": names }),
                "GET",
                one.clone(),
                200,
                record.clone(),
                json!(null),
            ),
            (
                "objects.batch_read",
                json!({ "ids": ["1"], "properties": names }),
                "POST",
                format!("{records}/batch/read"),
                200,
                batch_of(json!([record.clone()])),
                json!({ "inputs": [{ "id": "1" }], "properties": names }),
            ),
            (
                "objects.search",
                json!({ "query": "a", "properties": names }),
                "POST",
                format!("{records}/search"),
                200,
                page.clone(),
                json!({ "query": "a", "properties": names }),
            ),
            (
                "objects.create",
                json!({ "properties": properties.clone() }),
                "POST",
                records.clone(),
                201,
                record.clone(),
                json!({ "properties": properties.clone() }),
            ),
            (
                "objects.update",
                json!({ "record": "1", "properties": properties.clone() }),
                "PATCH",
                one.clone(),
                200,
                record.clone(),
                json!({ "properties": properties.clone() }),
            ),
            (
                "objects.batch_create",
                json!({ "inputs": [{ "properties": properties.clone() }] }),
                "POST",
                format!("{records}/batch/create"),
                201,
                batch_of(json!([record.clone()])),
                json!({ "inputs": [{ "properties": properties.clone() }] }),
            ),
            (
                "objects.batch_update",
                json!({ "inputs": [{ "id": "1", "properties": properties.clone() }] }),
                "POST",
                format!("{records}/batch/update"),
                200,
                batch_of(json!([record.clone()])),
                json!({ "inputs": [{ "id": "1", "properties": properties.clone() }] }),
            ),
            (
                "objects.archive",
                json!({ "record": "1" }),
                "DELETE",
                one.clone(),
                204,
                json!(null),
                json!(null),
            ),
            (
                "associations.list",
                json!({ "record": "1", "to_object_type": "contacts" }),
                "GET",
                format!("{one}/associations/contacts"),
                200,
                json!({ "results": [] }),
                json!(null),
            ),
            (
                "properties.list",
                json!({}),
                "GET",
                format!("/crm/properties/2026-09/{kind}"),
                200,
                json!({ "results": [property()] }),
                json!(null),
            ),
        ] {
            let (server, socket, key) = answering(status, response).await;
            let mut input = input;
            input["object_type"] = json!(kind);
            let output = invoke(&socket, &key, name, input)
                .await
                .unwrap_or_else(|e| panic!("{name} of {kind}: {e}"));
            let request = only_request(&server).await;
            assert_eq!(request.method.as_str(), verb, "{name} of {kind}");
            assert_eq!(request.url.path(), at, "{name} of {kind}");
            assert_eq!(body_of(&request), body, "{name} of {kind}");
            // What was read comes back with the type's own properties, as HubSpot wrote them.
            let read = match name {
                "objects.list" | "objects.search" => Some(&output["items"][0]),
                "objects.get" | "objects.create" | "objects.update" => Some(&output),
                "objects.batch_read" | "objects.batch_create" | "objects.batch_update" => Some(&output["results"][0]),
                _ => None,
            };
            if let Some(read) = read {
                assert_eq!(read["id"], "1", "{name} of {kind}");
                assert_eq!(read["properties"], properties, "{name} of {kind}");
            }
        }
    }
}

// ── Reading what HubSpot sends ───────────────────────────────────────────────

#[tokio::test]
async fn a_batch_carried_out_in_part_returns_what_worked_and_what_did_not() {
    // HubSpot answers 207 when some of the ids do not exist. That is not a
    // failure of the call: the records that were found are still wanted.
    let partial = json!({
        "status": "COMPLETE",
        "results": [contact()],
        "numErrors": 1,
        "errors": [{
            "status": "error", "category": "OBJECT_NOT_FOUND", "message": "Could not get some CONTACT objects, they may be deleted or not exist. Check that ids are valid.",
            "context": { "ids": ["999"] }
        }],
        "startedAt": "2026-10-09T08:00:00.000Z", "completedAt": "2026-10-09T08:00:00.120Z"
    });
    let (_server, socket, key) = answering(207, partial).await;
    let read = invoke(
        &socket,
        &key,
        "objects.batch_read",
        json!({ "object_type": "contacts", "ids": ["512", "999"] }),
    )
    .await
    .unwrap();
    assert_eq!(read["results"].as_array().unwrap().len(), 1);
    assert_eq!(read["results"][0]["id"], "512");
    assert_eq!(read["numErrors"], 1);
    assert_eq!(read["errors"][0]["category"], "OBJECT_NOT_FOUND");
    assert_eq!(read["errors"][0]["context"]["ids"], json!(["999"]));

    // None of them found: still an answer, with nothing in it.
    let (_server, socket, key) = answering(
        207,
        json!({ "status": "COMPLETE", "results": [], "numErrors": 1, "errors": [{ "category": "OBJECT_NOT_FOUND" }] }),
    )
    .await;
    let read = invoke(
        &socket,
        &key,
        "objects.batch_read",
        json!({ "object_type": "contacts", "ids": ["999"] }),
    )
    .await
    .unwrap();
    assert_eq!(read["results"], json!([]));
    assert_eq!(read["errors"][0]["category"], "OBJECT_NOT_FOUND");
}

#[tokio::test]
async fn two_records_are_associated_by_default_or_with_the_kinds_that_are_named() {
    // Without kinds: HubSpot's default association, asked for as a batch of one pair.
    let default = "/crm/associations/2026-09/contacts/companies/batch/associate/default";
    let (server, socket, key) = answering(200, associated_by_default()).await;
    let link = json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77" });
    let made = invoke(&socket, &key, "associations.create", link.clone())
        .await
        .unwrap();
    assert_eq!(made["types"], json!([kind(279)]), "the kind from the record's own end");
    let request = only_request(&server).await;
    assert_eq!((request.method.as_str(), request.url.path()), ("POST", default));
    assert_eq!(
        body_of(&request),
        json!({ "inputs": [{ "from": { "id": "512" }, "to": { "id": "77" } }] })
    );
    // The space around an id is not part of it: it is not sent, and the
    // association HubSpot answers with is still recognised as the one asked for.
    let (server, socket, key) = answering(200, associated_by_default()).await;
    let spaced =
        json!({ "object_type": "contacts", "record": " 512 ", "to_object_type": "companies", "to_record": " 77 " });
    let made = invoke(&socket, &key, "associations.create", spaced).await.unwrap();
    assert_eq!(
        (&made["fromObjectId"], &made["toObjectId"]),
        (&json!("512"), &json!("77"))
    );
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "inputs": [{ "from": { "id": "512" }, "to": { "id": "77" } }] })
    );
    // An empty list of kinds is no list.
    let (server, socket, key) = answering(200, associated_by_default()).await;
    let mut unlabelled = link.clone();
    unlabelled["types"] = json!([]);
    invoke(&socket, &key, "associations.create", unlabelled).await.unwrap();
    assert_eq!(only_request(&server).await.url.path(), default);

    // A pair HubSpot could not associate comes back in a success, with the
    // reason beside it. That is a refusal, and HubSpot's reason is kept.
    let refused = json!({
        "status": "COMPLETE", "results": [], "numErrors": 1,
        "errors": [{ "status": "error", "category": "VALIDATION_ERROR", "message": "No default association exists between CONTACT and TASK", "context": {} }]
    });
    let (_server, socket, key) = answering(207, refused).await;
    let err = invoke(&socket, &key, "associations.create", link.clone())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.message()
            .ends_with("No default association exists between CONTACT and TASK"),
        "{}",
        err.message()
    );

    // With kinds: they are the body, as a list, and HubSpot answers with the labels now set.
    let labelled = json!({ "fromObjectTypeId": "0-1", "fromObjectId": 512, "toObjectTypeId": "0-2", "toObjectId": 77, "labels": ["Billing contact"] });
    let (server, socket, key) = answering(201, labelled).await;
    let mut with_kinds = link.clone();
    with_kinds["types"] = json!([{ "associationCategory": "USER_DEFINED", "associationTypeId": 36 }, kind(279)]);
    let made = invoke(&socket, &key, "associations.create", with_kinds).await.unwrap();
    assert_eq!(
        made,
        json!({ "fromObjectId": "512", "toObjectId": "77", "labels": ["Billing contact"], "types": [] })
    );
    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "PUT");
    assert_eq!(
        request.url.path(),
        format!("{OBJECTS}/contacts/512/associations/companies/77")
    );
    assert_eq!(
        body_of(&request),
        json!([{ "associationCategory": "USER_DEFINED", "associationTypeId": 36 }, kind(279)])
    );
}

#[tokio::test]
async fn what_hubspot_leaves_empty_does_not_stop_an_answer_from_being_read() {
    for (name, input, sparse, id) in [
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "512" }),
            json!({ "id": "512" }),
            "512",
        ),
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "512" }),
            json!({ "id": "512", "properties": null, "createdAt": null, "updatedAt": null, "archived": null, "archivedAt": null, "associations": null }),
            "512",
        ),
        // An id written as a number is still an id.
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "512" }),
            json!({ "id": 512, "properties": {} }),
            "512",
        ),
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "512", "associations": ["companies"] }),
            json!({ "id": "512", "associations": { "companies": { "results": null, "paging": null } } }),
            "512",
        ),
        (
            "owners.get",
            json!({ "owner": "41" }),
            json!({ "id": 41, "email": null, "teams": null, "userId": null, "type": null }),
            "41",
        ),
        (
            "pipelines.get",
            json!({ "object_type": "deals", "pipeline": "default" }),
            json!({ "id": "default", "label": null, "stages": [{ "id": "s1", "label": null, "metadata": null }] }),
            "default",
        ),
    ] {
        let (_server, socket, key) = answering(200, sparse.clone()).await;
        let read = invoke(&socket, &key, name, input)
            .await
            .unwrap_or_else(|e| panic!("{name} {sparse}: {e}"));
        assert_eq!(read["id"], id, "{name} {sparse}");
    }
    let (_server, socket, key) = answering(
        200,
        json!({ "name": "amount", "label": null, "options": null, "type": "number" }),
    )
    .await;
    let input = json!({ "object_type": "deals", "property": "amount" });
    let read = invoke(&socket, &key, "properties.get", input).await.unwrap();
    assert_eq!((&read["label"], &read["options"]), (&json!(""), &json!([])));
}

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error_not_an_empty_result() {
    let contact_512 = json!({ "object_type": "contacts", "record": "512" });
    let link = json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77" });
    for (name, input, response) in [
        ("objects.get", contact_512.clone(), json!({})),
        ("objects.get", contact_512.clone(), json!({ "id": "" })),
        (
            "objects.get",
            contact_512.clone(),
            json!({ "id": null, "properties": {} }),
        ),
        (
            "objects.get",
            contact_512.clone(),
            json!({ "status": "error", "message": "not here" }),
        ),
        ("objects.list", json!({ "object_type": "contacts" }), json!({})),
        (
            "objects.list",
            json!({ "object_type": "contacts" }),
            json!({ "results": "none" }),
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts" }),
            json!({ "total": 0 }),
        ),
        (
            "objects.create",
            json!({ "object_type": "contacts", "properties": { "email": "a@example.test" } }),
            json!({ "properties": {} }),
        ),
        (
            "objects.update",
            json!({ "object_type": "contacts", "record": "512", "properties": { "firstname": "Ada" } }),
            json!(null),
        ),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts", "ids": ["512"] }),
            json!({ "status": "COMPLETE" }),
        ),
        (
            "objects.batch_create",
            json!({ "object_type": "contacts", "inputs": [{ "properties": { "email": "a@example.test" } }] }),
            json!({}),
        ),
        (
            "associations.list",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies" }),
            json!({}),
        ),
        // An answer that makes no association from this record.
        ("associations.create", link.clone(), json!({})),
        ("associations.create", link.clone(), batch_of(json!([]))),
        (
            "associations.create",
            link.clone(),
            batch_of(json!([{ "from": { "id": "77" }, "to": { "id": "512" }, "associationSpec": kind(280) }])),
        ),
        ("properties.list", json!({ "object_type": "contacts" }), json!({})),
        (
            "properties.get",
            json!({ "object_type": "contacts", "property": "email" }),
            json!({ "label": "Email" }),
        ),
        ("pipelines.list", json!({ "object_type": "deals" }), json!({})),
        (
            "pipelines.get",
            json!({ "object_type": "deals", "pipeline": "default" }),
            json!({ "label": "Sales" }),
        ),
        ("owners.list", json!({}), json!({})),
        (
            "owners.get",
            json!({ "owner": "41" }),
            json!({ "email": "ada@example.test" }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
    }
    let labelled = json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77", "types": [kind(279)] });
    let (_server, socket, key) = answering(201, json!({ "labels": [] })).await;
    let err = invoke(&socket, &key, "associations.create", labelled)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode, "{err}");
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_field_and_never_repeats_what_hubspot_sent() {
    // What HubSpot sends is the content of someone's CRM. An error is logged
    // and shown, so none of that content may travel in it, not in the
    // message and not in the cause behind it.
    let secret = "CONFIDENTIAL acquisition of Babbage Ltd";
    for (name, input, response, field) in [
        (
            "objects.get",
            json!({ "object_type": "deals", "record": "9001" }),
            json!({ "id": "9001", "properties": { "dealname": [secret] } }),
            "properties.dealname",
        ),
        (
            "objects.list",
            json!({ "object_type": "deals" }),
            json!({ "results": [deal(), { "id": "9002", "archived": secret }] }),
            "[1].archived",
        ),
        (
            "objects.search",
            json!({ "object_type": "deals" }),
            json!({ "results": [{ "id": "9001", "associations": { "contacts": { "results": [{ "id": { "is": secret } }] } } }] }),
            "[0].associations.contacts.results[0].id",
        ),
        (
            "objects.batch_read",
            json!({ "object_type": "deals", "ids": ["9001"] }),
            json!({ "results": [{ "id": "9001", "properties": { "amount": { "is": secret } } }] }),
            "results[0].properties.amount",
        ),
        (
            "owners.get",
            json!({ "owner": "41" }),
            json!({ "id": "41", "teams": [{ "id": "1", "name": [secret] }] }),
            "teams[0].name",
        ),
        (
            "pipelines.list",
            json!({ "object_type": "deals" }),
            json!({ "results": [{ "id": "default", "stages": [{ "id": "s1", "metadata": secret }] }] }),
            "[0].stages[0].metadata",
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name}");
        assert!(
            err.message().contains(&format!("`{field}`")),
            "{name}: the field is named: {}",
            err.message()
        );
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
        assert!(!everything.contains("Babbage"), "{name}: {everything}");
    }
}

// ── Paging ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn the_cursor_is_hubspots_after_value_passed_back_as_it_was_given() {
    // A cursor is an id for a list of records, and may be any text for other lists.
    let after = "NTI1Cg%3D%3D/+ &x=1";
    let next = json!({ "paging": { "next": { "after": after, "link": "https://api.hubapi.com/ignored" } } });
    let with_next = |mut page: Value| {
        page["paging"] = next["paging"].clone();
        page
    };
    for (name, input) in [
        (
            "objects.list",
            json!({ "object_type": "contacts", "properties": ["email"], "limit": 1 }),
        ),
        (
            "associations.list",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "limit": 1 }),
        ),
        ("owners.list", json!({ "limit": 1 })),
    ] {
        let (server, socket, key) = answering(200, with_next(json!({ "results": [] }))).await;
        let first = invoke(&socket, &key, name, input.clone()).await.unwrap();
        assert_eq!(first["next_cursor"], after, "{name}");
        let asked = only_request(&server).await;
        assert_eq!(
            query_of(&asked).get("after"),
            None,
            "{name}: the first page names no place"
        );

        // The other arguments are given again, as a caller in a loop would.
        let (server, socket, key) = answering(200, json!({ "results": [] })).await;
        let mut again = input.clone();
        again["cursor"] = first["next_cursor"].clone();
        let second = invoke(&socket, &key, name, again).await.unwrap();
        assert_eq!(
            second["next_cursor"],
            json!(null),
            "{name}: the last page has no cursor"
        );
        let request = only_request(&server).await;
        assert_eq!(request.method.as_str(), "GET");
        assert_eq!(
            query_of(&request)["after"],
            after,
            "{name}: nothing in it is read or changed"
        );
        assert_eq!(query_of(&request)["limit"], "1", "{name}");
        assert_eq!(
            request.url.path(),
            only_path(name),
            "{name}: a cursor cannot change what is read"
        );
        assert_eq!(request.url.fragment(), None);
    }

    // A search takes its place in the body, beside the search itself.
    let (server, socket, key) = answering(200, with_next(json!({ "total": 3, "results": [contact()] }))).await;
    let first = invoke(
        &socket,
        &key,
        "objects.search",
        json!({ "object_type": "contacts", "query": "ada", "limit": 1 }),
    )
    .await
    .unwrap();
    assert_eq!(first["next_cursor"], after);
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "query": "ada", "limit": 1 })
    );
    // HubSpot writes the place as a string. Written as a number, as its ids
    // sometimes are, it is still the place, and the list does not end there.
    let numbered = json!({ "results": [contact()], "paging": { "next": { "after": 513 } } });
    let (_server, socket, key) = answering(200, numbered).await;
    let page = invoke(&socket, &key, "objects.list", json!({ "object_type": "contacts" }))
        .await
        .unwrap();
    assert_eq!(page["next_cursor"], "513");

    let (server, socket, key) = answering(200, json!({ "total": 3, "results": [contact()] })).await;
    let input = json!({ "object_type": "contacts", "query": "ada", "limit": 1, "cursor": "1" });
    let second = invoke(&socket, &key, "objects.search", input).await.unwrap();
    assert_eq!(second["next_cursor"], json!(null));
    let request = only_request(&server).await;
    assert_eq!(body_of(&request), json!({ "query": "ada", "limit": 1, "after": "1" }));
    assert_eq!(request.url.query(), None);
}

/// Where each list is read from, whatever cursor it is given.
fn only_path(name: &str) -> String {
    match name {
        "objects.list" => format!("{OBJECTS}/contacts"),
        "associations.list" => format!("{OBJECTS}/contacts/512/associations/companies"),
        _ => "/crm/owners/2026-09".to_owned(),
    }
}

// ── Search's own limits ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_search_past_ten_thousand_results_is_its_own_error_and_says_what_to_do() {
    // A cursor at or past the cap cannot be answered, so HubSpot is not asked.
    let (server, socket, key) = answering(200, json!({ "total": 20_000, "results": [] })).await;
    for cursor in ["10000", "10200", " 12000 "] {
        let input = json!({ "object_type": "contacts", "cursor": cursor });
        let err = invoke(&socket, &key, "objects.search", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{cursor}");
        assert!(err.message().contains("10,000"), "{}", err.message());
        assert!(err.message().contains("narrow the filters"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // A page that begins before the cap and ends past it: HubSpot refuses it
    // with a 400 that does not say why, and the caller is told why.
    let refusal = || hubspot_error(400, "VALIDATION_ERROR", "There was a problem with the request.");
    let (_server, socket, key) = refusing(refusal()).await;
    let input = json!({ "object_type": "contacts", "cursor": "9900", "limit": 200 });
    let err = invoke(&socket, &key, "objects.search", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(err.message().contains("10,000"), "{}", err.message());
    assert!(err.message().contains("narrow the filters"), "{}", err.message());

    // The same page refused for a reason of its own: HubSpot's words are
    // kept, and the cap is named beside them, not in their place.
    let (_server, socket, key) = refusing(hubspot_error(
        400,
        "VALIDATION_ERROR",
        "Property \"emial\" does not exist",
    ))
    .await;
    let input = json!({ "object_type": "contacts", "cursor": "9900", "limit": 200 });
    let err = invoke(&socket, &key, "objects.search", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.message().contains("Property \"emial\" does not exist"),
        "{}",
        err.message()
    );
    assert!(err.message().contains("10,000"), "{}", err.message());

    // The last page inside the cap is asked for, and any other refusal stays HubSpot's own.
    let (server, socket, key) = answering(200, json!({ "total": 10_000, "results": [contact()] })).await;
    let input = json!({ "object_type": "contacts", "cursor": "9900", "limit": 100 });
    let page = invoke(&socket, &key, "objects.search", input.clone()).await.unwrap();
    assert_eq!(page["items"][0]["id"], "512");
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "after": "9900", "limit": 100 })
    );
    let (_server, socket, key) = refusing(hubspot_error(
        400,
        "VALIDATION_ERROR",
        "Property \"emial\" does not exist",
    ))
    .await;
    let err = invoke(&socket, &key, "objects.search", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.message().ends_with("Property \"emial\" does not exist"),
        "{}",
        err.message()
    );
}

#[tokio::test]
async fn the_limit_of_search_is_told_apart_from_the_accounts_other_limits() {
    let secondly = || rate_limited("SECONDLY", "You have reached your secondly limit.");
    let search = json!({ "object_type": "contacts", "query": "ada" });

    let (server, socket, key) = refusing(secondly().insert_header("retry-after", "1")).await;
    let err = invoke(&socket, &key, "objects.search", search.clone())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    assert_eq!(
        err.retry(),
        Retry::After(Duration::from_secs(1)),
        "the wait HubSpot gave is kept"
    );
    assert!(err.message().contains("five searches a second"), "{}", err.message());
    assert!(!server.received_requests().await.unwrap().is_empty());

    // The same answer to anything else is not about search.
    let (_server, socket, key) = refusing(secondly()).await;
    let err = invoke(&socket, &key, "objects.list", json!({ "object_type": "contacts" }))
        .await
        .unwrap_err();
    assert_eq!((err.kind(), err.retry()), (ErrorKind::RateLimited, Retry::Later));
    assert_eq!(err.message(), "hubspot is rate limiting requests");

    // A search that meets one of the account's general limits is told which.
    for (policy, said) in [("DAILY", "daily limit"), ("TEN_SECONDLY_ROLLING", "ten seconds")] {
        let (_server, socket, key) = refusing(rate_limited(policy, "You have reached your limit.")).await;
        let err = invoke(&socket, &key, "objects.search", search.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::RateLimited, "{policy}");
        assert!(err.message().contains(said), "{policy}: {}", err.message());
        assert!(!err.message().contains("searches"), "{policy}: {}", err.message());
    }
}

// ── What is refused before HubSpot is called ─────────────────────────────────

#[tokio::test]
async fn an_id_is_one_path_segment_whatever_it_contains() {
    for (id, sent) in [
        ("512", "512"),
        ("ada+tag@example.test", "ada%2Btag%40example.test"),
        ("512/associations/companies", "512%2Fassociations%2Fcompanies"),
        ("512?archived=true#x", "512%3Farchived%3Dtrue%23x"),
        ("../../../oauth/2026-09/token", "..%2F..%2F..%2Foauth%2F2026-09%2Ftoken"),
        ("a b", "a%20b"),
        ("A_b-c.d~e", "A_b-c.d~e"),
    ] {
        let (server, socket, key) = answering(200, contact()).await;
        invoke(
            &socket,
            &key,
            "objects.get",
            json!({ "object_type": "contacts", "record": id }),
        )
        .await
        .unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), format!("{OBJECTS}/contacts/{sent}"), "{id:?}");
        assert_eq!(request.url.query(), None, "{id:?}");

        // The object type is written the same way: it is the caller's text too.
        let (server, socket, key) = answering(200, json!({ "results": [] })).await;
        invoke(&socket, &key, "objects.list", json!({ "object_type": id }))
            .await
            .unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), format!("{OBJECTS}/{sent}"), "{id:?}");
        assert_eq!(request.url.query(), None, "{id:?}");
    }

    // Every id of an association, and the names of a property and a pipeline.
    let odd = "a/b?c";
    for (name, input, sent) in [
        (
            "associations.remove",
            json!({ "object_type": odd, "record": odd, "to_object_type": odd, "to_record": odd }),
            format!("{OBJECTS}/a%2Fb%3Fc/a%2Fb%3Fc/associations/a%2Fb%3Fc/a%2Fb%3Fc"),
        ),
        // A default association carries its two ids in the body, where they are only text.
        (
            "associations.create",
            json!({ "object_type": odd, "record": odd, "to_object_type": odd, "to_record": odd }),
            "/crm/associations/2026-09/a%2Fb%3Fc/a%2Fb%3Fc/batch/associate/default".to_owned(),
        ),
        (
            "associations.create",
            json!({ "object_type": odd, "record": odd, "to_object_type": odd, "to_record": odd, "types": [kind(1)] }),
            format!("{OBJECTS}/a%2Fb%3Fc/a%2Fb%3Fc/associations/a%2Fb%3Fc/a%2Fb%3Fc"),
        ),
        (
            "properties.get",
            json!({ "object_type": odd, "property": odd }),
            "/crm/properties/2026-09/a%2Fb%3Fc/a%2Fb%3Fc".to_owned(),
        ),
        (
            "pipelines.get",
            json!({ "object_type": odd, "pipeline": odd }),
            "/crm/pipelines/2026-09/a%2Fb%3Fc/a%2Fb%3Fc".to_owned(),
        ),
        (
            "owners.get",
            json!({ "owner": odd }),
            "/crm/owners/2026-09/a%2Fb%3Fc".to_owned(),
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any())
            .respond_with(answer(404, &json!(null)))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::NotFound, "{name}");
        let received = server.received_requests().await.unwrap();
        assert_eq!(received[0].url.path(), sent, "{name}");
        assert_eq!(received[0].url.query(), None, "{name}");
    }

    // A segment that is only dots would be resolved away, and address something else.
    let (server, socket, key) = answering(200, contact()).await;
    for id in [".", ".."] {
        for (name, input) in [
            ("objects.get", json!({ "object_type": "contacts", "record": id })),
            ("objects.get", json!({ "object_type": id, "record": "512" })),
            ("objects.archive", json!({ "object_type": "contacts", "record": id })),
            ("objects.list", json!({ "object_type": id })),
            ("objects.search", json!({ "object_type": id })),
            ("properties.list", json!({ "object_type": id })),
            ("pipelines.get", json!({ "object_type": "deals", "pipeline": id })),
            ("owners.get", json!({ "owner": id })),
            (
                "associations.remove",
                json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": id }),
            ),
        ] {
            let err = invoke(&socket, &key, name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {id:?}");
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn input_of_the_wrong_shape_is_refused_by_name_without_calling_hubspot() {
    let (server, socket, key) = answering(200, contact()).await;
    for (name, input, field) in [
        ("objects.get", json!({ "record": "512" }), "object_type"),
        ("objects.get", json!({ "object_type": "contacts" }), "record"),
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": 512 }),
            "record",
        ),
        ("objects.list", json!({ "object_type": ["contacts"] }), "object_type"),
        ("objects.create", json!({ "object_type": "contacts" }), "properties"),
        (
            "objects.update",
            json!({ "object_type": "contacts", "record": "512" }),
            "properties",
        ),
        ("objects.batch_read", json!({ "object_type": "contacts" }), "ids"),
        ("objects.batch_create", json!({ "object_type": "contacts" }), "inputs"),
        (
            "associations.list",
            json!({ "object_type": "contacts", "record": "512" }),
            "to_object_type",
        ),
        (
            "associations.create",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies" }),
            "to_record",
        ),
        ("properties.get", json!({ "object_type": "contacts" }), "property"),
        ("pipelines.list", json!({}), "object_type"),
        ("owners.get", json!({}), "owner"),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(
            err.message().contains(field),
            "{name}: the message names the field: {}",
            err.message()
        );
    }
    // A value of the wrong type is refused, and what was sent is not repeated.
    for (name, input) in [
        // HubSpot stores every value as a string, and that is how it is written.
        (
            "objects.create",
            json!({ "object_type": "deals", "properties": { "amount": 1500 } }),
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "LIKE", "value": "hunter2" }] }] }),
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "sorts": [{ "propertyName": "createdate", "direction": "hunter2" }] }),
        ),
        ("owners.get", json!({ "owner": "41", "idProperty": "hunter2" })),
        ("objects.list", json!({ "object_type": "contacts", "limit": "hunter2" })),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(!err.message().contains("hunter2"), "{}", err.message());
        assert!(!err.message().contains("1500"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_field_the_operation_does_not_know_is_refused_and_named() {
    // A key that is not known would be dropped without a word, and what it
    // said with it: a filter, the properties to return, "only the archived
    // ones". A search that drops its filter returns every record.
    let (server, socket, key) = answering(200, contact()).await;
    let filter = json!({ "propertyName": "email", "operator": "EQ", "value": "ada@example.test" });
    for (name, input, field) in [
        // Another way of writing a known field.
        (
            "objects.list",
            json!({ "object_type": "contacts", "Properties": ["email"] }),
            "Properties",
        ),
        (
            "objects.list",
            json!({ "object_type": "contacts", "after": "513" }),
            "after",
        ),
        ("objects.list", json!({ "objectType": "contacts" }), "objectType"),
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "512", "id_property": "email" }),
            "id_property",
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "filter_groups": [{ "filters": [filter.clone()] }] }),
            "filter_groups",
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "filters": [filter.clone()] }),
            "filters",
        ),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts", "ids": ["512"], "inputs": [{ "id": "513" }] }),
            "inputs",
        ),
        (
            "associations.create",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77", "label": "Billing" }),
            "label",
        ),
        ("owners.list", json!({ "Email": "ada@example.test" }), "Email"),
        (
            "pipelines.list",
            json!({ "object_type": "deals", "archived": true }),
            "archived",
        ),
        // The same below the top, where it is said in which field.
        (
            "objects.search",
            json!({ "object_type": "contacts", "filterGroups": [{ "filters": [filter.clone(), { "propertyName": "email", "operator": "EQ", "Value": "x" }] }] }),
            "filterGroups[0].filters[1].Value",
        ),
        (
            "objects.search",
            json!({ "object_type": "contacts", "sorts": [{ "propertyName": "createdate", "order": "DESCENDING" }] }),
            "sorts[0].order",
        ),
        (
            "objects.create",
            json!({ "object_type": "notes", "properties": { "hs_note_body": "x" }, "associations": [{ "to": { "id": "512", "type": "contact" }, "types": [kind(202)] }] }),
            "associations[0].to.type",
        ),
        (
            "objects.batch_update",
            json!({ "object_type": "contacts", "inputs": [{ "id": "512", "properties": { "firstname": "Ada" } }, { "id": "513", "property": { "firstname": "Grace" } }] }),
            "inputs[1].property",
        ),
        (
            "associations.create",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77", "types": [{ "associationCategory": "USER_DEFINED", "typeId": 36 }] }),
            "types[0].typeId",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert!(
            err.message().contains(&format!("`{field}`")),
            "{name}: the field is named: {}",
            err.message()
        );
    }

    // The name of a field is the caller's own text. One that does not look
    // like a name is not repeated, and neither is any value.
    for input in [
        json!({ "object_type": "contacts", "record": "512", "my password is hunter2": true }),
        json!({ "object_type": "contacts", "record": "512", "x": "hunter2", "a-very-long-key-that-goes-on-and-on-and-on-well-past-forty-characters": 1 }),
    ] {
        let err = invoke(&socket, &key, "objects.get", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(!err.message().contains("hunter2"), "{}", err.message());
        assert!(!err.message().contains("past-forty"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // A record's properties are whatever the account defines, so any name is
    // taken there, the names of this operation's own fields among them.
    let custom = json!({ "object_type": "contacts", "properties": { "favourite_engine": "Mark I", "properties": "x", "additionalProperties": "y", "limit": "z" } });
    invoke(&socket, &key, "objects.create", custom.clone()).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await)["properties"],
        custom["properties"]
    );

    // What the schema says is what is enforced.
    for operation in HubSpot::new().operations() {
        if operation.name.ends_with("identity.get") || operation.name.ends_with("resource.resolve") {
            continue;
        }
        let schema = &operation.input_schema;
        assert_eq!(schema["additionalProperties"], false, "{}", operation.name);
        assert!(
            schema["properties"].get("additionalProperties").is_none(),
            "{}: the fields themselves are left as they are",
            operation.name
        );
    }
    let create = HubSpot::new()
        .operations()
        .into_iter()
        .find(|o| o.name == "hubspot.objects.create")
        .unwrap();
    assert_eq!(
        create.input_schema["properties"]["properties"]["additionalProperties"]["type"], "string",
        "a record's properties stay open to any name"
    );
}

#[tokio::test]
async fn values_that_cannot_work_are_refused_before_hubspot_is_called() {
    let (server, socket, key) = answering(200, contact()).await;
    let many = |n: usize| (1..=n).map(|i| i.to_string()).collect::<Vec<_>>();
    let filter = json!({ "propertyName": "email", "operator": "EQ", "value": "a@example.test" });
    let group = |n: usize| json!({ "filters": vec![filter.clone(); n] });
    let contacts = |extra: Value| {
        let mut input = json!({ "object_type": "contacts" });
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        input
    };
    let link = |extra: Value| {
        let mut input =
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77" });
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        input
    };
    let bad = [
        // Blank ids and object types.
        ("objects.get", contacts(json!({ "record": "" }))),
        ("objects.get", contacts(json!({ "record": "  " }))),
        ("objects.get", json!({ "object_type": "", "record": "512" })),
        ("objects.list", json!({ "object_type": " " })),
        ("objects.archive", contacts(json!({ "record": "" }))),
        (
            "objects.update",
            contacts(json!({ "record": " ", "properties": { "firstname": "Ada" } })),
        ),
        (
            "associations.list",
            json!({ "object_type": "contacts", "record": "", "to_object_type": "companies" }),
        ),
        (
            "associations.list",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "" }),
        ),
        ("associations.create", link(json!({ "to_record": " " }))),
        ("associations.remove", link(json!({ "record": "" }))),
        ("properties.get", contacts(json!({ "property": "" }))),
        ("pipelines.list", json!({ "object_type": "" })),
        ("pipelines.get", json!({ "object_type": "deals", "pipeline": " " })),
        ("owners.get", json!({ "owner": "" })),
        ("owners.list", json!({ "email": " " })),
        // A page larger than HubSpot returns: 100 records, 200 search results, 500 associations or owners.
        ("objects.list", contacts(json!({ "limit": 0 }))),
        ("objects.list", contacts(json!({ "limit": 101 }))),
        ("objects.search", contacts(json!({ "limit": 0 }))),
        ("objects.search", contacts(json!({ "limit": 201 }))),
        (
            "associations.list",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "limit": 501 }),
        ),
        ("owners.list", json!({ "limit": 501 })),
        ("owners.list", json!({ "limit": 0 })),
        // A batch with nothing in it, or with more than HubSpot takes at once.
        ("objects.batch_read", contacts(json!({ "ids": [] }))),
        ("objects.batch_read", contacts(json!({ "ids": many(101) }))),
        ("objects.batch_read", contacts(json!({ "ids": ["512", " "] }))),
        (
            "objects.batch_read",
            contacts(json!({ "ids": ["512"], "idProperty": "" })),
        ),
        ("objects.batch_create", contacts(json!({ "inputs": [] }))),
        (
            "objects.batch_create",
            contacts(json!({ "inputs": vec![json!({ "properties": { "email": "a@example.test" } }); 101] })),
        ),
        ("objects.batch_update", contacts(json!({ "inputs": [] }))),
        (
            "objects.batch_update",
            contacts(json!({ "inputs": vec![json!({ "id": "1", "properties": { "firstname": "A" } }); 101] })),
        ),
        (
            "objects.batch_update",
            contacts(json!({ "inputs": [{ "id": "", "properties": { "firstname": "A" } }] })),
        ),
        // An update with nothing to change.
        ("objects.update", contacts(json!({ "record": "512", "properties": {} }))),
        (
            "objects.batch_update",
            contacts(
                json!({ "inputs": [{ "id": "512", "properties": { "firstname": "A" } }, { "id": "513", "properties": {} }] }),
            ),
        ),
        // A property with no name, and a list of names HubSpot would read as other names.
        ("objects.create", contacts(json!({ "properties": { "": "x" } }))),
        (
            "objects.update",
            contacts(json!({ "record": "512", "properties": { " ": "x" } })),
        ),
        ("objects.list", contacts(json!({ "properties": ["email", ""] }))),
        ("objects.list", contacts(json!({ "properties": ["email,firstname"] }))),
        (
            "objects.get",
            contacts(json!({ "record": "512", "associations": [" "] })),
        ),
        ("objects.get", contacts(json!({ "record": "512", "idProperty": " " }))),
        ("objects.search", contacts(json!({ "properties": ["a,b"] }))),
        // An association of a new record that names no record, or no kind.
        (
            "objects.create",
            contacts(
                json!({ "properties": { "email": "a@example.test" }, "associations": [{ "to": { "id": "" }, "types": [kind(279)] }] }),
            ),
        ),
        (
            "objects.create",
            contacts(
                json!({ "properties": { "email": "a@example.test" }, "associations": [{ "to": { "id": "77" }, "types": [] }] }),
            ),
        ),
        (
            "associations.create",
            link(json!({ "types": [{ "associationCategory": " ", "associationTypeId": 36 }] })),
        ),
        // What HubSpot's search refuses: 5 groups, 6 filters in one, 18 in all, one sort, 3,000 characters.
        ("objects.search", contacts(json!({ "filterGroups": vec![group(1); 6] }))),
        ("objects.search", contacts(json!({ "filterGroups": [group(7)] }))),
        ("objects.search", contacts(json!({ "filterGroups": vec![group(5); 4] }))),
        (
            "objects.search",
            contacts(json!({ "filterGroups": [{ "filters": [] }] })),
        ),
        (
            "objects.search",
            contacts(json!({ "sorts": [{ "propertyName": "createdate" }, { "propertyName": "email" }] })),
        ),
        (
            "objects.search",
            contacts(json!({ "sorts": [{ "propertyName": " " }] })),
        ),
        ("objects.search", contacts(json!({ "query": "a".repeat(3001) }))),
        // A filter without what its operator compares with.
        (
            "objects.search",
            contacts(
                json!({ "filterGroups": [{ "filters": [{ "propertyName": "", "operator": "EQ", "value": "x" }] }] }),
            ),
        ),
        (
            "objects.search",
            contacts(json!({ "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "EQ" }] }] })),
        ),
        (
            "objects.search",
            contacts(
                json!({ "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "IN", "value": "x" }] }] }),
            ),
        ),
        (
            "objects.search",
            contacts(
                json!({ "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "NOT_IN", "values": [] }] }] }),
            ),
        ),
        (
            "objects.search",
            contacts(
                json!({ "filterGroups": [{ "filters": [{ "propertyName": "amount", "operator": "BETWEEN", "value": "1" }] }] }),
            ),
        ),
    ];
    for (name, input) in bad {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The largest of each is taken.
    for (name, input, response) in [
        (
            "objects.list",
            contacts(json!({ "limit": 100 })),
            json!({ "results": [] }),
        ),
        (
            "objects.search",
            contacts(json!({ "limit": 200, "filterGroups": vec![group(6); 3], "query": "a".repeat(3000) })),
            json!({ "results": [] }),
        ),
        (
            "objects.batch_read",
            contacts(json!({ "ids": many(100) })),
            batch_of(json!([])),
        ),
        ("owners.list", json!({ "limit": 500 }), json!({ "results": [] })),
        (
            "objects.search",
            contacts(json!({ "filterGroups": [{ "filters": [
                { "propertyName": "amount", "operator": "BETWEEN", "value": "1", "highValue": "9" },
                { "propertyName": "dealstage", "operator": "IN", "values": ["a", "b"] },
                { "propertyName": "closedate", "operator": "NOT_HAS_PROPERTY" }] }] })),
            json!({ "results": [] }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        invoke(&socket, &key, name, input.clone())
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

// ── HubSpot's answers ────────────────────────────────────────────────────────

#[tokio::test]
async fn hubspots_refusals_reach_the_caller_as_errors_they_can_act_on() {
    let missing_scopes = || {
        socketkit_testkit::wiremock::ResponseTemplate::new(403).set_body_json(json!({
            "status": "error",
            "message": "This app hasn't been granted all required scopes to make this call. Read more about required scopes here: https://developers.hubspot.com/scopes.",
            "correlationId": "c033cdaa-2c40-4a64-ae48-b4cec88dad24",
            "errors": [{ "message": "One or more of the following scopes are required.", "context": { "requiredGranularScopes": ["crm.objects.deals.read", "crm.schemas.deals.read", "crm.objects.deals.sensitive.read.v2"] } }],
            "links": { "scopes": "https://developers.hubspot.com/scopes" },
            "category": "MISSING_SCOPES"
        }))
    };
    for (response, kind, retry, said) in [
        (
            hubspot_error(
                401,
                "EXPIRED_AUTHENTICATION",
                "The OAuth token used to make this call expired 5 minute(s) ago.",
            ),
            ErrorKind::ReconnectRequired,
            Retry::Never,
            "rejected the stored authorization",
        ),
        (
            hubspot_error(401, "INVALID_AUTHENTICATION", "Authentication credentials not found."),
            ErrorKind::ReconnectRequired,
            Retry::Never,
            "rejected the stored authorization",
        ),
        // The scopes that would do are named, so that one can be added to the app.
        (
            missing_scopes(),
            ErrorKind::AccessDenied,
            Retry::Never,
            "any one of these grants it: crm.objects.deals.read, crm.schemas.deals.read, crm.objects.deals.sensitive.read.v2",
        ),
        (
            hubspot_error(
                403,
                "FORBIDDEN",
                "You do not have permissions to view object type DEAL.",
            ),
            ErrorKind::AccessDenied,
            Retry::Never,
            "You do not have permissions to view object type DEAL.",
        ),
        (
            hubspot_error(
                404,
                "OBJECT_NOT_FOUND",
                "Object not found. objectId are usually numeric.",
            ),
            ErrorKind::NotFound,
            Retry::Never,
            "has no such resource",
        ),
        (
            hubspot_error(400, "VALIDATION_ERROR", "Property \"testproperty\" does not exist"),
            ErrorKind::InvalidInput,
            Retry::Never,
            "Property \"testproperty\" does not exist",
        ),
        (
            hubspot_error(409, "CONFLICT", "Contact already exists. Existing ID: 512"),
            ErrorKind::InvalidInput,
            Retry::Never,
            "Contact already exists. Existing ID: 512",
        ),
        // The two limits of an account, each named, with the wait when HubSpot gives one.
        (
            rate_limited(
                "TEN_SECONDLY_ROLLING",
                "You have reached your ten_secondly_rolling limit.",
            ),
            ErrorKind::RateLimited,
            Retry::Later,
            "in ten seconds, and they are used up; wait a few seconds and try again",
        ),
        (
            rate_limited(
                "TEN_SECONDLY_ROLLING",
                "You have reached your ten_secondly_rolling limit.",
            )
            .insert_header("retry-after", "7"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(7)),
            "in ten seconds",
        ),
        (
            rate_limited("DAILY", "You have reached your daily limit."),
            ErrorKind::RateLimited,
            Retry::Later,
            "daily limit of requests to hubspot is used up; it starts again at midnight in the account's time zone",
        ),
        (
            rate_limited("DAILY", "You have reached your daily limit.").insert_header("retry-after", "3600"),
            ErrorKind::RateLimited,
            Retry::After(Duration::from_secs(3600)),
            "daily limit",
        ),
        (
            hubspot_error(503, "SERVICE_UNAVAILABLE", "Service unavailable"),
            ErrorKind::Unexpected,
            Retry::Later,
            "HTTP 503",
        ),
    ] {
        let (_server, socket, key) = refusing(response).await;
        let input = json!({ "object_type": "deals", "record": "9001" });
        let err = invoke(&socket, &key, "objects.archive", input).await.unwrap_err();
        assert_eq!((err.kind(), err.retry()), (kind, retry), "{err}");
        assert!(err.message().contains(said), "{}", err.message());
        assert_eq!(err.provider().map(|p| p.as_str()), Some("hubspot"));
    }
}

#[tokio::test]
async fn a_write_is_sent_once_when_hubspot_fails_and_a_read_is_tried_again() {
    let unavailable = || hubspot_error(503, "SERVICE_UNAVAILABLE", "Service unavailable");
    for (name, input) in [
        (
            "objects.create",
            json!({ "object_type": "contacts", "properties": { "email": "ada@example.test" } }),
        ),
        (
            "objects.update",
            json!({ "object_type": "contacts", "record": "512", "properties": { "firstname": "Ada" } }),
        ),
        (
            "objects.batch_create",
            json!({ "object_type": "contacts", "inputs": [{ "properties": { "email": "ada@example.test" } }] }),
        ),
        (
            "objects.batch_update",
            json!({ "object_type": "contacts", "inputs": [{ "id": "512", "properties": { "firstname": "Ada" } }] }),
        ),
        // Reads by effect, but HubSpot takes them as POST, so they are not repeated either.
        ("objects.search", json!({ "object_type": "contacts", "query": "ada" })),
        (
            "objects.batch_read",
            json!({ "object_type": "contacts", "ids": ["512"] }),
        ),
        (
            "associations.create",
            json!({ "object_type": "contacts", "record": "512", "to_object_type": "companies", "to_record": "77" }),
        ),
        // `objects.archive` and `associations.remove`, and an association
        // made with named kinds, are not in this list. HubSpot takes them as
        // DELETE and PUT, which the transport still repeats after a server
        // error; see the guide.
    ] {
        let (server, socket, key) = refusing(unavailable()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have happened, so it is not sent again"
        );
    }

    for (name, input, response) in [
        (
            "objects.get",
            json!({ "object_type": "contacts", "record": "512" }),
            contact(),
        ),
        (
            "objects.list",
            json!({ "object_type": "contacts" }),
            json!({ "results": [contact()] }),
        ),
        (
            "properties.list",
            json!({ "object_type": "contacts" }),
            json!({ "results": [property()] }),
        ),
    ] {
        let (server, socket, key) = hubspot().await;
        Mock::given(any())
            .respond_with(unavailable())
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(any())
            .respond_with(answer(200, &response))
            .mount(&server)
            .await;
        invoke(&socket, &key, name, input)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(server.received_requests().await.unwrap().len(), 2, "{name}");
    }

    // A write HubSpot throttled was not carried out, so it is sent again.
    let (server, socket, key) = hubspot().await;
    Mock::given(any())
        .respond_with(rate_limited(
            "TEN_SECONDLY_ROLLING",
            "You have reached your ten_secondly_rolling limit.",
        ))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(answer(201, &contact()))
        .mount(&server)
        .await;
    let input = json!({ "object_type": "contacts", "properties": { "email": "ada@example.test" } });
    let created = invoke(&socket, &key, "objects.create", input).await.unwrap();
    assert_eq!(created["id"], "512");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let hubspot = HubSpot::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(hubspot.clone()), "pat-na1-good").await;
    let connection = socket.connection(key).await.unwrap();
    let mount = |verb: &'static str, at: String, status: u16, body: Value| {
        Mock::given(method(verb))
            .and(path(at))
            .respond_with(answer(status, &body))
            .mount(&server)
    };
    mount("POST", format!("{OBJECTS}/contacts"), 201, contact()).await;
    mount("PATCH", format!("{OBJECTS}/contacts/512"), 200, contact()).await;
    mount("GET", format!("{OBJECTS}/contacts/512"), 200, contact()).await;
    mount(
        "GET",
        format!("{OBJECTS}/contacts"),
        200,
        json!({ "results": [contact()], "paging": { "next": { "after": "513" } } }),
    )
    .await;
    mount(
        "POST",
        format!("{OBJECTS}/contacts/search"),
        200,
        json!({ "total": 1, "results": [contact()] }),
    )
    .await;
    mount(
        "POST",
        format!("{OBJECTS}/contacts/batch/read"),
        200,
        batch_of(json!([contact()])),
    )
    .await;
    mount(
        "PUT",
        format!("{OBJECTS}/contacts/512/associations/companies/77"),
        201,
        json!({ "fromObjectId": "512", "toObjectId": "77", "labels": [] }),
    )
    .await;
    mount("GET", format!("{OBJECTS}/contacts/512/associations/companies"), 200, json!({ "results": [{ "toObjectId": "77", "associationTypes": [{ "category": "HUBSPOT_DEFINED", "typeId": 279 }] }] })).await;
    mount("DELETE", format!("{OBJECTS}/contacts/512"), 204, json!(null)).await;
    mount(
        "GET",
        "/crm/properties/2026-09/contacts".to_owned(),
        200,
        json!({ "results": [property()] }),
    )
    .await;
    mount(
        "GET",
        "/crm/pipelines/2026-09/deals".to_owned(),
        200,
        json!({ "results": [pipeline()] }),
    )
    .await;
    mount(
        "GET",
        "/crm/owners/2026-09".to_owned(),
        200,
        json!({ "results": [owner()] }),
    )
    .await;

    let objects = hubspot.objects(&connection);
    let created = objects
        .create(
            "contacts",
            CreateObject::with([("email", "ada@example.test"), ("firstname", "Ada")]),
        )
        .await
        .unwrap();
    assert_eq!(created.id, "512");
    assert_eq!(created.properties["email"].as_deref(), Some("ada@example.test"));
    assert_eq!(
        created.properties["lastname"], None,
        "a property with no value is kept as none"
    );
    assert_eq!(created.created_at.as_deref(), Some("2026-10-01T09:00:00.000Z"));
    assert!(!created.archived);

    let changed = objects
        .update("contacts", "512", UpdateObject::with([("firstname", "Ada")]))
        .await
        .unwrap();
    assert_eq!(changed.id, "512");
    let read = objects
        .get(
            "contacts",
            "512",
            GetObject {
                properties: Some(vec!["email".into()]),
                ..GetObject::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(read.properties["firstname"].as_deref(), Some("Ada"));
    let page = objects.list("contacts", ListObjects::default()).await.unwrap();
    assert_eq!((page.items.len(), page.next_cursor.as_deref()), (1, Some("513")));
    let found = objects
        .search(
            "contacts",
            Search {
                filter_groups: Some(vec![FilterGroup {
                    filters: vec![Filter::new("email", Operator::Eq, "ada@example.test")],
                }]),
                ..Search::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((found.items[0].id.as_str(), found.next_cursor), ("512", None));
    let batch = objects
        .batch_read(
            "contacts",
            BatchRead {
                ids: vec!["512".into()],
                ..BatchRead::default()
            },
        )
        .await
        .unwrap();
    assert_eq!((batch.results.len(), batch.errors.len()), (1, 0));

    let associations = hubspot.associations(&connection);
    let labelled = CreateAssociation {
        types: Some(vec![AssociationType::hubspot_defined(279)]),
    };
    let made = associations
        .create("contacts", "512", "companies", "77", labelled)
        .await
        .unwrap();
    assert_eq!(
        (made.from_object_id.as_str(), made.to_object_id.as_str()),
        ("512", "77")
    );
    let linked = associations
        .list("contacts", "512", "companies", Paging::default())
        .await
        .unwrap();
    assert_eq!(linked.items[0].to_object_id, "77");
    assert_eq!(linked.items[0].association_types[0].type_id, Some(279));
    objects.archive("contacts", "512").await.unwrap();

    let fields = hubspot
        .properties(&connection)
        .list("contacts", Default::default())
        .await
        .unwrap();
    assert_eq!(
        (fields[0].name.as_str(), fields[0].options[1].value.as_str()),
        ("lifecyclestage", "customer")
    );
    let pipelines = hubspot.pipelines(&connection).list("deals").await.unwrap();
    assert_eq!(pipelines[0].stages[1].metadata["isClosed"].as_deref(), Some("true"));
    let owners = hubspot.owners(&connection).list(Default::default()).await.unwrap();
    assert_eq!(
        (owners.items[0].id.as_str(), owners.items[0].user_id),
        ("41", Some(2_620_022))
    );

    // What reached HubSpot is what the named operations send.
    let received = server.received_requests().await.unwrap();
    let sent = |verb: &str, at: String| -> Value {
        let request = received
            .iter()
            .find(|request| request.method.as_str() == verb && request.url.path() == at)
            .unwrap_or_else(|| panic!("{verb} {at}"));
        json!({ "query": query_of(request), "body": body_of(request) })
    };
    assert_eq!(
        sent("POST", format!("{OBJECTS}/contacts")),
        json!({ "query": {}, "body": { "properties": { "email": "ada@example.test", "firstname": "Ada" } } })
    );
    assert_eq!(
        sent("PATCH", format!("{OBJECTS}/contacts/512")),
        json!({ "query": {}, "body": { "properties": { "firstname": "Ada" } } })
    );
    assert_eq!(
        sent("GET", format!("{OBJECTS}/contacts/512")),
        json!({ "query": { "properties": "email" }, "body": null })
    );
    assert_eq!(
        sent("POST", format!("{OBJECTS}/contacts/search")),
        json!({ "query": {}, "body": { "filterGroups": [{ "filters": [{ "propertyName": "email", "operator": "EQ", "value": "ada@example.test" }] }] } })
    );
    assert_eq!(
        sent("POST", format!("{OBJECTS}/contacts/batch/read")),
        json!({ "query": {}, "body": { "inputs": [{ "id": "512" }] } })
    );
    assert_eq!(
        sent("PUT", format!("{OBJECTS}/contacts/512/associations/companies/77")),
        json!({ "query": {}, "body": [kind(279)] })
    );
}
