//! Writing and sending Gmail against a local server that answers as Google
//! does: what reaches Gmail in `raw`, read back out of it, and what never does.
//!
//! How a message is written line by line is tested beside the code that
//! writes it, in `models/gmail_rfc2822.rs`. This file holds what a caller of
//! the operations can and cannot make them send.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use serde_json::{Value, json};
use socketkit_core::ErrorKind;
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer};

mod support;
use support::gmail::{
    GMAIL_DRAFT, GMAIL_DRAFTS, GMAIL_MESSAGE, GMAIL_MESSAGES, GMAIL_THREAD, gmail_draft_ref, gmail_headers_of,
    gmail_metadata, gmail_metadata_with, gmail_ref, gmail_sent,
};
use support::{answer, answering, body_of, google, google_error, invoke, only_request, query_of};

fn grace() -> Value {
    json!([{ "email": "grace@example.test", "name": "Grace Hopper" }])
}

/// One text as an encoded word, as a header carries what is outside ASCII.
fn word(text: &str) -> String {
    format!("=?UTF-8?B?{}?=", STANDARD.encode(text))
}

/// A server where the message being answered is `original`, and sending works.
async fn answering_a_reply_to(original: Value) -> (MockServer, socketkit_core::Socket, socketkit_core::ConnectionKey) {
    let (server, socket, key) = google().await;
    Mock::given(method("GET"))
        .and(path(format!("{GMAIL_MESSAGES}/{GMAIL_MESSAGE}")))
        .respond_with(answer(200, &original))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(format!("{GMAIL_MESSAGES}/send")))
        .respond_with(answer(200, &gmail_ref(&["SENT"])))
        .mount(&server)
        .await;
    (server, socket, key)
}

// ── Sending ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_message_is_written_as_mail_travels_with_everyone_it_goes_to() {
    let (server, socket, key) = answering(200, gmail_ref(&["SENT"])).await;
    let input = json!({
        "to": [{ "email": "grace@example.test", "name": "Grace Hopper" }, { "email": " zoe@example.test ", "name": "Zoë Müller" }],
        "cc": [{ "email": "alan@example.test", "name": "Turing, Alan" }],
        "bcc": [{ "email": "quiet@example.test" }],
        "subject": "Grüße: the plan for Q3",
        "text": "Plan attached.\nSee you Monday. ☕",
        "html": "<p>Plan attached.</p><p>See you Monday. ☕</p>"
    });
    let sent = invoke(&socket, &key, "gmail_messages.send", input).await.unwrap();
    assert_eq!(sent["labelIds"], json!(["SENT"]));
    let body = body_of(&only_request(&server).await);
    assert_eq!(body.as_object().unwrap().len(), 1, "the message, and nothing beside it");
    let sent = gmail_sent(&body["raw"]);
    assert_eq!(
        sent.names(),
        ["To", "Cc", "Bcc", "Subject", "MIME-Version", "Content-Type"]
    );
    assert_eq!(
        sent.header("To").unwrap(),
        format!(
            "Grace Hopper <grace@example.test>, {} <zoe@example.test>",
            word("Zoë Müller")
        )
    );
    assert_eq!(sent.header("Cc"), Some("\"Turing, Alan\" <alan@example.test>"));
    // People in blind copy reach Gmail with the rest; hiding them from the others is Gmail's part.
    assert_eq!(sent.header("Bcc"), Some("quiet@example.test"));
    assert_eq!(sent.header("Subject").unwrap(), word("Grüße: the plan for Q3"));
    assert_eq!(sent.header("MIME-Version"), Some("1.0"));
    assert_eq!(sent.text.as_deref(), Some("Plan attached.\r\nSee you Monday. ☕"));
    assert_eq!(
        sent.html.as_deref(),
        Some("<p>Plan attached.</p><p>See you Monday. ☕</p>")
    );
}

#[tokio::test]
async fn mail_can_go_to_people_in_copy_alone_and_with_only_a_subject_or_only_a_body() {
    for (input, headers, text, html) in [
        (
            json!({ "cc": grace(), "subject": "Monday" }),
            vec!["Cc", "Subject"],
            Some(""),
            None,
        ),
        (
            json!({ "bcc": grace(), "text": "See you Monday." }),
            vec!["Bcc"],
            Some("See you Monday."),
            None,
        ),
        (
            json!({ "to": grace(), "cc": [], "html": "<p>Monday</p>" }),
            vec!["To"],
            None,
            Some("<p>Monday</p>"),
        ),
    ] {
        let (server, socket, key) = answering(200, gmail_ref(&["SENT"])).await;
        invoke(&socket, &key, "gmail_messages.send", input.clone())
            .await
            .unwrap();
        let sent = gmail_sent(&body_of(&only_request(&server).await)["raw"]);
        let names = sent.names();
        let (addressed, rest) = names.split_at(headers.len());
        assert_eq!(addressed, headers, "{input}");
        assert_eq!(rest, ["MIME-Version", "Content-Type", "Content-Transfer-Encoding"]);
        assert_eq!((sent.text.as_deref(), sent.html.as_deref()), (text, html), "{input}");
    }
}

#[tokio::test]
async fn mail_that_could_add_a_header_or_reach_no_one_is_refused_before_google_is_called() {
    let (server, socket, key) = answering(200, gmail_ref(&["SENT"])).await;
    let to = |email: &str| json!([{ "email": email }]);
    let named = |name: &str| json!([{ "email": "grace@example.test", "name": name }]);
    let said = |mut input: Value| {
        input["text"] = json!("x");
        input
    };
    for (input, says) in [
        // A line break in a header would end it and begin one of the writer's choosing.
        (
            json!({ "to": grace(), "subject": "Monday\r\nBcc: eve@example.test" }),
            "`subject` has a line break",
        ),
        (
            json!({ "to": grace(), "subject": "Monday\nBcc: eve@example.test" }),
            "`subject` has a line break",
        ),
        (
            json!({ "to": grace(), "subject": "Monday\rBcc: eve@example.test" }),
            "`subject` has a line break",
        ),
        (
            json!({ "to": grace(), "subject": "Monday\u{0}" }),
            "`subject` has a line break",
        ),
        (
            said(json!({ "to": to("grace@example.test\r\nBcc: eve@example.test") })),
            "`to[0]` needs an `email`",
        ),
        (
            said(json!({ "to": named("Grace\r\nBcc: eve@example.test") })),
            "`to[0]` has a `name`",
        ),
        (
            said(json!({ "to": grace(), "cc": named("Grace\nHopper") })),
            "`cc[0]` has a `name`",
        ),
        (
            said(json!({ "to": grace(), "bcc": [{ "email": "alan@example.test" }, { "email": "x\n@example.test" }] })),
            "`bcc[1]` needs an `email`",
        ),
        // One mailbox each, and each a mailbox.
        (
            said(json!({ "to": to("grace@example.test, eve@example.test") })),
            "`to[0]` needs an `email`",
        ),
        (
            said(json!({ "to": to("Grace <grace@example.test>") })),
            "`to[0]` needs an `email`",
        ),
        (said(json!({ "to": to("grace") })), "`to[0]` needs an `email`"),
        (said(json!({ "to": to("  ") })), "`to[0]` needs an `email`"),
        (
            said(json!({ "to": [{ "name": "Grace Hopper" }] })),
            "missing field `email`",
        ),
        (
            said(json!({ "to": ["grace@example.test"] })),
            "a field of the wrong type",
        ),
        (said(json!({ "to": "grace@example.test" })), "a field of the wrong type"),
        // It has to reach someone, and say something.
        (said(json!({})), "a message needs at least one recipient"),
        (
            said(json!({ "to": [], "cc": [], "bcc": [] })),
            "a message needs at least one recipient",
        ),
        (
            json!({ "to": grace() }),
            "a message needs a `subject`, `text` or `html`",
        ),
        (
            json!({ "to": grace(), "subject": " ", "text": "\n", "html": "" }),
            "a message needs a `subject`, `text` or `html`",
        ),
        // What is not a field of a message is not dropped in silence.
        (
            said(json!({ "to": grace(), "from": "eve@example.test" })),
            "`from` is not a field",
        ),
        (
            said(json!({ "to": grace(), "threadId": "t1" })),
            "`threadId` is not a field",
        ),
        (
            said(json!({ "to": grace(), "raw": "VG86IGV2ZUBleGFtcGxlLnRlc3Q" })),
            "`raw` is not a field",
        ),
    ] {
        let err = invoke(&socket, &key, "gmail_messages.send", input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}: {err}");
        assert!(err.message().contains(says), "{input}: {err}");
        assert!(
            !err.message().contains("eve"),
            "a refusal does not repeat what was given: {err}"
        );
    }

    // A draft may be empty, and may not be malformed. A reply has to say something.
    for (name, input, says) in [
        (
            "gmail_drafts.create",
            json!({ "subject": "a\r\nBcc: eve@example.test" }),
            "`subject` has a line break",
        ),
        (
            "gmail_drafts.create",
            json!({ "to": to("eve@example.test\r\n") , "cc": to("not an address") }),
            "`cc[0]` needs an `email`",
        ),
        (
            "gmail_drafts.update",
            json!({ "draft": GMAIL_DRAFT, "bcc": named("x\ny") }),
            "`bcc[0]` has a `name`",
        ),
        (
            "gmail_drafts.update",
            json!({ "draft": " ", "subject": "Monday" }),
            "a draft id is required",
        ),
        (
            "gmail_drafts.update",
            json!({ "subject": "Monday" }),
            "missing field `draft`",
        ),
        (
            "gmail_messages.send_draft",
            json!({ "draft": "  " }),
            "a draft id is required",
        ),
        (
            "gmail_messages.send_draft",
            json!({ "draft": GMAIL_DRAFT, "raw": "VG86" }),
            "`raw` is not a field",
        ),
        (
            "gmail_messages.reply",
            json!({ "message": GMAIL_MESSAGE }),
            "a reply needs `text` or `html`",
        ),
        (
            "gmail_messages.reply",
            json!({ "message": GMAIL_MESSAGE, "text": " ", "html": "" }),
            "a reply needs `text` or `html`",
        ),
        (
            "gmail_messages.reply",
            json!({ "message": GMAIL_MESSAGE, "text": "x", "to": to("eve@example.test\n") , "cc": to("nobody") }),
            "`cc[0]` needs an `email`",
        ),
        (
            "gmail_messages.reply",
            json!({ "message": GMAIL_MESSAGE, "text": "x", "subject": "Re: a\nBcc: eve@example.test" }),
            "`subject` has a line break",
        ),
        (
            "gmail_messages.reply",
            json!({ "message": "", "text": "x" }),
            "a message id is required",
        ),
        (
            "gmail_messages.reply",
            json!({ "message": GMAIL_MESSAGE, "text": "x", "threadId": "t9" }),
            "`threadId` is not a field",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert!(err.message().contains(says), "{name} {input}: {err}");
        assert!(!err.message().contains("eve"), "{name}: {err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Replying ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_reply_stays_in_the_thread_of_the_message_it_answers() {
    let (server, socket, key) = answering_a_reply_to(gmail_metadata()).await;
    let input = json!({ "message": GMAIL_MESSAGE, "text": "Monday works.", "html": "<p>Monday works.</p>" });
    let sent = invoke(&socket, &key, "gmail_messages.reply", input).await.unwrap();
    assert_eq!(sent["threadId"], GMAIL_THREAD);

    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 2, "the original is read, then the reply is sent");
    // Headers only: the body of the original is not needed, and not fetched.
    assert_eq!(received[0].method.as_str(), "GET");
    assert_eq!(query_of(&received[0]), json!({ "format": "metadata" }));
    let body = body_of(&received[1]);
    assert_eq!(body["threadId"], GMAIL_THREAD);
    assert_eq!(body.as_object().unwrap().len(), 2);
    let reply = gmail_sent(&body["raw"]);
    assert_eq!(
        reply.names(),
        [
            "To",
            "Subject",
            "In-Reply-To",
            "References",
            "MIME-Version",
            "Content-Type"
        ]
    );
    assert_eq!(reply.header("To"), Some("Grace Hopper <grace@example.test>"));
    assert_eq!(reply.header("Subject"), Some("Re: Q3 plan"));
    assert_eq!(reply.header("In-Reply-To"), Some("<CAF1plan@mail.example.test>"));
    assert_eq!(
        reply.header("References"),
        Some("<CAF0kickoff@mail.example.test> <CAF1plan@mail.example.test>")
    );
    assert_eq!(reply.text.as_deref(), Some("Monday works."));
    assert_eq!(reply.html.as_deref(), Some("<p>Monday works.</p>"));
}

#[tokio::test]
async fn a_reply_goes_to_the_sender_unless_the_caller_says_otherwise() {
    let headers = |subject: &str| {
        gmail_headers_of(&[
            ("from", "=?UTF-8?Q?Gr=C3=A4ce?= <grace@example.test>"),
            // The sender again, however it is written, redirects nothing.
            ("REPLY-TO", "GRACE@example.test"),
            ("Cc", "alan@example.test"),
            ("subject", subject),
            ("Message-Id", "<CAF1plan@mail.example.test>"),
        ])
    };
    for (subject, extra, to, cc, written) in [
        // The sender, and nobody who was only in copy.
        (
            "Q3 plan",
            json!({}),
            format!("{} <grace@example.test>", word("Gräce")),
            None,
            "Re: Q3 plan",
        ),
        // An answer is not marked as one twice.
        (
            "RE: Q3 plan",
            json!({}),
            format!("{} <grace@example.test>", word("Gräce")),
            None,
            "RE: Q3 plan",
        ),
        (
            "Q3 plan",
            json!({ "to": [{ "email": "ada@example.test" }], "cc": [{ "email": "alan@example.test", "name": "Alan" }], "subject": "Q3 plan, revised" }),
            "ada@example.test".to_owned(),
            Some("Alan <alan@example.test>"),
            "Q3 plan, revised",
        ),
    ] {
        let (server, socket, key) = answering_a_reply_to(gmail_metadata_with(headers(subject))).await;
        let mut input = json!({ "message": GMAIL_MESSAGE, "text": "Agreed." });
        input
            .as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        invoke(&socket, &key, "gmail_messages.reply", input.clone())
            .await
            .unwrap();
        let received = server.received_requests().await.unwrap();
        let reply = gmail_sent(&body_of(&received[1])["raw"]);
        assert_eq!(reply.header("To"), Some(to.as_str()), "{input}");
        assert_eq!(reply.header("Cc"), cc, "{input}");
        assert_eq!(reply.header("Subject"), Some(written), "{input}");
        // A thread's first message lists no thread before it.
        assert_eq!(reply.header("References"), Some("<CAF1plan@mail.example.test>"));
    }
}

#[tokio::test]
async fn a_reply_is_not_sent_to_an_address_the_original_chose_and_nobody_was_shown() {
    // Mail that is seen to come from a colleague and asks for its answers
    // to go to someone else. Whoever approves the reply was shown neither.
    let redirected = gmail_headers_of(&[
        ("From", "Grace Hopper <grace@example.test>"),
        ("Reply-To", "Grace Hopper <eve@elsewhere.test>"),
        ("Subject", "Q3 plan"),
        ("Message-ID", "<CAF1plan@mail.example.test>"),
    ]);
    let (server, socket, key) = answering_a_reply_to(gmail_metadata_with(redirected.clone())).await;
    let input = json!({ "message": GMAIL_MESSAGE, "text": "The figures are attached." });
    let err = invoke(&socket, &key, "gmail_messages.reply", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(
        err.message().contains("`replyTo`") && err.message().contains("`to`"),
        "{err}"
    );
    assert!(!format!("{err} {err:?}").contains("elsewhere"), "{err:?}");
    let received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "the original was read, and nothing was sent");
    assert_eq!(received[0].method.as_str(), "GET");

    // Named by the caller, that address is where the reply goes.
    let (server, socket, key) = answering_a_reply_to(gmail_metadata_with(redirected)).await;
    let input = json!({ "message": GMAIL_MESSAGE, "text": "The figures are attached.", "to": [{ "email": "eve@elsewhere.test" }] });
    invoke(&socket, &key, "gmail_messages.reply", input).await.unwrap();
    let received = server.received_requests().await.unwrap();
    let reply = gmail_sent(&body_of(&received[1])["raw"]);
    assert_eq!(reply.header("To"), Some("eve@elsewhere.test"));
}

#[tokio::test]
async fn nothing_in_the_original_can_add_a_header_to_the_reply() {
    // Whoever wrote the original chose these. Each hides a line break and a
    // header of their own: in an encoded word, where it survives unfolding,
    // and in the list of the thread.
    let hidden = |text: &str| word(&format!("{text}\r\nBcc: eve@example.test"));
    let hostile = gmail_headers_of(&[
        ("From", &format!("{} <grace@example.test>", hidden("Grace"))),
        ("Subject", &hidden("Plan")),
        ("Message-ID", "<m1@mail.example.test>"),
        (
            "References",
            "<m0@mail.example.test>\r\nBcc: eve@example.test\r\n <m0b@mail.example.test\r\nBcc: eve@example.test>",
        ),
    ]);
    let (server, socket, key) = answering_a_reply_to(gmail_metadata_with(hostile)).await;
    let input = json!({ "message": GMAIL_MESSAGE, "text": "Agreed." });
    invoke(&socket, &key, "gmail_messages.reply", input).await.unwrap();
    let received = server.received_requests().await.unwrap();
    // `gmail_sent` reads the message as a mail server would: line by line.
    let reply = gmail_sent(&body_of(&received[1])["raw"]);
    assert_eq!(
        reply.names(),
        [
            "To",
            "Subject",
            "In-Reply-To",
            "References",
            "MIME-Version",
            "Content-Type",
            "Content-Transfer-Encoding"
        ],
        "the headers a reply has, and no other"
    );
    assert_eq!(
        reply.header("To"),
        Some("\"Grace  Bcc: eve@example.test\" <grace@example.test>"),
        "a name, however odd"
    );
    assert_eq!(
        reply.header("Subject").unwrap(),
        word("Re: Plan  Bcc: eve@example.test")
    );
    assert_eq!(
        reply.header("References"),
        Some("<m0@mail.example.test> <m1@mail.example.test>")
    );
}

#[tokio::test]
async fn a_reply_that_could_not_be_tied_to_its_thread_or_to_anyone_is_not_sent() {
    let named = [("From", "grace@example.test"), ("Subject", "Plan")];
    let no_id = gmail_metadata_with(gmail_headers_of(&named));
    let mut no_thread = gmail_metadata();
    no_thread.as_object_mut().unwrap().remove("threadId");
    let nobody = gmail_metadata_with(gmail_headers_of(&[
        ("From", "undisclosed-recipients:;"),
        ("Message-ID", "<m1@x.test>"),
    ]));
    for (original, kind, says) in [
        (
            no_id,
            ErrorKind::Decode,
            "google answered without the `Message-ID` of the message being answered",
        ),
        (
            no_thread,
            ErrorKind::Decode,
            "google answered without the thread of the message being answered",
        ),
        (
            json!({ "threadId": GMAIL_THREAD }),
            ErrorKind::Decode,
            "google answered without a message",
        ),
        (nobody, ErrorKind::InvalidInput, "names no one to reply to; give `to`"),
    ] {
        let (server, socket, key) = answering_a_reply_to(original).await;
        let input = json!({ "message": GMAIL_MESSAGE, "text": "Agreed." });
        let err = invoke(&socket, &key, "gmail_messages.reply", input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().contains(says), "{err}");
        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1, "{says}");
        assert_eq!(received[0].method.as_str(), "GET", "nothing was sent");
    }

    // The original is gone: Gmail says so, and nothing is sent.
    let (server, socket, key) = google().await;
    let gone = google_error(404, "notFound", "Requested entity was not found.");
    Mock::given(any()).respond_with(gone).mount(&server).await;
    let input = json!({ "message": GMAIL_MESSAGE, "text": "Agreed." });
    let err = invoke(&socket, &key, "gmail_messages.reply", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(only_request(&server).await.method.as_str(), "GET");
}

// ── Drafts ───────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_draft_holds_the_message_a_send_would_and_may_be_begun_empty() {
    let (server, socket, key) = answering(200, gmail_draft_ref()).await;
    let created = invoke(&socket, &key, "gmail_drafts.create", json!({})).await.unwrap();
    assert_eq!(created["id"], GMAIL_DRAFT);
    assert_eq!(created["message"]["labelIds"], json!(["DRAFT"]));
    let request = only_request(&server).await;
    assert_eq!((request.method.as_str(), request.url.path()), ("POST", GMAIL_DRAFTS));
    let body = body_of(&request);
    assert_eq!(body.as_object().unwrap().len(), 1);
    assert_eq!(
        body["message"].as_object().unwrap().len(),
        1,
        "Gmail's draft resource, holding the message"
    );
    let empty = gmail_sent(&body["message"]["raw"]);
    assert_eq!(
        empty.names(),
        ["MIME-Version", "Content-Type", "Content-Transfer-Encoding"]
    );
    assert_eq!((empty.text.as_deref(), empty.html), (Some(""), None));

    // An update is the whole draft again, put in the place of the old one.
    let (server, socket, key) = answering(200, gmail_draft_ref()).await;
    let input = json!({ "draft": GMAIL_DRAFT, "to": grace(), "subject": "Monday", "html": "<p>See you Monday.</p>" });
    invoke(&socket, &key, "gmail_drafts.update", input).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(
        (request.method.as_str(), request.url.path()),
        ("PUT", format!("{GMAIL_DRAFTS}/{GMAIL_DRAFT}").as_str())
    );
    let draft = gmail_sent(&body_of(&request)["message"]["raw"]);
    assert_eq!(draft.header("To"), Some("Grace Hopper <grace@example.test>"));
    assert_eq!(draft.header("Subject"), Some("Monday"));
    assert_eq!(
        (draft.text, draft.html.as_deref()),
        (None, Some("<p>See you Monday.</p>"))
    );
}

#[tokio::test]
async fn sending_a_draft_names_it_and_sends_nothing_of_its_own() {
    let (server, socket, key) = answering(200, gmail_ref(&["SENT"])).await;
    let input = json!({ "draft": format!(" {GMAIL_DRAFT} ") });
    let sent = invoke(&socket, &key, "gmail_messages.send_draft", input).await.unwrap();
    assert_eq!(sent["labelIds"], json!(["SENT"]));
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), format!("{GMAIL_DRAFTS}/send"));
    assert_eq!(body_of(&request), json!({ "id": GMAIL_DRAFT }));
}

// ── When Google fails ────────────────────────────────────────────────────────

#[tokio::test]
async fn a_request_with_no_content_states_its_length() {
    // Google refuses a POST that does not say how long it is, and one with
    // nothing in it goes out without saying. So an empty object is sent.
    for name in ["gmail_messages.trash", "gmail_messages.untrash"] {
        let (server, socket, key) = answering(200, gmail_ref(&["TRASH"])).await;
        invoke(&socket, &key, name, json!({ "message": GMAIL_MESSAGE }))
            .await
            .unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.headers.get("content-length").unwrap(), "2", "{name}");
    }
}

#[tokio::test]
async fn mail_is_sent_once_when_google_fails() {
    let unavailable = || google_error(503, "backendError", "Backend Error");
    for (name, input) in [
        ("gmail_messages.send", json!({ "to": grace(), "subject": "Monday" })),
        ("gmail_messages.send_draft", json!({ "draft": GMAIL_DRAFT })),
        (
            "gmail_messages.modify",
            json!({ "message": GMAIL_MESSAGE, "removeLabelIds": ["INBOX"] }),
        ),
        ("gmail_messages.trash", json!({ "message": GMAIL_MESSAGE })),
        ("gmail_messages.untrash", json!({ "message": GMAIL_MESSAGE })),
        ("gmail_drafts.create", json!({ "subject": "Monday" })),
        // `gmail_drafts.update` and `gmail_drafts.delete` are not in this
        // list: the transport still repeats a PUT and a DELETE after a
        // server error. Both come to the same draft when done twice.
    ] {
        let (server, socket, key) = google().await;
        Mock::given(any()).respond_with(unavailable()).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have been sent, so it is not sent again"
        );
    }

    // A reply reads first, and then sends once.
    let (server, socket, key) = google().await;
    Mock::given(method("GET"))
        .respond_with(answer(200, &gmail_metadata()))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .respond_with(unavailable())
        .mount(&server)
        .await;
    let input = json!({ "message": GMAIL_MESSAGE, "text": "Agreed." });
    let err = invoke(&socket, &key, "gmail_messages.reply", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected);
    let received = server.received_requests().await.unwrap();
    let posts = received.iter().filter(|request| request.method.as_str() == "POST");
    assert_eq!(posts.count(), 1);
}
