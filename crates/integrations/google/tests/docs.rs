//! Google Docs against a local server that answers as Google does.
//!
//! What every operation sends and returns is in the table in `operations.rs`,
//! and how a document's structure becomes text is tested beside that code, in
//! `src/models/document_text.rs`. This file holds the rest of what is
//! particular to Docs.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::ErrorKind;
use socketkit_google::models::{DocsAppendText, DocsCreateDocument};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::{any, method};
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::docs::{
    BUDGET_TEXT, DOCUMENT, DOCUMENT_FIELDS, PLAN_TEXT, REVISION, created_document, document, document_text,
    document_updated, document_with_content, paragraph, status_error, tab_content,
};
use support::{answer, answering, body_of, google, invoke, only_request, query_of};

// ── Reading ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn getting_a_document_asks_for_its_tabs_and_for_nothing_written_in_them() {
    // Three tabs inside one another, as deep as Google's answer goes here.
    let deep = json!({
        "documentId": DOCUMENT, "title": "Handbook", "revisionId": REVISION,
        "tabs": [
            { "tabProperties": { "tabId": "t.0", "title": "Start", "index": 0 }, "childTabs": [
                { "tabProperties": { "tabId": "t.a", "title": "People", "parentTabId": "t.0", "index": 0, "nestingLevel": 1 }, "childTabs": [
                    { "tabProperties": { "tabId": "t.b", "title": "Leave", "parentTabId": "t.a", "index": 0, "nestingLevel": 2, "iconEmoji": "🌴" } }
                ] },
                { "tabProperties": { "tabId": "t.c", "title": "Money", "parentTabId": "t.0", "index": 1, "nestingLevel": 1 } }
            ] },
            { "tabProperties": { "tabId": "t.d", "title": "End", "index": 1 } }
        ]
    });
    let (server, socket, key) = answering(200, deep).await;
    let found = invoke(&socket, &key, "docs_documents.get", json!({ "document": DOCUMENT }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    // Google fills in the tabs only when asked to; the mask keeps their text out.
    assert_eq!(
        query_of(&request),
        json!({ "includeTabsContent": "true", "fields": DOCUMENT_FIELDS })
    );
    let fields = DOCUMENT_FIELDS;
    assert!(
        !fields.contains("documentTab") && !fields.contains("body"),
        "what is written in a tab is megabytes, and is not asked for"
    );

    // Every tab, a child straight after its parent, each with where it stands.
    let tabs: Vec<_> = found["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tab| {
            (
                tab["tabId"].as_str().unwrap(),
                tab["title"].as_str().unwrap(),
                tab["nestingLevel"].as_i64().unwrap(),
                tab["index"].as_i64().unwrap(),
                tab["parentTabId"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        tabs,
        [
            ("t.0", "Start", 0, 0, None),
            ("t.a", "People", 1, 0, Some("t.0")),
            ("t.b", "Leave", 2, 0, Some("t.a")),
            ("t.c", "Money", 1, 1, Some("t.0")),
            ("t.d", "End", 0, 1, None),
        ]
    );
    assert_eq!(found["tabs"][2]["iconEmoji"], "🌴");
    assert_eq!(found.get("text"), None, "a document's text is `read`'s to return");
}

#[tokio::test]
async fn a_document_is_read_whole_as_it_stands_without_what_is_only_suggested() {
    let (server, socket, key) = answering(200, document_with_content()).await;
    let read = invoke(&socket, &key, "docs_documents.read", json!({ "document": DOCUMENT }))
        .await
        .unwrap();
    // Google's own default shows suggestions to an account that may edit and
    // hides them from one that may only read. Asking says which, for both.
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "includeTabsContent": "true", "suggestionsViewMode": "PREVIEW_WITHOUT_SUGGESTIONS" })
    );
    // Each tab's text is returned once. The whole document joined as one
    // text is for a caller in Rust, and would only repeat it here.
    assert_eq!(read.get("text"), None);
    let tabs: Vec<_> = read["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tab| (tab["title"].as_str().unwrap(), tab["text"].as_str().unwrap()))
        .collect();
    assert_eq!(tabs, [("Plan", PLAN_TEXT), ("Budget", BUDGET_TEXT), ("Notes", "")]);
    // What Google sent beside the words is not passed on.
    let written = read.to_string();
    for layout in [
        "documentStyle",
        "namedStyles",
        "paragraphStyle",
        "startIndex",
        "sectionBreak",
    ] {
        assert!(!written.contains(layout), "{layout}");
    }
}

#[tokio::test]
async fn a_document_of_one_tab_reads_as_that_tab_and_a_blank_one_as_no_text() {
    let one = |blocks: Vec<Value>| {
        json!({
            "documentId": DOCUMENT, "title": "Notes", "revisionId": REVISION,
            "tabs": [{ "tabProperties": { "tabId": "t.0", "title": "Tab 1", "index": 0 }, "documentTab": tab_content(blocks) }]
        })
    };
    let (_server, socket, key) = answering(
        200,
        one(vec![
            paragraph("Minutes", "HEADING_2"),
            paragraph("We agreed to ship.", "NORMAL_TEXT"),
        ]),
    )
    .await;
    let read = invoke(&socket, &key, "docs_documents.read", json!({ "document": DOCUMENT }))
        .await
        .unwrap();
    assert_eq!(read["tabs"][0]["text"], "## Minutes\nWe agreed to ship.");
    assert_eq!(read["tabs"].as_array().unwrap().len(), 1);

    // What `documents.create` leaves behind: one paragraph that is only its own end.
    let (_server, socket, key) = answering(200, one(vec![paragraph("", "NORMAL_TEXT")])).await;
    let blank = invoke(&socket, &key, "docs_documents.read", json!({ "document": DOCUMENT }))
        .await
        .unwrap();
    assert_eq!(blank["tabs"][0]["text"], "");
    assert_eq!(blank["tabs"].as_array().unwrap().len(), 1);

    // An answer in the shape Google used before tabs: the body at the top.
    let mut legacy = tab_content(vec![paragraph("Written before tabs.", "NORMAL_TEXT")]);
    legacy["documentId"] = json!(DOCUMENT);
    legacy["title"] = json!("Old notes");
    let (_server, socket, key) = answering(200, legacy).await;
    let old = invoke(&socket, &key, "docs_documents.read", json!({ "document": DOCUMENT }))
        .await
        .unwrap();
    assert_eq!(
        old["tabs"],
        json!([{ "tabId": "", "title": "", "parentTabId": null, "index": 0, "nestingLevel": 0, "iconEmoji": null, "text": "Written before tabs." }])
    );
}

#[tokio::test]
async fn a_document_id_stays_one_segment_of_the_path_whatever_it_contains() {
    for (id, sent) in [
        (DOCUMENT, format!("/v1/documents/{DOCUMENT}")),
        ("  doc-1  ", "/v1/documents/doc-1".to_owned()),
        ("a/b", "/v1/documents/a%2Fb".to_owned()),
        (
            "../../drive/v3/files",
            "/v1/documents/..%2F..%2Fdrive%2Fv3%2Ffiles".to_owned(),
        ),
        ("doc?fields=*", "/v1/documents/doc%3Ffields%3D%2A".to_owned()),
        ("doc#tab=t.0", "/v1/documents/doc%23tab%3Dt.0".to_owned()),
        ("doc:batchUpdate", "/v1/documents/doc%3AbatchUpdate".to_owned()),
    ] {
        for name in ["docs_documents.get", "docs_documents.read"] {
            let (server, socket, key) = answering(200, document()).await;
            invoke(&socket, &key, name, json!({ "document": id })).await.unwrap();
            let request = only_request(&server).await;
            assert_eq!(request.url.path(), sent, "{name} {id}");
            assert_eq!(request.url.fragment(), None, "{name} {id}");
            assert!(
                query_of(&request).get("fields").is_none_or(|fields| fields != "*"),
                "{name} {id}"
            );
        }
        let (server, socket, key) = answering(200, document_updated()).await;
        let input = json!({ "document": id, "text": "x" });
        invoke(&socket, &key, "docs_documents.append_text", input)
            .await
            .unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), format!("{sent}:batchUpdate"), "{id}");
        assert_eq!(request.method.as_str(), "POST");
    }
}

// ── Writing ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn text_is_added_exactly_as_given_at_the_end_of_the_first_tab_or_of_the_one_named() {
    let insert = |text: &str, end: Value| json!({ "requests": [{ "insertText": { "text": text, "endOfSegmentLocation": end } }] });
    for (input, sent) in [
        // No tab: the end of the body of the first one. The location is
        // still named, or Google would not know where the text goes.
        (json!({ "text": "Agreed." }), insert("Agreed.", json!({}))),
        (
            json!({ "text": "Agreed.", "tabId": null }),
            insert("Agreed.", json!({})),
        ),
        (
            json!({ "text": "Agreed.", "tabId": " t.8f2k " }),
            insert("Agreed.", json!({ "tabId": "t.8f2k" })),
        ),
        // Newlines are where paragraphs begin, and they are the caller's to
        // place: nothing is added, taken away or trimmed.
        (json!({ "text": "\n" }), insert("\n", json!({}))),
        (
            json!({ "text": "\n## Décision\n\n  – ship on the 12th 🚀\n" }),
            insert("\n## Décision\n\n  – ship on the 12th 🚀\n", json!({})),
        ),
        (json!({ "text": "  " }), insert("  ", json!({}))),
    ] {
        let (server, socket, key) = answering(200, document_updated()).await;
        let mut input = input;
        input["document"] = json!(DOCUMENT);
        let updated = invoke(&socket, &key, "docs_documents.append_text", input.clone())
            .await
            .unwrap_or_else(|e| panic!("{input}: {e}"));
        let request = only_request(&server).await;
        assert_eq!(request.method.as_str(), "POST");
        assert_eq!(query_of(&request), json!({}));
        assert_eq!(body_of(&request), sent, "{input}");
        assert_eq!(updated["writeControl"]["requiredRevisionId"], REVISION);
    }
}

#[tokio::test]
async fn a_tab_that_is_named_and_blank_is_refused_and_not_taken_for_the_first() {
    // The text would go into a tab the caller did not mean.
    let (server, socket, key) = google().await;
    for tab in ["", "  "] {
        let input = json!({ "document": DOCUMENT, "text": "Agreed.", "tabId": tab });
        let err = invoke(&socket, &key, "docs_documents.append_text", input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.message().contains("`tabId`"), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_new_document_is_sent_its_title_and_nothing_else() {
    let (server, socket, key) = answering(200, created_document()).await;
    let created = invoke(
        &socket,
        &key,
        "docs_documents.create",
        json!({ "title": " Q3 plan: draft " }),
    )
    .await
    .unwrap();
    let request = only_request(&server).await;
    assert_eq!((request.method.as_str(), request.url.path()), ("POST", "/v1/documents"));
    assert_eq!(
        body_of(&request),
        json!({ "title": " Q3 plan: draft " }),
        "the title as it was given"
    );
    assert_eq!(created["documentId"], DOCUMENT);

    // Google ignores content given to `create`, so no field for it is offered.
    let (server, socket, key) = google().await;
    for input in [
        json!({ "title": "Q3 plan", "body": { "content": [] } }),
        json!({ "title": "Q3 plan", "text": "First line" }),
    ] {
        let err = invoke(&socket, &key, "docs_documents.create", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.message().contains("is not a field of this operation"), "{err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn what_could_only_fail_is_refused_before_google_is_called() {
    let (server, socket, key) = google().await;
    for (name, input, says) in [
        (
            "docs_documents.get",
            json!({ "document": "" }),
            "a document id is required",
        ),
        (
            "docs_documents.get",
            json!({ "document": "   " }),
            "a document id is required",
        ),
        (
            "docs_documents.read",
            json!({ "document": "" }),
            "a document id is required",
        ),
        // A segment of dots would be resolved away and address something else.
        (
            "docs_documents.get",
            json!({ "document": ".." }),
            "a document id is not valid",
        ),
        (
            "docs_documents.read",
            json!({ "document": "." }),
            "a document id is not valid",
        ),
        (
            "docs_documents.append_text",
            json!({ "document": "..", "text": "x" }),
            "a document id is not valid",
        ),
        (
            "docs_documents.append_text",
            json!({ "document": "", "text": "x" }),
            "a document id is required",
        ),
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": "" }),
            "`text` is required",
        ),
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT }),
            "missing field `text`",
        ),
        ("docs_documents.create", json!({ "title": "" }), "`title` is required"),
        (
            "docs_documents.create",
            json!({ "title": " \n " }),
            "`title` is required",
        ),
        ("docs_documents.create", json!({}), "missing field `title`"),
        ("docs_documents.get", json!({}), "missing field `document`"),
        // A tab named under another word would be dropped, and the text
        // would go to the first tab without a word said.
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": "x", "tab": "t.8f2k" }),
            "`tab` is not a field of this operation; check its spelling",
        ),
        (
            "docs_documents.append_text",
            json!({ "document": 7, "text": "x" }),
            "`document` has the wrong type",
        ),
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": ["x"] }),
            "the input has a field of the wrong type",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert_eq!(err.message(), says, "{name} {input}");
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "none of these reached Google"
    );
}

#[tokio::test]
async fn a_document_is_created_and_added_to_once_when_google_fails() {
    for (name, input) in [
        ("docs_documents.create", json!({ "title": "Q3 plan" })),
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": "Agreed." }),
        ),
    ] {
        let (server, socket, key) = google().await;
        Mock::given(any())
            .respond_with(answer(
                503,
                &status_error(503, "UNAVAILABLE", "The service is currently unavailable."),
            ))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have been done, and doing it again would write it twice"
        );
    }
}

// ── What Google answers ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_success_without_the_document_is_an_error() {
    for (name, input, response) in [
        ("docs_documents.get", json!({ "document": DOCUMENT }), json!({})),
        (
            "docs_documents.get",
            json!({ "document": DOCUMENT }),
            json!({ "documentId": "", "title": "Plan" }),
        ),
        (
            "docs_documents.get",
            json!({ "document": DOCUMENT }),
            json!({ "tabs": [{ "tabProperties": { "tabId": "t.0" } }] }),
        ),
        ("docs_documents.read", json!({ "document": DOCUMENT }), json!({})),
        (
            "docs_documents.read",
            json!({ "document": DOCUMENT }),
            json!({ "title": "Plan", "tabs": [] }),
        ),
        (
            "docs_documents.read",
            json!({ "document": DOCUMENT }),
            json!({ "body": { "content": [paragraph("Words", "NORMAL_TEXT")] } }),
        ),
        (
            "docs_documents.create",
            json!({ "title": "Plan" }),
            json!({ "title": "Plan" }),
        ),
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": "x" }),
            json!({}),
        ),
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": "x" }),
            json!({ "replies": [{}] }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
        assert_eq!(err.message(), "google answered without a document", "{name} {response}");
    }
    // Nothing at all where a document was asked for.
    let (_server, socket, key) = answering(204, Value::Null).await;
    let err = invoke(&socket, &key, "docs_documents.read", json!({ "document": DOCUMENT }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode, "{err}");
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_place_and_never_what_was_written_there() {
    let secret = "CONFIDENTIAL: the offer to Grace is 140k";
    let within = |element: Value| {
        json!({
            "documentId": DOCUMENT, "title": secret,
            "tabs": [{ "tabProperties": { "tabId": "t.0", "title": secret }, "documentTab": { "body": { "content": [
                paragraph(secret, "NORMAL_TEXT"),
                element
            ] } } }]
        })
    };
    for (name, response, place) in [
        (
            "docs_documents.read",
            within(json!({ "paragraph": { "elements": [{ "textRun": { "content": { "text": secret } } }] } })),
            "tabs[0].documentTab.body.content[1].paragraph.elements[0].textRun.content",
        ),
        (
            "docs_documents.read",
            within(json!({ "table": { "tableRows": [{ "tableCells": [{ "content": secret }] }] } })),
            "tabs[0].documentTab.body.content[1].table.tableRows[0].tableCells[0].content",
        ),
        (
            "docs_documents.read",
            within(json!({ "paragraph": { "bullet": { "listId": [secret] } } })),
            "tabs[0].documentTab.body.content[1].paragraph.bullet.listId",
        ),
        (
            "docs_documents.read",
            json!({ "documentId": DOCUMENT, "title": [secret] }),
            "title",
        ),
        (
            "docs_documents.get",
            json!({ "documentId": DOCUMENT, "title": secret, "tabs": [{ "tabProperties": { "tabId": "t.0", "title": { "is": secret } } }] }),
            "tabs[0].tabProperties.title",
        ),
        (
            "docs_documents.get",
            json!({ "documentId": DOCUMENT, "tabs": [{ "childTabs": [{ "tabProperties": { "index": secret } }] }] }),
            "tabs[0].childTabs[0].tabProperties.index",
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        let err = invoke(&socket, &key, name, json!({ "document": DOCUMENT }))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {place}: {err}");
        assert_eq!(
            err.message(),
            format!("google sent a document that could not be read, at `{place}`"),
            "{name}"
        );
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name} {place}: {everything}");
        assert!(!everything.contains("140k"), "{name} {place}: {everything}");
    }
}

#[tokio::test]
async fn googles_refusals_of_a_document_reach_the_caller() {
    for (name, input, status, word, message, kind, ends) in [
        (
            "docs_documents.read",
            json!({ "document": DOCUMENT }),
            404,
            "NOT_FOUND",
            "Requested entity was not found.",
            ErrorKind::NotFound,
            "has no such resource",
        ),
        // A document the account may not open, or a token without the scope.
        (
            "docs_documents.get",
            json!({ "document": DOCUMENT }),
            403,
            "PERMISSION_DENIED",
            "The caller does not have permission",
            ErrorKind::AccessDenied,
            "The caller does not have permission",
        ),
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": "x" }),
            403,
            "PERMISSION_DENIED",
            "Request had insufficient authentication scopes.",
            ErrorKind::AccessDenied,
            "Request had insufficient authentication scopes.",
        ),
        // A tab the document does not have is Google's to explain.
        (
            "docs_documents.append_text",
            json!({ "document": DOCUMENT, "text": "x", "tabId": "t.nope" }),
            400,
            "INVALID_ARGUMENT",
            "Invalid requests[0].insertText: The tab ID t.nope does not exist.",
            ErrorKind::InvalidInput,
            "Invalid requests[0].insertText: The tab ID t.nope does not exist.",
        ),
    ] {
        let (server, socket, key) = google().await;
        Mock::given(any())
            .respond_with(answer(status, &status_error(status, word, message)))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{name}: {err}");
        assert!(err.message().ends_with(ends), "{name}: {}", err.message());
    }
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let google = Google::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(google.clone()), "ya29.good").await;
    let connection = socket.connection(key).await.unwrap();
    let documents = google.docs_documents(&connection);

    // Both reads go to the same address; what they ask for is the difference.
    Mock::given(method("GET"))
        .respond_with(answer(200, &document()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let found = documents.get(DOCUMENT).await.unwrap();
    assert_eq!(
        (found.document_id.as_str(), found.title.as_str()),
        (DOCUMENT, "Q3 plan")
    );
    assert_eq!(found.revision_id.as_deref(), Some(REVISION));
    let budget = &found.tabs[1];
    assert_eq!((budget.tab_id.as_str(), budget.title.as_str()), ("t.8f2k", "Budget"));
    assert_eq!(
        (budget.nesting_level, budget.parent_tab_id.as_deref()),
        (1, Some("t.0"))
    );

    Mock::given(method("GET"))
        .respond_with(answer(200, &document_with_content()))
        .mount(&server)
        .await;
    let read = documents.read(DOCUMENT).await.unwrap();
    assert_eq!(read.text, document_text());
    assert_eq!(read.tabs[1].tab, *budget, "the same tab, now with its text");
    assert_eq!(read.tabs[1].text, BUDGET_TEXT);

    Mock::given(method("POST"))
        .respond_with(answer(200, &created_document()))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let created = documents
        .create(DocsCreateDocument {
            title: "Q3 plan".into(),
        })
        .await
        .unwrap();
    assert_eq!(created.document_id, DOCUMENT);
    assert_eq!(created.tabs[0].tab_id, "t.0");

    Mock::given(method("POST"))
        .respond_with(answer(200, &document_updated()))
        .mount(&server)
        .await;
    let updated = documents
        .append_text(
            &created.document_id,
            DocsAppendText {
                text: "\nDecision: open in Paris first.".into(),
                tab_id: Some(budget.tab_id.clone()),
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.document_id, DOCUMENT);
    assert_eq!(
        updated
            .write_control
            .and_then(|control| control.required_revision_id)
            .as_deref(),
        Some(REVISION)
    );

    let sent: Vec<(String, String, Value, Value)> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| {
            (
                request.method.to_string(),
                request.url.path().to_owned(),
                query_of(request),
                body_of(request),
            )
        })
        .collect();
    let path = format!("/v1/documents/{DOCUMENT}");
    assert_eq!(
        sent,
        [
            (
                "GET".to_owned(),
                path.clone(),
                json!({ "includeTabsContent": "true", "fields": DOCUMENT_FIELDS }),
                Value::Null
            ),
            (
                "GET".to_owned(),
                path.clone(),
                json!({ "includeTabsContent": "true", "suggestionsViewMode": "PREVIEW_WITHOUT_SUGGESTIONS" }),
                Value::Null
            ),
            (
                "POST".to_owned(),
                "/v1/documents".to_owned(),
                json!({ "fields": DOCUMENT_FIELDS }),
                json!({ "title": "Q3 plan" })
            ),
            (
                "POST".to_owned(),
                format!("{path}:batchUpdate"),
                json!({}),
                json!({ "requests": [{ "insertText": {
                    "text": "\nDecision: open in Paris first.",
                    "endOfSegmentLocation": { "tabId": "t.8f2k" }
                } }] })
            ),
        ]
    );
}
