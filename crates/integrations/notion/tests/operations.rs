//! Every Notion operation, called by name against a local server that answers as Notion does.

use serde_json::json;
use socketkit_core::{Effect, ErrorKind, Integration};
use socketkit_notion::Notion;
use socketkit_notion::models::{Paging, RichText, SearchFilter, SearchObject, SearchQuery};
use socketkit_testkit::wiremock::matchers::{method, path};
use socketkit_testkit::wiremock::{Mock, ResponseTemplate};

mod support;
use support::{
    BLOCK, COMMENT, Case, DATABASE, PAGE, PAGE_URL, SOURCE, THREAD, USER, VERSION, answering, block, body_of, comment,
    contains, data_source, database, invoke, list, notion, only_request, page, page_returned, query_of, said, to_do,
    user,
};

/// `pages.read` makes more than one request, so its requests are checked in `tests/read.rs`.
const MANY_REQUESTS: &str = "pages.read";

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, path: String, query, body, response, returns| Case { name, input, verb, path, query, body, response, returns };
    let paragraph = json!({ "paragraph": { "rich_text": [{ "text": { "content": "Hello" } }] } });
    let filter = json!({ "and": [{ "property": "Status", "status": { "equals": "Done" } }, { "property": "Due", "date": { "is_empty": true } }] });
    let row = json!({ "object": "page", "id": PAGE, "properties": { "Name": { "type": "title", "title": [{ "plain_text": "Roadmap" }] } } });
    vec![
        // search. Notion offers it only as POST; the place in the list goes in the body.
        case("search.run", json!({ "query": "roadmap", "filter": { "value": "page" }, "sort": { "direction": "descending" }, "cursor": "cur-1", "limit": 10 }),
            "POST", "/search".into(), json!({}),
            json!({ "query": "roadmap", "filter": { "property": "object", "value": "page" }, "sort": { "timestamp": "last_edited_time", "direction": "descending" }, "start_cursor": "cur-1", "page_size": 10 }),
            list("page_or_data_source", json!([page(), data_source()]), Some("cur-2")),
            json!({ "items": [
                { "object": "page", "id": PAGE, "url": "https://www.notion.so/Roadmap-0123abcd456789abcdef0123456789ab" },
                { "object": "data_source", "id": SOURCE, "title": [{ "plain_text": "Tasks" }], "parent": { "database_id": DATABASE } }
            ], "next_cursor": "cur-2" })),

        // pages. An address is read for its id.
        case("pages.get", json!({ "page": PAGE_URL }), "GET", format!("/pages/{PAGE}"), json!({}), json!(null), page(), page_returned()),
        case("pages.property", json!({ "page": PAGE, "property": "Tasks", "limit": 2 }), "GET", format!("/pages/{PAGE}/properties/Tasks"), json!({ "page_size": "2" }), json!(null),
            json!({ "object": "list", "type": "property_item", "has_more": true, "next_cursor": "cur-2",
                    "property_item": { "id": "rel1", "type": "relation", "relation": {}, "next_url": "https://api.notion.com/v1/pages/x/properties/rel1?start_cursor=cur-2" },
                    "results": [{ "object": "property_item", "id": "rel1", "type": "relation", "relation": { "id": BLOCK } }] }),
            json!({ "id": "rel1", "type": "relation", "items": [{ "object": "property_item", "type": "relation", "relation": { "id": BLOCK } }], "next_cursor": "cur-2" })),
        case("pages.create", json!({ "parent": { "data_source_id": SOURCE }, "properties": { "Name": { "title": [{ "text": { "content": "Roadmap" } }] } }, "children": [paragraph.clone()], "icon": { "type": "emoji", "emoji": "🗺️" } }),
            "POST", "/pages".into(), json!({}),
            json!({ "parent": { "data_source_id": SOURCE }, "properties": { "Name": { "title": [{ "text": { "content": "Roadmap" } }] } }, "children": [paragraph.clone()], "icon": { "type": "emoji", "emoji": "🗺️" } }),
            page(), page_returned()),
        // A `null` is how Notion is told to empty a property or remove a cover, so it is sent.
        case("pages.update", json!({ "page": PAGE, "properties": { "Status": { "status": { "name": "Done" } }, "Due": { "date": null } }, "cover": null }),
            "PATCH", format!("/pages/{PAGE}"), json!({}),
            json!({ "properties": { "Status": { "status": { "name": "Done" } }, "Due": { "date": null } }, "cover": null }),
            page(), json!({ "id": PAGE })),
        case("pages.archive", json!({ "page": "0123abcd456789abcdef0123456789ab" }), "PATCH", format!("/pages/{PAGE}"), json!({}), json!({ "in_trash": true }),
            json!({ "object": "page", "id": PAGE, "in_trash": true }), json!({ "id": PAGE, "in_trash": true })),

        // blocks
        case("blocks.get", json!({ "block": BLOCK }), "GET", format!("/blocks/{BLOCK}"), json!({}), json!(null), to_do(),
            json!({ "id": BLOCK, "type": "to_do", "has_children": false, "in_trash": false, "to_do": { "checked": false, "rich_text": [{ "plain_text": "Ship it" }] }, "parent": { "page_id": PAGE } })),
        case("blocks.children", json!({ "block": PAGE, "cursor": "cur-1", "limit": 50 }), "GET", format!("/blocks/{PAGE}/children"), json!({ "start_cursor": "cur-1", "page_size": "50" }), json!(null),
            list("block", json!([to_do()]), None), json!({ "items": [{ "id": BLOCK, "type": "to_do" }], "next_cursor": null })),
        case("blocks.append", json!({ "block": PAGE, "children": [paragraph.clone()], "position": { "type": "after_block", "after_block": { "id": BLOCK } } }),
            "PATCH", format!("/blocks/{PAGE}/children"), json!({}),
            json!({ "children": [paragraph.clone()], "position": { "type": "after_block", "after_block": { "id": BLOCK } } }),
            list("block", json!([block(COMMENT, "paragraph", json!({ "rich_text": said("Hello") }), false)]), None),
            json!({ "items": [{ "id": COMMENT, "type": "paragraph", "paragraph": { "rich_text": [{ "plain_text": "Hello" }] } }], "next_cursor": null })),
        case("blocks.update", json!({ "block": BLOCK, "content": { "to_do": { "checked": true } } }), "PATCH", format!("/blocks/{BLOCK}"), json!({}), json!({ "to_do": { "checked": true } }),
            to_do(), json!({ "id": BLOCK, "type": "to_do" })),
        case("blocks.delete", json!({ "block": BLOCK }), "DELETE", format!("/blocks/{BLOCK}"), json!({}), json!(null),
            json!({ "object": "block", "id": BLOCK, "type": "to_do", "in_trash": true }), json!({ "id": BLOCK, "in_trash": true })),

        // databases. A database names its data sources; the rows and the schema are asked of one of those.
        case("databases.get", json!({ "database": DATABASE }), "GET", format!("/databases/{DATABASE}"), json!({}), json!(null), database(),
            json!({ "id": DATABASE, "title": [{ "plain_text": "Tasks" }], "data_sources": [{ "id": SOURCE, "name": "Tasks" }], "parent": { "page_id": PAGE }, "in_trash": false })),
        case("databases.data_source", json!({ "data_source": SOURCE }), "GET", format!("/data_sources/{SOURCE}"), json!({}), json!(null), data_source(),
            json!({ "id": SOURCE, "parent": { "database_id": DATABASE }, "database_parent": { "page_id": PAGE }, "properties": { "Status": { "id": "%3EfC", "type": "status" } } })),
        case("databases.query", json!({ "data_source": SOURCE, "filter": filter.clone(), "sorts": [{ "property": "Due", "direction": "ascending" }, { "timestamp": "last_edited_time", "direction": "descending" }], "limit": 25 }),
            "POST", format!("/data_sources/{SOURCE}/query"), json!({}),
            json!({ "filter": filter.clone(), "sorts": [{ "property": "Due", "direction": "ascending" }, { "timestamp": "last_edited_time", "direction": "descending" }], "page_size": 25 }),
            list("page_or_data_source", json!([row.clone()]), None),
            json!({ "items": [{ "object": "page", "id": PAGE, "properties": { "Name": { "type": "title" } } }], "next_cursor": null })),

        // users
        case("users.list", json!({ "limit": 100 }), "GET", "/users".into(), json!({ "page_size": "100" }), json!(null),
            list("user", json!([user()]), Some("cur-2")),
            json!({ "items": [{ "id": USER, "type": "person", "name": "Ada Lovelace", "person": { "email": "ada@example.test" } }], "next_cursor": "cur-2" })),
        case("users.get", json!({ "user": USER }), "GET", format!("/users/{USER}"), json!({}), json!(null), user(), json!({ "id": USER, "name": "Ada Lovelace" })),

        // comments
        case("comments.list", json!({ "block": PAGE, "limit": 5 }), "GET", "/comments".into(), json!({ "block_id": PAGE, "page_size": "5" }), json!(null),
            list("comment", json!([comment()]), None),
            json!({ "items": [{ "id": COMMENT, "discussion_id": THREAD, "parent": { "page_id": PAGE }, "created_by": { "id": USER }, "rich_text": [{ "plain_text": "Looks good" }] }], "next_cursor": null })),
        case("comments.create", json!({ "parent": { "page_id": PAGE }, "rich_text": [{ "text": { "content": "Looks good" } }] }), "POST", "/comments".into(), json!({}),
            json!({ "parent": { "page_id": PAGE }, "rich_text": [{ "text": { "content": "Looks good" } }] }),
            comment(), json!({ "id": COMMENT, "discussion_id": THREAD })),
    ]
}

#[tokio::test]
async fn the_table_above_covers_every_operation_notion_offers() {
    let listed: Vec<String> = Notion::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = cases().iter().map(|c| format!("notion.{}", c.name)).collect();
    tested.extend(
        ["identity.get", "resource.resolve", MANY_REQUESTS]
            .iter()
            .map(|name| format!("notion.{name}")),
    );
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist"
    );
    assert_eq!(listed.len(), 21);
}

#[tokio::test]
async fn every_operation_sends_the_right_request_and_returns_what_notion_sent() {
    for case in cases() {
        let (server, socket, key) = notion().await;
        Mock::given(method(case.verb))
            .and(path(format!("/v1{}", case.path)))
            .respond_with(ResponseTemplate::new(200).set_body_json(case.response.clone()))
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
        let header = |name: &str| request.headers.get(name).unwrap().to_str().unwrap().to_owned();
        assert_eq!(header("authorization"), "Bearer ntn-good", "{}", case.name);
        assert_eq!(header("notion-version"), VERSION, "{}", case.name);
        assert_eq!(
            query_of(&request),
            case.query,
            "{}: exactly these parameters reach Notion",
            case.name
        );
        assert_eq!(
            body_of(&request),
            case.body,
            "{}: exactly this body reaches Notion",
            case.name
        );
    }
}

#[tokio::test]
async fn every_operation_describes_its_input_and_marks_what_it_changes() {
    let operations = Notion::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };

    // A host lets a read run freely and asks a person before anything else,
    // so each effect is stated here and not derived from the code under test.
    let expected = [
        ("search.run", Effect::Read),
        ("pages.get", Effect::Read),
        ("pages.property", Effect::Read),
        ("pages.read", Effect::Read),
        ("blocks.get", Effect::Read),
        ("blocks.children", Effect::Read),
        ("databases.get", Effect::Read),
        ("databases.data_source", Effect::Read),
        ("databases.query", Effect::Read),
        ("users.list", Effect::Read),
        ("users.get", Effect::Read),
        ("comments.list", Effect::Read),
        ("pages.create", Effect::Write),
        ("pages.update", Effect::Write),
        ("blocks.append", Effect::Write),
        ("blocks.update", Effect::Write),
        ("comments.create", Effect::Write),
        // What is trashed is gone from where people look for it.
        ("pages.archive", Effect::Destructive),
        ("blocks.delete", Effect::Destructive),
    ];
    assert_eq!(
        expected.len(),
        operations.len() - 2,
        "identity and lookup are the other two"
    );
    for (name, effect) in expected {
        let operation = find(&format!("notion.{name}"));
        assert_eq!(operation.effect, effect, "{name}");
        // Notion has no scopes; the description says which capability is needed.
        assert!(operation.required_scopes.is_empty(), "{name}");
        assert_eq!(operation.input_schema["type"], "object", "{name}");
        assert!(!operation.description.is_empty(), "{name}");
    }

    // Nothing that changes a workspace is sent as a GET, which the transport
    // always repeats after a server error. The two reads Notion offers only
    // as POST are the only reads that are not a GET.
    for case in cases() {
        let effect = find(&format!("notion.{}", case.name)).effect;
        let posted_read = matches!(case.name, "search.run" | "databases.query");
        match effect {
            Effect::Read if posted_read => assert_eq!(case.verb, "POST", "{}", case.name),
            Effect::Read => assert_eq!(case.verb, "GET", "{}", case.name),
            _ => assert_ne!(case.verb, "GET", "{}", case.name),
        }
    }

    let create = find("notion.pages.create");
    assert_eq!(
        create.input_schema["required"],
        json!(["parent"]),
        "a page needs only its parent"
    );
    for field in ["parent", "properties", "children", "icon", "cover"] {
        assert!(
            create.input_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    for field in ["id", "properties", "parent", "in_trash", "url"] {
        assert!(
            create.output_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    let read = find("notion.pages.read");
    assert_eq!(read.input_schema["required"], json!(["page"]));
    for field in ["markdown", "truncated", "truncation", "requests"] {
        assert!(
            read.output_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
}

#[tokio::test]
async fn a_field_the_operation_does_not_know_is_refused_and_nothing_is_sent() {
    let (server, socket, key) = answering(200, page()).await;
    for (name, input, field) in [
        ("pages.get", json!({ "page": PAGE, "pagee": 1 }), "`pagee`"),
        // The mistake that would matter most: a trash flag slipped into a change.
        ("pages.update", json!({ "page": PAGE, "in_trash": true }), "`in_trash`"),
        (
            "search.run",
            json!({ "filter": { "object": "page" } }),
            "`filter.object`",
        ),
        (
            "blocks.append",
            json!({ "block": BLOCK, "children": [], "after": BLOCK }),
            "`after`",
        ),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
        assert!(err.message().contains(field), "{name}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn what_is_not_an_id_is_refused_and_never_reaches_a_path() {
    let (server, socket, key) = answering(200, page()).await;
    for (name, input) in [
        ("pages.get", json!({ "page": "roadmap" })),
        ("pages.archive", json!({ "page": "" })),
        ("blocks.delete", json!({ "block": "../users" })),
        (
            "blocks.children",
            json!({ "block": format!("{BLOCK}/children?page_size=1") }),
        ),
        ("databases.get", json!({ "database": "0123abcd" })),
        (
            "databases.query",
            json!({ "data_source": "zzzzabcd456789abcdef0123456789ab" }),
        ),
        ("users.get", json!({ "user": "me" })),
        ("comments.list", json!({ "block": "page" })),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
        assert!(err.message().contains("is a UUID"), "{name}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_page_size_notion_does_not_take_is_refused() {
    let (server, socket, key) = answering(200, list("user", json!([]), None)).await;
    for (name, input) in [
        ("users.list", json!({ "limit": 0 })),
        ("users.list", json!({ "limit": 101 })),
        ("search.run", json!({ "limit": 101 })),
        ("databases.query", json!({ "data_source": SOURCE, "limit": 0 })),
        (
            "pages.property",
            json!({ "page": PAGE, "property": "Tasks", "limit": 500 }),
        ),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
        assert_eq!(err.message(), "`limit` is from 1 to 100", "{name}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_cursor_is_passed_on_only_while_notion_says_there_is_more() {
    // Notion may leave a cursor in the last page; `has_more` is what counts.
    let last = json!({ "object": "list", "results": [user()], "next_cursor": "cur-9", "has_more": false });
    let (_server, socket, key) = answering(200, last).await;
    let users = invoke(&socket, &key, "users.list", json!({})).await.unwrap();
    assert_eq!(users["next_cursor"], json!(null));
    assert_eq!(users["items"][0]["id"], USER);

    // With nothing asked for, nothing is added to the request.
    let (server, socket, key) = answering(200, list("user", json!([]), None)).await;
    invoke(&socket, &key, "users.list", json!({ "cursor": "  " }))
        .await
        .unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({}));
    let (server, socket, key) = answering(200, list("page_or_data_source", json!([]), None)).await;
    invoke(&socket, &key, "search.run", json!({})).await.unwrap();
    assert_eq!(body_of(&only_request(&server).await), json!({}));
}

#[tokio::test]
async fn a_success_that_is_not_what_was_asked_for_is_an_error() {
    for (name, input, body) in [
        ("pages.get", json!({ "page": PAGE }), json!({})),
        (
            "pages.get",
            json!({ "page": PAGE }),
            json!({ "object": "error", "code": "unauthorized", "message": "no" }),
        ),
        ("pages.get", json!({ "page": PAGE }), json!({ "object": "page" })),
        ("blocks.get", json!({ "block": BLOCK }), page()),
        ("databases.get", json!({ "database": DATABASE }), data_source()),
        ("users.list", json!({}), json!({ "results": "none" })),
        ("users.list", json!({}), user()),
        (
            "comments.create",
            json!({ "discussion_id": THREAD, "markdown": "ok" }),
            json!(null),
        ),
    ] {
        let (_server, socket, key) = answering(200, body.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} accepted {body}");
    }
}

#[tokio::test]
async fn a_value_of_the_wrong_shape_is_reported_by_where_it_is_and_not_by_what_it_says() {
    let mut odd = page();
    odd["properties"]["Name"]["id"] = json!({ "secret": "launch codes" });
    let (_server, socket, key) = answering(200, odd).await;
    let err = invoke(&socket, &key, "pages.get", json!({ "page": PAGE }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert_eq!(
        err.message(),
        "notion sent a page that could not be read, at `properties.Name.id`"
    );
    assert!(!format!("{err:?}").contains("launch codes"));
}

#[tokio::test]
async fn a_kind_of_object_notion_adds_later_is_passed_on_as_other() {
    let found = list(
        "page_or_data_source",
        json!([{ "object": "view", "id": "v-1" }, { "object": "page", "id": PAGE }]),
        None,
    );
    let (_server, socket, key) = answering(200, found).await;
    let results = invoke(&socket, &key, "search.run", json!({ "query": "x" }))
        .await
        .unwrap();
    assert_eq!(results["items"][0], json!({ "object": "other" }));
    assert_eq!(results["items"][1]["id"], PAGE);
}

#[tokio::test]
async fn a_property_with_one_value_is_returned_as_one_item() {
    let item = json!({ "object": "property_item", "id": "%3EfC", "type": "status", "status": { "name": "Done" } });
    let (server, socket, key) = answering(200, item).await;
    // The id is given back to Notion as the page gave it, already encoded.
    let input = json!({ "page": PAGE, "property": "%3EfC" });
    let property = invoke(&socket, &key, "pages.property", input).await.unwrap();
    assert_eq!(property["id"], "%3EfC");
    assert_eq!(property["type"], "status");
    assert_eq!(property["next_cursor"], json!(null));
    assert_eq!(property["items"].as_array().unwrap().len(), 1);
    assert_eq!(property["items"][0]["status"]["name"], "Done");
    assert_eq!(
        only_request(&server).await.url.path(),
        format!("/v1/pages/{PAGE}/properties/%3EfC")
    );
}

#[tokio::test]
async fn a_property_name_is_one_segment_of_the_path_whatever_it_contains() {
    for (property, segment) in [
        ("Due / Start", "Due%20%2F%20Start"),
        ("Done?", "Done%3F"),
        ("100% sure", "100%25%20sure"),
        ("Zeit für #1", "Zeit%20f%C3%BCr%20%231"),
        ("../../users", "..%2F..%2Fusers"),
    ] {
        let item = json!({ "object": "property_item", "id": "x", "type": "number", "number": 1 });
        let (server, socket, key) = answering(200, item).await;
        let input = json!({ "page": PAGE, "property": property });
        invoke(&socket, &key, "pages.property", input).await.unwrap();
        let request = only_request(&server).await;
        assert_eq!(
            request.url.path(),
            format!("/v1/pages/{PAGE}/properties/{segment}"),
            "{property}"
        );
        assert_eq!(request.url.query(), None, "{property}");
    }
    let (server, socket, key) = answering(200, json!({})).await;
    for property in ["", "  ", ".", ".."] {
        let input = json!({ "page": PAGE, "property": property });
        let err = invoke(&socket, &key, "pages.property", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{property:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_change_to_a_block_cannot_trash_it_or_touch_two_kinds() {
    let (server, socket, key) = answering(200, to_do()).await;
    for (content, says) in [
        // `blocks.update` is a write a host may let through; trashing is not.
        (json!({ "in_trash": true }), "blocks.delete"),
        (json!({ "archived": true }), "blocks.delete"),
        (
            json!({ "to_do": { "checked": true }, "in_trash": true }),
            "one kind of block",
        ),
        (json!({}), "one kind of block"),
        (json!({ "to_do": true }), "is an object"),
    ] {
        let input = json!({ "block": BLOCK, "content": content });
        let err = invoke(&socket, &key, "blocks.update", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{content}");
        assert!(err.message().contains(says), "{content}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn blocks_are_added_a_hundred_at_most_and_at_a_place_that_is_whole() {
    let paragraph = json!({ "paragraph": { "rich_text": [] } });
    let (server, socket, key) = answering(200, list("block", json!([]), None)).await;
    for (input, says) in [
        (json!({ "block": PAGE, "children": [] }), "from 1 to 100"),
        (
            json!({ "block": PAGE, "children": vec![paragraph.clone(); 101] }),
            "from 1 to 100",
        ),
        (
            json!({ "block": PAGE, "children": [paragraph.clone()], "position": { "type": "after_block" } }),
            "after_block.id",
        ),
        (
            json!({ "block": PAGE, "children": [paragraph.clone()], "position": { "type": "start", "after_block": { "id": BLOCK } } }),
            "`after_block` goes with",
        ),
    ] {
        let err = invoke(&socket, &key, "blocks.append", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.message().contains(says), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // A hundred is allowed, and the start of the page is a place.
    let input = json!({ "block": PAGE, "children": vec![paragraph.clone(); 100], "position": { "type": "start" } });
    invoke(&socket, &key, "blocks.append", input).await.unwrap();
    let sent = body_of(&only_request(&server).await);
    assert_eq!(sent["children"].as_array().unwrap().len(), 100);
    assert_eq!(sent["position"], json!({ "type": "start" }));
}

#[tokio::test]
async fn a_page_is_created_somewhere_and_changed_in_something() {
    let (server, socket, key) = answering(200, page()).await;
    let paragraph = json!({ "paragraph": { "rich_text": [] } });
    for (name, input, says) in [
        ("pages.create", json!({ "parent": {} }), "needs a `parent`"),
        (
            "pages.create",
            json!({ "parent": { "block_id": BLOCK } }),
            "needs a `parent`",
        ),
        (
            "pages.create",
            json!({ "parent": { "page_id": PAGE }, "children": vec![paragraph; 101] }),
            "at most 100 blocks",
        ),
        ("pages.update", json!({ "page": PAGE }), "nothing to change"),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
        assert!(err.message().contains(says), "{name}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // A page with only a parent is a page: nothing else is made up for it.
    let input = json!({ "parent": { "page_id": PAGE } });
    invoke(&socket, &key, "pages.create", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "parent": { "page_id": PAGE } })
    );
}

#[tokio::test]
async fn a_comment_goes_on_one_thing_and_says_something_once() {
    let (server, socket, key) = answering(200, comment()).await;
    let text = json!([{ "text": { "content": "Looks good" } }]);
    for (input, says) in [
        (json!({ "rich_text": text.clone() }), "goes on one thing"),
        (
            json!({ "parent": { "page_id": PAGE }, "discussion_id": THREAD, "markdown": "ok" }),
            "goes on one thing",
        ),
        (
            json!({ "parent": { "page_id": PAGE, "block_id": BLOCK }, "markdown": "ok" }),
            "goes on one thing",
        ),
        (json!({ "parent": { "page_id": PAGE } }), "says something once"),
        (
            json!({ "parent": { "page_id": PAGE }, "markdown": "  " }),
            "says something once",
        ),
        (
            json!({ "parent": { "page_id": PAGE }, "rich_text": [] }),
            "says something once",
        ),
        (
            json!({ "parent": { "page_id": PAGE }, "rich_text": text.clone(), "markdown": "ok" }),
            "says something once",
        ),
    ] {
        let err = invoke(&socket, &key, "comments.create", input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}");
        assert!(err.message().contains(says), "{input}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // A reply names its thread, and Markdown is sent as it was written.
    let input = json!({ "discussion_id": THREAD, "markdown": "**Agreed**, ship it." });
    invoke(&socket, &key, "comments.create", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "discussion_id": THREAD, "markdown": "**Agreed**, ship it." })
    );
}

#[tokio::test]
async fn the_typed_methods_send_what_the_operations_send() {
    let (server, socket, key) = notion().await;
    Mock::given(method("POST"))
        .and(path("/v1/search"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list("page_or_data_source", json!([page()]), None)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/blocks/{PAGE}/children")))
        .respond_with(ResponseTemplate::new(200).set_body_json(list("block", json!([to_do()]), None)))
        .mount(&server)
        .await;
    // The connection knows where the server is; the methods only need the version.
    let notion = Notion::new();
    let connection = socket.connection(key).await.unwrap();

    let search = SearchQuery {
        query: Some("roadmap".into()),
        filter: Some(SearchFilter {
            value: Some(SearchObject::DataSource),
            in_trash: Some(true),
        }),
        ..SearchQuery::default()
    };
    let found = notion.search(&connection).run(search).await.unwrap();
    assert_eq!(found.items.len(), 1);
    let blocks = notion
        .blocks(&connection)
        .children(PAGE, Paging::default())
        .await
        .unwrap();
    assert_eq!(blocks.items[0].id, BLOCK);
    assert_eq!(blocks.items[0].kind.as_deref(), Some("to_do"));

    let received = server.received_requests().await.unwrap();
    assert_eq!(
        body_of(&received[0]),
        json!({ "query": "roadmap", "filter": { "property": "object", "value": "data_source", "in_trash": true } })
    );
    // Plain text needs nothing but its content.
    assert_eq!(
        serde_json::to_value(RichText::plain("Hello")).unwrap(),
        json!({ "type": "text", "text": { "content": "Hello" } })
    );
}
