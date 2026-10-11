//! Every Salesforce operation, called by name against a local server that answers as Salesforce's REST API does.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use socketkit_core::{Effect, ErrorKind, Integration};
use socketkit_salesforce::models::{Find, FindIn, GetRecord, ListSObjects, QueryOptions, RecordFields, SearchScope};
use socketkit_salesforce::{Salesforce, escape_soql, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    ACCOUNT, API, CASE, CONTACT, Case, EMAIL, EVENT, LEAD, NOTE, OPPORTUNITY, TASK, TOKEN, USER, account,
    account_returned, answer, answering, attributes, body_of, case, contains, created, email_message, header, invoke,
    only_request, opportunity, query_of, refusal, salesforce,
};

const ACCOUNTS: &str = "SELECT Id, Name, Industry, Owner.Name FROM Account WHERE Industry = 'Energy' LIMIT 2";
const OPPORTUNITIES: &str = "SELECT Id, Name, StageName, Amount, Account.Name, (SELECT Role, Contact.Name, Contact.Email FROM OpportunityContactRoles) FROM Opportunity WHERE IsClosed = false";
const DELETED_TASKS: &str = "SELECT Id, Subject, IsDeleted FROM Task WHERE IsDeleted = true";
const PEOPLE: &str = "FIND {Hopper} IN NAME FIELDS RETURNING Contact(Id, Name, Email), Lead(Id, Name, Company)";

fn fields(pairs: Value) -> Map<String, Value> {
    pairs.as_object().cloned().unwrap()
}

/// The list of every object type, three of them, as Salesforce describes each.
fn every_object() -> Value {
    let object = |name: &str, label: &str, prefix: Value, custom: bool| {
        json!({
            "name": name, "label": label, "labelPlural": format!("{label}s"), "keyPrefix": prefix, "custom": custom,
            "queryable": true, "searchable": true, "createable": true, "updateable": true, "deletable": true,
            "activateable": false, "customSetting": false, "deprecatedAndHidden": false, "feedEnabled": true,
            "layoutable": true, "mergeable": false, "mruEnabled": true, "replicateable": true, "retrieveable": true,
            "triggerable": true, "undeletable": true,
            "urls": { "sobject": format!("{API}/sobjects/{name}"), "describe": format!("{API}/sobjects/{name}/describe") }
        })
    };
    json!({
        "encoding": "UTF-8",
        "maxBatchSize": 200,
        "sobjects": [
            object("Account", "Account", json!("001"), false),
            object("Case", "Case", json!("500"), false),
            object("Invoice__c", "Customer Invoice", json!("a01"), true),
            // An object with no ids of its own has no prefix.
            object("AccountHistory", "Account History", json!(null), false)
        ]
    })
}

/// One field as Salesforce describes it, with what the tests do not vary.
fn field_described(name: &str, label: &str, kind: &str, rest: Value) -> Value {
    let mut field = json!({
        "name": name, "label": label, "type": kind, "length": 18, "nillable": true, "defaultedOnCreate": false,
        "createable": true, "updateable": true, "filterable": true, "sortable": true, "custom": false,
        "calculated": false, "externalId": false, "unique": false, "idLookup": false, "nameField": false,
        "referenceTo": [], "relationshipName": null, "picklistValues": [], "inlineHelpText": null,
        "precision": 0, "scale": 0, "soapType": "xsd:string", "byteLength": 18
    });
    field
        .as_object_mut()
        .unwrap()
        .extend(rest.as_object().cloned().unwrap());
    field
}

/// A case as Salesforce describes the object, cut to three fields.
fn case_described() -> Value {
    let statuses = json!([
        { "active": true, "defaultValue": true, "label": "New", "validFor": null, "value": "New" },
        { "active": true, "defaultValue": false, "label": "Escalated", "validFor": null, "value": "Escalated" },
        { "active": false, "defaultValue": false, "label": null, "validFor": null, "value": "On Hold" }
    ]);
    let fields = json!([
        field_described(
            "Id",
            "Case ID",
            "id",
            json!({ "nillable": false, "defaultedOnCreate": true, "createable": false, "updateable": false, "idLookup": true })
        ),
        field_described(
            "Status",
            "Status",
            "picklist",
            json!({ "length": 255, "defaultedOnCreate": true, "inlineHelpText": "Where the case stands", "picklistValues": statuses })
        ),
        field_described(
            "AccountId",
            "Account ID",
            "reference",
            json!({ "referenceTo": ["Account"], "relationshipName": "Account" })
        )
    ]);
    let children = json!([
        { "childSObject": "EmailMessage", "field": "ParentId", "relationshipName": "EmailMessages", "cascadeDelete": true, "deprecatedAndHidden": false },
        { "childSObject": "CaseShare", "field": "CaseId", "relationshipName": null, "cascadeDelete": true }
    ]);
    let record_types = json!([{
        "recordTypeId": "012000000000000AAA", "name": "Master", "developerName": "Master", "active": true, "available": true,
        "defaultRecordTypeMapping": true, "master": true,
        "urls": { "layout": format!("{API}/sobjects/Case/describe/layouts/012000000000000AAA") }
    }]);
    json!({
        "name": "Case", "label": "Case", "labelPlural": "Cases", "keyPrefix": "500", "custom": false,
        "queryable": true, "searchable": true, "createable": true, "updateable": true, "deletable": true,
        "fields": fields,
        "childRelationships": children,
        "recordTypeInfos": record_types,
        "urls": { "sobject": format!("{API}/sobjects/Case") }
    })
}

fn limits() -> Value {
    json!({
        "DailyApiRequests": { "Max": 15000, "Remaining": 14998, "Ant Migration Tool": { "Max": 0, "Remaining": 0 } },
        "DataStorageMB": { "Max": 5, "Remaining": 5 },
        "SingleEmail": { "Max": 15, "Remaining": 15 }
    })
}

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let row = |name, input, verb, path, query, body, options, status, response, returns| Case { name, input, verb, path, query, body, options, status, response, returns };
    let found = json!({ "searchRecords": [
        { "attributes": attributes("Contact", CONTACT), "Id": CONTACT, "Name": "Grace Hopper", "Email": "grace@acme.example" },
        { "attributes": attributes("Lead", LEAD), "Id": LEAD, "Name": "Grace Hopper", "Company": "Navy" }
    ] });
    let found_returned = json!({ "records": [
        { "type": "Contact", "id": CONTACT, "fields": { "Name": "Grace Hopper", "Email": "grace@acme.example" } },
        { "type": "Lead", "id": LEAD, "fields": { "Name": "Grace Hopper", "Company": "Navy" } }
    ] });
    vec![
        // query: an account with the record its lookup leads to.
        row("query.run", json!({ "soql": ACCOUNTS }), "GET", "/query", json!({ "q": ACCOUNTS }), json!(null), None, 200,
            json!({ "totalSize": 1, "done": true, "records": [account()] }),
            json!({ "totalSize": 1, "done": true, "records": [account_returned()], "next_cursor": null })),
        // query: an opportunity with the records of a subquery, in batches of 200, and more to come.
        row("query.run", json!({ "soql": OPPORTUNITIES, "batch_size": 200 }), "GET", "/query", json!({ "q": OPPORTUNITIES }), json!(null), Some("batchSize=200"), 200,
            json!({ "totalSize": 412, "done": false, "nextRecordsUrl": format!("{API}/query/01gxx0000004RpzAAE-200"), "records": [opportunity()] }),
            json!({ "totalSize": 412, "done": false, "next_cursor": format!("{API}/query/01gxx0000004RpzAAE-200"), "records": [{
                "type": "Opportunity", "id": OPPORTUNITY,
                "fields": { "StageName": "Negotiation/Review", "Amount": 48000.0, "Account": { "Name": "Acme" },
                    "OpportunityContactRoles": { "totalSize": 1, "records": [{ "Role": "Decision Maker", "Contact": { "Email": "grace@acme.example" } }] } }
            }] })),
        // query: a count has a size and no records.
        row("query.run", json!({ "soql": "SELECT COUNT() FROM Lead WHERE IsConverted = false" }), "GET", "/query", json!({ "q": "SELECT COUNT() FROM Lead WHERE IsConverted = false" }), json!(null), None, 200,
            json!({ "totalSize": 42, "done": true, "records": [] }),
            json!({ "totalSize": 42, "done": true, "records": [], "next_cursor": null })),
        row("query.run_all", json!({ "soql": DELETED_TASKS }), "GET", "/queryAll", json!({ "q": DELETED_TASKS }), json!(null), None, 200,
            json!({ "totalSize": 1, "done": true, "records": [{ "attributes": attributes("Task", TASK), "Id": TASK, "Subject": "Call back", "IsDeleted": true }] }),
            json!({ "totalSize": 1, "done": true, "records": [{ "type": "Task", "id": TASK, "fields": { "Subject": "Call back", "IsDeleted": true } }], "next_cursor": null })),

        // search
        row("search.run", json!({ "sosl": PEOPLE }), "GET", "/search", json!({ "q": PEOPLE }), json!(null), None, 200,
            found.clone(), found_returned.clone()),
        // Salesforce offers this search as a POST; it changes nothing.
        row("search.find",
            json!({ "text": "Hopper", "objects": [{ "name": "Contact", "fields": ["Id", "Name", "Email"], "limit": 5 }, { "name": "Lead" }], "fields": ["Id", "Name", "Company"], "within": "name", "limit": 10, "overall_limit": 20 }),
            "POST", "/parameterizedSearch", json!({}),
            json!({ "q": "Hopper", "fields": ["Id", "Name", "Company"], "sobjects": [{ "name": "Contact", "fields": ["Id", "Name", "Email"], "limit": 5 }, { "name": "Lead" }], "in": "NAME", "defaultLimit": 10, "overallLimit": 20 }),
            None, 200, found, found_returned),

        // sobjects
        row("sobjects.list", json!({ "contains": "invoice" }), "GET", "/sobjects", json!({}), json!(null), None, 200,
            every_object(),
            json!([{ "name": "Invoice__c", "label": "Customer Invoice", "labelPlural": "Customer Invoices", "keyPrefix": "a01", "custom": true, "queryable": true, "createable": true }])),
        row("sobjects.describe", json!({ "object": "Case" }), "GET", "/sobjects/Case/describe", json!({}), json!(null), None, 200,
            case_described(),
            json!({ "name": "Case", "keyPrefix": "500", "createable": true,
                "fields": [
                    { "name": "Id", "type": "id", "nillable": false, "createable": false, "idLookup": true, "picklistValues": [], "referenceTo": [] },
                    { "name": "Status", "type": "picklist", "inlineHelpText": "Where the case stands", "picklistValues": [
                        { "value": "New", "label": "New", "active": true, "defaultValue": true },
                        { "value": "Escalated", "active": true, "defaultValue": false },
                        { "value": "On Hold", "label": null, "active": false }
                    ] },
                    { "name": "AccountId", "type": "reference", "referenceTo": ["Account"], "relationshipName": "Account", "nillable": true, "defaultedOnCreate": false }
                ],
                "childRelationships": [
                    { "childSObject": "EmailMessage", "field": "ParentId", "relationshipName": "EmailMessages", "cascadeDelete": true },
                    { "childSObject": "CaseShare", "relationshipName": null }
                ],
                "recordTypeInfos": [{ "recordTypeId": "012000000000000AAA", "name": "Master", "master": true, "defaultRecordTypeMapping": true }] })),

        // records: reading
        row("records.get", json!({ "object": "Case", "id": CASE, "fields": ["CaseNumber", "Subject", "Status", "Priority"] }), "GET", "/sobjects/Case/500xx000000bcdeAAA", json!({ "fields": "CaseNumber,Subject,Status,Priority" }), json!(null), None, 200,
            case(), json!({ "type": "Case", "id": CASE, "fields": { "CaseNumber": "00001026", "Subject": "Pump will not start", "Status": "Escalated" } })),
        // Without `fields`, nothing is asked of Salesforce but the record.
        row("records.get", json!({ "object": "EmailMessage", "id": EMAIL }), "GET", "/sobjects/EmailMessage/02sxx000000rstuAAA", json!({}), json!(null), None, 200,
            email_message(), json!({ "type": "EmailMessage", "id": EMAIL, "fields": { "ParentId": CASE, "FromAddress": "grace@acme.example", "TextBody": "It still does not start.", "Incoming": true } })),
        // Another system's id may hold anything; it stays one segment of the path.
        row("records.get_by_external_id", json!({ "object": "Contact", "field": "ERP_Id__c", "value": "C-17/a b", "fields": ["Name", "Account.Name"] }), "GET", "/sobjects/Contact/ERP_Id__c/C-17%2Fa%20b", json!({ "fields": "Name,Account.Name" }), json!(null), None, 200,
            json!({ "attributes": attributes("Contact", CONTACT), "Id": CONTACT, "Name": "Grace Hopper", "Account": { "attributes": attributes("Account", ACCOUNT), "Name": "Acme" } }),
            json!({ "type": "Contact", "id": CONTACT, "fields": { "Name": "Grace Hopper", "Account": { "Name": "Acme" } } })),

        // records: writing
        row("records.create", json!({ "object": "Lead", "fields": { "LastName": "Turing", "Company": "Bletchley", "Email": "alan@bletchley.example", "NumberOfEmployees": 9000, "HasOptedOutOfEmail": false } }), "POST", "/sobjects/Lead", json!({}),
            json!({ "LastName": "Turing", "Company": "Bletchley", "Email": "alan@bletchley.example", "NumberOfEmployees": 9000, "HasOptedOutOfEmail": false }), None, 201,
            created(LEAD), json!({ "id": LEAD, "created": true })),
        // A note on a record: the parent is one of its fields.
        row("records.create", json!({ "object": "Note", "fields": { "ParentId": ACCOUNT, "Title": "Renewal call", "Body": "They want a three-year term.\nSend the draft by Friday." } }), "POST", "/sobjects/Note", json!({}),
            json!({ "ParentId": ACCOUNT, "Title": "Renewal call", "Body": "They want a three-year term.\nSend the draft by Friday." }), None, 201,
            created(NOTE), json!({ "id": NOTE, "created": true })),
        // Salesforce answers an update with no content. `null` clears a field.
        row("records.update", json!({ "object": "Task", "id": TASK, "fields": { "Status": "Completed", "Description": null } }), "PATCH", "/sobjects/Task/00Txx000003fghiAAA", json!({}),
            json!({ "Status": "Completed", "Description": null }), None, 204,
            json!(null), json!(null)),
        row("records.upsert", json!({ "object": "Contact", "field": "ERP_Id__c", "value": "C-18", "fields": { "LastName": "Lovelace", "Account": { "ERP_Id__c": "A-17" } } }), "PATCH", "/sobjects/Contact/ERP_Id__c/C-18", json!({}),
            json!({ "LastName": "Lovelace", "Account": { "ERP_Id__c": "A-17" } }), None, 201,
            json!({ "id": CONTACT, "success": true, "errors": [], "created": true }), json!({ "id": CONTACT, "created": true })),
        row("records.upsert", json!({ "object": "Account", "field": "ERP_Id__c", "value": "A-17", "fields": { "Industry": "Energy" } }), "PATCH", "/sobjects/Account/ERP_Id__c/A-17", json!({}),
            json!({ "Industry": "Energy" }), None, 200,
            json!({ "id": ACCOUNT, "success": true, "errors": [], "created": false }), json!({ "id": ACCOUNT, "created": false })),
        row("records.delete", json!({ "object": "Event", "id": EVENT }), "DELETE", "/sobjects/Event/00Uxx000001jklmAAA", json!({}), json!(null), None, 204,
            json!(null), json!(null)),

        // limits
        row("limits.get", json!({}), "GET", "/limits", json!({}), json!(null), None, 200,
            limits(),
            json!({ "DailyApiRequests": { "Max": 15000, "Remaining": 14998 }, "DataStorageMB": { "Max": 5, "Remaining": 5 }, "SingleEmail": { "Max": 15, "Remaining": 15 } })),
    ]
}

/// Every operation Salesforce offers, with what it does to the
/// organisation's data. A host lets a read run freely and asks a person
/// before anything else, so each effect is stated here and not derived from
/// the code under test.
const EXPECTED: [(&str, Effect); 13] = [
    // SOQL and SOSL have no statement that changes a record.
    ("query.run", Effect::Read),
    ("query.run_all", Effect::Read),
    ("search.run", Effect::Read),
    ("search.find", Effect::Read),
    ("sobjects.list", Effect::Read),
    ("sobjects.describe", Effect::Read),
    ("records.get", Effect::Read),
    ("records.get_by_external_id", Effect::Read),
    ("records.create", Effect::Write),
    ("records.update", Effect::Write),
    ("records.upsert", Effect::Write),
    ("records.delete", Effect::Destructive),
    ("limits.get", Effect::Read),
];

#[tokio::test]
async fn salesforce_offers_exactly_these_operations_and_each_has_a_row_below() {
    let mut listed: Vec<String> = Salesforce::new().operations().into_iter().map(|o| o.name).collect();
    let mut expected: Vec<String> = EXPECTED.iter().map(|(name, _)| format!("salesforce.{name}")).collect();
    expected.extend([
        "salesforce.identity.get".to_owned(),
        "salesforce.resource.resolve".to_owned(),
    ]);
    listed.sort();
    expected.sort();
    assert_eq!(listed, expected);

    for (name, _) in EXPECTED {
        assert!(cases().iter().any(|case| case.name == name), "{name} has no test case");
    }
    for case in cases() {
        assert!(
            EXPECTED.iter().any(|(name, _)| *name == case.name),
            "{}: a test case names an operation that does not exist",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_salesforce_sent() {
    for case in cases() {
        let (server, socket, key) = salesforce().await;
        Mock::given(method(case.verb))
            .and(path(format!("{API}{}", case.path)))
            .respond_with(answer(case.status, &case.response))
            .mount(&server)
            .await;

        let output = invoke(&socket, &key, case.name, case.input.clone())
            .await
            .unwrap_or_else(|e| panic!("{} {}: {e}", case.name, case.input));
        assert!(
            contains(&output, &case.returns),
            "{}: returned {output}, expected {}",
            case.name,
            case.returns
        );

        let request = only_request(&server).await;
        assert_eq!(
            header(&request, "authorization"),
            Some(format!("Bearer {TOKEN}").as_str()),
            "{}",
            case.name
        );
        assert_eq!(
            query_of(&request),
            case.query,
            "{}: exactly these parameters reach Salesforce",
            case.name
        );
        assert_eq!(
            body_of(&request),
            case.body,
            "{}: exactly this body reaches Salesforce",
            case.name
        );
        assert_eq!(
            header(&request, "sforce-query-options"),
            case.options,
            "{}: exactly this batch size is asked for",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_says_what_it_changes_what_it_needs_and_what_it_takes() {
    let operations = Salesforce::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    for (name, effect) in EXPECTED {
        let operation = find(&format!("salesforce.{name}"));
        assert_eq!(operation.effect, effect, "{name}");
        // Salesforce has one scope for the data API, for reading and writing alike.
        assert_eq!(operation.required_scopes, ["api"], "{name}");
        assert_eq!(operation.input_schema["type"], "object", "{name}");
        assert_eq!(operation.input_schema["additionalProperties"], false, "{name}");
        assert!(!operation.description.is_empty(), "{name}");
    }
    assert_eq!(find("salesforce.identity.get").effect, Effect::Read);
    assert_eq!(find("salesforce.identity.get").required_scopes, ["id"]);
    assert_eq!(find("salesforce.resource.resolve").effect, Effect::Read);
    assert_eq!(find("salesforce.resource.resolve").required_scopes, ["api"]);

    // Whatever changes a record is never a read, and is never sent as a GET,
    // which the transport repeats after a server error. The one read
    // Salesforce offers only as a POST is the only read that is not a GET.
    for case in cases() {
        let effect = find(&format!("salesforce.{}", case.name)).effect;
        match (effect, case.verb) {
            (Effect::Read, "POST") => assert_eq!(case.name, "search.find"),
            (Effect::Read, verb) => assert_eq!(verb, "GET", "{}", case.name),
            (_, verb) => assert!(matches!(verb, "POST" | "PATCH" | "DELETE"), "{}: {verb}", case.name),
        }
        if matches!(case.verb, "POST" | "PATCH" | "DELETE") && case.name != "search.find" {
            assert_ne!(effect, Effect::Read, "{}: a write must not be called a read", case.name);
        }
    }

    let required = |name: &str| -> Vec<String> {
        find(name).input_schema["required"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|field| field.as_str().map(str::to_owned))
            .collect()
    };
    // A query is needed for the first batch only: a cursor stands in for it.
    assert!(required("salesforce.query.run").is_empty());
    assert!(required("salesforce.query.run_all").is_empty());
    assert_eq!(required("salesforce.search.run"), ["sosl"]);
    assert_eq!(required("salesforce.search.find"), ["text"]);
    assert_eq!(required("salesforce.records.get"), ["object", "id"]);
    assert_eq!(required("salesforce.records.create"), ["object", "fields"]);
    assert_eq!(
        required("salesforce.records.upsert"),
        ["object", "field", "value", "fields"]
    );
    assert_eq!(required("salesforce.records.delete"), ["object", "id"]);
    assert!(required("salesforce.limits.get").is_empty());
    assert!(required("salesforce.sobjects.list").is_empty());

    let query = find("salesforce.query.run");
    for field in ["soql", "cursor", "batch_size"] {
        assert!(
            query.input_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    for field in ["totalSize", "done", "records", "next_cursor"] {
        assert!(
            query.output_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    let record = &find("salesforce.records.get").output_schema["properties"];
    for field in ["type", "id", "fields"] {
        assert!(record.get(field).is_some(), "{field} is described");
    }
    let describe = &find("salesforce.sobjects.describe").output_schema;
    assert!(describe["properties"].get("fields").is_some());
    assert!(describe["$defs"]["Field"]["properties"].get("picklistValues").is_some());
}

// ── What is sent ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_query_reaches_salesforce_character_for_character() {
    // What a person typed, placed in the query with the escaping helper.
    let name = "O'Brien & Søn + 100% \\ \"quoted\"\n東京";
    let soql = format!("SELECT Id FROM Account WHERE Name = '{}'", escape_soql(name));
    let (server, socket, key) = answering(200, json!({ "totalSize": 0, "done": true, "records": [] })).await;
    invoke(&socket, &key, "query.run", json!({ "soql": soql }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(query_of(&request), json!({ "q": soql }));
    // Nothing in the query is left for a server to read two ways: a space is
    // not a plus, and a plus is not a space.
    let written = request.url.query().unwrap();
    assert!(written.starts_with("q=SELECT%20Id%20FROM%20Account"), "{written}");
    assert!(written.contains("%2B") && !written.contains('+'), "{written}");
    assert!(written.contains("O%5C%27Brien"), "the quote arrives escaped: {written}");
}

#[tokio::test]
async fn the_text_of_a_search_is_looked_for_as_it_stands() {
    let (server, socket, key) = answering(200, json!({ "searchRecords": [] })).await;
    let input = json!({ "text": "  Smith-Jones (UK) *\"x\"} RETURNING User(Id)  " });
    let output = invoke(&socket, &key, "search.find", input).await.unwrap();
    assert_eq!(output, json!({ "records": [] }));
    // Every character SOSL gives a meaning to is escaped, and nothing but
    // the text is sent: no types, no fields, no limits.
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "q": r#"Smith\-Jones \(UK\) \*\"x\"\} RETURNING User\(Id\)"# })
    );
}

#[tokio::test]
async fn the_list_of_object_types_is_narrowed_before_it_is_passed_on() {
    for (input, names) in [
        (json!({}), vec!["Account", "Case", "Invoice__c", "AccountHistory"]),
        (json!({ "custom": true }), vec!["Invoice__c"]),
        (json!({ "custom": false }), vec!["Account", "Case", "AccountHistory"]),
        // By API name or by label, whatever the case.
        (json!({ "contains": "ACCOUNT" }), vec!["Account", "AccountHistory"]),
        (json!({ "contains": "customer" }), vec!["Invoice__c"]),
        (json!({ "contains": "account", "custom": true }), vec![]),
        (
            json!({ "contains": "  " }),
            vec!["Account", "Case", "Invoice__c", "AccountHistory"],
        ),
    ] {
        let (_server, socket, key) = answering(200, every_object()).await;
        let listed = invoke(&socket, &key, "sobjects.list", input.clone()).await.unwrap();
        let listed: Vec<&str> = listed
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|o| o["name"].as_str())
            .collect();
        assert_eq!(listed, names, "{input}");
    }
    // A row is a few words, not the object's whole description.
    let (_server, socket, key) = answering(200, every_object()).await;
    let listed = invoke(&socket, &key, "sobjects.list", json!({ "contains": "history" }))
        .await
        .unwrap();
    assert_eq!(
        listed,
        json!([{ "name": "AccountHistory", "label": "Account History", "labelPlural": "Account Historys", "keyPrefix": null,
            "custom": false, "queryable": true, "searchable": true, "createable": true, "updateable": true, "deletable": true }])
    );
}

#[tokio::test]
async fn only_an_allowance_is_reported_as_one() {
    let (_server, socket, key) = answering(
        200,
        json!({
            "DailyApiRequests": { "Max": 15000, "Remaining": 0 },
            "PermissionSets": { "Max": 1500, "Remaining": 1499, "CreateCustom": { "Max": 1000, "Remaining": 997 } },
            "Odd": "not an allowance",
            "HalfSaid": { "Max": 3 }
        }),
    )
    .await;
    let limits = invoke(&socket, &key, "limits.get", json!({})).await.unwrap();
    assert_eq!(
        limits,
        json!({ "DailyApiRequests": { "Max": 15000, "Remaining": 0 }, "PermissionSets": { "Max": 1500, "Remaining": 1499 } })
    );
}

// ── What is refused before Salesforce is called ──────────────────────────────

#[tokio::test]
async fn input_of_the_wrong_shape_is_refused_by_name_without_calling_salesforce() {
    let (server, socket, key) = answering(200, account()).await;
    for (name, input, field) in [
        ("query.run", json!({}), "soql"),
        ("query.run", json!({ "soql": 7 }), "soql"),
        (
            "query.run",
            json!({ "soql": "SELECT Id FROM Account", "batch_size": "200" }),
            "batch_size",
        ),
        ("search.run", json!({}), "sosl"),
        ("search.find", json!({ "objects": [{ "name": "Account" }] }), "text"),
        (
            "search.find",
            json!({ "text": "Acme", "within": "everywhere" }),
            "within",
        ),
        ("sobjects.describe", json!({}), "object"),
        ("records.get", json!({ "object": "Account" }), "id"),
        (
            "records.get",
            json!({ "object": "Account", "id": ACCOUNT, "fields": "Name" }),
            "fields",
        ),
        ("records.create", json!({ "object": "Account" }), "fields"),
        (
            "records.create",
            json!({ "object": "Account", "fields": ["Name"] }),
            "fields",
        ),
        (
            "records.upsert",
            json!({ "object": "Account", "field": "ERP_Id__c", "fields": {} }),
            "value",
        ),
        ("records.delete", json!({ "id": ACCOUNT }), "object"),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(
            err.message().contains(field),
            "{name}: the message names the field: {}",
            err.message()
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn values_that_cannot_work_are_refused_before_salesforce_is_called() {
    let (server, socket, key) = answering(200, account()).await;
    let soql = "SELECT Id FROM Account";
    for (name, input, reason) in [
        ("query.run", json!({ "soql": "  " }), "a query is required"),
        // Neither a query nor the place in one: there is nothing to ask for.
        (
            "query.run",
            json!({}),
            "give `soql`, or the `cursor` of the batch before",
        ),
        (
            "query.run_all",
            json!({ "soql": null, "batch_size": 200 }),
            "give `soql`, or the `cursor` of the batch before",
        ),
        (
            "query.run",
            json!({ "cursor": "  " }),
            "give `soql`, or the `cursor` of the batch before",
        ),
        (
            "query.run",
            json!({ "soql": soql, "batch_size": 199 }),
            "`batch_size` is from 200 to 2000",
        ),
        (
            "query.run_all",
            json!({ "soql": soql, "batch_size": 2001 }),
            "`batch_size` is from 200 to 2000",
        ),
        ("search.run", json!({ "sosl": "" }), "a search is required"),
        ("search.find", json!({ "text": " " }), "`text` is required"),
        (
            "search.find",
            json!({ "text": "Acme", "limit": 0 }),
            "`limit` is from 1 to 2000",
        ),
        (
            "search.find",
            json!({ "text": "Acme", "overall_limit": 2001 }),
            "`overall_limit` is from 1 to 2000",
        ),
        (
            "search.find",
            json!({ "text": "Acme", "objects": [{ "name": "Account", "limit": 0 }] }),
            "an object's `limit` is from 1 to 2000",
        ),
        // A field is returned of a type that is named.
        (
            "search.find",
            json!({ "text": "Acme", "fields": ["Name"] }),
            "`fields` needs `objects`",
        ),
        (
            "records.update",
            json!({ "object": "Account", "id": ACCOUNT, "fields": {} }),
            "`fields` names nothing to change",
        ),
        (
            "records.get",
            json!({ "object": "", "id": ACCOUNT }),
            "an object type is required",
        ),
        (
            "records.get",
            json!({ "object": "Account", "id": " " }),
            "a record id is required",
        ),
        (
            "records.get_by_external_id",
            json!({ "object": "Account", "field": "ERP_Id__c", "value": "" }),
            "an external id is required",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(err.message().contains(reason), "{name} {input}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Salesforce's answers ─────────────────────────────────────────────────────

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error_not_an_empty_result() {
    for (name, input) in [
        ("query.run", json!({ "soql": "SELECT Id FROM Account" })),
        ("query.run_all", json!({ "soql": "SELECT Id FROM Account" })),
        ("search.run", json!({ "sosl": "FIND {Acme}" })),
        ("search.find", json!({ "text": "Acme" })),
        ("sobjects.list", json!({})),
        ("sobjects.describe", json!({ "object": "Account" })),
        ("records.get", json!({ "object": "Account", "id": ACCOUNT })),
        (
            "records.get_by_external_id",
            json!({ "object": "Account", "field": "ERP_Id__c", "value": "A-17" }),
        ),
        (
            "records.create",
            json!({ "object": "Account", "fields": { "Name": "Acme" } }),
        ),
        (
            "records.upsert",
            json!({ "object": "Account", "field": "ERP_Id__c", "value": "A-17", "fields": {} }),
        ),
        ("limits.get", json!({})),
    ] {
        for body in [json!({}), json!(null)] {
            let (_server, socket, key) = answering(200, body.clone()).await;
            let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Decode, "{name} answered {body}: {err}");
        }
    }
    // A list that holds something that is not a record is not passed on short.
    let (_server, socket, key) = answering(
        200,
        json!({ "totalSize": 2, "done": true, "records": [account(), { "Id": CONTACT, "Name": "no attributes" }] }),
    )
    .await;
    let err = invoke(&socket, &key, "query.run", json!({ "soql": "SELECT Id FROM Account" }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    // A write Salesforce says did not succeed is not reported as saved.
    let (_server, socket, key) = answering(200, json!({ "id": LEAD, "success": false, "errors": [] })).await;
    let err = invoke(
        &socket,
        &key,
        "records.create",
        json!({ "object": "Lead", "fields": {} }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn an_upsert_whose_answer_cannot_be_read_says_the_record_was_written() {
    // Before version 46.0 Salesforce answered an upsert that changed a
    // record with no content. The record is written either way.
    let (server, socket, key) = answering(204, json!(null)).await;
    let input =
        json!({ "object": "Account", "field": "ERP_Id__c", "value": "A-17", "fields": { "Industry": "Energy" } });
    let err = invoke(&socket, &key, "records.upsert", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(err.message().contains("accepted the record"), "{}", err.message());
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "it is not sent again"
    );
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_place_and_never_repeats_what_salesforce_sent() {
    let mut described = case_described();
    described["fields"][1]["picklistValues"][2]["value"] = json!({ "secret": "the customer's own words" });
    let (_server, socket, key) = answering(200, described).await;
    let err = invoke(&socket, &key, "sobjects.describe", json!({ "object": "Case" }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(
        err.message().contains("fields[1].picklistValues[2].value"),
        "{}",
        err.message()
    );
    assert!(!format!("{err:?}").contains("the customer's own words"), "{err:?}");

    let mut listed = every_object();
    listed["sobjects"][2]["name"] = json!(["Invoice__c", "a customer's secret"]);
    let (_server, socket, key) = answering(200, listed).await;
    let err = invoke(&socket, &key, "sobjects.list", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(err.message().contains("[2].name"), "{}", err.message());
    assert!(!format!("{err:?}").contains("secret"), "{err:?}");
}

#[tokio::test]
async fn what_salesforce_leaves_null_does_not_stop_an_object_from_being_read() {
    let sparse = json!({
        "name": "Thing__c", "label": null, "labelPlural": null, "keyPrefix": null, "custom": null,
        "fields": [{ "name": "Odd__c", "label": null, "type": null, "length": null, "nillable": null, "referenceTo": null,
            "relationshipName": null, "picklistValues": null }],
        "childRelationships": null, "recordTypeInfos": null
    });
    let (_server, socket, key) = answering(200, sparse).await;
    let described = invoke(&socket, &key, "sobjects.describe", json!({ "object": "Thing__c" }))
        .await
        .unwrap();
    assert_eq!(described["name"], "Thing__c");
    assert_eq!(described["label"], "");
    assert_eq!(described["custom"], false);
    assert_eq!(described["fields"][0]["picklistValues"], json!([]));
    assert_eq!(described["fields"][0]["referenceTo"], json!([]));
    assert_eq!(described["childRelationships"], json!([]));
}

#[tokio::test]
async fn a_write_is_sent_once_when_salesforce_fails_and_a_read_is_tried_again() {
    let unavailable = || refusal(503, "SERVER_UNAVAILABLE", "The server is temporarily unavailable.");
    for (name, input) in [
        (
            "records.create",
            json!({ "object": "Lead", "fields": { "LastName": "Turing" } }),
        ),
        (
            "records.update",
            json!({ "object": "Task", "id": TASK, "fields": { "Status": "Completed" } }),
        ),
        (
            "records.upsert",
            json!({ "object": "Account", "field": "ERP_Id__c", "value": "A-17", "fields": {} }),
        ),
        // A read by effect, but Salesforce takes it as a POST, so it is not repeated either.
        ("search.find", json!({ "text": "Acme" })),
        // `records.delete` is not in this list. The transport still repeats a
        // DELETE after a server error; see the guide.
    ] {
        let (server, socket, key) = salesforce().await;
        Mock::given(any()).respond_with(unavailable()).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have happened, so it is not sent again"
        );
    }

    let (server, socket, key) = salesforce().await;
    Mock::given(any())
        .respond_with(unavailable())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(answer(200, &case()))
        .mount(&server)
        .await;
    let read = invoke(&socket, &key, "records.get", json!({ "object": "Case", "id": CASE }))
        .await
        .unwrap();
    assert_eq!(read["id"], CASE);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let salesforce = Salesforce::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(salesforce.clone()), TOKEN).await;
    let connection = socket.connection(key).await.unwrap();
    let mount = |verb: &'static str, at: String, status: u16, body: Value| {
        Mock::given(method(verb))
            .and(path(format!("{API}{at}")))
            .respond_with(answer(status, &body))
            .mount(&server)
    };
    mount(
        "GET",
        "/query".into(),
        200,
        json!({ "totalSize": 1, "done": true, "records": [opportunity()] }),
    )
    .await;
    mount(
        "POST",
        "/parameterizedSearch".into(),
        200,
        json!({ "searchRecords": [account()] }),
    )
    .await;
    mount("GET", "/sobjects".into(), 200, every_object()).await;
    mount("GET", "/sobjects/Case/describe".into(), 200, case_described()).await;
    mount("GET", format!("/sobjects/Account/{ACCOUNT}"), 200, account()).await;
    mount("POST", "/sobjects/Lead".into(), 201, created(LEAD)).await;
    mount("PATCH", format!("/sobjects/Lead/{LEAD}"), 204, json!(null)).await;
    mount("DELETE", format!("/sobjects/Lead/{LEAD}"), 204, json!(null)).await;
    mount("GET", "/limits".into(), 200, limits()).await;

    // A query, and the relationships that come back nested in it.
    let result = salesforce
        .query(&connection)
        .run(OPPORTUNITIES, QueryOptions::default())
        .await
        .unwrap();
    assert_eq!((result.total_size, result.done, result.next_cursor), (1, true, None));
    let deal = &result.records[0];
    assert_eq!(deal.object_type, "Opportunity");
    assert_eq!(deal.id.as_deref(), Some(OPPORTUNITY));
    assert_eq!(deal.fields["Amount"], 48000.0);
    let account_of = deal.parent("Account").unwrap();
    assert_eq!(
        (account_of.object_type.as_str(), account_of.id.as_deref()),
        ("Account", Some(ACCOUNT))
    );
    let roles = deal.children("OpportunityContactRoles");
    assert_eq!(roles.len(), 1);
    assert_eq!(roles[0].fields["Role"], "Decision Maker");
    let contact = roles[0].parent("Contact").unwrap();
    assert_eq!(
        contact.id.as_deref(),
        Some(CONTACT),
        "read from the record's own address"
    );
    assert_eq!(contact.fields["Email"], "grace@acme.example");

    let find = Find {
        text: "Acme".into(),
        objects: Some(vec![FindIn {
            name: "Account".into(),
            fields: Some(vec!["Id".into(), "Name".into()]),
            limit: None,
        }]),
        within: Some(SearchScope::Name),
        ..Find::default()
    };
    let found = salesforce.search(&connection).find(find).await.unwrap();
    assert_eq!(found.records[0].fields["Name"], "Acme");

    let custom = ListSObjects {
        custom: Some(true),
        ..ListSObjects::default()
    };
    let objects = salesforce.sobjects(&connection).list(custom).await.unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(
        (objects[0].name.as_str(), objects[0].key_prefix.as_deref()),
        ("Invoice__c", Some("a01"))
    );

    let described = salesforce.sobjects(&connection).describe("Case").await.unwrap();
    let status = described.fields.iter().find(|field| field.name == "Status").unwrap();
    assert_eq!(status.field_type, "picklist");
    let active: Vec<&str> = status
        .picklist_values
        .iter()
        .filter(|value| value.active)
        .map(|value| value.value.as_str())
        .collect();
    assert_eq!(active, ["New", "Escalated"]);
    let lookup = described.fields.iter().find(|field| field.name == "AccountId").unwrap();
    assert_eq!(
        (lookup.reference_to.as_slice(), lookup.relationship_name.as_deref()),
        (&["Account".to_owned()][..], Some("Account"))
    );
    assert_eq!(described.child_relationships[0].child_sobject, "EmailMessage");

    let records = salesforce.records(&connection);
    let got = records.get("Account", ACCOUNT, GetRecord::default()).await.unwrap();
    assert_eq!(got.parent("Owner").unwrap().id.as_deref(), Some(USER));
    let lead = RecordFields {
        fields: fields(json!({ "LastName": "Turing", "Company": "Bletchley" })),
    };
    let saved = records.create("Lead", lead).await.unwrap();
    assert_eq!((saved.id.as_str(), saved.created), (LEAD, true));
    let changes = RecordFields {
        fields: fields(json!({ "Status": "Working - Contacted" })),
    };
    records.update("Lead", LEAD, changes).await.unwrap();
    records.delete("Lead", LEAD).await.unwrap();

    let limits = salesforce.limits(&connection).get().await.unwrap();
    assert_eq!(
        (limits.daily_api_requests.max, limits.daily_api_requests.remaining),
        (15000, 14998)
    );
    assert_eq!(limits.others["DataStorageMB"].remaining, 5);

    let sent: Vec<(String, String, Value)> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method.as_str() != "GET")
        .map(|request| {
            (
                request.method.as_str().to_owned(),
                request.url.path().to_owned(),
                body_of(request),
            )
        })
        .collect();
    assert_eq!(
        sent,
        [
            (
                "POST".to_owned(),
                format!("{API}/parameterizedSearch"),
                json!({ "q": "Acme", "sobjects": [{ "name": "Account", "fields": ["Id", "Name"] }], "in": "NAME" })
            ),
            (
                "POST".to_owned(),
                format!("{API}/sobjects/Lead"),
                json!({ "LastName": "Turing", "Company": "Bletchley" })
            ),
            (
                "PATCH".to_owned(),
                format!("{API}/sobjects/Lead/{LEAD}"),
                json!({ "Status": "Working - Contacted" })
            ),
            ("DELETE".to_owned(), format!("{API}/sobjects/Lead/{LEAD}"), json!(null)),
        ]
    );
}
