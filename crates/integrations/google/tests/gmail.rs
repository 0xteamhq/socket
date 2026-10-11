//! Gmail against a local server that answers as Google does: finding and
//! reading mail, labels, and what a wrong call or a wrong answer comes to.
//!
//! What every operation sends and returns is in the table in `operations.rs`.
//! Writing and sending mail is in `gmail_compose.rs`.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::ErrorKind;
use socketkit_google::models::{
    GmailAddress, GmailFormat, GmailGetMessage, GmailGetThread, GmailListMessages, GmailModifyMessage, GmailReply,
    GmailSendMessage, Paging,
};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::gmail::{
    GMAIL_ATTACHMENT, GMAIL_DRAFT, GMAIL_DRAFTS, GMAIL_LABELS, GMAIL_MESSAGE, GMAIL_MESSAGES, GMAIL_THREAD,
    GMAIL_THREADS, gmail_data, gmail_draft_ref, gmail_label, gmail_message, gmail_metadata, gmail_ref, gmail_sent,
};
use support::{answer, answering, body_of, google, google_error, invoke, only_request, query_of};

/// One row of each list, as Gmail returns it.
fn rows() -> [(&'static str, &'static str, Value); 3] {
    [
        (
            "gmail_messages.list",
            "messages",
            json!({ "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD }),
        ),
        (
            "gmail_threads.list",
            "threads",
            json!({ "id": GMAIL_THREAD, "snippet": "Plan", "historyId": "9" }),
        ),
        (
            "gmail_drafts.list",
            "drafts",
            json!({ "id": GMAIL_DRAFT, "message": { "id": "18c9", "threadId": "18c9" } }),
        ),
    ]
}

// ── Lists ────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_list_sends_only_what_was_asked_for() {
    for (name, field, row) in rows() {
        // Nothing asked for, and a search or a cursor with nothing in it, are the same.
        for input in [json!({}), json!({ "q": "  ", "cursor": " " })] {
            let (server, socket, key) = answering(200, json!({ field: [row.clone()] })).await;
            invoke(&socket, &key, name, input.clone()).await.unwrap();
            assert_eq!(query_of(&only_request(&server).await), json!({}), "{name} {input}");
        }
    }
    // No labels is no filter. "Leave Spam out" is said, as it was asked for.
    for name in ["gmail_messages.list", "gmail_threads.list"] {
        let (server, socket, key) = answering(200, json!({})).await;
        let input = json!({ "labelIds": [], "includeSpamTrash": false });
        invoke(&socket, &key, name, input).await.unwrap();
        assert_eq!(
            query_of(&only_request(&server).await),
            json!({ "includeSpamTrash": "false" }),
            "{name}"
        );
    }
}

#[tokio::test]
async fn a_list_is_one_request_and_carries_ids_only_however_much_it_finds() {
    let found: Vec<Value> = (0..120)
        .map(|n| json!({ "id": format!("18c1a2b3c4d5{n:04x}"), "threadId": GMAIL_THREAD }))
        .collect();
    let (server, socket, key) = answering(200, json!({ "messages": found, "resultSizeEstimate": 120 })).await;
    let page = invoke(&socket, &key, "gmail_messages.list", json!({ "q": "has:attachment" }))
        .await
        .unwrap();
    assert_eq!(page["items"].as_array().unwrap().len(), 120);
    assert_eq!(
        page["items"][119],
        json!({ "id": "18c1a2b3c4d50077", "threadId": GMAIL_THREAD, "labelIds": [] }),
        "a row is what Gmail sent: no message was read to fill it in"
    );
    only_request(&server).await;
}

#[tokio::test]
async fn every_list_pages_with_googles_token_and_takes_from_1_to_500() {
    for (name, field, row) in rows() {
        let (server, socket, key) = answering(200, json!({ field: [row], "nextPageToken": "0987654321" })).await;
        let input = json!({ "cursor": "1234567890", "limit": 500 });
        let page = invoke(&socket, &key, name, input).await.unwrap();
        assert_eq!(page["next_cursor"], "0987654321", "{name}");
        assert_eq!(
            query_of(&only_request(&server).await),
            json!({ "pageToken": "1234567890", "maxResults": "500" }),
            "{name}"
        );

        // The last page has no token, or an empty one, and Gmail leaves an empty list out.
        for last in [
            json!({ "resultSizeEstimate": 0 }),
            json!({ field: [], "nextPageToken": "" }),
        ] {
            let (_server, socket, key) = answering(200, last.clone()).await;
            let page = invoke(&socket, &key, name, json!({})).await.unwrap();
            assert_eq!(page, json!({ "items": [], "next_cursor": null }), "{name} {last}");
        }

        let (server, socket, key) = answering(200, json!({})).await;
        for limit in [0, 501] {
            let err = invoke(&socket, &key, name, json!({ "limit": limit }))
                .await
                .unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {limit}");
            assert!(err.message().contains("`limit` is from 1 to 500"), "{err}");
        }
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }
}

// ── Reading ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_message_is_read_whole_or_as_its_headers_or_as_its_ids() {
    let minimal = json!({ "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD, "labelIds": ["INBOX"], "snippet": "Attached", "sizeEstimate": 58213 });
    for (format, response) in [
        ("full", gmail_message()),
        ("metadata", gmail_metadata()),
        ("minimal", minimal),
    ] {
        let (server, socket, key) = answering(200, response).await;
        let input = json!({ "message": GMAIL_MESSAGE, "format": format });
        let read = invoke(&socket, &key, "gmail_messages.get", input).await.unwrap();
        assert_eq!(query_of(&only_request(&server).await), json!({ "format": format }));
        assert_eq!(read["id"], GMAIL_MESSAGE, "{format}");
        assert_eq!(
            read["labelIds"].as_array().unwrap().last().unwrap(),
            "INBOX",
            "{format}"
        );
        let (subject, text, files) = match format {
            "full" => (json!("Q3 plan"), json!("Attached is the plan for Q3.\r\n"), 1),
            "metadata" => (json!("Q3 plan"), json!(null), 0),
            _ => (json!(null), json!(null), 0),
        };
        assert_eq!((&read["subject"], &read["text"]), (&subject, &text), "{format}");
        assert_eq!(read["attachments"].as_array().unwrap().len(), files, "{format}");
    }

    // A thread and a draft take the same choice, under the same name.
    let thread = json!({ "id": GMAIL_THREAD, "messages": [gmail_metadata(), gmail_metadata()] });
    let (server, socket, key) = answering(200, thread).await;
    let input = json!({ "thread": GMAIL_THREAD, "format": "metadata" });
    let read = invoke(&socket, &key, "gmail_threads.get", input).await.unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({ "format": "metadata" }));
    assert_eq!(read["messages"][1]["from"], "Grace Hopper <grace@example.test>");
    assert_eq!(read["messages"][1]["html"], json!(null));

    let (server, socket, key) = answering(200, json!({ "id": GMAIL_DRAFT, "message": gmail_metadata() })).await;
    let input = json!({ "draft": GMAIL_DRAFT, "format": "minimal" });
    invoke(&socket, &key, "gmail_drafts.get", input).await.unwrap();
    assert_eq!(query_of(&only_request(&server).await), json!({ "format": "minimal" }));
}

#[tokio::test]
async fn a_message_gmail_built_oddly_is_still_read() {
    // Header names in capitals, a subject in encoded words, a recipient
    // list written twice, a body in Latin-1 without padding, an inline
    // picture, and a part with nothing in it.
    let odd = json!({
        "id": GMAIL_MESSAGE,
        "payload": {
            "mimeType": "multipart/related",
            "headers": [
                { "name": "SUBJECT", "value": "=?ISO-8859-1?Q?Caf=E9?= =?UTF-8?B?IOKYlQ==?=" },
                { "name": "to", "value": "ada@example.test" },
                { "name": "TO", "value": "alan@example.test" },
                { "name": "message-id", "value": "<odd@mail.example.test>" }
            ],
            "parts": [
                { "mimeType": "text/html", "headers": [{ "name": "content-type", "value": "text/html; charset=iso-8859-1" }],
                  "body": { "size": 11, "data": "PGI-Q2Fm6TwvYj4" } },
                { "mimeType": "image/png", "filename": "logo.png",
                  "headers": [{ "name": "Content-ID", "value": "<logo@example.test>" }, { "name": "Content-Disposition", "value": "inline" }],
                  "body": { "attachmentId": "ANGjdJ_logo", "size": 2048 } },
                { "mimeType": "text/plain", "body": { "size": 0 } }
            ]
        }
    });
    let (_server, socket, key) = answering(200, odd).await;
    let read = invoke(&socket, &key, "gmail_messages.get", json!({ "message": GMAIL_MESSAGE }))
        .await
        .unwrap();
    assert_eq!(read["subject"], "Café ☕");
    assert_eq!(read["to"], "ada@example.test, alan@example.test");
    assert_eq!(read["messageId"], "<odd@mail.example.test>");
    assert_eq!(read["html"], "<b>Café</b>");
    assert_eq!(read["text"], json!(null));
    assert_eq!(
        read["attachments"],
        json!([{ "attachmentId": "ANGjdJ_logo", "filename": "logo.png", "mimeType": "image/png", "size": 2048, "inline": true, "contentId": "logo@example.test", "partId": null }])
    );
}

#[tokio::test]
async fn an_attachment_comes_as_gmail_sends_it_and_an_empty_file_is_still_a_file() {
    for (response, returned) in [
        (
            json!({ "attachmentId": GMAIL_ATTACHMENT, "size": 9, "data": gmail_data("%PDF-1.7\n") }),
            json!({ "size": 9, "data": "JVBERi0xLjcK" }),
        ),
        (json!({ "size": 0 }), json!({ "size": 0, "data": "" })),
        (json!({ "size": 0, "data": "" }), json!({ "size": 0, "data": "" })),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let input = json!({ "message": GMAIL_MESSAGE, "attachment": GMAIL_ATTACHMENT });
        let read = invoke(&socket, &key, "gmail_messages.attachment_get", input)
            .await
            .unwrap();
        assert_eq!(read, returned, "{response}");
    }
}

// ── Labels ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_change_of_labels_sends_the_lists_that_name_one_and_no_other() {
    for (what, changes, sent) in [
        (
            "archive",
            json!({ "removeLabelIds": ["INBOX"] }),
            json!({ "removeLabelIds": ["INBOX"] }),
        ),
        (
            "mark as read",
            json!({ "removeLabelIds": ["UNREAD"], "addLabelIds": [] }),
            json!({ "removeLabelIds": ["UNREAD"] }),
        ),
        (
            "star and file",
            json!({ "addLabelIds": ["STARRED", "Label_12"], "removeLabelIds": [] }),
            json!({ "addLabelIds": ["STARRED", "Label_12"] }),
        ),
    ] {
        let (server, socket, key) = answering(200, gmail_ref(&["IMPORTANT"])).await;
        let mut input = json!({ "message": GMAIL_MESSAGE });
        input
            .as_object_mut()
            .unwrap()
            .extend(changes.as_object().unwrap().clone());
        let changed = invoke(&socket, &key, "gmail_messages.modify", input).await.unwrap();
        assert_eq!(
            changed["labelIds"],
            json!(["IMPORTANT"]),
            "{what}: the labels it has now"
        );
        assert_eq!(body_of(&only_request(&server).await), sent, "{what}");
    }
}

// ── What is refused, and what is wrong ───────────────────────────────────────

#[tokio::test]
async fn what_could_only_fail_is_refused_before_google_is_called() {
    let (server, socket, key) = answering(200, gmail_message()).await;
    let many: Vec<String> = (0..101).map(|n| format!("Label_{n}")).collect();
    for (name, input, says) in [
        (
            "gmail_messages.get",
            json!({ "message": " " }),
            "a message id is required",
        ),
        (
            "gmail_messages.get",
            json!({ "message": ".." }),
            "a message id is not valid",
        ),
        (
            "gmail_messages.get",
            json!({ "message": "." }),
            "a message id is not valid",
        ),
        // Only the three formats that are decoded here; `raw` would come back unread.
        (
            "gmail_messages.get",
            json!({ "message": "m1", "format": "raw" }),
            "a field of the wrong type",
        ),
        (
            "gmail_messages.get",
            json!({ "message": "m1", "format": "FULL" }),
            "a field of the wrong type",
        ),
        (
            "gmail_messages.get",
            json!({ "message": "m1", "metadataHeaders": ["Subject"] }),
            "`metadataHeaders` is not a field",
        ),
        (
            "gmail_messages.attachment_get",
            json!({ "message": "m1", "attachment": "" }),
            "an attachment id is required",
        ),
        (
            "gmail_messages.attachment_get",
            json!({ "message": "", "attachment": "a1" }),
            "a message id is required",
        ),
        (
            "gmail_messages.list",
            json!({ "labelIds": ["INBOX", " "] }),
            "`labelIds` has a blank id",
        ),
        (
            "gmail_threads.list",
            json!({ "labelIds": [""] }),
            "`labelIds` has a blank id",
        ),
        (
            "gmail_drafts.list",
            json!({ "labelIds": ["DRAFT"] }),
            "`labelIds` is not a field",
        ),
        ("gmail_threads.get", json!({ "thread": "" }), "a thread id is required"),
        (
            "gmail_threads.get",
            json!({ "thread": "t1", "format": "raw" }),
            "a field of the wrong type",
        ),
        ("gmail_labels.get", json!({ "label": "  " }), "a label id is required"),
        ("gmail_labels.list", json!({ "q": "x" }), "not a field"),
        (
            "gmail_profile.get",
            json!({ "user": "eve@example.test" }),
            "not a field",
        ),
        ("gmail_drafts.get", json!({ "draft": "" }), "a draft id is required"),
        (
            "gmail_drafts.delete",
            json!({ "draft": ".." }),
            "a draft id is not valid",
        ),
        (
            "gmail_messages.trash",
            json!({ "message": "" }),
            "a message id is required",
        ),
        (
            "gmail_messages.untrash",
            json!({ "message": ".." }),
            "a message id is not valid",
        ),
        // A change of labels has to name one.
        (
            "gmail_messages.modify",
            json!({ "message": "m1" }),
            "a change needs `addLabelIds` or `removeLabelIds`",
        ),
        (
            "gmail_messages.modify",
            json!({ "message": "m1", "addLabelIds": [], "removeLabelIds": [] }),
            "a change needs `addLabelIds` or `removeLabelIds`",
        ),
        (
            "gmail_messages.modify",
            json!({ "message": "m1", "addLabelIds": ["STARRED", ""] }),
            "`addLabelIds` has a blank id",
        ),
        (
            "gmail_messages.modify",
            json!({ "message": "m1", "removeLabelIds": [" "] }),
            "`removeLabelIds` has a blank id",
        ),
        (
            "gmail_messages.modify",
            json!({ "message": "m1", "removeLabelIds": many }),
            "`removeLabelIds` takes at most 100 labels",
        ),
        (
            "gmail_messages.modify",
            json!({ "message": "", "addLabelIds": ["STARRED"] }),
            "a message id is required",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert!(err.message().contains(says), "{name} {input}: {err}");
        assert!(!err.message().contains("eve"), "{name}: {err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_id_stays_one_segment_of_the_path_whatever_it_contains() {
    let star = json!(["STARRED"]);
    for (name, input, verb, sent) in [
        (
            "gmail_messages.get",
            json!({ "message": "a/b?c#d" }),
            "GET",
            format!("{GMAIL_MESSAGES}/a%2Fb%3Fc%23d"),
        ),
        (
            "gmail_messages.get",
            json!({ "message": "../drafts" }),
            "GET",
            format!("{GMAIL_MESSAGES}/..%2Fdrafts"),
        ),
        (
            "gmail_messages.get",
            json!({ "message": " 18c1 " }),
            "GET",
            format!("{GMAIL_MESSAGES}/18c1"),
        ),
        (
            "gmail_messages.attachment_get",
            json!({ "message": "m/1", "attachment": "ANG/x?alt=media#" }),
            "GET",
            format!("{GMAIL_MESSAGES}/m%2F1/attachments/ANG%2Fx%3Falt%3Dmedia%23"),
        ),
        (
            "gmail_messages.modify",
            json!({ "message": "m1/modify?x", "addLabelIds": star }),
            "POST",
            format!("{GMAIL_MESSAGES}/m1%2Fmodify%3Fx/modify"),
        ),
        (
            "gmail_messages.trash",
            json!({ "message": "m1/../m2" }),
            "POST",
            format!("{GMAIL_MESSAGES}/m1%2F..%2Fm2/trash"),
        ),
        (
            "gmail_messages.untrash",
            json!({ "message": "m 1" }),
            "POST",
            format!("{GMAIL_MESSAGES}/m%201/untrash"),
        ),
        (
            "gmail_threads.get",
            json!({ "thread": "t1/../../labels" }),
            "GET",
            format!("{GMAIL_THREADS}/t1%2F..%2F..%2Flabels"),
        ),
        (
            "gmail_labels.get",
            json!({ "label": "Label_1#x" }),
            "GET",
            format!("{GMAIL_LABELS}/Label_1%23x"),
        ),
        (
            "gmail_drafts.get",
            json!({ "draft": "r-1?format=raw" }),
            "GET",
            format!("{GMAIL_DRAFTS}/r-1%3Fformat%3Draw"),
        ),
        (
            "gmail_drafts.update",
            json!({ "draft": "r-1/send", "subject": "x" }),
            "PUT",
            format!("{GMAIL_DRAFTS}/r-1%2Fsend"),
        ),
        (
            "gmail_drafts.delete",
            json!({ "draft": "../messages/m1" }),
            "DELETE",
            format!("{GMAIL_DRAFTS}/..%2Fmessages%2Fm1"),
        ),
    ] {
        let (server, socket, key) = answering(200, json!({})).await;
        // The empty answer is an error of its own; what matters here is where the request went.
        let _ = invoke(&socket, &key, name, input.clone()).await;
        let request = only_request(&server).await;
        assert_eq!(
            (request.method.as_str(), request.url.path()),
            (verb, sent.as_str()),
            "{name} {input}"
        );
        assert_eq!(request.url.query(), None, "{name} {input}");
        assert_eq!(request.url.fragment(), None, "{name} {input}");
    }
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_field_and_never_repeats_gmail() {
    let secret = "CONFIDENTIAL offer letter for Grace";
    let message = json!({ "message": GMAIL_MESSAGE });
    let send = json!({ "to": [{ "email": "grace@example.test" }], "subject": "x" });
    for (name, input, response, place) in [
        // A body that is not base64 is named by where it sits, and not shown.
        (
            "gmail_messages.get",
            message.clone(),
            json!({ "id": "m1", "payload": { "parts": [{}, { "body": { "data": secret } }] } }),
            "payload.parts[1].body.data",
        ),
        (
            "gmail_messages.get",
            message.clone(),
            json!({ "id": "m1", "labelIds": secret }),
            "labelIds",
        ),
        (
            "gmail_messages.get",
            message.clone(),
            json!({ "id": "m1", "payload": { "headers": [{ "name": "Subject", "value": [secret] }] } }),
            "payload.headers[0].value",
        ),
        (
            "gmail_threads.get",
            json!({ "thread": GMAIL_THREAD }),
            json!({ "id": "t1", "messages": [{ "id": "m1", "payload": { "body": { "data": secret } } }] }),
            "messages[0].payload.body.data",
        ),
        (
            "gmail_drafts.get",
            json!({ "draft": GMAIL_DRAFT }),
            json!({ "id": "r1", "message": { "id": "m1", "snippet": { "text": secret } } }),
            "message.snippet",
        ),
        (
            "gmail_messages.list",
            json!({}),
            json!({ "messages": [{ "id": "m1" }, { "id": "m2", "threadId": [secret] }] }),
            "[1].threadId",
        ),
        (
            "gmail_threads.list",
            json!({}),
            json!({ "threads": [{ "id": "t1", "snippet": [secret] }] }),
            "[0].snippet",
        ),
        (
            "gmail_labels.list",
            json!({}),
            json!({ "labels": [{ "id": "L1", "name": { "is": secret } }] }),
            "[0].name",
        ),
        (
            "gmail_labels.get",
            json!({ "label": "L1" }),
            json!({ "id": "L1", "color": secret }),
            "color",
        ),
        (
            "gmail_messages.attachment_get",
            json!({ "message": "m1", "attachment": "a1" }),
            json!({ "size": secret, "data": "eA" }),
            "size",
        ),
        (
            "gmail_profile.get",
            json!({}),
            json!({ "emailAddress": [secret] }),
            "emailAddress",
        ),
        (
            "gmail_messages.send",
            send.clone(),
            json!({ "id": "m9", "labelIds": secret }),
            "labelIds",
        ),
        (
            "gmail_drafts.create",
            send,
            json!({ "id": "r9", "message": secret }),
            "message",
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
        assert!(err.message().ends_with(&format!("at `{place}`")), "{name}: {err}");
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
    }
}

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error() {
    let message = json!({ "message": GMAIL_MESSAGE });
    let send = json!({ "to": [{ "email": "grace@example.test" }], "subject": "x" });
    for (name, input, response) in [
        ("gmail_messages.get", message.clone(), json!({})),
        (
            "gmail_messages.get",
            message.clone(),
            json!({ "id": "", "snippet": "Plan" }),
        ),
        ("gmail_messages.get", message.clone(), json!(null)),
        (
            "gmail_messages.list",
            json!({}),
            json!({ "messages": [{ "threadId": GMAIL_THREAD }] }),
        ),
        ("gmail_messages.list", json!({}), json!([])),
        ("gmail_messages.list", json!({}), json!(null)),
        (
            "gmail_messages.attachment_get",
            json!({ "message": "m1", "attachment": "a1" }),
            json!({}),
        ),
        (
            "gmail_messages.attachment_get",
            json!({ "message": "m1", "attachment": "a1" }),
            json!({ "attachmentId": "a1" }),
        ),
        // A file of some size, and none of it.
        (
            "gmail_messages.attachment_get",
            json!({ "message": "m1", "attachment": "a1" }),
            json!({ "size": 41230 }),
        ),
        ("gmail_messages.send", send.clone(), json!({})),
        (
            "gmail_messages.send",
            send.clone(),
            json!({ "threadId": GMAIL_THREAD, "labelIds": ["SENT"] }),
        ),
        ("gmail_messages.send_draft", json!({ "draft": GMAIL_DRAFT }), json!({})),
        (
            "gmail_messages.modify",
            json!({ "message": "m1", "addLabelIds": ["STARRED"] }),
            json!({ "labelIds": ["STARRED"] }),
        ),
        ("gmail_messages.trash", message.clone(), json!(null)),
        ("gmail_messages.untrash", message, json!({})),
        ("gmail_threads.get", json!({ "thread": GMAIL_THREAD }), json!({})),
        (
            "gmail_threads.get",
            json!({ "thread": GMAIL_THREAD }),
            json!({ "id": GMAIL_THREAD, "messages": [{ "snippet": "Plan" }] }),
        ),
        (
            "gmail_threads.list",
            json!({}),
            json!({ "threads": [{ "snippet": "Plan" }] }),
        ),
        (
            "gmail_labels.list",
            json!({}),
            json!({ "labels": [{ "name": "Projects" }] }),
        ),
        ("gmail_labels.list", json!({}), json!("labels")),
        (
            "gmail_labels.get",
            json!({ "label": "Label_12" }),
            json!({ "name": "Projects" }),
        ),
        ("gmail_profile.get", json!({}), json!({ "messagesTotal": 20481 })),
        (
            "gmail_drafts.list",
            json!({}),
            json!({ "drafts": [{ "message": { "id": "m1" } }] }),
        ),
        (
            "gmail_drafts.get",
            json!({ "draft": GMAIL_DRAFT }),
            json!({ "id": GMAIL_DRAFT }),
        ),
        (
            "gmail_drafts.get",
            json!({ "draft": GMAIL_DRAFT }),
            json!({ "message": { "id": "m1" } }),
        ),
        (
            "gmail_drafts.create",
            send.clone(),
            json!({ "message": { "id": "m1" } }),
        ),
        (
            "gmail_drafts.update",
            json!({ "draft": GMAIL_DRAFT, "subject": "x" }),
            json!({}),
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
    }

    // A draft that was deleted is the one answer with nothing in it.
    for nothing in [json!(null), json!({})] {
        let (_server, socket, key) = answering(200, nothing).await;
        let input = json!({ "draft": GMAIL_DRAFT });
        assert_eq!(
            invoke(&socket, &key, "gmail_drafts.delete", input).await.unwrap(),
            json!(null)
        );
    }
}

#[tokio::test]
async fn googles_refusals_of_mail_reach_the_caller() {
    for (response, kind) in [
        (
            google_error(404, "notFound", "Requested entity was not found."),
            ErrorKind::NotFound,
        ),
        (
            google_error(
                403,
                "insufficientPermissions",
                "Request had insufficient authentication scopes.",
            ),
            ErrorKind::AccessDenied,
        ),
        (
            google_error(400, "invalidArgument", "Invalid id value"),
            ErrorKind::InvalidInput,
        ),
        // Gmail's own limit on how fast one person's mailbox is read.
        (
            google_error(403, "userRateLimitExceeded", "User-rate limit exceeded."),
            ErrorKind::RateLimited,
        ),
        (
            google_error(429, "rateLimitExceeded", "Too many concurrent requests for user."),
            ErrorKind::RateLimited,
        ),
    ] {
        let (server, socket, key) = google().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let input = json!({ "message": GMAIL_MESSAGE });
        let err = invoke(&socket, &key, "gmail_messages.get", input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
    }
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let google = Google::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(google.clone()), support::TOKEN).await;
    let connection = socket.connection(key).await.unwrap();
    let one = format!("{GMAIL_MESSAGES}/{GMAIL_MESSAGE}");
    for (verb, at, response) in [
        (
            "GET",
            GMAIL_MESSAGES.to_owned(),
            json!({ "messages": [{ "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD }], "nextPageToken": "0987654321" }),
        ),
        ("GET", one.clone(), gmail_message()),
        (
            "GET",
            format!("{GMAIL_THREADS}/{GMAIL_THREAD}"),
            json!({ "id": GMAIL_THREAD, "messages": [gmail_metadata()] }),
        ),
        ("GET", format!("{GMAIL_LABELS}/Label_12"), gmail_label()),
        (
            "GET",
            support::gmail::GMAIL_PROFILE.to_owned(),
            json!({ "emailAddress": "ada@example.test", "historyId": "987654" }),
        ),
        ("POST", format!("{one}/modify"), gmail_ref(&["IMPORTANT"])),
        ("POST", format!("{GMAIL_MESSAGES}/send"), gmail_ref(&["SENT"])),
        ("POST", GMAIL_DRAFTS.to_owned(), gmail_draft_ref()),
    ] {
        Mock::given(method(verb))
            .and(path(at))
            .respond_with(answer(200, &response))
            .mount(&server)
            .await;
    }

    let messages = google.gmail_messages(&connection);
    let unread = messages
        .list(
            GmailListMessages {
                q: Some("from:grace".into()),
                label_ids: Some(vec!["INBOX".into(), "UNREAD".into()]),
                ..GmailListMessages::default()
            },
            Paging {
                cursor: None,
                limit: Some(10),
            },
        )
        .await
        .unwrap();
    assert_eq!(unread.next_cursor.as_deref(), Some("0987654321"));
    let first = &unread.items[0];
    assert_eq!(
        (first.id.as_str(), first.thread_id.as_deref()),
        (GMAIL_MESSAGE, Some(GMAIL_THREAD))
    );

    let read = messages.get(&first.id, GmailGetMessage::default()).await.unwrap();
    assert_eq!(read.from.as_deref(), Some("Grace Hopper <grace@example.test>"));
    assert_eq!(read.subject.as_deref(), Some("Q3 plan"));
    assert_eq!(read.text.as_deref(), Some("Attached is the plan for Q3.\r\n"));
    assert_eq!(
        read.html.as_deref(),
        Some("<div>Attached is the plan for Q3.</div>\r\n")
    );
    assert_eq!(read.attachments[0].attachment_id.as_deref(), Some(GMAIL_ATTACHMENT));
    assert_eq!(read.attachments[0].filename, "q3-plan.pdf");
    assert!(read.label_ids.iter().any(|label| label == "UNREAD"));

    let headers = GmailGetThread {
        format: Some(GmailFormat::Metadata),
    };
    let thread = google
        .gmail_threads(&connection)
        .get(GMAIL_THREAD, headers)
        .await
        .unwrap();
    assert_eq!(
        thread.messages[0].message_id.as_deref(),
        Some("<CAF1plan@mail.example.test>")
    );
    assert_eq!(
        google
            .gmail_labels(&connection)
            .get("Label_12")
            .await
            .unwrap()
            .messages_unread,
        Some(3)
    );
    assert_eq!(
        google.gmail_profile(&connection).get().await.unwrap().email_address,
        "ada@example.test"
    );

    let archived = GmailModifyMessage {
        remove_label_ids: Some(vec!["INBOX".into(), "UNREAD".into()]),
        ..GmailModifyMessage::default()
    };
    assert_eq!(
        messages.modify(&first.id, archived).await.unwrap().label_ids,
        ["IMPORTANT"]
    );
    let reply = GmailReply {
        text: Some("Monday works.".into()),
        ..GmailReply::default()
    };
    assert_eq!(messages.reply(&first.id, reply).await.unwrap().label_ids, ["SENT"]);
    let draft = GmailSendMessage {
        to: Some(vec![GmailAddress::named("grace@example.test", "Grace Hopper")]),
        subject: Some("Monday".into()),
        text: Some("See you Monday.".into()),
        ..GmailSendMessage::default()
    };
    assert_eq!(
        google.gmail_drafts(&connection).create(draft).await.unwrap().id,
        GMAIL_DRAFT
    );

    // The same requests the named operations make, in the order they were called.
    let received = server.received_requests().await.unwrap();
    let went: Vec<(&str, &str, Value)> = received
        .iter()
        .map(|request| (request.method.as_str(), request.url.path(), query_of(request)))
        .collect();
    let none = json!({});
    assert_eq!(
        went,
        [
            (
                "GET",
                GMAIL_MESSAGES,
                json!({ "q": "from:grace", "labelIds": ["INBOX", "UNREAD"], "maxResults": "10" })
            ),
            ("GET", one.as_str(), none.clone()),
            (
                "GET",
                format!("{GMAIL_THREADS}/{GMAIL_THREAD}").as_str(),
                json!({ "format": "metadata" })
            ),
            ("GET", format!("{GMAIL_LABELS}/Label_12").as_str(), none.clone()),
            ("GET", support::gmail::GMAIL_PROFILE, none.clone()),
            ("POST", format!("{one}/modify").as_str(), none.clone()),
            ("GET", one.as_str(), json!({ "format": "metadata" })),
            ("POST", format!("{GMAIL_MESSAGES}/send").as_str(), none.clone()),
            ("POST", GMAIL_DRAFTS, none),
        ]
    );
    assert_eq!(body_of(&received[5]), json!({ "removeLabelIds": ["INBOX", "UNREAD"] }));
    let answered = body_of(&received[7]);
    assert_eq!(answered["threadId"], GMAIL_THREAD);
    assert_eq!(
        gmail_sent(&answered["raw"]).header("To"),
        Some("Grace Hopper <grace@example.test>")
    );
    let drafted = gmail_sent(&body_of(&received[8])["message"]["raw"]);
    assert_eq!(
        (drafted.header("Subject"), drafted.text.as_deref()),
        (Some("Monday"), Some("See you Monday."))
    );
}
