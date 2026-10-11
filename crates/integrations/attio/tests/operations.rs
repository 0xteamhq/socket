//! Every Attio operation, called by name against a local server that answers as Attio does.

use serde_json::{Value, json};
use socketkit_attio::Attio;
use socketkit_core::{Effect, ErrorKind, Integration, Retry};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, ResponseTemplate};

mod support;
use support::{
    Case, TOKEN, answer, answering, attio, attio_error, attribute, body_of, call_recording,
    call_recording_with_transcript, comment, contains, entry, entry_returned, invoke, list, meeting, member, note,
    object, only_request, query_of, record, record_returned, task, thread,
};

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, path, query, body, response, returns| Case { name, input, verb, path, query, body, response, returns };
    let none = || json!(null);
    let person = json!({ "name": "Ada Lovelace", "email_addresses": ["ada@example.test"] });
    vec![
        // objects
        case("objects.list", json!({}), "GET", "/objects", json!({}), none(),
            json!({ "data": [object()] }), json!([{ "id": { "object_id": "obj-people" }, "api_slug": "people", "plural_noun": "People" }])),
        case("objects.get", json!({ "object": "people" }), "GET", "/objects/people", json!({}), none(),
            json!({ "data": object() }), json!({ "id": { "workspace_id": "ws-1", "object_id": "obj-people" }, "api_slug": "people", "singular_noun": "Person" })),

        // attributes
        case("attributes.list", json!({ "target": "objects", "identifier": "people", "limit": 1, "show_archived": true }), "GET", "/objects/people/attributes",
            json!({ "limit": "1", "show_archived": "true" }), none(),
            json!({ "data": [attribute()] }),
            json!({ "items": [{ "id": { "attribute_id": "attr-email" }, "api_slug": "email_addresses", "type": "email-address", "is_unique": true, "is_multiselect": true, "is_writable": true }], "next_cursor": "1" })),
        case("attributes.get", json!({ "target": "lists", "identifier": "sales", "attribute": "stage" }), "GET", "/lists/sales/attributes/stage", json!({}), none(),
            json!({ "data": attribute() }), json!({ "api_slug": "email_addresses", "title": "Email addresses" })),
        case("attributes.options", json!({ "target": "objects", "identifier": "companies", "attribute": "categories" }), "GET", "/objects/companies/attributes/categories/options", json!({}), none(),
            json!({ "data": [{ "id": { "workspace_id": "ws-1", "object_id": "obj-co", "attribute_id": "attr-cat", "option_id": "opt-1" }, "title": "SaaS", "is_archived": false }] }),
            json!([{ "id": { "option_id": "opt-1" }, "title": "SaaS", "is_archived": false }])),
        case("attributes.statuses", json!({ "target": "lists", "identifier": "sales", "attribute": "stage", "show_archived": false }), "GET", "/lists/sales/attributes/stage/statuses",
            json!({ "show_archived": "false" }), none(),
            json!({ "data": [{ "id": { "workspace_id": "ws-1", "object_id": "list-1", "attribute_id": "attr-stage", "status_id": "st-1" }, "title": "In progress", "is_archived": false, "celebration_enabled": false, "target_time_in_status": "P7D" }] }),
            json!([{ "id": { "status_id": "st-1" }, "title": "In progress", "target_time_in_status": "P7D" }])),

        // records: reading. Attio takes the query only as POST; it changes nothing.
        case("records.query", json!({ "object": "people", "filter": { "name": "Ada Lovelace" }, "sorts": [{ "direction": "asc", "attribute": "name", "field": "last_name" }], "limit": 1 }),
            "POST", "/objects/people/records/query", json!({}),
            json!({ "filter": { "name": "Ada Lovelace" }, "sorts": [{ "direction": "asc", "attribute": "name", "field": "last_name" }], "limit": 1, "offset": 0 }),
            json!({ "data": [record()] }), json!({ "items": [record_returned()], "next_cursor": "1" })),
        case("records.get", json!({ "object": "people", "record": "rec-1" }), "GET", "/objects/people/records/rec-1", json!({}), none(),
            json!({ "data": record() }), record_returned()),
        case("records.entries", json!({ "object": "people", "record": "rec-1" }), "GET", "/objects/people/records/rec-1/entries", json!({ "limit": "100", "offset": "0" }), none(),
            json!({ "data": [{ "list_id": "list-1", "list_api_slug": "sales", "entry_id": "ent-1", "created_at": "2026-10-02T09:00:00.000000000Z" }] }),
            json!({ "items": [{ "list_id": "list-1", "list_api_slug": "sales", "entry_id": "ent-1" }], "next_cursor": null })),

        // records: writing
        case("records.create", json!({ "object": "people", "values": person.clone() }), "POST", "/objects/people/records", json!({}),
            json!({ "data": { "values": person.clone() } }), json!({ "data": record() }), record_returned()),
        case("records.update", json!({ "object": "people", "record": "rec-1", "values": { "job_title": "Analyst" } }), "PATCH", "/objects/people/records/rec-1", json!({}),
            json!({ "data": { "values": { "job_title": "Analyst" } } }), json!({ "data": record() }), record_returned()),
        case("records.assert", json!({ "object": "people", "matching_attribute": "email_addresses", "values": person.clone() }), "PUT", "/objects/people/records",
            json!({ "matching_attribute": "email_addresses" }),
            json!({ "data": { "values": person.clone() } }), json!({ "data": record() }), record_returned()),
        case("records.delete", json!({ "object": "people", "record": "rec-1" }), "DELETE", "/objects/people/records/rec-1", json!({}), none(),
            json!({}), none()),

        // lists
        case("lists.list", json!({}), "GET", "/lists", json!({}), none(),
            json!({ "data": [list()] }), json!([{ "id": { "list_id": "list-1" }, "api_slug": "sales", "name": "Sales", "parent_object": ["companies"] }])),
        case("lists.get", json!({ "list": "sales" }), "GET", "/lists/sales", json!({}), none(),
            json!({ "data": list() }), json!({ "api_slug": "sales", "workspace_access": "read-and-write", "workspace_member_access": [{ "workspace_member_id": "mem-1", "level": "full-access" }] })),

        // entries. Attio takes the query only as POST; it changes nothing.
        case("entries.query", json!({ "list": "sales", "filter": { "stage": "In progress" } }), "POST", "/lists/sales/entries/query", json!({}),
            json!({ "filter": { "stage": "In progress" }, "limit": 50, "offset": 0 }),
            json!({ "data": [entry()] }), json!({ "items": [entry_returned()], "next_cursor": null })),
        case("entries.get", json!({ "list": "sales", "entry": "ent-1" }), "GET", "/lists/sales/entries/ent-1", json!({}), none(),
            json!({ "data": entry() }), entry_returned()),
        case("entries.create", json!({ "list": "sales", "parent_object": "companies", "parent_record_id": "rec-2" }), "POST", "/lists/sales/entries", json!({}),
            json!({ "data": { "parent_object": "companies", "parent_record_id": "rec-2", "entry_values": {} } }),
            json!({ "data": entry() }), entry_returned()),
        case("entries.update", json!({ "list": "sales", "entry": "ent-1", "entry_values": { "stage": "Won" } }), "PATCH", "/lists/sales/entries/ent-1", json!({}),
            json!({ "data": { "entry_values": { "stage": "Won" } } }), json!({ "data": entry() }), entry_returned()),
        case("entries.delete", json!({ "list": "sales", "entry": "ent-1" }), "DELETE", "/lists/sales/entries/ent-1", json!({}), none(),
            json!({}), none()),

        // notes
        case("notes.list", json!({ "parent_object": "people", "parent_record_id": "rec-1" }), "GET", "/notes",
            json!({ "limit": "10", "offset": "0", "parent_object": "people", "parent_record_id": "rec-1" }), none(),
            json!({ "data": [note()] }),
            json!({ "items": [{ "id": { "note_id": "note-1" }, "title": "Call", "content_plaintext": "Asked about pricing." }], "next_cursor": null })),
        case("notes.get", json!({ "note": "note-1" }), "GET", "/notes/note-1", json!({}), none(),
            json!({ "data": note() }),
            json!({ "id": { "note_id": "note-1" }, "parent_object": "people", "parent_record_id": "rec-1", "content_markdown": "Asked about **pricing**.", "tags": [{ "type": "workspace-member" }] })),
        case("notes.create", json!({ "parent_object": "people", "parent_record_id": "rec-1", "title": "Call", "content": "Asked about pricing." }), "POST", "/notes", json!({}),
            json!({ "data": { "parent_object": "people", "parent_record_id": "rec-1", "title": "Call", "format": "plaintext", "content": "Asked about pricing." } }),
            json!({ "data": note() }), json!({ "id": { "note_id": "note-1" }, "title": "Call" })),
        case("notes.delete", json!({ "note": "note-1" }), "DELETE", "/notes/note-1", json!({}), none(), json!({}), none()),

        // tasks
        case("tasks.list", json!({ "linked_object": "people", "linked_record_id": "rec-1", "assignee": "grace@example.test", "is_completed": false, "sort": "created_at:desc" }), "GET", "/tasks",
            json!({ "limit": "50", "offset": "0", "sort": "created_at:desc", "linked_object": "people", "linked_record_id": "rec-1", "assignee": "grace@example.test", "is_completed": "false" }), none(),
            json!({ "data": [task()] }),
            json!({ "items": [{ "id": { "task_id": "task-1" }, "content_plaintext": "Send the contract", "is_completed": false }], "next_cursor": null })),
        case("tasks.get", json!({ "task": "task-1" }), "GET", "/tasks/task-1", json!({}), none(),
            json!({ "data": task() }),
            json!({ "deadline_at": "2026-10-20T17:00:00.000000000Z", "linked_records": [{ "target_object_id": "people", "target_record_id": "rec-1" }], "assignees": [{ "referenced_actor_type": "workspace-member", "referenced_actor_id": "mem-1" }] })),
        // Attio asks for every field of a new task, so what was not said goes out as its default.
        case("tasks.create", json!({ "content": "Send the contract" }), "POST", "/tasks", json!({}),
            json!({ "data": { "content": "Send the contract", "format": "plaintext", "deadline_at": null, "is_completed": false, "linked_records": [], "assignees": [] } }),
            json!({ "data": task() }), json!({ "id": { "task_id": "task-1" } })),
        case("tasks.update", json!({ "task": "task-1", "is_completed": true }), "PATCH", "/tasks/task-1", json!({}),
            json!({ "data": { "is_completed": true } }), json!({ "data": task() }), json!({ "id": { "task_id": "task-1" } })),
        case("tasks.delete", json!({ "task": "task-1" }), "DELETE", "/tasks/task-1", json!({}), none(), json!({}), none()),

        // threads
        case("threads.list", json!({ "object": "people", "record_id": "rec-1" }), "GET", "/threads",
            json!({ "limit": "10", "offset": "0", "object": "people", "record_id": "rec-1" }), none(),
            json!({ "data": [{ "id": { "workspace_id": "ws-1", "thread_id": "thr-1" }, "created_at": "2026-10-01T09:00:00.000000000Z", "comments": [comment()], "has_more_comments": true }] }),
            json!({ "items": [{ "id": { "thread_id": "thr-1" }, "comments": [{ "content_plaintext": "Looks good" }], "has_more_comments": true }], "next_cursor": null })),
        case("threads.get", json!({ "thread": "thr-1", "limit": 1, "created_after": "2026-10-01T00:00:00Z" }), "GET", "/threads/thr-1",
            json!({ "limit": "1", "created_after": "2026-10-01T00:00:00Z" }), none(),
            json!({ "data": thread(), "pagination": { "next_cursor": "comments-2" } }),
            json!({ "id": { "thread_id": "thr-1" }, "comments": [{ "id": { "comment_id": "com-1" }, "author": { "type": "workspace-member", "id": "mem-1" }, "record": { "record_id": "rec-1" } }], "next_cursor": "comments-2" })),
        case("threads.comment", json!({ "content": "Looks good", "author": "mem-1", "thread_id": "thr-1" }), "POST", "/comments", json!({}),
            json!({ "data": { "format": "plaintext", "content": "Looks good", "author": { "type": "workspace-member", "id": "mem-1" }, "thread_id": "thr-1" } }),
            json!({ "data": comment() }), json!({ "id": { "comment_id": "com-1" }, "thread_id": "thr-1", "content_plaintext": "Looks good" })),

        // workspace members
        case("workspace_members.list", json!({}), "GET", "/workspace_members", json!({}), none(),
            json!({ "data": [member()] }), json!([{ "id": { "workspace_member_id": "mem-1" }, "email_address": "grace@example.test", "access_level": "admin" }])),
        case("workspace_members.get", json!({ "member": "mem-1" }), "GET", "/workspace_members/mem-1", json!({}), none(),
            json!({ "data": member() }), json!({ "first_name": "Grace", "last_name": "Hopper" })),

        // meetings
        case("meetings.list", json!({ "linked_object": "people", "linked_record_id": "rec-1", "participants": ["ada@example.test", "grace@example.test"], "sort": "start_desc", "ends_from": "2026-10-01T00:00:00Z", "limit": 2 }), "GET", "/meetings",
            json!({ "limit": "2", "linked_object": "people", "linked_record_id": "rec-1", "participants": "ada@example.test,grace@example.test", "sort": "start_desc", "ends_from": "2026-10-01T00:00:00Z" }), none(),
            json!({ "data": [meeting()], "pagination": { "next_cursor": "meetings-2" } }),
            json!({ "items": [{ "id": { "meeting_id": "meet-1" }, "title": "Pricing review", "start": { "datetime": "2026-10-12T16:00:00.000Z", "timezone": "Europe/London" } }], "next_cursor": "meetings-2" })),
        case("meetings.get", json!({ "meeting": "meet-1" }), "GET", "/meetings/meet-1", json!({}), none(),
            json!({ "data": meeting() }),
            json!({ "participants": [{ "status": "accepted", "is_organizer": true, "email_address": "grace@example.test" }], "linked_records": [{ "object_slug": "people", "record_id": "rec-1" }] })),

        // call recordings
        case("call_recordings.list", json!({ "meeting": "meet-1" }), "GET", "/meetings/meet-1/call_recordings", json!({}), none(),
            json!({ "data": [call_recording()], "pagination": { "next_cursor": null } }),
            json!({ "items": [{ "id": { "call_recording_id": "cr-1" }, "status": "completed", "transcript": null }], "next_cursor": null })),
        case("call_recordings.get", json!({ "meeting": "meet-1", "call_recording": "cr-1" }), "GET", "/meetings/meet-1/call_recordings/cr-1", json!({}), none(),
            json!({ "data": call_recording_with_transcript() }),
            json!({ "id": { "meeting_id": "meet-1", "call_recording_id": "cr-1" }, "video_url": "https://files.attio.example/cr-1.mp4",
                "transcript": { "segments": [
                    { "speech": "Hello,", "start_time": 0.51, "end_time": 0.81, "speaker": { "name": "Alex Bell" } },
                    { "speech": "I'm here.", "start_time": 4.21, "end_time": 4.91, "speaker": { "name": "Tom Watson" } }
                ], "raw_transcript": "[00:00] Alex Bell: Hello,\n[00:04] Tom Watson: I'm here." } })),
    ]
}

#[tokio::test]
async fn the_table_below_covers_every_operation_attio_offers() {
    let listed: Vec<String> = Attio::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = cases().iter().map(|c| format!("attio.{}", c.name)).collect();
    tested.extend(["attio.identity.get".to_owned(), "attio.resource.resolve".to_owned()]);
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist"
    );
    assert_eq!(listed.len(), 40);
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_attio_sent() {
    for case in cases() {
        let (server, socket, key) = attio().await;
        Mock::given(method(case.verb))
            .and(path(format!("/v2{}", case.path)))
            .respond_with(answer(200, &case.response))
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
            "{}: exactly these parameters reach Attio",
            case.name
        );
        assert_eq!(
            body_of(&request),
            case.body,
            "{}: exactly this body reaches Attio",
            case.name
        );
    }
}

/// What each operation does to a workspace's data and what Attio asks of a
/// token for it, written out here from Attio's reference and not read back
/// from the code. A host runs a `Read` unasked and asks a person before
/// anything else, so a write that was marked `Read` has to fail this test.
#[rustfmt::skip]
fn marked() -> Vec<(&'static str, Effect, Vec<&'static str>)> {
    use Effect::{Destructive, Read, Write};
    vec![
        ("objects.list", Read, vec!["object_configuration:read"]),
        ("objects.get", Read, vec!["object_configuration:read"]),
        ("attributes.list", Read, vec!["object_configuration:read", "list_configuration:read"]),
        ("attributes.get", Read, vec!["object_configuration:read", "list_configuration:read"]),
        ("attributes.options", Read, vec!["object_configuration:read", "list_configuration:read"]),
        ("attributes.statuses", Read, vec!["object_configuration:read", "list_configuration:read"]),
        ("records.query", Read, vec!["record_permission:read", "object_configuration:read"]),
        ("records.get", Read, vec!["record_permission:read", "object_configuration:read"]),
        ("records.entries", Read, vec!["record_permission:read", "object_configuration:read", "list_entry:read"]),
        ("records.create", Write, vec!["record_permission:read-write", "object_configuration:read"]),
        ("records.update", Write, vec!["record_permission:read-write", "object_configuration:read"]),
        ("records.assert", Write, vec!["record_permission:read-write", "object_configuration:read"]),
        ("records.delete", Destructive, vec!["record_permission:read-write", "object_configuration:read"]),
        ("lists.list", Read, vec!["list_configuration:read"]),
        ("lists.get", Read, vec!["list_configuration:read"]),
        ("entries.query", Read, vec!["list_entry:read", "list_configuration:read"]),
        ("entries.get", Read, vec!["list_entry:read", "list_configuration:read"]),
        ("entries.create", Write, vec!["list_entry:read-write", "list_configuration:read"]),
        ("entries.update", Write, vec!["list_entry:read-write", "list_configuration:read"]),
        ("entries.delete", Destructive, vec!["list_entry:read-write", "list_configuration:read"]),
        ("notes.list", Read, vec!["note:read", "object_configuration:read", "record_permission:read"]),
        ("notes.get", Read, vec!["note:read", "object_configuration:read", "record_permission:read"]),
        ("notes.create", Write, vec!["note:read-write", "object_configuration:read", "record_permission:read"]),
        ("notes.delete", Destructive, vec!["note:read-write"]),
        ("tasks.list", Read, vec!["task:read", "object_configuration:read", "record_permission:read", "user_management:read"]),
        ("tasks.get", Read, vec!["task:read", "object_configuration:read", "record_permission:read", "user_management:read"]),
        ("tasks.create", Write, vec!["task:read-write", "object_configuration:read", "record_permission:read", "user_management:read"]),
        ("tasks.update", Write, vec!["task:read-write", "object_configuration:read", "record_permission:read", "user_management:read"]),
        ("tasks.delete", Destructive, vec!["task:read-write"]),
        ("threads.list", Read, vec!["comment:read"]),
        ("threads.get", Read, vec!["comment:read"]),
        ("threads.comment", Write, vec!["comment:read-write"]),
        ("workspace_members.list", Read, vec!["user_management:read"]),
        ("workspace_members.get", Read, vec!["user_management:read"]),
        ("meetings.list", Read, vec!["meeting:read", "record_permission:read"]),
        ("meetings.get", Read, vec!["meeting:read", "record_permission:read"]),
        ("call_recordings.list", Read, vec!["meeting:read", "call_recording:read"]),
        ("call_recordings.get", Read, vec!["meeting:read", "call_recording:read"]),
        ("identity.get", Read, vec![]),
        ("resource.resolve", Read, vec!["object_configuration:read"]),
    ]
}

#[tokio::test]
async fn every_operation_is_marked_with_what_it_changes_and_the_scopes_it_needs() {
    let operations = Attio::new().operations();
    let marked = marked();
    assert_eq!(operations.len(), marked.len(), "every operation is marked here");
    for (name, effect, scopes) in marked {
        let operation = operations
            .iter()
            .find(|o| o.name == format!("attio.{name}"))
            .unwrap_or_else(|| panic!("{name} is not offered"));
        assert_eq!(operation.effect, effect, "{name}");
        assert_eq!(operation.required_scopes, scopes, "{name}");
        // The verb agrees with the mark, but for the two queries Attio takes as POST.
        if let Some(case) = cases().iter().find(|c| c.name == name) {
            let is_get = case.verb == "GET";
            let is_query = matches!(name, "records.query" | "entries.query");
            assert_eq!(
                effect == Effect::Read,
                is_get || is_query,
                "{name} is sent as {}",
                case.verb
            );
            assert_eq!(effect == Effect::Destructive, case.verb == "DELETE", "{name}");
        }
        assert_eq!(operation.input_schema["type"], "object", "{name}");
        assert_eq!(
            operation.input_schema["additionalProperties"], false,
            "{name}: a field it does not list is not allowed"
        );
    }
}

#[tokio::test]
async fn an_operations_input_names_what_is_required_and_describes_each_field() {
    let operations = Attio::new().operations();
    let schema = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == format!("attio.{name}"))
            .unwrap()
            .input_schema
            .clone()
    };
    let required = |name: &str| {
        let mut names: Vec<String> = schema(name)["required"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|n| n.as_str().unwrap().to_owned())
            .collect();
        names.sort();
        names
    };
    assert_eq!(required("records.get"), ["object", "record"]);
    assert_eq!(required("records.query"), ["object"]);
    assert_eq!(required("records.create"), ["object", "values"]);
    assert_eq!(required("records.assert"), ["matching_attribute", "object", "values"]);
    assert_eq!(
        required("entries.create"),
        ["list", "parent_object", "parent_record_id"]
    );
    assert_eq!(required("entries.update"), ["entry", "entry_values", "list"]);
    assert_eq!(
        required("notes.create"),
        ["content", "parent_object", "parent_record_id", "title"]
    );
    assert_eq!(required("tasks.create"), ["content"]);
    assert_eq!(required("tasks.update"), ["task"]);
    assert_eq!(required("threads.comment"), ["author", "content"]);
    assert_eq!(required("attributes.list"), ["identifier", "target"]);
    assert!(required("objects.list").is_empty());
    assert!(required("notes.list").is_empty());

    let query = schema("records.query");
    for field in ["object", "filter", "filter_view_id", "sorts", "cursor", "limit"] {
        let described = query["properties"][field]["description"].as_str();
        assert!(described.is_some_and(|d| !d.is_empty()), "records.query.{field}");
    }
    let attributes = schema("attributes.list");
    assert_eq!(attributes["properties"]["target"]["$ref"], "#/$defs/AttributeTarget");
    assert_eq!(
        attributes["$defs"]["AttributeTarget"]["enum"],
        json!(["objects", "lists"])
    );
}

// ── What is sent ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn options_that_are_not_set_are_not_sent() {
    for (name, input, query, body) in [
        (
            "records.query",
            json!({ "object": "people" }),
            json!({}),
            json!({ "limit": 50, "offset": 0 }),
        ),
        (
            "tasks.list",
            json!({}),
            json!({ "limit": "50", "offset": "0" }),
            json!(null),
        ),
        (
            "notes.list",
            json!({}),
            json!({ "limit": "10", "offset": "0" }),
            json!(null),
        ),
        ("meetings.list", json!({}), json!({}), json!(null)),
        (
            "attributes.list",
            json!({ "target": "lists", "identifier": "sales" }),
            json!({}),
            json!(null),
        ),
        ("threads.get", json!({ "thread": "thr-1" }), json!({}), json!(null)),
        // A filter that is blank says nothing, and is not sent as an empty one.
        (
            "notes.list",
            json!({ "parent_object": " ", "parent_record_id": "" }),
            json!({ "limit": "10", "offset": "0" }),
            json!(null),
        ),
        (
            "meetings.list",
            json!({ "participants": [" "], "timezone": "" }),
            json!({}),
            json!(null),
        ),
    ] {
        let (server, socket, key) = answering(200, json!({ "data": [], "id": {} })).await;
        // `threads.get` reads one thing; an empty list is not one, which is another test's concern.
        let _ = invoke(&socket, &key, name, input).await;
        let request = only_request(&server).await;
        assert_eq!(query_of(&request), query, "{name}");
        assert_eq!(body_of(&request), body, "{name}");
    }
}

#[tokio::test]
async fn what_the_caller_wrote_as_null_is_sent_as_null() {
    // A deadline is taken away by sending `null` for it, so that has to be
    // told apart from not naming it.
    let (server, socket, key) = answering(200, json!({ "data": task() })).await;
    invoke(
        &socket,
        &key,
        "tasks.update",
        json!({ "task": "task-1", "deadline_at": null }),
    )
    .await
    .unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": { "deadline_at": null } })
    );

    // Values and filters are Attio's own forms and go out exactly as written.
    let (server, socket, key) = answering(200, json!({ "data": record() })).await;
    let values = json!({ "job_title": null, "email_addresses": [], "company": [{ "target_object": "companies", "target_record_id": "rec-2" }] });
    invoke(
        &socket,
        &key,
        "records.update",
        json!({ "object": "people", "record": "rec-1", "values": values }),
    )
    .await
    .unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": { "values": values } })
    );

    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    let filter = json!({ "$and": [{ "stage": { "$not_empty": true } }, { "owner": { "referenced_actor_id": null } }] });
    invoke(
        &socket,
        &key,
        "entries.query",
        json!({ "list": "sales", "filter": filter }),
    )
    .await
    .unwrap();
    assert_eq!(body_of(&only_request(&server).await)["filter"], filter);
}

#[tokio::test]
async fn a_new_note_or_task_or_comment_carries_everything_that_was_said() {
    let (server, socket, key) = answering(200, json!({ "data": note() })).await;
    let input = json!({
        "parent_object": " people ", "parent_record_id": "rec-1", "title": "Call", "content": "# Pricing\n\nAsked.",
        "format": "markdown", "created_at": "2026-09-30T10:00:00Z", "meeting_id": "meet-1"
    });
    invoke(&socket, &key, "notes.create", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": {
            "parent_object": "people", "parent_record_id": "rec-1", "title": "Call", "format": "markdown",
            "content": "# Pricing\n\nAsked.", "created_at": "2026-09-30T10:00:00Z", "meeting_id": "meet-1"
        } })
    );

    let (server, socket, key) = answering(200, json!({ "data": task() })).await;
    let linked = json!([{ "target_object": "people", "target_record_id": "rec-1" }, "ada@example.test"]);
    let assignees = json!([{ "workspace_member_email_address": "grace@example.test" }]);
    let input = json!({
        "content": "Send the contract", "deadline_at": "2026-10-20T17:00:00Z", "is_completed": true,
        "linked_records": linked, "assignees": assignees
    });
    invoke(&socket, &key, "tasks.create", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": {
            "content": "Send the contract", "format": "plaintext", "deadline_at": "2026-10-20T17:00:00Z",
            "is_completed": true, "linked_records": linked, "assignees": assignees
        } })
    );

    for (place, sent) in [
        (
            json!({ "record": { "object": "people", "record_id": "rec-1" } }),
            json!({ "record": { "object": "people", "record_id": "rec-1" } }),
        ),
        (
            json!({ "entry": { "list": "sales", "entry_id": "ent-1" } }),
            json!({ "entry": { "list": "sales", "entry_id": "ent-1" } }),
        ),
    ] {
        let (server, socket, key) = answering(200, json!({ "data": comment() })).await;
        let mut input = json!({ "content": "Looks good", "author": "mem-1" });
        input
            .as_object_mut()
            .unwrap()
            .extend(place.as_object().unwrap().clone());
        invoke(&socket, &key, "threads.comment", input).await.unwrap();
        let mut expected = json!({ "format": "plaintext", "content": "Looks good", "author": { "type": "workspace-member", "id": "mem-1" } });
        expected
            .as_object_mut()
            .unwrap()
            .extend(sent.as_object().unwrap().clone());
        assert_eq!(body_of(&only_request(&server).await), json!({ "data": expected }));
    }
}

#[tokio::test]
async fn every_filter_reaches_attio_under_its_own_name() {
    for (name, input, query, body) in [
        // A place in the attributes is Attio's `offset`.
        (
            "attributes.list",
            json!({ "target": "lists", "identifier": "sales", "cursor": "20", "limit": 10 }),
            json!({ "limit": "10", "offset": "20" }),
            json!(null),
        ),
        (
            "meetings.list",
            json!({ "starts_before": "2026-11-01T00:00:00Z", "ends_from": "2026-10-01T00:00:00Z", "timezone": "Europe/London", "sort": "start_asc" }),
            json!({ "sort": "start_asc", "ends_from": "2026-10-01T00:00:00Z", "starts_before": "2026-11-01T00:00:00Z", "timezone": "Europe/London" }),
            json!(null),
        ),
        (
            "threads.get",
            json!({ "thread": "thr-1", "cursor": "comments-2" }),
            json!({ "cursor": "comments-2" }),
            json!(null),
        ),
        // Notes on every record of one object, and tasks assigned to nobody.
        (
            "notes.list",
            json!({ "parent_object": "people" }),
            json!({ "limit": "10", "offset": "0", "parent_object": "people" }),
            json!(null),
        ),
        (
            "tasks.list",
            json!({ "assignee": "null", "is_completed": true }),
            json!({ "limit": "50", "offset": "0", "assignee": "null", "is_completed": "true" }),
            json!(null),
        ),
        (
            "records.query",
            json!({ "object": "people", "filter_view_id": "view-1" }),
            json!({}),
            json!({ "filter_view_id": "view-1", "limit": 50, "offset": 0 }),
        ),
        // A sort through a reference: the company's name, for people.
        (
            "entries.query",
            json!({ "list": "sales", "sorts": [{ "direction": "desc", "path": [["sales", "parent_record"], ["companies", "name"]] }] }),
            json!({}),
            json!({ "sorts": [{ "direction": "desc", "path": [["sales", "parent_record"], ["companies", "name"]] }], "limit": 50, "offset": 0 }),
        ),
    ] {
        let (server, socket, key) = answering(200, json!({ "data": [], "id": {} })).await;
        let _ = invoke(&socket, &key, name, input).await;
        let request = only_request(&server).await;
        assert_eq!(query_of(&request), query, "{name}");
        assert_eq!(body_of(&request), body, "{name}");
    }
}

#[tokio::test]
async fn threads_are_asked_for_by_the_record_or_the_entry_they_are_on() {
    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    invoke(
        &socket,
        &key,
        "threads.list",
        json!({ "list": "sales", "entry_id": "ent-1", "limit": 50 }),
    )
    .await
    .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "limit": "50", "offset": "0", "list": "sales", "entry_id": "ent-1" })
    );
}

#[tokio::test]
async fn an_id_is_one_path_segment_whatever_it_contains() {
    for (name, input, expected) in [
        (
            "objects.get",
            json!({ "object": "people/../lists" }),
            "/v2/objects/people%2F..%2Flists",
        ),
        (
            "records.get",
            json!({ "object": "people", "record": "rec-1?limit=1#x" }),
            "/v2/objects/people/records/rec-1%3Flimit%3D1%23x",
        ),
        (
            "records.delete",
            json!({ "object": "a b", "record": "r/1" }),
            "/v2/objects/a%20b/records/r%2F1",
        ),
        (
            "entries.get",
            json!({ "list": "sales", "entry": "é" }),
            "/v2/lists/sales/entries/%C3%A9",
        ),
        (
            "attributes.get",
            json!({ "target": "objects", "identifier": "people", "attribute": "a/b" }),
            "/v2/objects/people/attributes/a%2Fb",
        ),
        (
            "call_recordings.get",
            json!({ "meeting": "m/1", "call_recording": "c%2F1" }),
            "/v2/meetings/m%2F1/call_recordings/c%252F1",
        ),
        // The space around an id is not part of it.
        ("notes.get", json!({ "note": " note-1 " }), "/v2/notes/note-1"),
    ] {
        let (server, socket, key) = answering(200, json!({ "data": { "id": { "x": "1" } } })).await;
        let _ = invoke(&socket, &key, name, input).await;
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), expected, "{name}");
        assert_eq!(request.url.query(), None, "{name}");
    }

    for (name, input) in [
        ("objects.get", json!({ "object": ".." })),
        ("records.get", json!({ "object": "people", "record": "." })),
        ("records.get", json!({ "object": "", "record": "rec-1" })),
        ("entries.delete", json!({ "list": "sales", "entry": "  " })),
        ("tasks.delete", json!({ "task": "" })),
    ] {
        let (server, socket, key) = answering(200, json!({})).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}: {err}");
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }
}

// ── Paging ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_full_page_gives_the_place_of_the_next_and_a_short_one_ends_the_list() {
    // Two were asked for and two came back, so there may be more, from the third on.
    let (server, socket, key) = answering(200, json!({ "data": [record(), record()] })).await;
    let page = invoke(
        &socket,
        &key,
        "records.query",
        json!({ "object": "people", "limit": 2, "cursor": "4" }),
    )
    .await
    .unwrap();
    assert_eq!(page["next_cursor"], "6");
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "limit": 2, "offset": 4 })
    );

    // One came back where two were asked for: Attio's rule is that the list ends there.
    let (_server, socket, key) = answering(200, json!({ "data": [record()] })).await;
    let page = invoke(
        &socket,
        &key,
        "records.query",
        json!({ "object": "people", "limit": 2 }),
    )
    .await
    .unwrap();
    assert_eq!(page["next_cursor"], json!(null));

    // The same for a list asked for with a GET, with the page size Attio uses when none is named.
    let ten: Vec<Value> = (0..10).map(|_| note()).collect();
    let (server, socket, key) = answering(200, json!({ "data": ten })).await;
    let page = invoke(&socket, &key, "notes.list", json!({ "cursor": "10" }))
        .await
        .unwrap();
    assert_eq!(page["next_cursor"], "20");
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "limit": "10", "offset": "10" })
    );

    // A cursor is the caller's to write. One at the very end of what can be
    // counted gives no next page, and is not an error.
    let (_server, socket, key) = answering(200, json!({ "data": [record()] })).await;
    let input = json!({ "object": "people", "limit": 1, "cursor": u64::MAX.to_string() });
    let page = invoke(&socket, &key, "records.query", input).await.unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 1);
    assert_eq!(page["next_cursor"], json!(null));

    // Every attribute comes back when no limit is named, so there is no next page to offer.
    let (_server, socket, key) = answering(200, json!({ "data": [attribute(), attribute()] })).await;
    let page = invoke(
        &socket,
        &key,
        "attributes.list",
        json!({ "target": "objects", "identifier": "people" }),
    )
    .await
    .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 2);
    assert_eq!(page["next_cursor"], json!(null));
}

#[tokio::test]
async fn a_cursor_attio_gave_is_passed_back_as_it_is() {
    let (server, socket, key) =
        answering(200, json!({ "data": [meeting()], "pagination": { "next_cursor": "" } })).await;
    let page = invoke(
        &socket,
        &key,
        "meetings.list",
        json!({ "cursor": "eyJhIjoxfQ==.sig/+" }),
    )
    .await
    .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "cursor": "eyJhIjoxfQ==.sig/+" })
    );
    assert_eq!(page["next_cursor"], json!(null), "an empty cursor is no cursor");

    let (server, socket, key) = answering(200, json!({ "data": [], "pagination": { "next_cursor": null } })).await;
    invoke(
        &socket,
        &key,
        "call_recordings.list",
        json!({ "meeting": "meet-1", "cursor": "next-1", "limit": 200 }),
    )
    .await
    .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "limit": "200", "cursor": "next-1" })
    );
}

#[tokio::test]
async fn a_cursor_that_is_not_a_place_in_the_list_and_a_limit_out_of_range_are_refused() {
    for (name, input, says) in [
        (
            "records.query",
            json!({ "object": "people", "cursor": "https://evil.example/" }),
            "`cursor`",
        ),
        (
            "records.query",
            json!({ "object": "people", "cursor": "-1" }),
            "`cursor`",
        ),
        ("notes.list", json!({ "cursor": "ten" }), "`cursor`"),
        (
            "records.query",
            json!({ "object": "people", "limit": 0 }),
            "`limit` is from 1 to 500",
        ),
        (
            "records.query",
            json!({ "object": "people", "limit": 501 }),
            "`limit` is from 1 to 500",
        ),
        (
            "entries.query",
            json!({ "list": "sales", "limit": 501 }),
            "`limit` is from 1 to 500",
        ),
        (
            "records.entries",
            json!({ "object": "people", "record": "rec-1", "limit": 1001 }),
            "`limit` is from 1 to 1000",
        ),
        ("notes.list", json!({ "limit": 51 }), "`limit` is from 1 to 50"),
        (
            "threads.list",
            json!({ "object": "people", "record_id": "rec-1", "limit": 51 }),
            "`limit` is from 1 to 50",
        ),
        (
            "threads.get",
            json!({ "thread": "thr-1", "limit": 251 }),
            "`limit` is from 1 to 250",
        ),
        ("tasks.list", json!({ "limit": 0 }), "`limit` is from 1 to 500"),
        ("meetings.list", json!({ "limit": 201 }), "`limit` is from 1 to 200"),
        (
            "call_recordings.list",
            json!({ "meeting": "meet-1", "limit": 0 }),
            "`limit` is from 1 to 200",
        ),
        (
            "attributes.list",
            json!({ "target": "objects", "identifier": "people", "limit": 0 }),
            "`limit` is at least 1",
        ),
    ] {
        let (server, socket, key) = answering(200, json!({ "data": [] })).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}: {err}");
        assert!(err.message().contains(says), "{name}: {}", err.message());
        assert!(!err.message().contains("evil.example"), "{}", err.message());
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }

    // A blank cursor is the first page.
    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    invoke(&socket, &key, "notes.list", json!({ "cursor": " " }))
        .await
        .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "limit": "10", "offset": "0" })
    );
}

// ── What is refused before Attio is called ───────────────────────────────────

#[tokio::test]
async fn input_of_the_wrong_shape_is_refused_by_name_without_calling_attio() {
    for (name, input, says) in [
        ("records.get", json!({ "object": "people" }), "missing field `record`"),
        (
            "records.create",
            json!({ "object": "people" }),
            "missing field `values`",
        ),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record_id": "rec-1", "title": "Call" }),
            "missing field `content`",
        ),
        (
            "threads.comment",
            json!({ "content": "x", "thread_id": "thr-1" }),
            "missing field `author`",
        ),
        (
            "attributes.list",
            json!({ "identifier": "people" }),
            "missing field `target`",
        ),
        (
            "records.query",
            json!({ "object": "people", "limit": "ten" }),
            "wrong type",
        ),
        (
            "records.create",
            json!({ "object": "people", "values": ["name"] }),
            "wrong type",
        ),
        (
            "attributes.list",
            json!({ "target": "records", "identifier": "people" }),
            "wrong type",
        ),
        ("tasks.list", json!({ "sort": "newest" }), "wrong type"),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "attribute": "name" }] }),
            "missing field `direction`",
        ),
    ] {
        let (server, socket, key) = answering(200, json!({})).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}: {err}");
        assert!(err.message().contains(says), "{name}: {}", err.message());
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }
}

#[tokio::test]
async fn a_field_the_operation_does_not_know_is_refused_and_named() {
    for (name, input, says) in [
        ("objects.list", json!({ "limit": 5 }), "`limit` is not a field"),
        (
            "records.get",
            json!({ "object": "people", "record": "rec-1", "fields": ["name"] }),
            "`fields` is not a field",
        ),
        (
            "records.query",
            json!({ "object": "people", "filters": {} }),
            "`filters` is not a field",
        ),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc", "attribute": "name", "order": 1 }] }),
            "`sorts[0].order` is not a field",
        ),
        (
            "threads.comment",
            json!({ "content": "x", "author": "mem-1", "record": { "object": "people", "record_id": "rec-1", "id": "x" } }),
            "`record.id` is not a field",
        ),
        (
            "tasks.update",
            json!({ "task": "task-1", "content": "new text" }),
            "`content` is not a field",
        ),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record_id": "rec-1", "title": "t", "content": "c", "body": "c" }),
            "`body` is not a field",
        ),
    ] {
        let (server, socket, key) = answering(200, json!({})).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}: {err}");
        assert!(err.message().contains(says), "{name}: {}", err.message());
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }

    // An attribute's slug is the workspace's to choose, so anything is a field of `values`.
    let (server, socket, key) = answering(200, json!({ "data": record() })).await;
    let values = json!({ "favourite colour!": "green", "nested": { "anything": [1, 2] } });
    invoke(
        &socket,
        &key,
        "records.create",
        json!({ "object": "people", "values": values }),
    )
    .await
    .unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": { "values": values } })
    );
}

#[tokio::test]
async fn values_that_cannot_work_are_refused_before_attio_is_called() {
    let long = "x".repeat(2001);
    for (name, input, says) in [
        (
            "records.create",
            json!({ "object": "people", "values": {} }),
            "`values` needs at least one attribute",
        ),
        (
            "records.update",
            json!({ "object": "people", "record": "rec-1", "values": {} }),
            "`values` needs at least one attribute",
        ),
        (
            "records.assert",
            json!({ "object": "people", "matching_attribute": "email_addresses", "values": {} }),
            "`values` needs at least one attribute",
        ),
        (
            "records.assert",
            json!({ "object": "people", "matching_attribute": " ", "values": { "name": "Ada" } }),
            "a matching attribute is required",
        ),
        (
            "entries.update",
            json!({ "list": "sales", "entry": "ent-1", "entry_values": {} }),
            "`entry_values` needs at least one attribute",
        ),
        (
            "entries.create",
            json!({ "list": "sales", "parent_object": "", "parent_record_id": "rec-2" }),
            "`parent_object` is required",
        ),
        (
            "entries.create",
            json!({ "list": "sales", "parent_object": "companies", "parent_record_id": " " }),
            "`parent_record_id` is required",
        ),
        (
            "records.query",
            json!({ "object": "people", "filter": {}, "filter_view_id": "view-1" }),
            "cannot be used together",
        ),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc" }] }),
            "every sort needs `attribute` or `path`",
        ),
        (
            "entries.query",
            json!({ "list": "sales", "sorts": [{ "direction": "asc", "attribute": "stage", "path": [["sales", "stage"]] }] }),
            "every sort needs `attribute` or `path`",
        ),
        // To Attio a blank assignee means "assigned to nobody", so it is neither dropped nor sent.
        (
            "tasks.list",
            json!({ "assignee": "" }),
            "the word `null` for tasks assigned to nobody",
        ),
        (
            "tasks.list",
            json!({ "assignee": "  " }),
            "the word `null` for tasks assigned to nobody",
        ),
        (
            "notes.create",
            json!({ "parent_object": " ", "parent_record_id": "rec-1", "title": "t", "content": "c" }),
            "`parent_object` is required",
        ),
        (
            "tasks.list",
            json!({ "linked_record_id": "rec-1" }),
            "`linked_object` and `linked_record_id` are given together",
        ),
        ("tasks.create", json!({ "content": "  " }), "a task needs `content`"),
        (
            "tasks.create",
            json!({ "content": long }),
            "`content` is at most 2000 characters",
        ),
        (
            "tasks.update",
            json!({ "task": "task-1" }),
            "an update needs at least one field to change",
        ),
        ("threads.list", json!({}), "name a record"),
        (
            "threads.list",
            json!({ "object": "people" }),
            "`object` and `record_id` are given together",
        ),
        (
            "threads.list",
            json!({ "object": "people", "record_id": "rec-1", "list": "sales", "entry_id": "ent-1" }),
            "name a record",
        ),
        (
            "threads.comment",
            json!({ "content": " ", "author": "mem-1", "thread_id": "thr-1" }),
            "a comment needs `content`",
        ),
        (
            "threads.comment",
            json!({ "content": "x", "author": " ", "thread_id": "thr-1" }),
            "`author` is required",
        ),
        (
            "threads.comment",
            json!({ "content": "x", "author": "mem-1" }),
            "exactly one of `thread_id`, `record` and `entry`",
        ),
        (
            "threads.comment",
            json!({ "content": "x", "author": "mem-1", "thread_id": "thr-1", "record": { "object": "people", "record_id": "rec-1" } }),
            "exactly one of",
        ),
        (
            "threads.comment",
            json!({ "content": "x", "author": "mem-1", "record": { "object": "people", "record_id": "" } }),
            "`record.record_id` is required",
        ),
        (
            "meetings.list",
            json!({ "linked_object": "people" }),
            "`linked_object` and `linked_record_id` are given together",
        ),
    ] {
        let (server, socket, key) = answering(200, json!({})).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}: {err}");
        assert!(err.message().contains(says), "{name}: {}", err.message());
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }

    // The limit is on characters, and a task of exactly that many goes out.
    let (server, socket, key) = answering(200, json!({ "data": task() })).await;
    invoke(&socket, &key, "tasks.create", json!({ "content": "é".repeat(2000) }))
        .await
        .unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

// ── What comes back ──────────────────────────────────────────────────────────

#[tokio::test]
async fn what_attio_leaves_empty_does_not_stop_a_thing_from_being_read() {
    let bare = |id: Value| json!({ "data": { "id": id } });
    for (name, input, response) in [
        (
            "records.get",
            json!({ "object": "people", "record": "rec-1" }),
            json!({ "data": { "id": { "record_id": "rec-1" }, "values": null, "web_url": null } }),
        ),
        (
            "entries.get",
            json!({ "list": "sales", "entry": "ent-1" }),
            bare(json!({ "entry_id": "ent-1" })),
        ),
        (
            "notes.get",
            json!({ "note": "note-1" }),
            json!({ "data": { "id": { "note_id": "note-1" }, "title": null, "tags": null, "content_markdown": null } }),
        ),
        (
            "tasks.get",
            json!({ "task": "task-1" }),
            json!({ "data": { "id": { "task_id": "task-1" }, "linked_records": null, "assignees": null, "is_completed": null } }),
        ),
        (
            "meetings.get",
            json!({ "meeting": "meet-1" }),
            json!({ "data": { "id": { "meeting_id": "meet-1" }, "start": { "date": "2026-10-12" }, "participants": [{ "email_address": null, "name": null }], "description": null } }),
        ),
        (
            "call_recordings.get",
            json!({ "meeting": "meet-1", "call_recording": "cr-1" }),
            json!({ "data": { "id": { "call_recording_id": "cr-1" }, "status": "processing", "transcript": null, "video_url": null } }),
        ),
        (
            "attributes.get",
            json!({ "target": "objects", "identifier": "people", "attribute": "name" }),
            bare(json!({ "attribute_id": "attr-1" })),
        ),
        (
            "lists.get",
            json!({ "list": "sales" }),
            json!({ "data": { "id": { "list_id": "list-1" }, "workspace_access": null, "workspace_member_access": null } }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        let output = invoke(&socket, &key, name, input)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(output["id"].is_object(), "{name}: {output}");
    }

    let (_server, socket, key) = answering(
        200,
        json!({ "data": { "id": { "record_id": "rec-1" }, "values": null } }),
    )
    .await;
    let record = invoke(
        &socket,
        &key,
        "records.get",
        json!({ "object": "people", "record": "rec-1" }),
    )
    .await
    .unwrap();
    assert_eq!(record["values"], json!({}));
    assert_eq!(record["current"], json!({}));
}

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error_not_an_empty_result() {
    for (name, input) in [
        ("objects.get", json!({ "object": "people" })),
        ("records.get", json!({ "object": "people", "record": "rec-1" })),
        (
            "records.create",
            json!({ "object": "people", "values": { "name": "Ada" } }),
        ),
        ("entries.get", json!({ "list": "sales", "entry": "ent-1" })),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record_id": "rec-1", "title": "t", "content": "c" }),
        ),
        ("tasks.get", json!({ "task": "task-1" })),
        ("threads.get", json!({ "thread": "thr-1" })),
        ("workspace_members.get", json!({ "member": "mem-1" })),
        (
            "call_recordings.get",
            json!({ "meeting": "meet-1", "call_recording": "cr-1" }),
        ),
        // A list that is not there is not an empty list.
        ("objects.list", json!({})),
        ("records.query", json!({ "object": "people" })),
        ("meetings.list", json!({})),
    ] {
        for response in [
            json!({}),
            json!({ "data": null }),
            json!({ "data": {} }),
            json!({ "data": { "id": {} } }),
            json!({ "status_code": 200, "message": "ok" }),
        ] {
            let (_server, socket, key) = answering(200, response.clone()).await;
            let err = invoke(&socket, &key, name, input.clone())
                .await
                .expect_err(&format!("{name} accepted {response}"));
            assert_eq!(err.kind(), ErrorKind::Decode, "{name} with {response}: {err}");
        }
    }

    // A delete answers with nothing, and that is its success.
    let (_server, socket, key) = answering(200, json!({})).await;
    let done = invoke(&socket, &key, "notes.delete", json!({ "note": "note-1" })).await;
    assert_eq!(done.unwrap(), json!(null));
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_field_and_never_repeats_what_attio_sent() {
    let mut odd = record();
    odd["values"]["email_addresses"] = json!("ada-secret@example.test");
    let (_server, socket, key) = answering(200, json!({ "data": odd })).await;
    let err = invoke(
        &socket,
        &key,
        "records.get",
        json!({ "object": "people", "record": "rec-1" }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(err.message().contains("values.email_addresses"), "{}", err.message());
    assert!(!err.message().contains("ada-secret"), "{}", err.message());
    assert!(!format!("{err:?}").contains("ada-secret"), "{err:?}");

    let mut odd = task();
    odd["is_completed"] = json!("confidential-yes");
    let (_server, socket, key) = answering(200, json!({ "data": [odd] })).await;
    let err = invoke(&socket, &key, "tasks.list", json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(err.message().contains("is_completed"), "{}", err.message());
    assert!(!format!("{err:?}").contains("confidential"), "{err:?}");
}

// ── Attio's refusals ─────────────────────────────────────────────────────────

#[tokio::test]
async fn attios_refusals_reach_the_caller_as_errors_they_can_act_on() {
    let get = json!({ "object": "people", "record": "rec-1" });
    for (response, kind, says) in [
        (
            attio_error(401, "auth_error", "invalid_token", "Invalid access token"),
            ErrorKind::ReconnectRequired,
            "rejected the stored authorization",
        ),
        // A token without the scope: Attio's own words are the only place the scope is named.
        (
            attio_error(
                403,
                "auth_error",
                "unauthorized",
                "The token is missing the record_permission:read scope",
            ),
            ErrorKind::AccessDenied,
            "record_permission:read",
        ),
        (
            attio_error(404, "invalid_request_error", "not_found", "Record not found"),
            ErrorKind::NotFound,
            "no such resource",
        ),
        (
            attio_error(
                400,
                "invalid_request_error",
                "filter_error",
                "Unknown attribute slug: nmae",
            ),
            ErrorKind::InvalidInput,
            "Unknown attribute slug: nmae",
        ),
        // A conflict that is the caller's to fix, unlike the one in the test below.
        (
            attio_error(409, "invalid_request_error", "slug_conflict", "That slug is taken"),
            ErrorKind::InvalidInput,
            "That slug is taken",
        ),
    ] {
        let (server, socket, key) = attio().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let err = invoke(&socket, &key, "records.get", get.clone()).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().contains(says), "{}", err.message());
        assert_eq!(err.provider().map(|p| p.as_str()), Some("attio"));
        assert_eq!(err.retry(), Retry::Never, "{err}");
    }
}

#[tokio::test]
async fn a_write_that_met_another_write_is_to_be_tried_again_and_is_not_the_callers_mistake() {
    let conflict = || {
        attio_error(
            409,
            "invalid_request_error",
            "concurrent_write_conflict",
            "The record was modified by another request while this write was being validated. Please try again.",
        )
    };
    // A create is not sent again by Socket: the caller is told it can be.
    for (name, input) in [
        (
            "records.create",
            json!({ "object": "people", "values": { "name": "Ada" } }),
        ),
        (
            "records.update",
            json!({ "object": "people", "record": "rec-1", "values": { "name": "Ada" } }),
        ),
    ] {
        let (server, socket, key) = attio().await;
        Mock::given(any()).respond_with(conflict()).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}: {err}");
        assert_eq!(err.retry(), Retry::Later, "{name}");
        assert!(err.message().contains("try again"), "{}", err.message());
        assert_eq!(server.received_requests().await.unwrap().len(), 1, "{name}");
    }

    // An assert finds its record by a matching value, so Socket sends it again itself.
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(conflict())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(answer(200, &json!({ "data": record() })))
        .mount(&server)
        .await;
    let input = json!({ "object": "people", "matching_attribute": "email_addresses", "values": { "email_addresses": ["ada@example.test"] } });
    let asserted = invoke(&socket, &key, "records.assert", input).await.unwrap();
    assert_eq!(asserted["id"]["record_id"], "rec-1");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn content_that_is_too_large_is_the_callers_to_shorten() {
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(attio_error(
            413,
            "invalid_request_error",
            "validation_type",
            "Note content is too large",
        ))
        .mount(&server)
        .await;
    let input = json!({ "parent_object": "people", "parent_record_id": "rec-1", "title": "Call", "content": "c" });
    let err = invoke(&socket, &key, "notes.create", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput, "{err}");
    assert_eq!(err.retry(), Retry::Never);
    assert!(err.message().contains("too large"), "{}", err.message());
}

#[tokio::test]
async fn throttling_is_reported_with_the_time_attio_says_to_wait_until() {
    // Attio states when the limit resets as a date, usually the next second.
    let (server, socket, key) = attio().await;
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(3600);
    let reset = httpdate_of(later);
    Mock::given(any())
        .respond_with(
            attio_error(
                429,
                "rate_limit_error",
                "rate_limit_exceeded",
                "Rate limit exceeded, please try again later",
            )
            .insert_header("retry-after", reset.as_str()),
        )
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "records.query", json!({ "object": "people" }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::RateLimited);
    let Retry::After(wait) = err.retry() else {
        panic!("attio said when to come back: {:?}", err.retry())
    };
    assert!((3590..=3600).contains(&wait.as_secs()), "{wait:?}");

    // A limit that resets within the second is waited out, for a write as well:
    // Attio did not carry out what it throttled.
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(
            attio_error(429, "rate_limit_error", "rate_limit_exceeded", "Rate limit exceeded")
                .insert_header("retry-after", "0"),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(answer(200, &json!({ "data": note() })))
        .mount(&server)
        .await;
    let input = json!({ "parent_object": "people", "parent_record_id": "rec-1", "title": "Call", "content": "c" });
    let created = invoke(&socket, &key, "notes.create", input).await.unwrap();
    assert_eq!(created["id"]["note_id"], "note-1");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

/// `time` as an HTTP date, which is how Attio writes `Retry-After`.
fn httpdate_of(time: std::time::SystemTime) -> String {
    const DAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let secs = time.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs();
    let days = secs / 86_400;
    // Civil date from days since 1970-01-01.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + u64::from(month <= 2);
    let (h, m, s) = ((secs % 86_400) / 3600, (secs % 3600) / 60, secs % 60);
    format!(
        "{}, {day:02} {} {year} {h:02}:{m:02}:{s:02} GMT",
        DAYS[usize::try_from(days % 7).unwrap()],
        MONTHS[usize::try_from(month - 1).unwrap()]
    )
}

#[tokio::test]
async fn a_write_is_sent_once_when_attio_fails_and_a_read_is_tried_again() {
    let failing = || attio_error(503, "api_error", "service_unavailable", "Try again");
    for (name, input) in [
        (
            "records.create",
            json!({ "object": "people", "values": { "name": "Ada" } }),
        ),
        (
            "records.update",
            json!({ "object": "people", "record": "rec-1", "values": { "name": "Ada" } }),
        ),
        (
            "entries.create",
            json!({ "list": "sales", "parent_object": "companies", "parent_record_id": "rec-2" }),
        ),
        (
            "entries.update",
            json!({ "list": "sales", "entry": "ent-1", "entry_values": { "stage": "Won" } }),
        ),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record_id": "rec-1", "title": "t", "content": "c" }),
        ),
        ("tasks.create", json!({ "content": "Send the contract" })),
        ("tasks.update", json!({ "task": "task-1", "is_completed": true })),
        (
            "threads.comment",
            json!({ "content": "x", "author": "mem-1", "thread_id": "thr-1" }),
        ),
        // Reads by effect, but Attio takes them as POST, so they are not repeated either.
        ("records.query", json!({ "object": "people" })),
        ("entries.query", json!({ "list": "sales" })),
        // `records.assert` and the deletes are not in this list. The transport
        // repeats a PUT and a DELETE after a server error; each ends in the
        // same state when it runs twice. See the guide.
    ] {
        let (server, socket, key) = attio().await;
        Mock::given(any()).respond_with(failing()).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have happened, so it is not sent again"
        );
    }

    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(failing())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(answer(200, &json!({ "data": record() })))
        .mount(&server)
        .await;
    let read = invoke(
        &socket,
        &key,
        "records.get",
        json!({ "object": "people", "record": "rec-1" }),
    )
    .await
    .unwrap();
    assert_eq!(read["id"]["record_id"], "rec-1");
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test]
async fn a_delete_and_an_assert_are_sent_again_after_a_server_error() {
    let failing = || attio_error(503, "api_error", "service_unavailable", "Try again");
    for (name, input, response) in [
        ("notes.delete", json!({ "note": "note-1" }), json!({})),
        (
            "records.delete",
            json!({ "object": "people", "record": "rec-1" }),
            json!({}),
        ),
        (
            "records.assert",
            json!({ "object": "people", "matching_attribute": "email_addresses", "values": { "email_addresses": ["ada@example.test"] } }),
            json!({ "data": record() }),
        ),
    ] {
        let (server, socket, key) = attio().await;
        Mock::given(any())
            .respond_with(failing())
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

    // What the guide warns of: a delete that went through before the error
    // finds nothing the second time, and that is what the caller is told.
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(failing())
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(any())
        .respond_with(attio_error(404, "invalid_request_error", "not_found", "Note not found"))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "notes.delete", json!({ "note": "note-1" }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
}

#[tokio::test]
async fn an_answer_that_is_not_json_is_an_error_and_not_a_result() {
    let (server, socket, key) = attio().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("<html>maintenance</html>"))
        .mount(&server)
        .await;
    let outcome = invoke(&socket, &key, "objects.list", json!({})).await;
    assert!(outcome.is_err(), "{outcome:?}");
}
