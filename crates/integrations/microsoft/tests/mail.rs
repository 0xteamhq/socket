//! Outlook mail against a local server that answers as Microsoft Graph does.
//!
//! What every operation sends and returns is in the table in `operations.rs`.
//! This file holds what is particular to mail.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::ErrorKind;
use socketkit_microsoft::models::{
    BodyType, DraftMessage, GetMessage, ItemBody, ListMessages, Recipient, ReplyContent, SendMail, UpdateMessage,
};
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    AS_HTML, AS_TEXT, answer, answering, body_of, graph_error, invoke, message, microsoft, only_request, prefer,
    query_of, to,
};

fn text(content: &str) -> Value {
    json!({ "contentType": "text", "content": content })
}

// ── Reading ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_list_reads_the_whole_mailbox_or_one_folder() {
    let (server, socket, key) = answering(200, json!({ "value": [message()] })).await;
    invoke(&socket, &key, "mail.list", json!({})).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/v1.0/me/messages");
    assert_eq!(query_of(&request), json!({}), "Graph's own defaults apply");

    // A filter, a sort or a search with nothing in it is none at all.
    let (server, socket, key) = answering(200, json!({ "value": [] })).await;
    let blank = json!({ "filter": "  ", "orderBy": " ", "search": "" });
    invoke(&socket, &key, "mail.list", blank).await.unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({}));

    for (folder, sent) in [
        ("sentitems", "/v1.0/me/mailFolders/sentitems/messages"),
        ("AAMkAGI2=", "/v1.0/me/mailFolders/AAMkAGI2%3D/messages"),
        (
            "inbox/childFolders",
            "/v1.0/me/mailFolders/inbox%2FchildFolders/messages",
        ),
    ] {
        let (server, socket, key) = answering(200, json!({ "value": [] })).await;
        invoke(&socket, &key, "mail.list", json!({ "folder": folder }))
            .await
            .unwrap();
        assert_eq!(only_request(&server).await.url.path(), sent, "{folder}");
    }
}

#[tokio::test]
async fn a_search_is_sent_as_one_quoted_phrase_whatever_it_contains() {
    for (search, sent) in [
        ("pizza", "\"pizza\""),
        ("from:grace subject:plan", "\"from:grace subject:plan\""),
        // A quote in the search would end the phrase early and start another parameter's worth of syntax.
        (
            "subject:\"q3 plan\" OR from:ada",
            "\"subject:\\\"q3 plan\\\" OR from:ada\"",
        ),
        ("a\\b", "\"a\\\\b\""),
        ("x\"&$filter=isRead eq false", "\"x\\\"&$filter=isRead eq false\""),
    ] {
        let (server, socket, key) = answering(200, json!({ "value": [] })).await;
        invoke(&socket, &key, "mail.list", json!({ "search": search }))
            .await
            .unwrap();
        let request = only_request(&server).await;
        assert_eq!(query_of(&request), json!({ "$search": sent }), "{search}");
    }
}

#[tokio::test]
async fn a_conversation_id_cannot_widen_the_filter_it_is_put_in() {
    for (conversation, sent) in [
        (
            "AAQkAGI2=",
            "receivedDateTime ge 1900-01-01T00:00:00Z and conversationId eq 'AAQkAGI2='",
        ),
        (
            "it's",
            "receivedDateTime ge 1900-01-01T00:00:00Z and conversationId eq 'it''s'",
        ),
        // Every quote is doubled, so the whole of it stays inside the one string.
        (
            "x' or isRead eq false or 'a' eq 'a",
            "receivedDateTime ge 1900-01-01T00:00:00Z and conversationId eq 'x'' or isRead eq false or ''a'' eq ''a'",
        ),
    ] {
        let (server, socket, key) = answering(200, json!({ "value": [] })).await;
        invoke(
            &socket,
            &key,
            "mail.conversation",
            json!({ "conversation": conversation, "limit": 20 }),
        )
        .await
        .unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), "/v1.0/me/messages");
        assert_eq!(
            query_of(&request),
            json!({ "$filter": sent, "$orderby": "receivedDateTime asc", "$top": "20" }),
            "{conversation}"
        );
    }
}

#[tokio::test]
async fn a_body_comes_as_plain_text_unless_html_is_asked_for() {
    for (input, expected) in [
        (json!({ "message": "msg-1" }), AS_TEXT),
        (json!({ "message": "msg-1", "bodyType": "text" }), AS_TEXT),
        (json!({ "message": "msg-1", "bodyType": "html" }), AS_HTML),
    ] {
        let (server, socket, key) = answering(200, message()).await;
        invoke(&socket, &key, "mail.get", input.clone()).await.unwrap();
        assert_eq!(prefer(&only_request(&server).await), Some(expected), "{input}");
    }
    let (server, socket, key) = answering(200, json!({ "value": [message()] })).await;
    invoke(&socket, &key, "mail.list", json!({ "bodyType": "html" }))
        .await
        .unwrap();
    assert_eq!(prefer(&only_request(&server).await), Some(AS_HTML));

    // A format that is neither is refused, never passed into the header.
    let (server, socket, key) = answering(200, message()).await;
    for bad in ["markdown", "TEXT", "text\", outlook.allow-unsafe-html", ""] {
        let err = invoke(
            &socket,
            &key,
            "mail.get",
            json!({ "message": "msg-1", "bodyType": bad }),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{bad:?}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn the_next_page_of_a_list_keeps_its_search_and_its_body_format() {
    let (server, socket, key) = microsoft().await;
    // Graph writes a folder's messages with the folder as a key in brackets.
    let next = format!(
        "{}/v1.0/me/mailFolders('inbox')/messages?%24search=%22pizza%22&%24skiptoken=abc",
        server.uri()
    );
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [message()], "@odata.nextLink": next })))
        .mount(&server)
        .await;
    let first = invoke(
        &socket,
        &key,
        "mail.list",
        json!({ "folder": "inbox", "search": "pizza" }),
    )
    .await
    .unwrap();
    assert_eq!(first["next_cursor"], next);

    server.reset().await;
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [message()] })))
        .mount(&server)
        .await;
    let input = json!({ "folder": "inbox", "search": "pizza", "bodyType": "html", "cursor": next });
    let second = invoke(&socket, &key, "mail.list", input).await.unwrap();
    assert_eq!(second["next_cursor"], json!(null));
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/v1.0/me/mailFolders/inbox/messages");
    assert_eq!(
        request.url.query(),
        Some("%24search=%22pizza%22&%24skiptoken=abc"),
        "Graph's place in the search, and the search, once"
    );
    assert_eq!(prefer(&request), Some(AS_HTML));
}

#[tokio::test]
async fn folders_are_listed_at_the_top_or_under_a_parent() {
    let folders = json!({ "value": [{ "id": "f-1", "displayName": "Projects" }] });
    for (input, path, query) in [
        (json!({}), "/v1.0/me/mailFolders", json!({})),
        (
            json!({ "includeHidden": true }),
            "/v1.0/me/mailFolders",
            json!({ "includeHiddenFolders": "true" }),
        ),
        // Graph leaves hidden folders out unless asked, so "no" is not sent.
        (json!({ "includeHidden": false }), "/v1.0/me/mailFolders", json!({})),
        (
            json!({ "parent": "inbox", "includeHidden": true, "limit": 25 }),
            "/v1.0/me/mailFolders/inbox/childFolders",
            json!({ "includeHiddenFolders": "true", "$top": "25" }),
        ),
    ] {
        let (server, socket, key) = answering(200, folders.clone()).await;
        let page = invoke(&socket, &key, "mail_folders.list", input.clone()).await.unwrap();
        assert_eq!(page["items"][0]["displayName"], "Projects");
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), path, "{input}");
        assert_eq!(query_of(&request), query, "{input}");
    }
}

#[tokio::test]
async fn what_graph_leaves_empty_does_not_stop_mail_from_being_read() {
    for sparse in [
        // A draft with nothing in it yet.
        json!({
            "id": "msg-3", "subject": null, "bodyPreview": null, "body": null, "from": null, "sender": null,
            "toRecipients": null, "ccRecipients": null, "bccRecipients": null, "replyTo": null,
            "receivedDateTime": null, "sentDateTime": null, "isRead": null, "isDraft": null, "hasAttachments": null,
            "categories": null, "flag": null, "importance": null, "conversationId": null, "webLink": null
        }),
        json!({ "id": "msg-3" }),
        json!({ "id": "msg-3", "from": { "emailAddress": null }, "toRecipients": [{ "emailAddress": { "name": null, "address": null } }], "flag": { "flagStatus": null } }),
    ] {
        let (_server, socket, key) = answering(200, sparse.clone()).await;
        let read = invoke(&socket, &key, "mail.get", json!({ "message": "msg-3" }))
            .await
            .unwrap_or_else(|e| panic!("{sparse}: {e}"));
        assert_eq!(read["id"], "msg-3");
        assert_eq!(read["subject"], "");
        assert_eq!(read["isRead"], false);
        assert_eq!(read["hasAttachments"], false);
    }

    // An attachment that is another item, or a link to a file, has no content of its own.
    let item = json!({ "value": [
        { "@odata.type": "#microsoft.graph.itemAttachment", "id": "att-2", "name": "Fwd: plan", "contentType": null, "size": 4096, "isInline": false },
        { "@odata.type": "microsoft.graph.referenceAttachment", "id": "att-3", "name": null, "size": null, "isInline": null }
    ] });
    let (_server, socket, key) = answering(200, item).await;
    let page = invoke(&socket, &key, "mail.attachments_list", json!({ "message": "msg-1" }))
        .await
        .unwrap();
    assert_eq!(page["items"][0]["@odata.type"], "#microsoft.graph.itemAttachment");
    assert_eq!(page["items"][1]["name"], "");
    assert!(
        page["items"][1].get("contentBytes").is_none(),
        "a file is never part of a description"
    );
}

#[tokio::test]
async fn a_success_without_the_message_is_an_error_and_never_repeats_what_graph_sent() {
    let secret = "CONFIDENTIAL offer letter for Grace";
    for (name, input, response) in [
        ("mail.get", json!({ "message": "msg-1" }), json!({})),
        (
            "mail.get",
            json!({ "message": "msg-1" }),
            json!({ "id": "", "subject": secret }),
        ),
        (
            "mail.get",
            json!({ "message": "msg-1" }),
            json!({ "id": "msg-1", "subject": secret, "from": secret }),
        ),
        (
            "mail.list",
            json!({}),
            json!({ "value": [{ "id": "msg-1", "bodyPreview": secret, "toRecipients": secret }] }),
        ),
        ("mail.list", json!({}), json!({ "values": [{ "subject": secret }] })),
        (
            "mail.create_draft",
            json!({ "subject": "x" }),
            json!({ "subject": secret }),
        ),
        (
            "mail.move_to",
            json!({ "message": "msg-1", "folder": "archive" }),
            json!({ "subject": secret }),
        ),
        (
            "mail.attachment_get",
            json!({ "message": "msg-1", "attachment": "att-1" }),
            json!({ "name": secret }),
        ),
        (
            "mail_folders.get",
            json!({ "folder": "inbox" }),
            json!({ "displayName": secret }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
    }
}

// ── Writing and sending ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_reply_carries_a_comment_or_a_body_and_never_both() {
    // Graph refuses both together; with neither it makes an empty reply to fill in.
    let (server, socket, key) = answering(201, json!({ "id": "msg-9", "isDraft": true })).await;
    invoke(&socket, &key, "mail.create_reply", json!({ "message": "msg-1" }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), "/v1.0/me/messages/msg-1/createReply");
    assert_eq!(body_of(&request), json!({}));

    let (server, socket, key) = answering(202, json!(null)).await;
    let input = json!({ "message": "msg-1", "body": text("Agreed."), "ccRecipients": to("alan@contoso.example") });
    invoke(&socket, &key, "mail.reply", input).await.unwrap();
    assert_eq!(
        body_of(&only_request(&server).await),
        json!({ "message": { "body": text("Agreed."), "ccRecipients": to("alan@contoso.example") } })
    );

    let (server, socket, key) = answering(202, json!(null)).await;
    for name in [
        "mail.create_reply",
        "mail.create_reply_all",
        "mail.create_forward",
        "mail.reply",
    ] {
        let input = json!({ "message": "msg-1", "comment": "Agreed.", "body": text("Agreed."), "toRecipients": to("alan@contoso.example") });
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn mail_that_could_not_arrive_or_would_arrive_empty_is_refused_before_graph_is_called() {
    let (server, socket, key) = answering(202, json!(null)).await;
    let bad = [
        // Nobody to send it to.
        ("mail.send", json!({ "subject": "Monday", "body": text("x") })),
        (
            "mail.send",
            json!({ "subject": "Monday", "body": text("x"), "toRecipients": [] }),
        ),
        ("mail.create_forward", json!({ "message": "msg-1", "comment": "FYI" })),
        // Nothing to say. A draft may be empty; what is sent at once may not.
        ("mail.reply", json!({ "message": "msg-1" })),
        (
            "mail.reply",
            json!({ "message": "msg-1", "ccRecipients": to("alan@contoso.example") }),
        ),
        ("mail.reply", json!({ "message": "msg-1", "comment": "  " })),
        ("mail.send", json!({ "toRecipients": to("grace@contoso.example") })),
        (
            "mail.send",
            json!({ "toRecipients": to("grace@contoso.example"), "subject": " ", "importance": "high" }),
        ),
        // A body with nothing in it says nothing either.
        (
            "mail.send",
            json!({ "toRecipients": to("grace@contoso.example"), "body": text(" ") }),
        ),
        ("mail.reply", json!({ "message": "msg-1", "body": text("") })),
        (
            "mail.reply",
            json!({ "message": "msg-1", "comment": " ", "body": { "contentType": "html", "content": "\n" } }),
        ),
        // A search is not filtered or sorted further; asking for both would
        // give results that are not what was asked for.
        ("mail.list", json!({ "search": "pizza", "filter": "isRead eq false" })),
        (
            "mail.list",
            json!({ "search": "pizza", "orderBy": "receivedDateTime desc" }),
        ),
        // A flag that says nothing.
        ("mail.update", json!({ "message": "msg-1", "flag": {} })),
        // A recipient with no address, wherever it is.
        (
            "mail.send",
            json!({ "subject": "x", "toRecipients": [{ "emailAddress": { "name": "Grace" } }] }),
        ),
        (
            "mail.send",
            json!({ "subject": "x", "toRecipients": to("grace@contoso.example"), "ccRecipients": [{}] }),
        ),
        (
            "mail.create_draft",
            json!({ "bccRecipients": [{ "emailAddress": { "address": " " } }] }),
        ),
        (
            "mail.update_draft",
            json!({ "message": "msg-9", "replyTo": [{ "emailAddress": {} }] }),
        ),
        (
            "mail.reply",
            json!({ "message": "msg-1", "comment": "x", "toRecipients": [{ "emailAddress": { "address": "" } }] }),
        ),
        // A body with no content would blank the text.
        ("mail.update_draft", json!({ "message": "msg-9", "body": {} })),
        (
            "mail.send",
            json!({ "toRecipients": to("grace@contoso.example"), "body": { "contentType": "html" } }),
        ),
        // A change that changes nothing.
        ("mail.update_draft", json!({ "message": "msg-9" })),
        ("mail.update", json!({ "message": "msg-1" })),
        ("mail.update", json!({ "message": "msg-1", "isRead": null })),
        // Ids and folders that are not there.
        ("mail.get", json!({ "message": " " })),
        ("mail.send_draft", json!({ "message": "" })),
        ("mail.delete", json!({ "message": ".." })),
        ("mail.move_to", json!({ "message": "msg-1", "folder": "" })),
        ("mail.move_to", json!({ "message": "", "folder": "archive" })),
        ("mail.attachment_get", json!({ "message": "msg-1", "attachment": "." })),
        ("mail.conversation", json!({ "conversation": " " })),
        ("mail.list", json!({ "folder": ".." })),
        ("mail.list", json!({ "limit": 0 })),
        ("mail_folders.get", json!({ "folder": "" })),
        ("mail_folders.list", json!({ "parent": ".." })),
    ];
    for (name, input) in bad {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
    }
    // A missing plain argument is named.
    for (name, input, field) in [
        ("mail.get", json!({}), "message"),
        ("mail.move_to", json!({ "message": "msg-1" }), "folder"),
        ("mail.attachment_get", json!({ "message": "msg-1" }), "attachment"),
        ("mail.conversation", json!({}), "conversation"),
    ] {
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
        assert!(err.message().contains(field), "{name}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn mail_can_go_to_people_in_copy_alone_and_with_only_a_subject_or_only_a_body() {
    for input in [
        json!({ "subject": "FYI", "ccRecipients": to("alan@contoso.example") }),
        json!({ "subject": "FYI", "bccRecipients": to("alan@contoso.example") }),
        json!({ "body": text("See you Monday."), "toRecipients": to("grace@contoso.example") }),
    ] {
        let (server, socket, key) = answering(202, json!(null)).await;
        invoke(&socket, &key, "mail.send", input.clone()).await.unwrap();
        assert_eq!(body_of(&only_request(&server).await), json!({ "message": input }));
    }
}

#[tokio::test]
async fn attachments_and_folders_are_paged_like_every_other_list() {
    let page = json!({ "value": [{ "id": "x-1", "name": "plan.pdf", "displayName": "Projects" }] });
    for (name, input, path, first_query) in [
        (
            "mail.attachments_list",
            json!({ "message": "msg-1" }),
            "/v1.0/me/messages/msg-1/attachments",
            json!({ "$select": "id,name,contentType,size,isInline,lastModifiedDateTime", "$top": "5" }),
        ),
        (
            "mail_folders.list",
            json!({ "parent": "inbox" }),
            "/v1.0/me/mailFolders/inbox/childFolders",
            json!({ "$top": "5" }),
        ),
    ] {
        let (server, socket, key) = answering(200, page.clone()).await;
        let mut first = input.clone();
        first["limit"] = json!(5);
        invoke(&socket, &key, name, first).await.unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), path, "{name}");
        assert_eq!(query_of(&request), first_query, "{name}");

        server.reset().await;
        Mock::given(any()).respond_with(answer(200, &page)).mount(&server).await;
        let mut next = input.clone();
        next["cursor"] = json!(format!("{}{path}?%24skip=5&%24top=5", server.uri()));
        invoke(&socket, &key, name, next).await.unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), path, "{name}");
        assert_eq!(request.url.query(), Some("%24skip=5&%24top=5"), "{name}");
    }
}

#[tokio::test]
async fn sending_a_draft_states_the_length_of_what_it_sends() {
    // Graph wants a length on this request, and a POST with nothing in it
    // goes out without one, which a server may refuse outright. So an empty
    // object is sent: it says nothing, and it has a length.
    let (server, socket, key) = answering(202, json!(null)).await;
    invoke(&socket, &key, "mail.send_draft", json!({ "message": "msg-9" }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(body_of(&request), json!({}));
    assert_eq!(request.headers.get("content-length").unwrap(), "2");
}

#[tokio::test]
async fn mail_is_sent_once_when_graph_fails() {
    let unavailable = || graph_error(503, "serviceNotAvailable", "The service is temporarily unavailable.");
    for (name, input) in [
        (
            "mail.send",
            json!({ "subject": "x", "toRecipients": to("grace@contoso.example") }),
        ),
        ("mail.send_draft", json!({ "message": "msg-9" })),
        ("mail.reply", json!({ "message": "msg-1", "comment": "x" })),
        ("mail.create_draft", json!({ "subject": "x" })),
        ("mail.create_reply", json!({ "message": "msg-1" })),
        ("mail.update_draft", json!({ "message": "msg-9", "subject": "x" })),
        ("mail.update", json!({ "message": "msg-1", "isRead": true })),
        ("mail.move_to", json!({ "message": "msg-1", "folder": "archive" })),
        // `mail.delete` is not in this list. On this branch the transport
        // still repeats a DELETE after a server error; see the guide.
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any()).respond_with(unavailable()).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have been sent, so it is not sent again"
        );
    }
}

#[tokio::test]
async fn graphs_refusals_of_mail_reach_the_caller() {
    for (response, kind, reason) in [
        (
            graph_error(
                404,
                "ErrorItemNotFound",
                "The specified object was not found in the store.",
            ),
            ErrorKind::NotFound,
            "has no such resource",
        ),
        (
            graph_error(
                403,
                "ErrorAccessDenied",
                "Access is denied. Check credentials and try again.",
            ),
            ErrorKind::AccessDenied,
            "Access is denied. Check credentials and try again.",
        ),
        // A sort the filter does not allow is Graph's to explain.
        (
            graph_error(
                400,
                "InefficientFilter",
                "The restriction or sort order is too complex for this operation.",
            ),
            ErrorKind::InvalidInput,
            "The restriction or sort order is too complex for this operation.",
        ),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let input = json!({ "filter": "isRead eq false", "orderBy": "receivedDateTime desc" });
        let err = invoke(&socket, &key, "mail.list", input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().ends_with(reason), "{}", err.message());
    }
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let microsoft = Microsoft::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(microsoft.clone()), "eyJ.good").await;
    let connection = socket.connection(key).await.unwrap();
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [message()] })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    let mail = microsoft.mail(&connection);

    let unread = mail
        .list(ListMessages {
            folder: Some("inbox".into()),
            filter: Some("isRead eq false".into()),
            ..ListMessages::default()
        })
        .await
        .unwrap();
    let first = &unread.items[0];
    assert_eq!(first.id, "msg-1");
    assert_eq!(first.conversation_id.as_deref(), Some("conv-1"));
    assert_eq!(first.subject, "Q3 plan");
    assert_eq!(
        first.from.as_ref().map(|from| from.email_address.address.as_str()),
        Some("grace@contoso.example")
    );
    assert_eq!(first.to_recipients[0].email_address.address, "ada@contoso.example");
    assert_eq!(first.cc_recipients[0].email_address.address, "alan@contoso.example");
    assert_eq!(first.received_date_time.as_deref(), Some("2026-10-09T08:15:00Z"));
    assert_eq!(first.body_preview, "Attached is the plan");
    assert!(!first.is_read);
    assert!(first.has_attachments);
    assert!(first.web_link.is_some());

    server.reset().await;
    Mock::given(any())
        .respond_with(answer(200, &message()))
        .mount(&server)
        .await;
    let read = mail
        .get(
            "msg-1",
            GetMessage {
                body_type: Some(BodyType::Html),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        read.body.and_then(|body| body.content).as_deref(),
        Some("Attached is the plan for Q3.")
    );
    mail.update(
        "msg-1",
        UpdateMessage {
            is_read: Some(true),
            ..UpdateMessage::default()
        },
    )
    .await
    .unwrap();

    server.reset().await;
    Mock::given(any())
        .respond_with(answer(202, &json!(null)))
        .mount(&server)
        .await;
    let draft = DraftMessage {
        subject: Some("Monday".into()),
        body: Some(ItemBody::text("See you Monday.")),
        to_recipients: Some(vec![Recipient::new("grace@contoso.example")]),
        ..DraftMessage::default()
    };
    mail.send(SendMail {
        message: draft,
        save_to_sent_items: None,
    })
    .await
    .unwrap();
    mail.reply(
        "msg-1",
        ReplyContent {
            comment: Some("Agreed.".into()),
            ..ReplyContent::default()
        },
    )
    .await
    .unwrap();
    let sent: Vec<(String, Value)> = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .map(|request| (request.url.path().to_owned(), body_of(request)))
        .collect();
    assert_eq!(
        sent,
        [
            (
                "/v1.0/me/sendMail".to_owned(),
                json!({ "message": { "subject": "Monday", "body": text("See you Monday."), "toRecipients": to("grace@contoso.example") } })
            ),
            (
                "/v1.0/me/messages/msg-1/reply".to_owned(),
                json!({ "comment": "Agreed." })
            ),
        ]
    );
}
