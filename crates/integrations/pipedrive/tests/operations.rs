//! Every Pipedrive operation, called by name against a local server that answers as Pipedrive does.

use serde_json::{Value, json};
use socketkit_core::{Effect, ErrorKind, Integration};
use socketkit_pipedrive::Pipedrive;
use socketkit_testkit::wiremock::matchers::{method, path};
use socketkit_testkit::wiremock::{Mock, ResponseTemplate};

mod support;
use support::{
    BUDGET, Case, INDUSTRY, LEAD, TOKEN, activity, answering, body_of, contains, data, deal, deal_hit, deal_returned,
    field, found, invoke, lead, lead_hit, me, note, offset_page, ok, only_request, organization, page, person,
    pipedrive, query_of, requests,
};

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, path, query, body, status, response, returns| Case { name, input, verb, path, query, body, status, response, returns };
    let none = || json!(null);
    let deleted = |id: Value| data(json!({ "id": id }));
    vec![
        // deals: version 2
        case("deals.list", json!({ "owner_id": 7, "person_id": 11, "org_id": 5, "pipeline_id": 1, "stage_id": 3, "status": ["open", "won"], "limit": 50 }), "GET", "/v2/deals",
            json!({ "owner_id": "7", "person_id": "11", "org_id": "5", "pipeline_id": "1", "stage_id": "3", "status": "open,won", "limit": "50" }), none(), 200,
            page(json!([deal()]), Some("eyJpZCI6NDJ9")),
            json!({ "items": [{ "id": 42, "title": "Acme renewal", "stage_id": 3, "status": "open", "value": 12000.5 }], "next_cursor": "eyJpZCI6NDJ9" })),
        case("deals.get", json!({ "deal": 42 }), "GET", "/v2/deals/42", json!({}), none(), 200, data(deal()), deal_returned()),
        case("deals.search", json!({ "term": "Acme & Sons", "fields": ["title", "notes"], "status": "open", "organization_id": 5, "limit": 10 }), "GET", "/v2/deals/search",
            json!({ "term": "Acme & Sons", "fields": "title,notes", "status": "open", "organization_id": "5", "limit": "10" }), none(), 200,
            found(json!([deal_hit()]), None),
            json!({ "items": [{ "result_score": 1.22, "item": { "id": 42, "type": "deal", "title": "Acme renewal", "stage": { "id": 3, "name": "Negotiation" }, "organization": { "id": 5, "name": "Acme Ltd" }, "notes": ["They want a two-year term."] } }], "next_cursor": null })),
        case("deals.create", json!({ "title": "Acme renewal", "value": 12000.5, "currency": "EUR", "person_id": 11, "org_id": 5, "stage_id": 3, "custom_fields": { INDUSTRY: 12 } }), "POST", "/v2/deals", json!({}),
            json!({ "title": "Acme renewal", "value": 12000.5, "currency": "EUR", "person_id": 11, "org_id": 5, "stage_id": 3, "custom_fields": { INDUSTRY: 12 } }), 200,
            data(deal()), deal_returned()),
        case("deals.update", json!({ "deal": 42, "stage_id": 4, "status": "won" }), "PATCH", "/v2/deals/42", json!({}),
            json!({ "stage_id": 4, "status": "won" }), 200, data(deal()), json!({ "id": 42 })),
        case("deals.delete", json!({ "deal": 42 }), "DELETE", "/v2/deals/42", json!({}), none(), 200, deleted(json!(42)), none()),

        // persons: version 2
        case("persons.list", json!({ "org_id": 5, "sort_by": "update_time", "sort_direction": "desc" }), "GET", "/v2/persons",
            json!({ "org_id": "5", "sort_by": "update_time", "sort_direction": "desc" }), none(), 200,
            page(json!([person()]), None),
            json!({ "items": [{ "id": 11, "name": "Grace Hopper", "org_id": 5, "emails": [{ "value": "grace@acme.example", "primary": true }], "job_title": "CTO" }], "next_cursor": null })),
        case("persons.get", json!({ "person": 11 }), "GET", "/v2/persons/11", json!({}), none(), 200, data(person()),
            json!({ "id": 11, "name": "Grace Hopper", "phones": [{ "value": "+1 555 0100", "label": "mobile" }], "custom_fields": { INDUSTRY: 13 } })),
        case("persons.search", json!({ "term": "grace", "fields": ["email"], "exact_match": false }), "GET", "/v2/persons/search",
            json!({ "term": "grace", "fields": "email", "exact_match": "false" }), none(), 200,
            found(json!([{ "result_score": 0.5, "item": { "id": 11, "type": "person", "name": "Grace Hopper", "phones": ["+1 555 0100"], "emails": ["grace@acme.example"], "visible_to": 3,
                "owner": { "id": 7 }, "organization": { "id": 5, "name": "Acme Ltd", "address": null }, "custom_fields": [], "notes": [] } }]), None),
            json!({ "items": [{ "item": { "id": 11, "type": "person", "name": "Grace Hopper", "emails": ["grace@acme.example"], "organization": { "id": 5 } } }], "next_cursor": null })),
        case("persons.create", json!({ "name": "Grace Hopper", "org_id": 5, "emails": [{ "value": "grace@acme.example", "primary": true, "label": "work" }] }), "POST", "/v2/persons", json!({}),
            json!({ "name": "Grace Hopper", "org_id": 5, "emails": [{ "value": "grace@acme.example", "primary": true, "label": "work" }] }), 200,
            data(person()), json!({ "id": 11, "name": "Grace Hopper" })),
        case("persons.update", json!({ "person": 11, "phones": [{ "value": "+1 555 0199" }], "custom_fields": { INDUSTRY: null } }), "PATCH", "/v2/persons/11", json!({}),
            json!({ "phones": [{ "value": "+1 555 0199" }], "custom_fields": { INDUSTRY: null } }), 200, data(person()), json!({ "id": 11 })),
        case("persons.delete", json!({ "person": 11 }), "DELETE", "/v2/persons/11", json!({}), none(), 200, deleted(json!(11)), none()),

        // organizations: version 2
        case("organizations.list", json!({ "owner_id": 7, "updated_since": "2026-10-01T00:00:00Z" }), "GET", "/v2/organizations",
            json!({ "owner_id": "7", "updated_since": "2026-10-01T00:00:00Z" }), none(), 200,
            page(json!([organization()]), None),
            json!({ "items": [{ "id": 5, "name": "Acme Ltd", "address": { "value": "1 Main St, Springfield", "country": "US" }, "employee_count": 40 }], "next_cursor": null })),
        case("organizations.get", json!({ "organization": 5 }), "GET", "/v2/organizations/5", json!({}), none(), 200, data(organization()),
            json!({ "id": 5, "name": "Acme Ltd", "website": "https://acme.example", "industry": 12, "custom_fields": { BUDGET: null } })),
        case("organizations.search", json!({ "term": "ac", "limit": 100 }), "GET", "/v2/organizations/search", json!({ "term": "ac", "limit": "100" }), none(), 200,
            found(json!([{ "result_score": 0.3, "item": { "id": 5, "type": "organization", "name": "Acme Ltd", "address": "1 Main St, Springfield", "visible_to": 3, "owner": { "id": 7 }, "custom_fields": [], "notes": [] } }]), Some("eyJvIjo1fQ")),
            json!({ "items": [{ "item": { "id": 5, "type": "organization", "name": "Acme Ltd", "address": "1 Main St, Springfield" } }], "next_cursor": "eyJvIjo1fQ" })),
        case("organizations.create", json!({ "name": "Acme Ltd", "address": { "value": "1 Main St, Springfield" }, "employee_count": 40 }), "POST", "/v2/organizations", json!({}),
            json!({ "name": "Acme Ltd", "address": { "value": "1 Main St, Springfield" }, "employee_count": 40 }), 200, data(organization()), json!({ "id": 5 })),
        case("organizations.update", json!({ "organization": 5, "website": "https://acme.example", "label_ids": [4, 6] }), "PATCH", "/v2/organizations/5", json!({}),
            json!({ "website": "https://acme.example", "label_ids": [4, 6] }), 200, data(organization()), json!({ "id": 5 })),
        case("organizations.delete", json!({ "organization": 5 }), "DELETE", "/v2/organizations/5", json!({}), none(), 200, deleted(json!(5)), none()),

        // leads: version 1, paged by offset; their search is version 2
        case("leads.list", json!({ "owner_id": 7, "sort": "update_time DESC", "limit": 2 }), "GET", "/v1/leads", json!({ "owner_id": "7", "sort": "update_time DESC", "limit": "2" }), none(), 200,
            offset_page(json!([lead()]), 0, 2, Some(2)),
            json!({ "items": [{ "id": LEAD, "title": "Jane Doe lead", "person_id": 11, "value": { "amount": 999.0, "currency": "USD" }, "visible_to": "3" }], "next_cursor": "offset:2" })),
        case("leads.get", json!({ "lead": LEAD }), "GET", "/v1/leads/adf21080-0e10-11eb-879b-05d71fb426ec", json!({}), none(), 200, data(lead()),
            json!({ "id": LEAD, "title": "Jane Doe lead", "label_ids": ["f08b42a0-4e75-11ea-9643-03698ef1cfd6"], "source_name": "API", "custom_fields": { INDUSTRY: 12 } })),
        case("leads.search", json!({ "term": "jane", "person_id": 11 }), "GET", "/v2/leads/search", json!({ "term": "jane", "person_id": "11" }), none(), 200,
            found(json!([lead_hit()]), None),
            json!({ "items": [{ "result_score": 0.29, "item": { "id": LEAD, "type": "lead", "title": "Jane Doe lead", "person": { "id": 11, "name": "Grace Hopper" }, "value": 999.0 } }], "next_cursor": null })),
        case("leads.create", json!({ "title": "Jane Doe lead", "person_id": 11, "value": { "amount": 999, "currency": "USD" } }), "POST", "/v1/leads", json!({}),
            json!({ "title": "Jane Doe lead", "person_id": 11, "value": { "amount": 999.0, "currency": "USD" } }), 201, data(lead()), json!({ "id": LEAD, "custom_fields": { INDUSTRY: 12 } })),
        case("leads.update", json!({ "lead": LEAD, "is_archived": true }), "PATCH", "/v1/leads/adf21080-0e10-11eb-879b-05d71fb426ec", json!({}),
            json!({ "is_archived": true }), 200, data(lead()), json!({ "id": LEAD })),
        case("leads.delete", json!({ "lead": LEAD }), "DELETE", "/v1/leads/adf21080-0e10-11eb-879b-05d71fb426ec", json!({}), none(), 200, deleted(json!(LEAD)), none()),

        // activities: version 2
        case("activities.list", json!({ "deal_id": 42, "done": false, "sort_by": "due_date" }), "GET", "/v2/activities", json!({ "deal_id": "42", "done": "false", "sort_by": "due_date" }), none(), 200,
            page(json!([activity()]), None),
            json!({ "items": [{ "id": 8, "subject": "Renewal call", "type": "call", "done": true, "due_date": "2026-10-09", "deal_id": 42, "person_id": 11, "participants": [{ "person_id": 11, "primary": true }] }], "next_cursor": null })),
        // Who was invited is the heavy part, and has to be asked for.
        case("activities.get", json!({ "activity": 8 }), "GET", "/v2/activities/8", json!({ "include_fields": "attendees" }), none(), 200, data(activity()),
            json!({ "id": 8, "type": "call", "note": "<p>They want a two-year term.</p>", "public_description": "Quarterly renewal", "attendees": [{ "email": "grace@acme.example", "status": "accepted" }] })),
        case("activities.create", json!({ "subject": "Renewal call", "type": "call", "deal_id": 42, "done": true, "due_date": "2026-10-09", "participants": [{ "person_id": 11, "primary": true }], "note": "<p>They want a two-year term.</p>" }), "POST", "/v2/activities", json!({}),
            json!({ "subject": "Renewal call", "type": "call", "deal_id": 42, "done": true, "due_date": "2026-10-09", "participants": [{ "person_id": 11, "primary": true }], "note": "<p>They want a two-year term.</p>" }), 200,
            data(activity()), json!({ "id": 8, "subject": "Renewal call" })),
        case("activities.update", json!({ "activity": 8, "done": true }), "PATCH", "/v2/activities/8", json!({}), json!({ "done": true }), 200, data(activity()), json!({ "id": 8, "done": true })),
        case("activities.delete", json!({ "activity": 8 }), "DELETE", "/v2/activities/8", json!({}), none(), 200, deleted(json!(8)), none()),

        // notes: version 1, paged by offset; a change is a PUT there
        case("notes.list", json!({ "deal_id": 42, "sort": "add_time DESC" }), "GET", "/v1/notes", json!({ "deal_id": "42", "sort": "add_time DESC" }), none(), 200,
            offset_page(json!([note("<p>Call back on Monday.</p>")]), 0, 100, None),
            json!({ "items": [{ "id": 3, "content": "<p>Call back on Monday.</p>", "deal_id": 42, "user_id": 7, "add_time": "2026-10-09 10:40:00" }], "next_cursor": null })),
        case("notes.get", json!({ "note": 3 }), "GET", "/v1/notes/3", json!({}), none(), 200, data(note("<p>Call back on Monday.</p>")),
            json!({ "id": 3, "content": "<p>Call back on Monday.</p>", "deal_id": 42 })),
        case("notes.create", json!({ "content": "<p>Call back on Monday.</p>", "deal_id": 42 }), "POST", "/v1/notes", json!({}),
            json!({ "content": "<p>Call back on Monday.</p>", "deal_id": 42 }), 201, data(note("<p>Call back on Monday.</p>")), json!({ "id": 3, "deal_id": 42 })),
        case("notes.update", json!({ "note": 3, "content": "<p>Call back on Tuesday.</p>" }), "PUT", "/v1/notes/3", json!({}),
            json!({ "content": "<p>Call back on Tuesday.</p>" }), 200, data(note("<p>Call back on Tuesday.</p>")), json!({ "id": 3, "content": "<p>Call back on Tuesday.</p>" })),
        // Version 1 answers a deleted note with `true`, not with its id.
        case("notes.delete", json!({ "note": 3 }), "DELETE", "/v1/notes/3", json!({}), none(), 200, data(json!(true)), none()),

        // pipelines and stages: version 2
        case("pipelines.list", json!({}), "GET", "/v2/pipelines", json!({}), none(), 200,
            page(json!([{ "id": 1, "name": "Sales", "order_nr": 1, "is_deleted": false, "is_deal_probability_enabled": true, "add_time": "2025-01-05T09:00:00Z", "update_time": "2025-01-05T09:00:00Z" }]), None),
            json!({ "items": [{ "id": 1, "name": "Sales", "order_nr": 1, "is_deal_probability_enabled": true }], "next_cursor": null })),
        case("pipelines.stages", json!({ "pipeline": 1, "limit": 20 }), "GET", "/v2/stages", json!({ "pipeline_id": "1", "limit": "20" }), none(), 200,
            page(json!([{ "id": 3, "order_nr": 2, "name": "Negotiation", "is_deleted": false, "deal_probability": 60, "pipeline_id": 1, "is_deal_rot_enabled": true, "days_to_rotten": 14, "add_time": "2025-01-05T09:00:00Z", "update_time": null }]), None),
            json!({ "items": [{ "id": 3, "name": "Negotiation", "pipeline_id": 1, "order_nr": 2, "deal_probability": 60.0, "days_to_rotten": 14 }], "next_cursor": null })),

        // fields: version 2
        case("fields.deal_fields", json!({ "limit": 500 }), "GET", "/v2/dealFields", json!({ "limit": "500" }), none(), 200, page(json!([field()]), None),
            json!({ "items": [{ "field_code": INDUSTRY, "field_name": "Industry", "field_type": "enum", "is_custom_field": true, "options": [{ "id": 12, "label": "Software" }, { "id": 13, "label": "Retail" }] }], "next_cursor": null })),
        case("fields.person_fields", json!({}), "GET", "/v2/personFields", json!({}), none(), 200,
            page(json!([{ "field_name": "Name", "field_code": "name", "field_type": "varchar", "is_custom_field": false, "is_optional_response_field": false, "options": null, "subfields": null }]), Some("eyJmIjoxfQ")),
            json!({ "items": [{ "field_code": "name", "field_name": "Name", "is_custom_field": false, "options": [] }], "next_cursor": "eyJmIjoxfQ" })),
        case("fields.organization_fields", json!({ "cursor": "eyJmIjoxfQ" }), "GET", "/v2/organizationFields", json!({ "cursor": "eyJmIjoxfQ" }), none(), 200,
            page(json!([{ "field_name": "Address", "field_code": "address", "field_type": "address", "is_custom_field": false, "options": null,
                "subfields": [{ "field_code": "address_country", "field_name": "Country", "field_type": "varchar" }] }]), None),
            json!({ "items": [{ "field_code": "address", "field_type": "address", "subfields": [{ "field_code": "address_country" }] }], "next_cursor": null })),

        // users: version 1
        case("users.list", json!({}), "GET", "/v1/users", json!({}), none(), 200,
            data(json!([{ "id": 7, "name": "Ada Lovelace", "email": "ada@example.test", "active_flag": true, "is_you": true, "role_id": 1, "timezone_name": "Europe/London" },
                        { "id": 9, "name": "Alan Turing", "email": "alan@example.test", "active_flag": false, "is_you": false }])),
            json!([{ "id": 7, "name": "Ada Lovelace", "email": "ada@example.test", "active_flag": true }, { "id": 9, "name": "Alan Turing", "active_flag": false }])),
        case("users.me", json!({}), "GET", "/v1/users/me", json!({}), none(), 200, data(me()),
            json!({ "id": 7, "name": "Ada Lovelace", "email": "ada@example.test", "company_id": 1001, "company_name": "Acme Ltd", "company_domain": "acme", "company_country": "GB" })),

        // search: version 2
        case("search.items", json!({ "term": "renewal", "item_types": ["deal", "lead"], "exact_match": true }), "GET", "/v2/itemSearch",
            json!({ "term": "renewal", "item_types": "deal,lead", "exact_match": "true" }), none(), 200,
            found(json!([deal_hit(), lead_hit()]), Some("eyJzIjoyfQ")),
            json!({ "items": [{ "item": { "id": 42, "type": "deal", "title": "Acme renewal" } }, { "item": { "id": LEAD, "type": "lead", "title": "Jane Doe lead" } }], "next_cursor": "eyJzIjoyfQ" })),
    ]
}

/// Every operation with what it does and the least scopes that serve it,
/// stated here and not derived from the code under test: a host lets a read
/// run freely and asks a person before anything else.
fn expected() -> Vec<(&'static str, Effect, &'static [&'static str])> {
    use Effect::{Destructive, Read, Write};
    vec![
        ("deals.list", Read, &["deals:read"]),
        ("deals.get", Read, &["deals:read"]),
        ("deals.search", Read, &["deals:read"]),
        ("deals.create", Write, &["deals:full"]),
        ("deals.update", Write, &["deals:full"]),
        ("deals.delete", Destructive, &["deals:full"]),
        ("persons.list", Read, &["contacts:read"]),
        ("persons.get", Read, &["contacts:read"]),
        ("persons.search", Read, &["contacts:read"]),
        ("persons.create", Write, &["contacts:full"]),
        ("persons.update", Write, &["contacts:full"]),
        ("persons.delete", Destructive, &["contacts:full"]),
        ("organizations.list", Read, &["contacts:read"]),
        ("organizations.get", Read, &["contacts:read"]),
        ("organizations.search", Read, &["contacts:read"]),
        ("organizations.create", Write, &["contacts:full"]),
        ("organizations.update", Write, &["contacts:full"]),
        ("organizations.delete", Destructive, &["contacts:full"]),
        ("leads.list", Read, &["leads:read"]),
        ("leads.get", Read, &["leads:read"]),
        ("leads.search", Read, &["leads:read"]),
        ("leads.create", Write, &["leads:full"]),
        ("leads.update", Write, &["leads:full"]),
        ("leads.delete", Destructive, &["leads:full"]),
        ("activities.list", Read, &["activities:read"]),
        ("activities.get", Read, &["activities:read"]),
        ("activities.create", Write, &["activities:full"]),
        ("activities.update", Write, &["activities:full"]),
        ("activities.delete", Destructive, &["activities:full"]),
        ("notes.list", Read, &["deals:read", "contacts:read"]),
        ("notes.get", Read, &["deals:read", "contacts:read"]),
        ("notes.create", Write, &["deals:full", "contacts:full"]),
        ("notes.update", Write, &["deals:full", "contacts:full"]),
        ("notes.delete", Destructive, &["deals:full", "contacts:full"]),
        ("pipelines.list", Read, &["deals:read"]),
        ("pipelines.stages", Read, &["deals:read"]),
        ("fields.deal_fields", Read, &["deals:read"]),
        ("fields.person_fields", Read, &["contacts:read"]),
        ("fields.organization_fields", Read, &["contacts:read"]),
        ("users.list", Read, &["users:read"]),
        ("users.me", Read, &["base"]),
        ("search.items", Read, &["search:read"]),
    ]
}

#[tokio::test]
async fn the_operations_are_exactly_the_ones_listed_and_each_has_a_test_case() {
    let mut listed: Vec<String> = Pipedrive::new().operations().into_iter().map(|o| o.name).collect();
    let mut wanted: Vec<String> = expected()
        .iter()
        .map(|(name, _, _)| format!("pipedrive.{name}"))
        .collect();
    wanted.extend([
        "pipedrive.identity.get".to_owned(),
        "pipedrive.resource.resolve".to_owned(),
    ]);
    assert_eq!(listed.len(), 44);
    listed.sort();
    wanted.sort();
    assert_eq!(listed, wanted, "an operation was added, removed or renamed");

    let mut tested: Vec<&str> = cases().iter().map(|case| case.name).collect();
    let mut named: Vec<&str> = expected().iter().map(|(name, _, _)| *name).collect();
    tested.sort_unstable();
    named.sort_unstable();
    assert_eq!(
        tested, named,
        "every operation has exactly one row in the table of requests"
    );
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_pipedrive_sent() {
    for case in cases() {
        let (server, socket, key) = pipedrive().await;
        let answer = if case.response.is_null() {
            ResponseTemplate::new(case.status)
        } else {
            ResponseTemplate::new(case.status).set_body_json(case.response.clone())
        };
        // The path pins the version of the API each operation calls.
        Mock::given(method(case.verb))
            .and(path(format!("/api{}", case.path)))
            .respond_with(answer)
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
            &format!("Bearer {TOKEN}"),
            "{}",
            case.name
        );
        assert_eq!(
            query_of(&request),
            case.query,
            "{}: exactly these parameters reach Pipedrive",
            case.name
        );
        assert_eq!(
            body_of(&request),
            case.body,
            "{}: exactly this body reaches Pipedrive",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_names_its_effect_and_scopes_and_no_write_is_sent_as_a_read() {
    let operations = Pipedrive::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == format!("pipedrive.{name}"))
            .unwrap_or_else(|| panic!("{name}"))
    };
    for operation in &operations {
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert_eq!(
            operation.input_schema["additionalProperties"], false,
            "{}: says it takes no field it does not list",
            operation.name
        );
        assert!(!operation.description.is_empty(), "{}", operation.name);
    }
    for (name, effect, scopes) in expected() {
        let operation = find(name);
        assert_eq!(operation.effect, effect, "{name}");
        assert_eq!(operation.required_scopes, scopes, "{name}");
        // The name says what an operation does, and the effect has to agree.
        let action = name.rsplit('.').next().unwrap();
        match action {
            "create" | "update" => assert_eq!(effect, Effect::Write, "{name}"),
            "delete" => assert_eq!(effect, Effect::Destructive, "{name}"),
            _ => assert_eq!(effect, Effect::Read, "{name}"),
        }
    }
    // What the server is asked to do agrees too: only a read is a GET, which
    // the transport repeats after a server error, and only a delete deletes.
    for case in cases() {
        match find(case.name).effect {
            Effect::Read => assert_eq!(case.verb, "GET", "{}", case.name),
            Effect::Write => assert!(matches!(case.verb, "POST" | "PATCH" | "PUT"), "{}", case.name),
            Effect::Destructive => assert_eq!(case.verb, "DELETE", "{}", case.name),
        }
    }
    assert_eq!(find("identity.get").required_scopes, ["base"]);
    assert_eq!(find("identity.get").effect, Effect::Read);
    assert_eq!(
        find("resource.resolve").required_scopes,
        ["deals:read", "contacts:read", "leads:read"]
    );
    // Where more than one scope is listed, one of them is enough, and the
    // description says so: a host must not read the list as all of them.
    for operation in &operations {
        if operation.required_scopes.len() < 2 {
            continue;
        }
        let says = if operation.name == "pipedrive.resource.resolve" {
            "only the one for the kind of record given is needed"
        } else {
            "Either one of the scopes listed is enough"
        };
        assert!(
            operation.description.contains(says),
            "{}: {}",
            operation.name,
            operation.description
        );
    }
    let activities = &find("activities.list").description;
    for left_out in ["note", "public_description", "attendees"] {
        assert!(activities.contains(left_out), "{activities}");
    }

    let create = find("deals.create");
    assert_eq!(
        create.input_schema["required"],
        json!(["title"]),
        "a deal needs only its title"
    );
    for field in ["value", "stage_id", "person_id", "org_id", "custom_fields"] {
        assert!(
            create.input_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    for field in ["id", "title", "stage_id", "status", "custom_fields"] {
        assert!(
            create.output_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    assert_eq!(find("deals.get").input_schema["properties"]["deal"]["type"], "integer");
    assert_eq!(find("leads.get").input_schema["properties"]["lead"]["type"], "string");
}

#[tokio::test]
async fn a_field_an_operation_does_not_know_is_refused_and_nothing_is_sent() {
    let (server, socket, key) = answering(ok(data(deal()))).await;
    for (name, input, named) in [
        ("deals.list", json!({ "owner": 7 }), "`owner`"),
        ("deals.update", json!({ "deal": 42, "stage": 4 }), "`stage`"),
        (
            "persons.create",
            json!({ "name": "Grace", "emails": [{ "value": "g@acme.example" }, { "valeu": "x" }] }),
            "`emails[1].valeu`",
        ),
        (
            "organizations.create",
            json!({ "name": "Acme", "address": { "cuntry": "US" } }),
            "`address.cuntry`",
        ),
        (
            "leads.create",
            json!({ "title": "L", "person_id": 1, "value": { "amount": 1, "currency": "USD", "curency": "EUR" } }),
            "`value.curency`",
        ),
        ("users.me", json!({ "user": 7 }), "`user`"),
        ("pipelines.list", json!({ "pipeline": 1 }), "`pipeline`"),
    ] {
        let error = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{name}");
        assert!(error.message().contains(named), "{name}: {}", error.message());
    }
    // A custom field's key is the company's own, so the schema cannot list
    // the keys. Only the key of a custom field is taken there all the same:
    // a record's own field is not let through under that name.
    for (name, input) in [
        (
            "deals.update",
            json!({ "deal": 42, "custom_fields": { "status": "deleted" } }),
        ),
        (
            "deals.update",
            json!({ "deal": 42, "custom_fields": { INDUSTRY: 13, "is_deleted": true } }),
        ),
        (
            "deals.create",
            json!({ "title": "D", "custom_fields": { "is_archived": true } }),
        ),
        (
            "persons.create",
            json!({ "name": "Grace", "custom_fields": { "is_deleted": true } }),
        ),
        (
            "persons.update",
            json!({ "person": 11, "custom_fields": { format!("{INDUSTRY}_currency"): "EUR" } }),
        ),
        (
            "organizations.create",
            json!({ "name": "Acme", "custom_fields": { "": 1 } }),
        ),
        (
            "organizations.update",
            json!({ "organization": 5, "custom_fields": { INDUSTRY.to_uppercase(): 1 } }),
        ),
    ] {
        let error = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(
            error.message().contains("`custom_fields`"),
            "{name}: {}",
            error.message()
        );
    }
    assert!(requests(&server).await.is_empty(), "nothing reached Pipedrive");

    invoke(
        &socket,
        &key,
        "deals.update",
        json!({ "deal": 42, "custom_fields": { INDUSTRY: 13 } }),
    )
    .await
    .unwrap();
    assert_eq!(requests(&server).await.len(), 1, "only the last call reached Pipedrive");
}

/// True when a schema lists `value` among the values a field may take, at any depth.
fn offers(schema: &Value, value: &str) -> bool {
    match schema {
        Value::Object(fields) => fields.iter().any(|(name, inner)| match name.as_str() {
            "enum" => inner.as_array().is_some_and(|values| values.iter().any(|v| v == value)),
            "const" => inner == value,
            _ => offers(inner, value),
        }),
        Value::Array(items) => items.iter().any(|item| offers(item, value)),
        _ => false,
    }
}

#[tokio::test]
async fn a_change_to_a_record_cannot_delete_it() {
    // Pipedrive's own update takes `deleted` for a deal's status, and a deal
    // given it is deleted. A `write` must not be able to do that: deleting
    // is `delete`, which is marked `destructive` and asked about as such.
    let (server, socket, key) = answering(ok(data(deal()))).await;
    for (name, input) in [
        ("deals.create", json!({ "title": "Acme renewal", "status": "deleted" })),
        ("deals.update", json!({ "deal": 42, "status": "deleted" })),
        (
            "deals.update",
            json!({ "deal": 42, "stage_id": 4, "status": "deleted" }),
        ),
        ("deals.search", json!({ "term": "acme", "status": "deleted" })),
        // The flags Pipedrive's own create and update take are not fields here.
        ("deals.update", json!({ "deal": 42, "is_deleted": true })),
        ("deals.create", json!({ "title": "Acme renewal", "is_deleted": true })),
        ("deals.update", json!({ "deal": 42, "is_archived": true })),
        ("persons.update", json!({ "person": 11, "is_deleted": true })),
        ("organizations.update", json!({ "organization": 5, "is_deleted": true })),
        ("activities.update", json!({ "activity": 8, "is_deleted": true })),
        ("notes.update", json!({ "note": 3, "active_flag": false })),
        (
            "leads.create",
            json!({ "title": "L", "person_id": 11, "is_archived": true }),
        ),
    ] {
        let error = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{name} {input}");
    }
    assert!(requests(&server).await.is_empty(), "nothing reached Pipedrive");

    // The schema does not offer it either, so an agent is never shown the value.
    let operations = Pipedrive::new().operations();
    let schema = |name: &str| {
        let operation = operations.iter().find(|o| o.name == format!("pipedrive.{name}"));
        operation.unwrap_or_else(|| panic!("{name}")).input_schema.clone()
    };
    for name in ["deals.create", "deals.update", "deals.search"] {
        let schema = schema(name);
        assert_eq!(
            schema["$defs"]["DealStatus"]["enum"],
            json!(["open", "won", "lost"]),
            "{name}"
        );
        assert!(!offers(&schema, "deleted"), "{name}: {schema}");
    }
    // Listing the deleted deals is a read, and stays possible.
    assert!(offers(&schema("deals.list"), "deleted"));
    for (name, _, _) in expected() {
        let action = name.rsplit('.').next().unwrap();
        if !matches!(action, "create" | "update") {
            continue;
        }
        let properties = schema(name)["properties"].clone();
        assert!(properties.get("is_deleted").is_none(), "{name}");
        assert!(properties.get("active_flag").is_none(), "{name}");
        // Archiving a lead is the one such flag: it hides the lead from the
        // list and is undone by the same operation.
        assert_eq!(
            properties.get("is_archived").is_some(),
            name == "leads.update",
            "{name}"
        );
    }
    let (server, socket, key) = answering(ok(data(deal()))).await;
    for status in ["open", "won", "lost"] {
        invoke(&socket, &key, "deals.update", json!({ "deal": 42, "status": status }))
            .await
            .unwrap();
    }
    assert_eq!(requests(&server).await.len(), 3);
}

#[tokio::test]
async fn an_input_that_lacks_what_is_needed_or_has_the_wrong_type_is_refused_without_repeating_it() {
    let (server, socket, key) = answering(ok(data(deal()))).await;
    for (name, input, says) in [
        ("deals.create", json!({ "value": 10 }), "missing field `title`"),
        ("deals.create", json!({ "title": "   " }), "`title` is required"),
        ("deals.get", json!({}), "missing field `deal`"),
        ("deals.update", json!({ "deal": 42 }), "nothing to change"),
        ("deals.search", json!({ "term": "a" }), "at least two characters"),
        ("deals.list", json!({ "status": ["archived-secret"] }), "wrong type"),
        ("deals.list", json!({ "custom_fields": ["a,b"] }), "`custom_fields`"),
        (
            "deals.list",
            json!({ "custom_fields": (0..16).map(|n| format!("k{n}")).collect::<Vec<_>>() }),
            "at most 15",
        ),
        (
            "leads.create",
            json!({ "title": "No one's lead" }),
            "`person_id`, `organization_id` or both",
        ),
        (
            "notes.create",
            json!({ "content": "<p>secret-note-text</p>" }),
            "`deal_id`, `person_id`, `org_id` and `lead_id`",
        ),
        (
            "notes.create",
            json!({ "content": "", "deal_id": 1 }),
            "`content` is required",
        ),
        ("notes.update", json!({ "note": 3, "content": " " }), "cannot be empty"),
        (
            "persons.create",
            json!({ "name": "Grace", "emails": "secret@acme.example" }),
            "wrong type",
        ),
    ] {
        let error = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(error.kind(), ErrorKind::InvalidInput, "{name}");
        assert!(error.message().contains(says), "{name}: {}", error.message());
        assert!(!error.message().contains("secret"), "{name}: {}", error.message());
    }
    assert!(requests(&server).await.is_empty(), "nothing reached Pipedrive");

    // One character is enough when only whole matches are asked for.
    let (server, socket, key) = answering(ok(found(json!([]), None))).await;
    let nothing = invoke(
        &socket,
        &key,
        "search.items",
        json!({ "term": "a", "exact_match": true }),
    )
    .await
    .unwrap();
    assert_eq!(nothing, json!({ "items": [], "next_cursor": null }));
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "term": "a", "exact_match": "true" })
    );

    // An answer that is not a list of results is not "nothing found".
    let (_server, socket, key) = answering(ok(data(deal()))).await;
    let error = invoke(&socket, &key, "search.items", json!({ "term": "acme" }))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn a_list_returns_light_rows_and_the_heavy_part_is_a_second_call() {
    // A deal's custom fields are left out of a row…
    let (_server, socket, key) = answering(ok(page(json!([deal()]), None))).await;
    let rows = invoke(&socket, &key, "deals.list", json!({})).await.unwrap();
    assert!(rows["items"][0].get("custom_fields").is_none(), "{rows}");
    assert_eq!(rows["items"][0]["title"], "Acme renewal");

    // …unless some are asked for, and then Pipedrive is asked for those only.
    let (server, socket, key) = answering(ok(page(json!([deal()]), None))).await;
    let rows = invoke(
        &socket,
        &key,
        "deals.list",
        json!({ "custom_fields": [INDUSTRY, BUDGET] }),
    )
    .await
    .unwrap();
    assert_eq!(rows["items"][0]["custom_fields"][INDUSTRY], 12);
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "custom_fields": format!("{INDUSTRY},{BUDGET}") })
    );

    for (name, row) in [("persons.list", person()), ("organizations.list", organization())] {
        let (_server, socket, key) = answering(ok(page(json!([row]), None))).await;
        let rows = invoke(&socket, &key, name, json!({})).await.unwrap();
        assert!(rows["items"][0].get("custom_fields").is_none(), "{name}: {rows}");
    }
    let (_server, socket, key) = answering(ok(offset_page(json!([lead()]), 0, 100, None))).await;
    let rows = invoke(&socket, &key, "leads.list", json!({})).await.unwrap();
    assert!(rows["items"][0].get("custom_fields").is_none(), "{rows}");

    // What was written about an activity, and who was invited, come with `get`.
    let (_server, socket, key) = answering(ok(page(json!([activity()]), None))).await;
    let rows = invoke(&socket, &key, "activities.list", json!({})).await.unwrap();
    let row = &rows["items"][0];
    // Absent, not empty: a row that said `attendees: []` would say nobody was invited.
    for not_fetched in ["note", "public_description", "attendees"] {
        assert!(row.get(not_fetched).is_none(), "{not_fetched}: {row}");
    }
    assert_eq!(row["subject"], "Renewal call");
    assert_eq!(row["participants"], json!([{ "person_id": 11, "primary": true }]));
    let (_server, socket, key) = answering(ok(data(activity()))).await;
    let whole = invoke(&socket, &key, "activities.get", json!({ "activity": 8 }))
        .await
        .unwrap();
    assert_eq!(whole["note"], "<p>They want a two-year term.</p>");
    assert_eq!(whole["public_description"], "Quarterly renewal");
    assert_eq!(whole["attendees"][0]["email"], "grace@acme.example");

    // A search result says only what its kind of record has: a deal is not
    // a record with an empty list of emails.
    let (_server, socket, key) = answering(ok(found(json!([deal_hit(), lead_hit()]), None))).await;
    let hits = invoke(&socket, &key, "search.items", json!({ "term": "renewal" }))
        .await
        .unwrap();
    let deal_found = &hits["items"][0]["item"];
    for not_a_deals in ["emails", "phones", "custom_fields"] {
        assert!(deal_found.get(not_a_deals).is_none(), "{not_a_deals}: {deal_found}");
    }
    assert_eq!(deal_found["notes"], json!(["They want a two-year term."]));
    assert_eq!(hits["items"][1]["item"]["emails"], json!(["grace@acme.example"]));

    // A long note is cut short in a list, and says so; `get` returns it whole.
    let long = format!("<p>{}</p>", "word ".repeat(400));
    let (_server, socket, key) = answering(ok(offset_page(
        json!([note(&long), note("<p>Short.</p>")]),
        0,
        100,
        None,
    )))
    .await;
    let rows = invoke(&socket, &key, "notes.list", json!({})).await.unwrap();
    assert_eq!(rows["items"][0]["content"].as_str().unwrap().chars().count(), 500);
    assert_eq!(rows["items"][0]["truncated"], true);
    assert_eq!(rows["items"][1]["content"], "<p>Short.</p>");
    assert!(rows["items"][1].get("truncated").is_none(), "{rows}");
    let (_server, socket, key) = answering(ok(data(note(&long)))).await;
    let whole = invoke(&socket, &key, "notes.get", json!({ "note": 3 })).await.unwrap();
    assert_eq!(whole["content"], long);
    assert!(whole.get("truncated").is_none());
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_place_and_never_the_record() {
    let mut unreadable = deal();
    unreadable["title"] = json!({ "secret": "confidential-title" });
    let (_server, socket, key) = answering(ok(data(unreadable))).await;
    let error = invoke(&socket, &key, "deals.get", json!({ "deal": 42 }))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(error.message().contains("at `title`"), "{}", error.message());
    assert!(!format!("{error:?}").contains("confidential"), "{error:?}");

    // In a list the place says which row.
    let mut second = person();
    second["emails"] = json!("confidential@acme.example");
    let (_server, socket, key) = answering(ok(page(json!([person(), second]), None))).await;
    let error = invoke(&socket, &key, "persons.list", json!({})).await.unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
    assert!(error.message().contains("[1].emails"), "{}", error.message());
    assert!(!format!("{error:?}").contains("confidential"), "{error:?}");

    // A success without the record is not a record with blank fields.
    for body in [
        json!({}),
        json!({ "success": true }),
        data(json!(null)),
        data(json!({})),
        data(json!([])),
    ] {
        let (_server, socket, key) = answering(ok(body.clone())).await;
        let error = invoke(&socket, &key, "deals.get", json!({ "deal": 42 }))
            .await
            .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::Decode, "{body}");
    }
    // A delete Pipedrive did not confirm is not reported as done.
    let (_server, socket, key) = answering(ok(json!({}))).await;
    let error = invoke(&socket, &key, "deals.delete", json!({ "deal": 42 }))
        .await
        .unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Decode);
}
