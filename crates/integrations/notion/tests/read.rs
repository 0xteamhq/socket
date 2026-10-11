//! `pages.read`: a page's tree of blocks read whole and written as Markdown.

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, ErrorKind, Socket};
use socketkit_testkit::wiremock::matchers::{method, path, query_param, query_param_is_missing};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};

mod support;
use support::{PAGE, VERSION, block, invoke, list, notion, notion_error, page, query_of, said};

/// The id of the `n`th block of a test page.
fn id(n: u32) -> String {
    format!("b10c0000-0000-4000-8000-{n:012}")
}

/// A block that holds text and nothing nested.
fn text(n: u32, kind: &str, words: &str) -> Value {
    block(&id(n), kind, json!({ "rich_text": said(words) }), false)
}

/// The same, with blocks nested inside it.
fn parent(n: u32, kind: &str, words: &str) -> Value {
    block(&id(n), kind, json!({ "rich_text": said(words) }), true)
}

/// One run of text with some formatting.
fn run(words: &str, marks: &[&str], href: Option<&str>) -> Value {
    let set = |mark: &str| marks.contains(&mark);
    json!({
        "type": "text", "text": { "content": words, "link": href.map(|url| json!({ "url": url })) }, "plain_text": words, "href": href,
        "annotations": { "bold": set("bold"), "italic": set("italic"), "strikethrough": set("strikethrough"), "underline": set("underline"), "code": set("code"), "color": "default" }
    })
}

/// A server that has the page, with `top` as the blocks directly inside it.
async fn a_page_with(top: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = notion().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/pages/{PAGE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;
    inside(&server, PAGE, top).await;
    (server, socket, key)
}

/// Puts `blocks` inside the block or page `of`.
async fn inside(server: &MockServer, of: &str, blocks: Value) {
    Mock::given(method("GET"))
        .and(path(format!("/v1/blocks/{of}/children")))
        .and(query_param_is_missing("start_cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list("block", blocks, None)))
        .mount(server)
        .await;
}

async fn read(socket: &Socket, key: &ConnectionKey, options: Value) -> socketkit_core::Result<Value> {
    let mut input = json!({ "page": PAGE });
    input
        .as_object_mut()
        .unwrap()
        .extend(options.as_object().unwrap().clone());
    invoke(socket, key, "pages.read", input).await
}

/// The paths the server was asked for, in order, below `/v1`.
async fn asked(server: &MockServer) -> Vec<String> {
    let received = server.received_requests().await.unwrap();
    received
        .iter()
        .map(|request| request.url.path().trim_start_matches("/v1").to_owned())
        .collect()
}

#[tokio::test]
async fn a_page_is_written_as_markdown_with_every_block_accounted_for() {
    let sentence = json!([
        run("We ship on ", &[], None),
        run("Friday", &["bold"], None),
        run(", see ", &[], None),
        run("the spec", &[], Some("https://example.test/spec")),
        run(" and ", &[], None),
        run("cargo test", &["code"], None),
        run(".", &[], None),
    ]);
    let top = json!([
        text(1, "heading_1", "Launch plan"),
        block(
            &id(2),
            "paragraph",
            json!({ "rich_text": sentence, "color": "default" }),
            false
        ),
        parent(3, "bulleted_list_item", "First"),
        text(4, "bulleted_list_item", "Second"),
        text(5, "numbered_list_item", "One"),
        parent(6, "numbered_list_item", "Two"),
        block(
            &id(7),
            "to_do",
            json!({ "rich_text": said("Write tests"), "checked": false }),
            false
        ),
        block(
            &id(8),
            "to_do",
            json!({ "rich_text": said("Write code"), "checked": true }),
            false
        ),
        parent(9, "toggle", "Details"),
        parent(10, "quote", "Measure twice"),
        block(
            &id(11),
            "callout",
            json!({ "rich_text": said("Remember the cap"), "icon": { "type": "emoji", "emoji": "💡" } }),
            false
        ),
        block(
            &id(12),
            "code",
            json!({ "rich_text": said("fn main() {}"), "language": "rust", "caption": said("The entry point") }),
            false
        ),
        block(&id(13), "divider", json!({}), false),
        block(
            &id(14),
            "table",
            json!({ "table_width": 2, "has_column_header": true, "has_row_header": false }),
            true
        ),
        // A page inside the page has blocks of its own. They are that page's, and are not read.
        block(&id(15), "child_page", json!({ "title": "Meeting notes" }), true),
        block(
            &id(16),
            "link_to_page",
            json!({ "type": "page_id", "page_id": PAGE }),
            false
        ),
        block(
            &id(17),
            "image",
            json!({ "type": "external", "external": { "url": "https://example.test/arch.png" }, "caption": said("Architecture") }),
            false
        ),
        // A file Notion keeps has an address that expires, so it is named and not linked.
        block(
            &id(18),
            "file",
            json!({ "type": "file", "file": { "url": "https://files.example.test/signed?X-Amz-Signature=abc", "expiry_time": "2026-10-09T09:15:00.000Z" }, "name": "plan.pdf", "caption": [] }),
            false
        ),
        block(
            &id(19),
            "bookmark",
            json!({ "url": "https://example.test/post", "caption": [] }),
            false
        ),
        block(&id(20), "table_of_contents", json!({ "color": "default" }), false),
        block(&id(21), "unsupported", json!({ "block_type": "ai_block" }), false),
        text(22, "heading_3", "Risks"),
        block(&id(23), "equation", json!({ "expression": "E = mc^2" }), false),
        // A kind Notion adds after this was written still leaves a line, with its text.
        text(24, "whiteboard", "Sketch"),
    ]);
    let (server, socket, key) = a_page_with(top).await;
    inside(&server, &id(3), json!([text(31, "bulleted_list_item", "Nested")])).await;
    inside(&server, &id(6), json!([text(61, "numbered_list_item", "Two a")])).await;
    inside(&server, &id(9), json!([text(91, "paragraph", "Hidden text")])).await;
    inside(&server, &id(10), json!([text(101, "paragraph", "cut once")])).await;
    let row = |n: u32, cells: [&str; 2]| {
        block(
            &id(n),
            "table_row",
            json!({ "cells": [said(cells[0]), said(cells[1])] }),
            false,
        )
    };
    inside(
        &server,
        &id(14),
        json!([row(141, ["Name", "Owner"]), row(142, ["API", "Ada | Grace"])]),
    )
    .await;

    let content = read(&socket, &key, json!({})).await.unwrap();
    let expected = [
        "# Launch plan",
        "",
        "We ship on **Friday**, see [the spec](https://example.test/spec) and `cargo test`.",
        "",
        "- First",
        "  - Nested",
        "- Second",
        "1. One",
        "2. Two",
        "   1. Two a",
        "- [ ] Write tests",
        "- [x] Write code",
        "- Details",
        "",
        "  Hidden text",
        "",
        "> Measure twice",
        ">",
        "> cut once",
        "",
        "> 💡 Remember the cap",
        "",
        "```rust",
        "fn main() {}",
        "```",
        "The entry point",
        "",
        "---",
        "",
        "| Name | Owner |",
        "| --- | --- |",
        "| API | Ada \\| Grace |",
        "",
        "[Meeting notes](https://www.notion.so/b10c0000000040008000000000000015)",
        "",
        "[Linked page](https://www.notion.so/0123abcd456789abcdef0123456789ab)",
        "",
        "![Architecture](https://example.test/arch.png)",
        "",
        "[file: plan.pdf]",
        "",
        "[https://example.test/post](https://example.test/post)",
        "",
        "[table_of_contents]",
        "",
        "[unsupported: ai_block]",
        "",
        "### Risks",
        "",
        "$$ E = mc^2 $$",
        "",
        "[whiteboard] Sketch",
    ];
    let markdown = content["markdown"].as_str().unwrap();
    assert_eq!(markdown.lines().collect::<Vec<_>>(), expected, "\n{markdown}");
    assert_eq!(content["id"], PAGE);
    assert_eq!(content["title"], "Roadmap");
    assert_eq!(
        content["url"],
        "https://www.notion.so/Roadmap-0123abcd456789abcdef0123456789ab"
    );
    assert_eq!(content["truncated"], false);
    assert_eq!(content["truncation"], json!(null));
    assert_eq!(content["blocks"], 30, "24 at the top and 6 nested");
    // The page, its blocks, and one list for each of the five blocks with others inside.
    assert_eq!(content["requests"], 7);
    assert!(
        !markdown.contains("X-Amz-Signature"),
        "an address that expires is not passed on"
    );

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 7);
    for request in &received {
        assert_eq!(request.method.as_str(), "GET", "reading a page changes nothing");
        assert_eq!(request.headers.get("notion-version").unwrap(), VERSION);
    }
    // Every list is asked for a hundred at a time, the most Notion gives.
    assert_eq!(query_of(&received[1]), json!({ "page_size": "100" }));
}

#[tokio::test]
async fn the_top_of_the_page_is_read_whole_before_what_is_nested_in_it() {
    let (server, socket, key) = notion().await;
    Mock::given(method("GET"))
        .and(path(format!("/v1/pages/{PAGE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;
    // The page's own list comes in two parts.
    let children = format!("/v1/blocks/{PAGE}/children");
    Mock::given(path(children.clone()))
        .and(query_param_is_missing("start_cursor"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list(
            "block",
            json!([parent(1, "toggle", "A")]),
            Some("cur-2"),
        )))
        .mount(&server)
        .await;
    Mock::given(path(children))
        .and(query_param("start_cursor", "cur-2"))
        .respond_with(ResponseTemplate::new(200).set_body_json(list("block", json!([parent(2, "toggle", "B")]), None)))
        .mount(&server)
        .await;
    inside(&server, &id(1), json!([parent(11, "bulleted_list_item", "A1")])).await;
    inside(&server, &id(2), json!([text(21, "bulleted_list_item", "B1")])).await;
    inside(&server, &id(11), json!([text(111, "bulleted_list_item", "A1a")])).await;

    let content = read(&socket, &key, json!({})).await.unwrap();
    assert_eq!(
        content["markdown"], "- A\n  - A1\n    - A1a\n- B\n  - B1",
        "written in the page's order, however it was read"
    );
    assert_eq!(content["truncated"], false);
    assert_eq!(
        asked(&server).await,
        [
            format!("/pages/{PAGE}"),
            format!("/blocks/{PAGE}/children"),
            format!("/blocks/{PAGE}/children"),
            format!("/blocks/{}/children", id(1)),
            format!("/blocks/{}/children", id(2)),
            format!("/blocks/{}/children", id(11)),
        ],
        "a level at a time"
    );
}

#[tokio::test]
async fn the_depth_limit_leaves_a_mark_where_it_stopped_and_says_so() {
    let (server, socket, key) = a_page_with(json!([
        parent(1, "bulleted_list_item", "A"),
        text(2, "paragraph", "End")
    ]))
    .await;
    inside(&server, &id(1), json!([parent(11, "bulleted_list_item", "A1")])).await;
    inside(&server, &id(11), json!([text(111, "bulleted_list_item", "A1a")])).await;

    let content = read(&socket, &key, json!({ "max_depth": 2 })).await.unwrap();
    assert_eq!(
        content["markdown"],
        "- A\n  - A1\n\n    [not read: blocks nested deeper than the depth limit]\n\nEnd"
    );
    assert_eq!(content["truncated"], true);
    assert_eq!(content["truncation"], "blocks nested more than 2 deep were not read");
    assert_eq!(content["requests"], 3, "the third level was not asked for");
    assert_eq!(content["blocks"], 3);

    // One level is only the page's own blocks.
    let (_server, socket, key) = a_page_with(json!([parent(1, "toggle", "A")])).await;
    let content = read(&socket, &key, json!({ "max_depth": 1 })).await.unwrap();
    assert_eq!(content["requests"], 2);
    assert_eq!(content["truncated"], true);

    // Deep enough, and nothing is cut.
    let (server, socket, key) = a_page_with(json!([parent(1, "bulleted_list_item", "A")])).await;
    inside(&server, &id(1), json!([text(11, "bulleted_list_item", "A1")])).await;
    let content = read(&socket, &key, json!({ "max_depth": 2 })).await.unwrap();
    assert_eq!(content["truncated"], false);
    assert_eq!(content["markdown"], "- A\n  - A1");
}

#[tokio::test]
async fn the_request_limit_stops_the_reading_and_marks_every_place_left_unread() {
    let top = json!([
        parent(1, "toggle", "A"),
        parent(2, "toggle", "B"),
        text(3, "paragraph", "End")
    ]);
    let (server, socket, key) = a_page_with(top).await;
    inside(&server, &id(1), json!([text(11, "paragraph", "Inside A")])).await;
    inside(&server, &id(2), json!([text(21, "paragraph", "Inside B")])).await;

    // The page, its blocks, and what is inside A: three requests, and B is left.
    let content = read(&socket, &key, json!({ "max_requests": 3 })).await.unwrap();
    assert_eq!(
        content["markdown"],
        "- A\n\n  Inside A\n- B\n\n  [not read: more blocks, the request limit was reached]\n\nEnd"
    );
    assert_eq!(content["truncated"], true);
    assert_eq!(content["truncation"], "reading stopped after 3 requests");
    assert_eq!(content["requests"], 3);
    assert_eq!(server.received_requests().await.unwrap().len(), 3);

    // With one request only the page itself is read, and that is said too.
    let (server, socket, key) = a_page_with(json!([text(1, "paragraph", "Never read")])).await;
    let content = read(&socket, &key, json!({ "max_requests": 1 })).await.unwrap();
    assert_eq!(
        content["markdown"],
        "[not read: more blocks, the request limit was reached]"
    );
    assert_eq!(content["truncated"], true);
    assert_eq!(content["blocks"], 0);
    assert_eq!(asked(&server).await, [format!("/pages/{PAGE}")]);
}

#[tokio::test]
async fn a_list_that_never_ends_is_stopped_by_the_request_limit() {
    let (server, socket, key) = notion().await;
    Mock::given(path(format!("/v1/pages/{PAGE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;
    // Every part of the list says there is another.
    Mock::given(path(format!("/v1/blocks/{PAGE}/children")))
        .respond_with(ResponseTemplate::new(200).set_body_json(list(
            "block",
            json!([text(1, "paragraph", "Again")]),
            Some("cur-1"),
        )))
        .mount(&server)
        .await;
    let content = read(&socket, &key, json!({ "max_requests": 5 })).await.unwrap();
    assert_eq!(content["requests"], 5);
    assert_eq!(content["blocks"], 4);
    assert_eq!(content["truncated"], true);
    assert!(
        content["markdown"]
            .as_str()
            .unwrap()
            .ends_with("Again\n\n[not read: more blocks, the request limit was reached]")
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 5);
}

#[tokio::test]
async fn limits_out_of_range_are_refused_before_anything_is_asked() {
    let (server, socket, key) = a_page_with(json!([])).await;
    for (options, says) in [
        (json!({ "max_depth": 0 }), "`max_depth` is from 1 to 50"),
        (json!({ "max_depth": 51 }), "`max_depth` is from 1 to 50"),
        (json!({ "max_requests": 0 }), "`max_requests` is from 1 to 500"),
        (json!({ "max_requests": 501 }), "`max_requests` is from 1 to 500"),
    ] {
        let err = read(&socket, &key, options).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert_eq!(err.message(), says);
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    // The largest that are allowed are allowed, and an empty page is empty.
    let content = read(&socket, &key, json!({ "max_depth": 50, "max_requests": 500 }))
        .await
        .unwrap();
    assert_eq!(content["markdown"], "");
    assert_eq!(content["truncated"], false);
}

#[tokio::test]
async fn a_nested_block_notion_will_not_open_is_marked_and_the_rest_is_still_read() {
    let top = json!([
        block(
            &id(1),
            "synced_block",
            json!({ "synced_from": { "type": "block_id", "block_id": id(99) } }),
            true
        ),
        text(2, "paragraph", "After")
    ]);
    let (server, socket, key) = a_page_with(top).await;
    Mock::given(path(format!("/v1/blocks/{}/children", id(1))))
        .respond_with(notion_error(404, "object_not_found", "Could not find block"))
        .mount(&server)
        .await;
    let content = read(&socket, &key, json!({})).await.unwrap();
    assert_eq!(
        content["markdown"],
        "[not read: Notion did not give the blocks inside this one]\n\nAfter"
    );
    assert_eq!(content["truncated"], true);
    assert_eq!(
        content["truncation"],
        "Notion did not give the blocks inside one or more blocks"
    );
}

#[tokio::test]
async fn a_page_that_cannot_be_read_at_all_is_an_error_and_so_is_a_throttled_one() {
    // The page itself is not shared.
    let (server, socket, key) = notion().await;
    Mock::given(path(format!("/v1/pages/{PAGE}")))
        .respond_with(notion_error(404, "object_not_found", "Could not find page"))
        .mount(&server)
        .await;
    let err = read(&socket, &key, json!({})).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert!(err.message().ends_with("or it was not shared with this integration"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    // The page is there and its own blocks are refused: there is nothing to return.
    let (server, socket, key) = notion().await;
    Mock::given(path(format!("/v1/pages/{PAGE}")))
        .respond_with(ResponseTemplate::new(200).set_body_json(page()))
        .mount(&server)
        .await;
    Mock::given(path(format!("/v1/blocks/{PAGE}/children")))
        .respond_with(notion_error(403, "restricted_resource", "Insufficient permissions"))
        .mount(&server)
        .await;
    assert_eq!(
        read(&socket, &key, json!({})).await.unwrap_err().kind(),
        ErrorKind::AccessDenied
    );

    // Notion asks for a wait longer than the transport will sit through:
    // half a page is not passed off as the page.
    let (server, socket, key) = a_page_with(json!([parent(1, "toggle", "A")])).await;
    Mock::given(path(format!("/v1/blocks/{}/children", id(1))))
        .respond_with(notion_error(429, "rate_limited", "Slow down").insert_header("retry-after", "60"))
        .mount(&server)
        .await;
    assert_eq!(
        read(&socket, &key, json!({})).await.unwrap_err().kind(),
        ErrorKind::RateLimited
    );
}

#[tokio::test]
async fn text_keeps_its_formatting_where_markdown_has_a_mark_for_it() {
    let marked = json!([
        // Marks go around the words, not around the spaces beside them.
        run("bold ", &["bold"], None),
        run("and", &[], None),
        run(" both", &["bold", "italic"], None),
        run(" gone", &["strikethrough"], None),
        run(" under", &["underline"], None),
        run(" ", &["bold"], None),
        run("linked code", &["code"], Some("https://example.test/a")),
    ]);
    let mention = json!([
        { "type": "mention", "mention": { "type": "page", "page": { "id": PAGE } }, "plain_text": "Roadmap", "href": "https://www.notion.so/0123abcd456789abcdef0123456789ab",
          "annotations": { "bold": false, "italic": false, "strikethrough": false, "underline": false, "code": false, "color": "default" } },
        { "type": "equation", "equation": { "expression": "x^2" }, "plain_text": "x^2", "href": null },
    ]);
    let top = json!([
        block(&id(1), "paragraph", json!({ "rich_text": marked }), false),
        block(&id(2), "paragraph", json!({ "rich_text": mention }), false),
        // A paragraph with line breaks keeps them, in a list item too.
        text(3, "paragraph", "one\ntwo"),
        text(4, "bulleted_list_item", "first line\nsecond line"),
        text(5, "quote", "said\nagain"),
        // An empty paragraph is a gap on the page, not content.
        block(&id(6), "paragraph", json!({ "rich_text": [] }), false),
        // Code is written as it is, without marks, behind a fence it cannot close.
        block(
            &id(7),
            "code",
            json!({ "rich_text": [run("a ```` b", &["bold"], None), run("\nc", &[], None)], "language": "plain text", "caption": [] }),
            false
        ),
        text(8, "heading_2", "Two\nlines"),
        text(9, "heading_4", "Small"),
    ]);
    let (_server, socket, key) = a_page_with(top).await;
    let content = read(&socket, &key, json!({})).await.unwrap();
    let expected = [
        "**bold** and ***both*** ~~gone~~ under [`linked code`](https://example.test/a)",
        "",
        "[Roadmap](https://www.notion.so/0123abcd456789abcdef0123456789ab)$x^2$",
        "",
        "one",
        "two",
        "",
        "- first line",
        "  second line",
        "",
        "> said",
        "> again",
        "",
        "`````",
        "a ```` b",
        "c",
        "`````",
        "",
        "## Two lines",
        "",
        "#### Small",
    ];
    let markdown = content["markdown"].as_str().unwrap();
    assert_eq!(markdown.lines().collect::<Vec<_>>(), expected, "\n{markdown}");
}

#[tokio::test]
async fn lists_tables_and_layout_blocks_are_written_as_they_nest() {
    let cells = |n: u32, cells: Value| block(&id(n), "table_row", json!({ "cells": cells }), false);
    let top = json!([
        // A numbered list starts where Notion says, and starts again after anything else.
        block(
            &id(1),
            "numbered_list_item",
            json!({ "rich_text": said("Third"), "list_start_index": 3 }),
            false
        ),
        text(2, "numbered_list_item", "Fourth"),
        text(3, "paragraph", "Between"),
        text(4, "numbered_list_item", "First again"),
        // A table without a heading row gets an empty one, and a short row is filled out.
        block(
            &id(5),
            "table",
            json!({ "table_width": 3, "has_column_header": false }),
            true
        ),
        // A table whose rows could not be listed is still named.
        block(
            &id(6),
            "table",
            json!({ "table_width": 2, "has_column_header": true }),
            false
        ),
        // Columns only arrange what is inside them.
        block(&id(7), "column_list", json!({}), true),
        // A to-do with a note under it, and a quote inside a list item.
        block(
            &id(8),
            "to_do",
            json!({ "rich_text": said("Review"), "checked": false }),
            true
        ),
        // A callout with something inside it.
        block(
            &id(9),
            "callout",
            json!({ "rich_text": said("Careful"), "icon": { "type": "external", "external": { "url": "https://example.test/i.png" } } }),
            true
        ),
        // Media by kind: a video kept elsewhere is linked, one without a caption is named by its kind.
        block(
            &id(10),
            "video",
            json!({ "type": "external", "external": { "url": "https://example.test/v.mp4" }, "caption": [] }),
            false
        ),
        block(
            &id(11),
            "pdf",
            json!({ "type": "file", "file": { "url": "https://files.example.test/signed" }, "caption": [] }),
            false
        ),
        block(&id(12), "child_database", json!({ "title": "" }), false),
        block(
            &id(13),
            "embed",
            json!({ "url": "https://example.test/embed", "caption": said("A demo") }),
            false
        ),
        // A block Notion returned without saying what it is.
        json!({ "object": "block", "id": id(14) }),
    ]);
    let (server, socket, key) = a_page_with(top).await;
    inside(
        &server,
        &id(5),
        json!([
            cells(51, json!([said("a"), said("b\nc"), said("d")])),
            cells(52, json!([said("e")]))
        ]),
    )
    .await;
    inside(
        &server,
        &id(7),
        json!([
            block(&id(71), "column", json!({}), true),
            block(&id(72), "column", json!({}), true)
        ]),
    )
    .await;
    inside(&server, &id(71), json!([text(711, "paragraph", "Left")])).await;
    inside(&server, &id(72), json!([text(721, "paragraph", "Right")])).await;
    inside(
        &server,
        &id(8),
        json!([
            text(81, "quote", "by Friday"),
            text(82, "bulleted_list_item", "and tested")
        ]),
    )
    .await;
    inside(&server, &id(9), json!([text(91, "bulleted_list_item", "Really")])).await;

    let content = read(&socket, &key, json!({})).await.unwrap();
    let expected = [
        "3. Third",
        "4. Fourth",
        "",
        "Between",
        "",
        "1. First again",
        "",
        "|  |  |  |",
        "| --- | --- | --- |",
        "| a | b<br>c | d |",
        "| e |  |  |",
        "",
        "[table]",
        "",
        "Left",
        "",
        "Right",
        "",
        "- [ ] Review",
        "",
        "  > by Friday",
        "",
        "  - and tested",
        "",
        "> Careful",
        ">",
        "> - Really",
        "",
        "[video](https://example.test/v.mp4)",
        "",
        "[pdf]",
        "",
        "[Untitled](https://www.notion.so/b10c0000000040008000000000000012)",
        "",
        "[A demo](https://example.test/embed)",
        "",
        "[unknown]",
    ];
    let markdown = content["markdown"].as_str().unwrap();
    assert_eq!(markdown.lines().collect::<Vec<_>>(), expected, "\n{markdown}");
    assert_eq!(content["truncated"], false);
}
