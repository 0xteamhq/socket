//! Every Attio operation, called by name against a local server that answers as Attio does.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_attio::models::{
    AssignTo, CallRecording, CreateComment, CreateNote, CreateTask, Direction, ListAttributes, ListNotes, NoteFormat,
    OnRecord, QueryRecords, Record, Sort, Target, Transcript, WriteEntry, WriteRecord,
};
use socketkit_attio::{Attio, provider};
use socketkit_core::{Effect, ErrorKind, Integration};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    ATTRIBUTE, COMMENT, COMPANY, Case, ENTRY, LIST, MEETING, MEMBER, NOTE, OBJECT, RECORD, RECORDING, TASK, THREAD,
    WORKSPACE, answer, answering, attio, attio_error, attribute, body_of, comment, contains, entry, entry_current,
    invoke, list, meeting, member, note, object, only_request, query_of, record, record_current, recording,
    recording_row, task, thread, token,
};

/// What every operation sends and returns.
#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, path: &str, query, body, response, returns| Case { name, input, verb, path: path.to_owned(), query, body, response, returns };
    let person = format!("/objects/people/records/{RECORD}");
    let deal = format!("/lists/enterprise_sales/entries/{ENTRY}");
    let option = json!({ "id": { "workspace_id": WORKSPACE, "object_id": OBJECT, "attribute_id": ATTRIBUTE, "option_id": "08c2c59a-c18e-40c6-8dc4-95415313b2ea" }, "title": "Medium", "is_archived": false });
    let status = json!({ "id": { "workspace_id": WORKSPACE, "object_id": LIST, "attribute_id": ATTRIBUTE, "status_id": "11f07f01-c10f-4e05-a522-33e050bc52ee" }, "title": "In Progress", "is_archived": false, "celebration_enabled": false, "target_time_in_status": "P0Y0M1DT0H0M0S" });
    let on_person = json!({ "target_object": "people", "target_record_id": RECORD });
    vec![
        // meta
        case("meta.identify", json!({}), "GET", "/self", json!({}), json!(null), token(),
            json!({ "active": true, "workspace_id": WORKSPACE, "workspace_name": "Acme", "workspace_slug": "acme", "token_level": "workspace", "authorized_by_workspace_member_id": MEMBER,
                    "scope": "record_permission:read object_configuration:read note:read-write", "scopes": ["record_permission:read", "object_configuration:read", "note:read-write"] })),

        // objects
        case("objects.list", json!({}), "GET", "/objects", json!({}), json!(null), json!({ "data": [object()] }),
            json!([{ "id": { "workspace_id": WORKSPACE, "object_id": OBJECT }, "api_slug": "people", "singular_noun": "Person", "plural_noun": "People" }])),
        case("objects.get", json!({ "object": "people" }), "GET", "/objects/people", json!({}), json!(null), json!({ "data": object() }),
            json!({ "id": { "object_id": OBJECT }, "api_slug": "people", "created_at": "2022-11-21T13:22:49.061281000Z" })),

        // attributes: of an object, and of a list
        case("attributes.list", json!({ "target": "objects", "identifier": "people", "show_archived": true, "limit": 1 }), "GET", "/objects/people/attributes",
            json!({ "show_archived": "true", "limit": "1" }), json!(null), json!({ "data": [attribute()] }),
            json!({ "items": [{ "id": { "attribute_id": ATTRIBUTE }, "title": "Company", "api_slug": "company", "type": "record-reference", "is_unique": false, "is_multiselect": false, "is_writable": true,
                                "relationship": { "object_slug": "companies", "is_multiselect": true }, "config": { "record_reference": { "allowed_object_ids": [COMPANY] } } }], "next_cursor": "1" })),
        case("attributes.get", json!({ "target": "lists", "identifier": "enterprise_sales", "attribute": "stage" }), "GET", "/lists/enterprise_sales/attributes/stage", json!({}), json!(null),
            json!({ "data": attribute() }), json!({ "id": { "attribute_id": ATTRIBUTE }, "api_slug": "company", "description": "Where the person works" })),
        case("attributes.options", json!({ "target": "objects", "identifier": "companies", "attribute": "categories" }), "GET", "/objects/companies/attributes/categories/options", json!({}), json!(null),
            json!({ "data": [option.clone()] }), json!([{ "id": { "option_id": "08c2c59a-c18e-40c6-8dc4-95415313b2ea" }, "title": "Medium", "is_archived": false }])),
        case("attributes.statuses", json!({ "target": "lists", "identifier": LIST, "attribute": "stage", "show_archived": false }), "GET", &format!("/lists/{LIST}/attributes/stage/statuses"),
            json!({ "show_archived": "false" }), json!(null), json!({ "data": [status.clone()] }),
            json!([{ "id": { "status_id": "11f07f01-c10f-4e05-a522-33e050bc52ee" }, "title": "In Progress", "celebration_enabled": false, "target_time_in_status": "P0Y0M1DT0H0M0S" }])),

        // records: reading. The query is a POST that changes nothing.
        case("records.query",
            json!({ "object": "people", "filter": { "name": "Ada Lovelace", "email_addresses": { "email_domain": { "$eq": "example.com" } } },
                    "sorts": [{ "direction": "asc", "attribute": "name", "field": "last_name" }, { "direction": "desc", "path": [["people", "company"], ["companies", "name"]] }], "limit": 1, "cursor": "2" }),
            "POST", "/objects/people/records/query", json!({}),
            json!({ "filter": { "name": "Ada Lovelace", "email_addresses": { "email_domain": { "$eq": "example.com" } } },
                    "sorts": [{ "direction": "asc", "attribute": "name", "field": "last_name" }, { "direction": "desc", "path": [["people", "company"], ["companies", "name"]] }], "limit": 1, "offset": 2 }),
            json!({ "data": [record()] }),
            json!({ "items": [{ "id": { "workspace_id": WORKSPACE, "object_id": OBJECT, "record_id": RECORD }, "web_url": format!("https://app.attio.com/acme/person/{RECORD}"), "current": record_current() }], "next_cursor": "3" })),
        case("records.get", json!({ "object": "people", "record": RECORD }), "GET", &person, json!({}), json!(null), json!({ "data": record() }),
            json!({ "id": { "record_id": RECORD }, "created_at": "2022-11-21T13:22:49.061281000Z", "values": record()["values"].clone(), "current": record_current() })),
        case("records.entries", json!({ "object": "people", "record": RECORD }), "GET", &format!("{person}/entries"), json!({ "limit": "50" }), json!(null),
            json!({ "data": [{ "list_id": LIST, "list_api_slug": "enterprise_sales", "entry_id": ENTRY, "created_at": "2022-11-21T13:22:49.061281000Z" }] }),
            json!({ "items": [{ "list_id": LIST, "list_api_slug": "enterprise_sales", "entry_id": ENTRY }], "next_cursor": null })),

        // records: writing. The values go as given, a null among them included.
        case("records.create", json!({ "object": "people", "values": { "name": "Ada Lovelace", "email_addresses": ["ada@example.com", "countess@example.org"] } }), "POST", "/objects/people/records", json!({}),
            json!({ "data": { "values": { "name": "Ada Lovelace", "email_addresses": ["ada@example.com", "countess@example.org"] } } }),
            json!({ "data": record() }), json!({ "id": { "record_id": RECORD }, "current": record_current() })),
        case("records.update", json!({ "object": "people", "record": RECORD, "values": { "job_title": "Engineer", "company": [{ "target_object": "companies", "target_record_id": COMPANY }] } }), "PATCH", &person, json!({}),
            json!({ "data": { "values": { "job_title": "Engineer", "company": [{ "target_object": "companies", "target_record_id": COMPANY }] } } }),
            json!({ "data": record() }), json!({ "id": { "record_id": RECORD } })),
        case("records.assert", json!({ "object": "people", "matching_attribute": "email_addresses", "values": { "email_addresses": ["ada@example.com"], "job_title": null } }), "PUT", "/objects/people/records",
            json!({ "matching_attribute": "email_addresses" }), json!({ "data": { "values": { "email_addresses": ["ada@example.com"], "job_title": null } } }),
            json!({ "data": record() }), json!({ "id": { "record_id": RECORD }, "current": { "name": "Ada Lovelace" } })),
        case("records.delete", json!({ "object": "people", "record": RECORD }), "DELETE", &person, json!({}), json!(null), json!({}), json!(null)),

        // lists
        case("lists.list", json!({}), "GET", "/lists", json!({}), json!(null), json!({ "data": [list()] }),
            json!([{ "id": { "list_id": LIST }, "api_slug": "enterprise_sales", "name": "Enterprise sales", "parent_object": ["people"], "workspace_access": "read-and-write" }])),
        case("lists.get", json!({ "list": "enterprise_sales" }), "GET", "/lists/enterprise_sales", json!({}), json!(null), json!({ "data": list() }),
            json!({ "id": { "list_id": LIST }, "workspace_member_access": [{ "workspace_member_id": MEMBER, "level": "full-access" }], "created_by_actor": { "type": "workspace-member", "id": MEMBER } })),

        // entries. The query is a POST that changes nothing.
        case("entries.query", json!({ "list": "enterprise_sales", "filter": { "stage": "Won" } }), "POST", "/lists/enterprise_sales/entries/query", json!({}),
            json!({ "filter": { "stage": "Won" }, "limit": 25 }), json!({ "data": [entry()] }),
            json!({ "items": [{ "id": { "workspace_id": WORKSPACE, "list_id": LIST, "entry_id": ENTRY }, "parent_record_id": RECORD, "parent_object": "people", "current": entry_current() }], "next_cursor": null })),
        case("entries.get", json!({ "list": "enterprise_sales", "entry": ENTRY }), "GET", &deal, json!({}), json!(null), json!({ "data": entry() }),
            json!({ "id": { "entry_id": ENTRY }, "parent_record_id": RECORD, "entry_values": entry()["entry_values"].clone(), "current": entry_current() })),
        case("entries.create", json!({ "list": "enterprise_sales", "parent_object": "people", "parent_record": RECORD, "entry_values": { "stage": "Lead" } }), "POST", "/lists/enterprise_sales/entries", json!({}),
            json!({ "data": { "parent_object": "people", "parent_record_id": RECORD, "entry_values": { "stage": "Lead" } } }),
            json!({ "data": entry() }), json!({ "id": { "entry_id": ENTRY }, "current": entry_current() })),
        case("entries.update", json!({ "list": "enterprise_sales", "entry": ENTRY, "entry_values": { "stage": "Won" } }), "PATCH", &deal, json!({}),
            json!({ "data": { "entry_values": { "stage": "Won" } } }), json!({ "data": entry() }), json!({ "id": { "entry_id": ENTRY }, "current": { "stage": "Won" } })),
        case("entries.delete", json!({ "list": "enterprise_sales", "entry": ENTRY }), "DELETE", &deal, json!({}), json!(null), json!({}), json!(null)),

        // notes
        case("notes.list", json!({ "parent_object": "people", "parent_record_id": RECORD, "limit": 10 }), "GET", "/notes",
            json!({ "parent_object": "people", "parent_record_id": RECORD, "limit": "10" }), json!(null), json!({ "data": [note()] }),
            json!({ "items": [{ "id": { "note_id": NOTE }, "parent_object": "people", "parent_record_id": RECORD, "title": "Initial call", "meeting_id": null }], "next_cursor": null })),
        case("notes.get", json!({ "note": NOTE }), "GET", &format!("/notes/{NOTE}"), json!({}), json!(null), json!({ "data": note() }),
            json!({ "id": { "note_id": NOTE }, "title": "Initial call", "content_plaintext": "Introduction\nBudget agreed", "content_markdown": "# Introduction\nBudget agreed",
                    "tags": [{ "type": "workspace-member", "workspace_member_id": MEMBER }, { "type": "record", "object": "people", "record_id": RECORD }] })),
        case("notes.create", json!({ "parent_object": "people", "parent_record": RECORD, "title": "Initial call", "format": "markdown", "content": "# Introduction\nBudget agreed", "meeting_id": MEETING }), "POST", "/notes", json!({}),
            json!({ "data": { "parent_object": "people", "parent_record_id": RECORD, "title": "Initial call", "format": "markdown", "content": "# Introduction\nBudget agreed", "meeting_id": MEETING } }),
            json!({ "data": note() }), json!({ "id": { "note_id": NOTE }, "content_plaintext": "Introduction\nBudget agreed" })),
        case("notes.delete", json!({ "note": NOTE }), "DELETE", &format!("/notes/{NOTE}"), json!({}), json!(null), json!({}), json!(null)),

        // tasks
        case("tasks.list", json!({ "linked_object": "people", "linked_record_id": RECORD, "assignee": "susan@example.com", "is_completed": false, "sort": "created_at:desc", "limit": 5, "cursor": "5" }), "GET", "/tasks",
            json!({ "linked_object": "people", "linked_record_id": RECORD, "assignee": "susan@example.com", "is_completed": "false", "sort": "created_at:desc", "limit": "5", "offset": "5" }), json!(null),
            json!({ "data": [task()] }),
            json!({ "items": [{ "id": { "task_id": TASK }, "content_plaintext": "Follow up on the contract", "is_completed": false, "completed_at": null,
                                "linked_records": [{ "target_object_id": "people", "target_record_id": RECORD }], "assignees": [{ "referenced_actor_id": MEMBER }] }], "next_cursor": null })),
        case("tasks.get", json!({ "task": TASK }), "GET", &format!("/tasks/{TASK}"), json!({}), json!(null), json!({ "data": task() }),
            json!({ "id": { "task_id": TASK }, "deadline_at": "2026-11-01T15:00:00.000000000Z" })),
        // Attio asks for every field of a new task, so those not given go as nothing, not done, and none.
        case("tasks.create", json!({ "content": "Follow up on the contract", "linked_records": [on_person.clone()], "assignees": [{ "workspace_member_email_address": "susan@example.com" }, { "referenced_actor_id": MEMBER }] }), "POST", "/tasks", json!({}),
            json!({ "data": { "content": "Follow up on the contract", "format": "plaintext", "deadline_at": null, "is_completed": false, "linked_records": [on_person.clone()],
                              "assignees": [{ "workspace_member_email_address": "susan@example.com" }, { "referenced_actor_type": "workspace-member", "referenced_actor_id": MEMBER }] } }),
            json!({ "data": task() }), json!({ "id": { "task_id": TASK } })),
        // A null deadline removes it, and is sent as it is.
        case("tasks.update", json!({ "task": TASK, "deadline_at": null, "is_completed": true }), "PATCH", &format!("/tasks/{TASK}"), json!({}),
            json!({ "data": { "deadline_at": null, "is_completed": true } }), json!({ "data": task() }), json!({ "id": { "task_id": TASK } })),
        case("tasks.delete", json!({ "task": TASK }), "DELETE", &format!("/tasks/{TASK}"), json!({}), json!(null), json!({}), json!(null)),

        // threads
        case("threads.list", json!({ "object": "people", "record_id": RECORD }), "GET", "/threads", json!({ "object": "people", "record_id": RECORD, "limit": "50" }), json!(null),
            json!({ "data": [thread()] }),
            json!({ "items": [{ "id": { "thread_id": THREAD }, "created_at": "2023-01-01T15:00:00.000000000Z", "comment_count": 2, "record": { "record_id": RECORD, "object_id": OBJECT }, "entry": null }], "next_cursor": null })),
        case("threads.get", json!({ "thread": THREAD, "limit": 100, "cursor": "c-1" }), "GET", &format!("/threads/{THREAD}"), json!({ "limit": "100", "cursor": "c-1" }), json!(null),
            json!({ "data": thread(), "pagination": { "next_cursor": "c-2" } }),
            json!({ "id": { "thread_id": THREAD }, "comments": [{ "content_plaintext": "Let's close this deal.", "author": { "id": MEMBER } }, { "content_plaintext": "Agreed." }], "next_cursor": "c-2" })),
        // The comment is plain text, written as the member named.
        case("threads.comment", json!({ "author": MEMBER, "record": { "object": "people", "record_id": RECORD }, "content": "Renewal call went well." }), "POST", "/comments", json!({}),
            json!({ "data": { "format": "plaintext", "content": "Renewal call went well.", "author": { "type": "workspace-member", "id": MEMBER }, "record": { "object": "people", "record_id": RECORD } } }),
            json!({ "data": comment("Renewal call went well.") }),
            json!({ "id": { "comment_id": COMMENT }, "thread_id": THREAD, "content_plaintext": "Renewal call went well.", "record": { "record_id": RECORD }, "resolved_by": { "id": null, "type": null } })),

        // workspace members
        case("workspace_members.list", json!({}), "GET", "/workspace_members", json!({}), json!(null), json!({ "data": [member()] }),
            json!([{ "id": { "workspace_member_id": MEMBER }, "first_name": "Susan", "email_address": "susan@example.com", "access_level": "admin" }])),
        case("workspace_members.get", json!({ "member": MEMBER }), "GET", &format!("/workspace_members/{MEMBER}"), json!({}), json!(null), json!({ "data": member() }),
            json!({ "id": { "workspace_member_id": MEMBER }, "last_name": "Kare", "avatar_url": null })),

        // meetings, paged by Attio's own cursor
        case("meetings.list", json!({ "linked_object": "people", "linked_record_id": RECORD, "participants": ["ada@example.com", "grace@example.com"], "sort": "start_desc",
                                      "ends_from": "2026-10-01T00:00:00Z", "starts_before": "2026-11-01T00:00:00Z", "timezone": "Europe/London", "limit": 10, "cursor": "m-1" }), "GET", "/meetings",
            json!({ "linked_object": "people", "linked_record_id": RECORD, "participants": "ada@example.com,grace@example.com", "sort": "start_desc",
                    "ends_from": "2026-10-01T00:00:00Z", "starts_before": "2026-11-01T00:00:00Z", "timezone": "Europe/London", "limit": "10", "cursor": "m-1" }), json!(null),
            json!({ "data": [meeting()], "pagination": { "next_cursor": "m-2" } }),
            json!({ "items": [{ "id": { "meeting_id": MEETING }, "title": "Renewal call", "is_all_day": false, "start": { "datetime": "2026-10-12T16:00:00.000000000Z", "timezone": "Europe/London", "date": null },
                                "participants": [{ "status": "accepted", "is_organizer": true, "email_address": "ada@example.com" }], "linked_records": [{ "object_slug": "people", "record_id": RECORD }] }], "next_cursor": "m-2" })),
        case("meetings.get", json!({ "meeting": MEETING }), "GET", &format!("/meetings/{MEETING}"), json!({}), json!(null), json!({ "data": meeting() }),
            json!({ "id": { "meeting_id": MEETING }, "description": "Terms for next year", "created_by_actor": { "type": "system", "id": null } })),

        // call recordings. The list says whether one is ready; the transcript is a second call.
        case("call_recordings.list", json!({ "meeting": MEETING }), "GET", &format!("/meetings/{MEETING}/call_recordings"), json!({ "limit": "50" }), json!(null),
            json!({ "data": [recording_row()], "pagination": { "next_cursor": null } }),
            json!({ "items": [{ "id": { "meeting_id": MEETING, "call_recording_id": RECORDING }, "status": "completed" }], "next_cursor": null })),
        case("call_recordings.get", json!({ "meeting": MEETING, "recording": RECORDING }), "GET", &format!("/meetings/{MEETING}/call_recordings/{RECORDING}"), json!({}), json!(null),
            json!({ "data": recording() }),
            json!({ "id": { "call_recording_id": RECORDING }, "status": "completed", "video_url": null,
                    "transcript": { "text": "[00:00] Alex Bell: Hello, Mr Watson, come here.\n[00:04] Tom Watson: I'm here.",
                                    "entries": [{ "speaker": "Alex Bell", "startMs": 510, "endMs": 810, "text": "Hello," },
                                                { "speaker": "Alex Bell", "startMs": 810, "endMs": 2110, "text": "Mr Watson, come here." },
                                                { "speaker": "Tom Watson", "startMs": 4210, "endMs": 4910, "text": "I'm here." }] } })),
    ]
}

/// What each operation does to a workspace and what it needs, stated here
/// and not derived from the code under test: a host lets a read run freely
/// and asks a person before anything else.
fn expected() -> Vec<(&'static str, Effect, Vec<&'static str>)> {
    use Effect::{Destructive, Read, Write};
    let objects = vec!["object_configuration:read"];
    let attributes = vec!["object_configuration:read", "list_configuration:read"];
    let records = vec!["record_permission:read", "object_configuration:read"];
    let records_write = vec!["record_permission:read-write", "object_configuration:read"];
    let lists = vec!["list_configuration:read"];
    let entries = vec!["list_entry:read", "list_configuration:read"];
    let entries_write = vec!["list_entry:read-write", "list_configuration:read"];
    let notes = vec!["note:read", "object_configuration:read", "record_permission:read"];
    let tasks = vec![
        "task:read",
        "object_configuration:read",
        "record_permission:read",
        "user_management:read",
    ];
    let tasks_write = vec![
        "task:read-write",
        "object_configuration:read",
        "record_permission:read",
        "user_management:read",
    ];
    let on_either = [
        "object_configuration:read",
        "record_permission:read",
        "list_configuration:read",
        "list_entry:read",
    ];
    let threads: Vec<&str> = ["comment:read"].into_iter().chain(on_either).collect();
    let threads_write: Vec<&str> = ["comment:read-write"].into_iter().chain(on_either).collect();
    let members = vec!["user_management:read"];
    let meetings = vec!["meeting:read", "record_permission:read"];
    let recordings = vec!["meeting:read", "call_recording:read"];
    vec![
        // Attio describes any token to itself, whatever its scopes.
        ("meta.identify", Read, vec![]),
        ("objects.list", Read, objects.clone()),
        ("objects.get", Read, objects),
        ("attributes.list", Read, attributes.clone()),
        ("attributes.get", Read, attributes.clone()),
        ("attributes.options", Read, attributes.clone()),
        ("attributes.statuses", Read, attributes),
        // A POST, and a read all the same.
        ("records.query", Read, records.clone()),
        ("records.get", Read, records),
        (
            "records.entries",
            Read,
            vec!["record_permission:read", "object_configuration:read", "list_entry:read"],
        ),
        ("records.create", Write, records_write.clone()),
        ("records.update", Write, records_write.clone()),
        ("records.assert", Write, records_write.clone()),
        ("records.delete", Destructive, records_write),
        ("lists.list", Read, lists.clone()),
        ("lists.get", Read, lists),
        // A POST, and a read all the same.
        ("entries.query", Read, entries.clone()),
        ("entries.get", Read, entries),
        ("entries.create", Write, entries_write.clone()),
        ("entries.update", Write, entries_write.clone()),
        ("entries.delete", Destructive, entries_write),
        ("notes.list", Read, notes.clone()),
        ("notes.get", Read, notes),
        (
            "notes.create",
            Write,
            vec!["note:read-write", "object_configuration:read", "record_permission:read"],
        ),
        ("notes.delete", Destructive, vec!["note:read-write"]),
        ("tasks.list", Read, tasks.clone()),
        ("tasks.get", Read, tasks),
        ("tasks.create", Write, tasks_write.clone()),
        ("tasks.update", Write, tasks_write),
        ("tasks.delete", Destructive, vec!["task:read-write"]),
        ("threads.list", Read, threads.clone()),
        ("threads.get", Read, threads),
        ("threads.comment", Write, threads_write),
        ("workspace_members.list", Read, members.clone()),
        ("workspace_members.get", Read, members),
        ("meetings.list", Read, meetings.clone()),
        ("meetings.get", Read, meetings),
        ("call_recordings.list", Read, recordings.clone()),
        ("call_recordings.get", Read, recordings),
    ]
}

#[tokio::test]
async fn the_operations_are_exactly_these_and_each_has_a_test_case() {
    let listed: Vec<String> = Attio::new().operations().into_iter().map(|o| o.name).collect();
    // Written out, so that an operation cannot be added without being named
    // here, with its effect, beside it.
    let mut named: Vec<String> = expected().iter().map(|(name, _, _)| format!("attio.{name}")).collect();
    named.splice(
        0..0,
        ["attio.identity.get".to_owned(), "attio.resource.resolve".to_owned()],
    );
    assert_eq!(listed, named, "the operations Attio offers, in order");
    assert_eq!(listed.len(), 41);

    let tested: Vec<String> = cases().iter().map(|case| format!("attio.{}", case.name)).collect();
    assert_eq!(tested, named[2..], "every operation has exactly one test case");
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
            "Bearer at-good",
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

#[tokio::test]
async fn every_operation_says_what_it_changes_and_what_it_needs() {
    let operations = Attio::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == format!("attio.{name}"))
            .unwrap_or_else(|| panic!("{name}"))
    };
    for operation in &operations {
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert_eq!(
            operation.input_schema["additionalProperties"], false,
            "{}: a field it does not list is refused",
            operation.name
        );
        assert!(!operation.description.is_empty(), "{}", operation.name);
    }
    for (name, effect, scopes) in expected() {
        let operation = find(name);
        assert_eq!(operation.effect, effect, "{name}");
        assert_eq!(operation.required_scopes, scopes, "{name}");
    }

    // The two queries are the only reads Attio takes as a POST, and they are reads.
    let posted_reads = ["records.query", "entries.query"];
    for name in posted_reads {
        assert_eq!(find(name).effect, Effect::Read, "{name} changes nothing");
    }
    // Everything else is told apart by its verb: a read is a GET, which the
    // transport may repeat, and nothing that changes a workspace is a GET or
    // is called a read.
    for case in cases() {
        let effect = find(case.name).effect;
        match case.verb {
            "GET" => assert_eq!(effect, Effect::Read, "{}", case.name),
            "POST" if posted_reads.contains(&case.name) => assert_eq!(effect, Effect::Read, "{}", case.name),
            "POST" | "PATCH" | "PUT" => assert_eq!(effect, Effect::Write, "{}", case.name),
            "DELETE" => assert_eq!(effect, Effect::Destructive, "{}", case.name),
            other => panic!("{}: {other} is not a verb Attio takes here", case.name),
        }
    }
    // Stated once more by name, since this is what a person is or is not asked about.
    for name in [
        "records.create",
        "records.update",
        "records.assert",
        "entries.create",
        "entries.update",
        "notes.create",
        "tasks.create",
        "tasks.update",
        "threads.comment",
    ] {
        assert_eq!(find(name).effect, Effect::Write, "{name}");
    }
    for name in ["records.delete", "entries.delete", "notes.delete", "tasks.delete"] {
        assert_eq!(find(name).effect, Effect::Destructive, "{name}");
    }
    // A write needs a scope that lets it write.
    for (name, effect, scopes) in expected() {
        let writes = scopes.iter().any(|scope| scope.ends_with(":read-write"));
        assert_eq!(writes, effect != Effect::Read, "{name}: {scopes:?}");
    }

    // The two every integration offers: identity needs no scope, and a
    // lookup reads an object or a list.
    assert!(find("identity.get").required_scopes.is_empty());
    assert_eq!(
        find("resource.resolve").required_scopes,
        ["object_configuration:read", "list_configuration:read"]
    );
    assert_eq!(find("resource.resolve").effect, Effect::Read);
}

#[tokio::test]
async fn the_schemas_describe_what_goes_in_and_what_comes_back() {
    let operations = Attio::new().operations();
    let find = |name: &str| operations.iter().find(|o| o.name == name).unwrap();
    let required = |name: &str| -> Vec<String> {
        find(name).input_schema["required"]
            .as_array()
            .map(|names| names.iter().filter_map(Value::as_str).map(str::to_owned).collect())
            .unwrap_or_default()
    };
    assert_eq!(
        required("attio.records.query"),
        ["object"],
        "a query needs only its object"
    );
    assert_eq!(required("attio.records.create"), ["object", "values"]);
    assert_eq!(
        required("attio.records.assert"),
        ["object", "matching_attribute", "values"]
    );
    assert_eq!(
        required("attio.entries.create"),
        ["list", "parent_object", "parent_record"]
    );
    assert_eq!(
        required("attio.notes.create"),
        ["parent_object", "parent_record", "title", "content"]
    );
    assert_eq!(required("attio.tasks.create"), ["content"]);
    assert_eq!(required("attio.threads.comment"), ["author", "content"]);
    assert!(required("attio.objects.list").is_empty());

    let query = &find("attio.records.query").input_schema["properties"];
    for field in ["object", "filter", "sorts", "attributes", "cursor", "limit"] {
        assert!(query.get(field).is_some(), "{field} is described");
    }
    let record = &find("attio.records.get").output_schema["properties"];
    for field in ["id", "created_at", "web_url", "values", "current"] {
        assert!(record.get(field).is_some(), "{field} is described");
    }
    let row = &find("attio.records.query").output_schema;
    assert!(row["properties"].get("next_cursor").is_some());
    let transcript = serde_json::to_string(&find("attio.call_recordings.get").output_schema).unwrap();
    for field in ["startMs", "endMs", "speaker", "entries"] {
        assert!(transcript.contains(field), "{field} is described");
    }
}

#[tokio::test]
async fn a_query_returns_rows_small_enough_to_read_many_of() {
    // Every attribute's current value, and nothing of when it was set or by whom.
    let (_server, socket, key) = answering(200, json!({ "data": [record()] })).await;
    let page = invoke(&socket, &key, "records.query", json!({ "object": "people" }))
        .await
        .unwrap();
    assert_eq!(
        page["items"][0],
        json!({
            "id": { "workspace_id": WORKSPACE, "object_id": OBJECT, "record_id": RECORD },
            "created_at": "2022-11-21T13:22:49.061281000Z",
            "web_url": format!("https://app.attio.com/acme/person/{RECORD}"),
            "current": record_current()
        })
    );

    // Only the attributes asked for. One the record has no value for is
    // null; a name it has no attribute by, a misspelt slug, is not in the
    // row at all, so the two cannot be mistaken for each other.
    let (server, socket, key) = answering(200, json!({ "data": [record()] })).await;
    let input = json!({ "object": "people", "attributes": ["name", "job_title", "not_an_attribute", "job_titel"] });
    let page = invoke(&socket, &key, "records.query", input).await.unwrap();
    assert_eq!(
        page["items"][0]["current"],
        json!({ "name": "Ada Lovelace", "job_title": null })
    );
    let current = page["items"][0]["current"].as_object().unwrap();
    assert!(current.contains_key("job_title") && !current.contains_key("job_titel"));
    // Attio is not told which attributes: it has no way to be.
    assert_eq!(body_of(&only_request(&server).await), json!({ "limit": 25 }));

    let (_server, socket, key) = answering(200, json!({ "data": [entry()] })).await;
    let input = json!({ "list": "enterprise_sales", "attributes": ["stage", "stge"] });
    let page = invoke(&socket, &key, "entries.query", input).await.unwrap();
    assert_eq!(page["items"][0]["current"], json!({ "stage": "Won" }));
    assert!(page["items"][0].get("entry_values").is_none());

    // A list that names no attribute asks for nothing in particular, and is
    // read as no list: every attribute, never a row with none.
    for unset in [json!([]), json!(null)] {
        let (_server, socket, key) = answering(200, json!({ "data": [record()] })).await;
        let input = json!({ "object": "people", "attributes": unset });
        let page = invoke(&socket, &key, "records.query", input).await.unwrap();
        assert_eq!(page["items"][0]["current"], record_current(), "{unset}");
        let (_server, socket, key) = answering(200, json!({ "data": [entry()] })).await;
        let input = json!({ "list": "enterprise_sales", "attributes": unset });
        let page = invoke(&socket, &key, "entries.query", input).await.unwrap();
        assert_eq!(page["items"][0]["current"], entry_current(), "{unset}");
    }
}

#[tokio::test]
async fn a_list_of_notes_or_threads_leaves_the_text_for_a_second_call() {
    let (_server, socket, key) = answering(200, json!({ "data": [note()] })).await;
    let page = invoke(&socket, &key, "notes.list", json!({})).await.unwrap();
    let shown = page["items"][0].to_string();
    assert!(
        !shown.contains("Budget agreed") && !shown.contains("content"),
        "{shown}"
    );
    assert_eq!(page["items"][0]["title"], "Initial call");

    let (_server, socket, key) = answering(200, json!({ "data": [thread()] })).await;
    let input = json!({ "list": "enterprise_sales", "entry_id": ENTRY });
    let page = invoke(&socket, &key, "threads.list", input).await.unwrap();
    let shown = page["items"][0].to_string();
    assert!(
        !shown.contains("close this deal") && !shown.contains("comments"),
        "{shown}"
    );
    assert_eq!(page["items"][0]["comment_count"], 2);

    // A thread nobody has said anything in yet has no place to report.
    let empty = json!({ "id": { "workspace_id": WORKSPACE, "thread_id": THREAD }, "created_at": null, "comments": [] });
    let (_server, socket, key) = answering(200, json!({ "data": [empty] })).await;
    let page = invoke(
        &socket,
        &key,
        "threads.list",
        json!({ "object": "people", "record_id": RECORD }),
    )
    .await
    .unwrap();
    assert_eq!(
        page["items"][0],
        json!({ "id": { "workspace_id": WORKSPACE, "thread_id": THREAD }, "created_at": null, "comment_count": 0, "record": null, "entry": null })
    );
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent() {
    // A query with nothing asked sends only the page size.
    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    invoke(&socket, &key, "records.query", json!({ "object": "people" }))
        .await
        .unwrap();
    assert_eq!(body_of(&only_request(&server).await), json!({ "limit": 25 }));

    // A note without a format is plain text; no date and no meeting are sent.
    let (server, socket, key) = answering(200, json!({ "data": note() })).await;
    let input = json!({ "parent_object": "people", "parent_record": RECORD, "title": "Call", "content": "Went well" });
    invoke(&socket, &key, "notes.create", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": { "parent_object": "people", "parent_record_id": RECORD, "title": "Call", "format": "plaintext", "content": "Went well" } })
    );

    // A reply in a thread names the thread and neither a record nor an entry.
    let (server, socket, key) = answering(200, json!({ "data": comment("Agreed.") })).await;
    let input =
        json!({ "author": MEMBER, "thread_id": THREAD, "content": "Agreed.", "created_at": "2026-10-01T09:00:00Z" });
    invoke(&socket, &key, "threads.comment", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": { "format": "plaintext", "content": "Agreed.", "created_at": "2026-10-01T09:00:00Z",
                          "author": { "type": "workspace-member", "id": MEMBER }, "thread_id": THREAD } })
    );

    // A change to a task sends what was named: here, who it is assigned to.
    let (server, socket, key) = answering(200, json!({ "data": task() })).await;
    let input = json!({ "task": TASK, "assignees": [], "deadline_at": "2026-12-01T09:00:00Z" });
    invoke(&socket, &key, "tasks.update", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "data": { "assignees": [], "deadline_at": "2026-12-01T09:00:00Z" } })
    );

    // Lists that take filters send none when none is given.
    for (name, input) in [
        ("notes.list", json!({})),
        ("tasks.list", json!({})),
        ("meetings.list", json!({})),
    ] {
        let (server, socket, key) = answering(200, json!({ "data": [] })).await;
        let page = invoke(&socket, &key, name, input).await.unwrap();
        assert_eq!(page, json!({ "items": [], "next_cursor": null }), "{name}");
        assert_eq!(
            query_of(&only_request(&server).await),
            json!({ "limit": "50" }),
            "{name}"
        );
    }
}

#[tokio::test]
async fn an_input_that_cannot_be_right_is_refused_without_calling_attio() {
    let (server, socket, key) = attio().await;
    let values = json!({ "name": "Ada" });
    let bad = [
        // A change that names nothing to change.
        (
            "records.update",
            json!({ "object": "people", "record": RECORD, "values": {} }),
        ),
        (
            "records.assert",
            json!({ "object": "people", "matching_attribute": "email_addresses", "values": {} }),
        ),
        (
            "entries.update",
            json!({ "list": "enterprise_sales", "entry": ENTRY, "entry_values": {} }),
        ),
        ("entries.update", json!({ "list": "enterprise_sales", "entry": ENTRY })),
        ("tasks.update", json!({ "task": TASK })),
        // A deadline is a time or null.
        ("tasks.update", json!({ "task": TASK, "deadline_at": 1_767_225_600 })),
        ("tasks.update", json!({ "task": TASK, "deadline_at": { "at": "noon" } })),
        // Required fields, and their types.
        ("records.create", json!({ "object": "people" })),
        ("records.create", json!({ "object": "people", "values": ["Ada"] })),
        (
            "records.assert",
            json!({ "object": "people", "values": values.clone() }),
        ),
        ("records.query", json!({ "object": "people", "filter": "name = Ada" })),
        ("records.query", json!({ "object": "people", "limit": "ten" })),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record": RECORD, "title": "Call" }),
        ),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record": RECORD, "title": "Call", "content": "x", "format": "html" }),
        ),
        (
            "attributes.list",
            json!({ "target": "records", "identifier": "people" }),
        ),
        ("attributes.list", json!({ "identifier": "people" })),
        // A sort names an attribute or a path, and not both or neither.
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc" }] }),
        ),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc", "attribute": " " }] }),
        ),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc", "attribute": "name", "path": [["people", "name"]] }] }),
        ),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc", "path": [] }] }),
        ),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "asc", "path": [["people", ""]] }] }),
        ),
        (
            "records.query",
            json!({ "object": "people", "sorts": [{ "direction": "up", "attribute": "name" }] }),
        ),
        (
            "entries.query",
            json!({ "list": "enterprise_sales", "sorts": [{ "direction": "asc", "path": [["people"]] }] }),
        ),
        // A record is named with its object, a list entry with its list.
        ("notes.list", json!({ "parent_record_id": RECORD })),
        ("tasks.list", json!({ "linked_object": "people" })),
        ("tasks.list", json!({ "linked_record_id": RECORD })),
        ("meetings.list", json!({ "linked_record_id": RECORD })),
        ("threads.list", json!({})),
        ("threads.list", json!({ "object": "people" })),
        (
            "threads.list",
            json!({ "record_id": RECORD, "list": "enterprise_sales" }),
        ),
        (
            "threads.list",
            json!({ "object": "people", "record_id": RECORD, "list": "enterprise_sales", "entry_id": ENTRY }),
        ),
        // A comment goes in exactly one place, and says something.
        ("threads.comment", json!({ "author": MEMBER, "content": "Hello" })),
        (
            "threads.comment",
            json!({ "author": MEMBER, "content": "Hello", "thread_id": THREAD, "record": { "object": "people", "record_id": RECORD } }),
        ),
        (
            "threads.comment",
            json!({ "author": MEMBER, "content": "  ", "thread_id": THREAD }),
        ),
        (
            "threads.comment",
            json!({ "author": "", "content": "Hello", "thread_id": THREAD }),
        ),
        (
            "threads.comment",
            json!({ "author": MEMBER, "content": "Hello", "record": { "object": "people" } }),
        ),
        // A task says something, in no more than Attio takes, and each assignee is named one way.
        ("tasks.create", json!({ "content": " " })),
        ("tasks.create", json!({ "content": "x".repeat(2001) })),
        ("tasks.create", json!({ "content": "Call", "assignees": [{}] })),
        (
            "tasks.create",
            json!({ "content": "Call", "assignees": [{ "referenced_actor_id": MEMBER, "workspace_member_email_address": "susan@example.com" }] }),
        ),
        (
            "tasks.create",
            json!({ "content": "Call", "linked_records": [{ "target_object": "people" }] }),
        ),
        (
            "tasks.create",
            json!({ "content": "Call", "linked_records": [{ "target_object": "people", "target_record_id": "" }] }),
        ),
        // The people of a meeting are addresses, one each.
        (
            "meetings.list",
            json!({ "participants": ["ada@example.com,grace@example.com"] }),
        ),
        ("meetings.list", json!({ "participants": ["ada@example.com", " "] })),
        // A filter that was given and is blank is not a filter left out.
        // Attio reads an empty assignee as "assigned to nobody".
        ("tasks.list", json!({ "assignee": "" })),
        ("tasks.list", json!({ "assignee": "  " })),
        ("tasks.list", json!({ "assignee": "\t", "is_completed": false })),
        ("meetings.list", json!({ "ends_from": "" })),
        ("meetings.list", json!({ "starts_before": " " })),
        ("meetings.list", json!({ "timezone": "" })),
        ("notes.list", json!({ "parent_object": "" })),
        (
            "notes.list",
            json!({ "parent_object": "people", "parent_record_id": " " }),
        ),
        ("threads.list", json!({ "object": "people", "record_id": "" })),
        ("tasks.list", json!({ "linked_object": "", "linked_record_id": "" })),
        (
            "meetings.list",
            json!({ "linked_object": " ", "linked_record_id": RECORD }),
        ),
        // Nor is a blank deadline a way to say "none".
        ("tasks.create", json!({ "content": "Call", "deadline_at": "" })),
        ("tasks.update", json!({ "task": TASK, "deadline_at": " " })),
    ];
    for (name, input) in bad {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_task_takes_exactly_as_much_text_as_attio_does() {
    let (server, socket, key) = answering(200, json!({ "data": task() })).await;
    // 2000 characters, counted as characters and not as bytes.
    let longest = "é".repeat(2000);
    invoke(&socket, &key, "tasks.create", json!({ "content": longest }))
        .await
        .unwrap();
    assert_eq!(body_of(&only_request(&server).await)["data"]["content"], json!(longest));
}

#[tokio::test]
async fn an_answer_without_the_thing_asked_for_is_an_error_and_never_an_empty_one() {
    let asked = [
        ("objects.get", json!({ "object": "people" })),
        (
            "attributes.get",
            json!({ "target": "objects", "identifier": "people", "attribute": "name" }),
        ),
        ("records.get", json!({ "object": "people", "record": RECORD })),
        (
            "records.create",
            json!({ "object": "people", "values": { "name": "Ada" } }),
        ),
        ("lists.get", json!({ "list": "enterprise_sales" })),
        ("entries.get", json!({ "list": "enterprise_sales", "entry": ENTRY })),
        ("notes.get", json!({ "note": NOTE })),
        ("tasks.get", json!({ "task": TASK })),
        ("threads.get", json!({ "thread": THREAD })),
        (
            "threads.comment",
            json!({ "author": MEMBER, "thread_id": THREAD, "content": "Hello" }),
        ),
        ("workspace_members.get", json!({ "member": MEMBER })),
        ("meetings.get", json!({ "meeting": MEETING })),
        (
            "call_recordings.get",
            json!({ "meeting": MEETING, "recording": RECORDING }),
        ),
        ("objects.list", json!({})),
        ("records.query", json!({ "object": "people" })),
        ("notes.list", json!({})),
        ("meetings.list", json!({})),
    ];
    for answered in [json!({}), json!({ "data": null }), json!({ "data": {} }), json!(null)] {
        for (name, input) in &asked {
            // A list has to be a list; one thing has to have its id.
            let (_server, socket, key) = answering(200, answered.clone()).await;
            let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Decode, "{name} answered {answered}: {err}");
        }
    }
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_says_where_and_repeats_nothing_of_it() {
    let mut odd = record();
    odd["values"]["name"] = json!("Ada Lovelace, ada@example.com");
    let (_server, socket, key) = answering(200, json!({ "data": odd })).await;
    let err = invoke(
        &socket,
        &key,
        "records.get",
        json!({ "object": "people", "record": RECORD }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert_eq!(
        err.message(),
        "attio sent a record that could not be read, at `values.name`"
    );
    let shown = format!("{err} {err:?} {:?}", err.to_wire());
    assert!(!shown.contains("Ada") && !shown.contains("example.com"), "{shown}");
}

#[tokio::test]
async fn attios_refusals_reach_the_caller_as_errors_they_can_act_on() {
    for (response, kind, reason) in [
        (
            attio_error(
                404,
                "invalid_request_error",
                "not_found",
                "Object with slug/ID \"peple\" not found.",
            ),
            ErrorKind::NotFound,
            "attio has no such resource: Object with slug/ID \"peple\" not found.",
        ),
        (
            attio_error(
                403,
                "auth_error",
                "unauthorized",
                "You do not have the necessary permissions to create this record.",
            ),
            ErrorKind::AccessDenied,
            "attio denied the request: You do not have the necessary permissions to create this record.",
        ),
        (
            attio_error(
                400,
                "invalid_request_error",
                "value_not_found",
                "Cannot find select attribute with select option title \"In Progress\".",
            ),
            ErrorKind::InvalidInput,
            "attio rejected the request: Cannot find select attribute with select option title \"In Progress\".",
        ),
        (
            attio_error(401, "auth_error", "unauthorized", "Invalid token at-good"),
            ErrorKind::ReconnectRequired,
            "attio rejected the stored authorization",
        ),
    ] {
        let (server, socket, key) = attio().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let input = json!({ "object": "people", "values": { "stage": "In Progress" } });
        let err = invoke(&socket, &key, "records.create", input).await.unwrap_err();
        assert_eq!((err.kind(), err.message()), (kind, reason));
    }
}

#[tokio::test]
async fn a_write_is_sent_once_when_attio_fails_and_a_read_is_tried_again() {
    let unavailable = || attio_error(503, "api_error", "service_unavailable", "Try again later.");
    let values = json!({ "name": "Ada" });
    for (name, input) in [
        (
            "records.create",
            json!({ "object": "people", "values": values.clone() }),
        ),
        (
            "records.update",
            json!({ "object": "people", "record": RECORD, "values": values.clone() }),
        ),
        (
            "entries.create",
            json!({ "list": "enterprise_sales", "parent_object": "people", "parent_record": RECORD }),
        ),
        (
            "entries.update",
            json!({ "list": "enterprise_sales", "entry": ENTRY, "entry_values": { "stage": "Won" } }),
        ),
        (
            "notes.create",
            json!({ "parent_object": "people", "parent_record": RECORD, "title": "Call", "content": "x" }),
        ),
        ("tasks.create", json!({ "content": "Call" })),
        ("tasks.update", json!({ "task": TASK, "is_completed": true })),
        (
            "threads.comment",
            json!({ "author": MEMBER, "thread_id": THREAD, "content": "Hello" }),
        ),
        // Reads by effect, but Attio takes them as POST, so they are not repeated either.
        ("records.query", json!({ "object": "people" })),
        ("entries.query", json!({ "list": "enterprise_sales" })),
        // `records.assert` and the deletes are not in this list: the
        // transport repeats a PUT and a DELETE after a server error. Sent
        // twice, an assert writes the same values to the same record, and a
        // delete that already worked is answered "not found"; see the guide.
    ] {
        let (server, socket, key) = attio().await;
        Mock::given(any()).respond_with(unavailable()).mount(&server).await;
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
        .respond_with(unavailable())
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
        json!({ "object": "people", "record": RECORD }),
    )
    .await
    .unwrap();
    assert_eq!(read["id"]["record_id"], RECORD);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let attio = Attio::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(attio.clone()), "at-good").await;
    let connection = socket.connection(key).await.unwrap();
    let person = format!("/v2/objects/people/records/{RECORD}");
    for (verb, at, body) in [
        (
            "POST",
            "/v2/objects/people/records/query".to_owned(),
            json!({ "data": [record()] }),
        ),
        ("GET", person.clone(), json!({ "data": record() })),
        (
            "PUT",
            "/v2/objects/people/records".to_owned(),
            json!({ "data": record() }),
        ),
        (
            "GET",
            "/v2/objects/people/attributes".to_owned(),
            json!({ "data": [attribute()] }),
        ),
        (
            "POST",
            "/v2/lists/enterprise_sales/entries".to_owned(),
            json!({ "data": entry() }),
        ),
        ("GET", "/v2/notes".to_owned(), json!({ "data": [note()] })),
        ("POST", "/v2/notes".to_owned(), json!({ "data": note() })),
        ("POST", "/v2/tasks".to_owned(), json!({ "data": task() })),
        ("POST", "/v2/comments".to_owned(), json!({ "data": comment("Agreed.") })),
        (
            "GET",
            format!("/v2/meetings/{MEETING}/call_recordings/{RECORDING}"),
            json!({ "data": recording() }),
        ),
    ] {
        Mock::given(method(verb))
            .and(path(at))
            .respond_with(answer(200, &body))
            .mount(&server)
            .await;
    }

    let records = attio.records(&connection);
    let found = records
        .query(
            "people",
            QueryRecords {
                sorts: Some(vec![Sort::by("name", Direction::Asc)]),
                attributes: Some(vec!["name".into()]),
                limit: Some(1),
                ..QueryRecords::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(found.items[0].id.record_id, RECORD);
    assert_eq!(found.items[0].current["name"], "Ada Lovelace");
    assert_eq!(found.items[0].current.len(), 1);
    assert_eq!(found.next_cursor.as_deref(), Some("1"));

    // The full record keeps Attio's lists, and answers for one attribute at a time.
    let ada = records.get("people", RECORD).await.unwrap();
    assert_eq!(ada.values.all("email_addresses").len(), 2);
    assert_eq!(ada.values.active("email_addresses").len(), 2);
    let name = ada.values.newest("name").unwrap();
    assert_eq!(name.fields["first_name"], "Ada");
    assert_eq!(name.plain(), "Ada Lovelace");
    assert_eq!(ada.values.newest("job_title"), None);
    assert_eq!(serde_json::to_value(&ada.current).unwrap(), record_current());

    let mut values = serde_json::Map::new();
    values.insert("email_addresses".into(), json!(["ada@example.com"]));
    let asserted = records
        .assert("people", "email_addresses", WriteRecord { values })
        .await
        .unwrap();
    assert_eq!(asserted.id.record_id, RECORD);

    let fields = attio
        .attributes(&connection)
        .list(Target::Objects, "people", ListAttributes::default())
        .await
        .unwrap();
    assert_eq!(fields.items[0].kind.as_deref(), Some("record-reference"));
    assert_eq!(fields.next_cursor, None);

    let added = attio
        .entries(&connection)
        .create("enterprise_sales", "people", RECORD, WriteEntry::default())
        .await
        .unwrap();
    assert_eq!(added.current["stage"], "Won");
    assert_eq!(added.entry_values.newest("stage").unwrap().plain(), "Won");

    let notes = attio.notes(&connection);
    let listed = notes
        .list(ListNotes {
            parent_object: Some("people".into()),
            parent_record_id: Some(RECORD.into()),
            ..ListNotes::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.items[0].title.as_deref(), Some("Initial call"));
    let written = notes
        .create(
            "people",
            RECORD,
            CreateNote {
                title: "Initial call".into(),
                format: Some(NoteFormat::Markdown),
                content: "# Introduction".into(),
                ..CreateNote::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        written.content_plaintext.as_deref(),
        Some("Introduction\nBudget agreed")
    );

    let task = attio
        .tasks(&connection)
        .create(CreateTask {
            content: "Follow up on the contract".into(),
            assignees: Some(vec![AssignTo {
                referenced_actor_id: Some(MEMBER.into()),
                ..AssignTo::default()
            }]),
            ..CreateTask::default()
        })
        .await
        .unwrap();
    assert!(!task.is_completed);

    let reply = CreateComment {
        record: Some(OnRecord {
            object: "people".into(),
            record_id: RECORD.into(),
        }),
        content: "Agreed.".into(),
        ..CreateComment::default()
    };
    let said = attio.threads(&connection).comment(MEMBER, reply).await.unwrap();
    assert_eq!(said.thread_id.as_deref(), Some(THREAD));

    let call = attio
        .call_recordings(&connection)
        .get(MEETING, RECORDING)
        .await
        .unwrap();
    let transcript = call.transcript.unwrap();
    assert_eq!(transcript.entries.len(), 3);
    assert_eq!(transcript.entries[2].speaker.as_deref(), Some("Tom Watson"));
    assert_eq!(
        (transcript.entries[2].start_ms, transcript.entries[2].end_ms),
        (4210, 4910)
    );
    assert!(transcript.text.starts_with("[00:00] Alex Bell:"));

    // What went out is what the named operations send.
    let sent = |verb: &'static str, at: &'static str| {
        let server = &server;
        async move {
            let received = server.received_requests().await.unwrap();
            let request = received
                .iter()
                .find(|request| request.method.as_str() == verb && request.url.path() == at)
                .unwrap_or_else(|| panic!("{verb} {at}"));
            (query_of(request), body_of(request))
        }
    };
    assert_eq!(
        sent("POST", "/v2/objects/people/records/query").await.1,
        json!({ "sorts": [{ "direction": "asc", "attribute": "name" }], "limit": 1 })
    );
    assert_eq!(
        sent("PUT", "/v2/objects/people/records").await,
        (
            json!({ "matching_attribute": "email_addresses" }),
            json!({ "data": { "values": { "email_addresses": ["ada@example.com"] } } })
        )
    );
    assert_eq!(
        sent("POST", "/v2/lists/enterprise_sales/entries").await.1,
        json!({ "data": { "parent_object": "people", "parent_record_id": RECORD, "entry_values": {} } })
    );
    assert_eq!(
        sent("POST", "/v2/tasks").await.1["data"]["assignees"],
        json!([{ "referenced_actor_type": "workspace-member", "referenced_actor_id": MEMBER }])
    );
    assert_eq!(
        sent("GET", "/v2/objects/people/attributes").await.0,
        json!({ "limit": "50" })
    );
}

#[tokio::test]
async fn a_recording_without_a_transcript_says_so_and_one_being_read_has_none_yet() {
    for (sent, entries) in [
        (json!(null), None),
        (json!({ "segments": [], "raw_transcript": "" }), Some(0)),
        // Nulls where a list, a time or a name would be are read as none of it.
        (json!({ "segments": null, "raw_transcript": null }), Some(0)),
        (json!({}), Some(0)),
        (
            json!({ "segments": [{ "speech": "Hello?", "start_time": 1, "end_time": 2, "speaker": null }], "raw_transcript": null }),
            Some(1),
        ),
        (
            json!({ "segments": [{ "speech": "Hello?", "start_time": 1.0004, "end_time": 2, "speaker": { "name": null } }] }),
            Some(1),
        ),
        // A segment nobody is named for has no speaker, and is still what was said.
        (
            json!({ "segments": [{ "speech": "Hello?", "start_time": 1, "end_time": 2, "speaker": { "name": " " } }], "raw_transcript": "[00:01] Hello?" }),
            Some(1),
        ),
    ] {
        let mut processing = recording();
        processing["status"] = json!("processing");
        processing["transcript"] = sent.clone();
        let (_server, socket, key) = answering(200, json!({ "data": processing })).await;
        let input = json!({ "meeting": MEETING, "recording": RECORDING });
        let got = invoke(&socket, &key, "call_recordings.get", input).await.unwrap();
        assert_eq!(got["status"], "processing");
        assert_eq!(
            got["transcript"]["entries"].as_array().map(Vec::len),
            entries,
            "{sent}: {got}"
        );
        if entries == Some(1) {
            assert_eq!(
                got["transcript"]["entries"][0],
                json!({ "speaker": null, "startMs": 1000, "endMs": 2000, "text": "Hello?" })
            );
        }
    }
    // A segment without its times starts and ends at nought; it is not a reason to lose the rest.
    let mut untimed = recording();
    untimed["transcript"]["segments"][0] =
        json!({ "speech": null, "start_time": null, "end_time": null, "speaker": { "name": "Alex Bell" } });
    let (_server, socket, key) = answering(200, json!({ "data": untimed })).await;
    let input = json!({ "meeting": MEETING, "recording": RECORDING });
    let got = invoke(&socket, &key, "call_recordings.get", input).await.unwrap();
    assert_eq!(
        got["transcript"]["entries"][0],
        json!({ "speaker": "Alex Bell", "startMs": 0, "endMs": 0, "text": "" })
    );
    assert_eq!(got["transcript"]["entries"].as_array().unwrap().len(), 3);

    // A row of the list never claims there is no transcript: it has no such field.
    let (_server, socket, key) = answering(200, json!({ "data": [recording_row()] })).await;
    let page = invoke(&socket, &key, "call_recordings.list", json!({ "meeting": MEETING }))
        .await
        .unwrap();
    assert!(page["items"][0].get("transcript").is_none());
}

#[tokio::test]
async fn a_recording_is_written_and_read_back_as_the_same_recording() {
    let server = MockServer::start().await;
    let attio = Attio::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(attio.clone()), "at-good").await;
    let connection = socket.connection(key).await.unwrap();
    Mock::given(any())
        .respond_with(answer(200, &json!({ "data": recording() })))
        .mount(&server)
        .await;
    let call = attio
        .call_recordings(&connection)
        .get(MEETING, RECORDING)
        .await
        .unwrap();
    assert_eq!(
        call.transcript.as_ref().map(|transcript| transcript.entries.len()),
        Some(3)
    );

    // What an operation returns is what a caller stores and reads again.
    let written = serde_json::to_value(&call).unwrap();
    let read: CallRecording = serde_json::from_value(written.clone()).unwrap();
    assert_eq!(read, call, "nothing of the transcript is lost on the way back");
    assert_eq!(serde_json::to_value(&read).unwrap(), written);
    let transcript: Transcript = serde_json::from_value(written["transcript"].clone()).unwrap();
    assert_eq!(Some(&transcript), call.transcript.as_ref());
    assert_eq!(transcript.entries[1].text, "Mr Watson, come here.");

    // The same holds for a record, whose `current` is added to what Attio sent.
    let (_server, socket, key) = answering(200, json!({ "data": record() })).await;
    let got = invoke(
        &socket,
        &key,
        "records.get",
        json!({ "object": "people", "record": RECORD }),
    )
    .await
    .unwrap();
    let read: Record = serde_json::from_value(got.clone()).unwrap();
    assert_eq!(serde_json::to_value(&read).unwrap(), got);
}

#[tokio::test]
async fn a_null_where_a_list_or_a_flag_would_be_does_not_make_an_answer_unreadable() {
    let nulled = |mut thing: Value, fields: &[&str]| {
        for field in fields {
            thing[*field] = Value::Null;
        }
        json!({ "data": thing })
    };
    // A record with no values at all, and one attribute with `null` for its list.
    let mut sparse = record();
    sparse["values"]["job_title"] = Value::Null;
    let (_server, socket, key) = answering(200, json!({ "data": sparse })).await;
    let got = invoke(
        &socket,
        &key,
        "records.get",
        json!({ "object": "people", "record": RECORD }),
    )
    .await
    .unwrap();
    assert_eq!(got["current"], record_current());
    assert_eq!(got["values"]["job_title"], json!([]));
    let (_server, socket, key) = answering(200, nulled(record(), &["values", "web_url"])).await;
    let got = invoke(
        &socket,
        &key,
        "records.get",
        json!({ "object": "people", "record": RECORD }),
    )
    .await
    .unwrap();
    assert_eq!((&got["values"], &got["current"]), (&json!({}), &json!({})));
    let (_server, socket, key) = answering(200, nulled(entry(), &["entry_values"])).await;
    let got = invoke(
        &socket,
        &key,
        "entries.get",
        json!({ "list": "enterprise_sales", "entry": ENTRY }),
    )
    .await
    .unwrap();
    assert_eq!(got["current"], json!({}));

    for (name, input, answered, expected) in [
        (
            "tasks.get",
            json!({ "task": TASK }),
            nulled(task(), &["is_completed", "linked_records", "assignees", "deadline_at"]),
            json!({ "id": { "task_id": TASK }, "is_completed": false, "linked_records": [], "assignees": [], "deadline_at": null }),
        ),
        (
            "notes.get",
            json!({ "note": NOTE }),
            nulled(note(), &["tags", "title", "content_markdown"]),
            json!({ "id": { "note_id": NOTE }, "tags": [], "title": null }),
        ),
        (
            "lists.get",
            json!({ "list": "enterprise_sales" }),
            nulled(
                list(),
                &["parent_object", "workspace_member_access", "workspace_access"],
            ),
            json!({ "id": { "list_id": LIST }, "parent_object": [], "workspace_member_access": [] }),
        ),
        (
            "threads.get",
            json!({ "thread": THREAD }),
            nulled(thread(), &["comments", "created_at"]),
            json!({ "id": { "thread_id": THREAD }, "comments": [] }),
        ),
        (
            "meetings.get",
            json!({ "meeting": MEETING }),
            nulled(
                meeting(),
                &[
                    "participants",
                    "linked_records",
                    "is_all_day",
                    "start",
                    "created_by_actor",
                ],
            ),
            json!({ "id": { "meeting_id": MEETING }, "participants": [], "linked_records": [], "is_all_day": false, "start": null }),
        ),
        (
            "attributes.get",
            json!({ "target": "objects", "identifier": "people", "attribute": "company" }),
            nulled(
                attribute(),
                &["is_unique", "is_multiselect", "is_archived", "config", "relationship"],
            ),
            json!({ "id": { "attribute_id": ATTRIBUTE }, "is_unique": false, "is_multiselect": false, "config": null }),
        ),
        (
            "meta.identify",
            json!({}),
            nulled(token(), &["scope", "client_id", "token_level"])["data"].clone(),
            json!({ "workspace_id": WORKSPACE, "scope": "", "scopes": [] }),
        ),
    ] {
        let (_server, socket, key) = answering(200, answered.clone()).await;
        let got = invoke(&socket, &key, name, input)
            .await
            .unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(contains(&got, &expected), "{name}: {got}");
    }

    // A thread listed with no comments counts none.
    let (_server, socket, key) = answering(
        200,
        json!({ "data": [nulled(thread(), &["comments"])["data"].clone()] }),
    )
    .await;
    let input = json!({ "object": "people", "record_id": RECORD });
    let page = invoke(&socket, &key, "threads.list", input).await.unwrap();
    assert_eq!(page["items"][0]["comment_count"], 0);

    // An id is not such a field: without it the answer is not the thing asked for.
    let mut nameless = task();
    nameless["id"]["task_id"] = Value::Null;
    let (_server, socket, key) = answering(200, json!({ "data": nameless })).await;
    let err = invoke(&socket, &key, "tasks.get", json!({ "task": TASK }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn asking_for_unassigned_tasks_takes_the_word_and_nothing_less() {
    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    invoke(&socket, &key, "tasks.list", json!({ "assignee": " null " }))
        .await
        .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "assignee": "null", "limit": "50" })
    );
    // Leaving the filter out asks about everyone's tasks, and sends no filter.
    let (server, socket, key) = answering(200, json!({ "data": [] })).await;
    invoke(&socket, &key, "tasks.list", json!({ "assignee": null }))
        .await
        .unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({ "limit": "50" }));
    // A blank is neither, and never reaches Attio as an empty `assignee`.
    let (server, socket, key) = attio().await;
    let err = invoke(&socket, &key, "tasks.list", json!({ "assignee": "" }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.message().contains("`null`"),
        "it says how to ask: {}",
        err.message()
    );
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_delete_the_transport_repeats_after_a_failure_can_report_not_found() {
    // The transport sends a DELETE again after a server error. If the first
    // one did delete the record, the second finds nothing: the call reports
    // `NotFound` for a delete that worked. The guide says to read it as "gone".
    let (server, socket, key) = attio().await;
    Mock::given(method("DELETE"))
        .respond_with(attio_error(503, "api_error", "service_unavailable", "Try again later."))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(attio_error(
            404,
            "invalid_request_error",
            "not_found",
            &format!("Record with ID \"{RECORD}\" not found."),
        ))
        .mount(&server)
        .await;
    let err = invoke(
        &socket,
        &key,
        "records.delete",
        json!({ "object": "people", "record": RECORD }),
    )
    .await
    .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2, "sent, and sent again");
    assert!(received.iter().all(|request| request.method.as_str() == "DELETE"));

    // When the first one did not delete it, the second does, and the call succeeds.
    let (server, socket, key) = attio().await;
    Mock::given(method("DELETE"))
        .respond_with(attio_error(503, "api_error", "service_unavailable", "Try again later."))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("DELETE"))
        .respond_with(answer(200, &json!({})))
        .mount(&server)
        .await;
    let done = invoke(
        &socket,
        &key,
        "records.delete",
        json!({ "object": "people", "record": RECORD }),
    )
    .await;
    assert_eq!(done.unwrap(), Value::Null);
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}
