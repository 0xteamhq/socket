//! Teams, channels, chats and their messages against a local server that answers as Microsoft Graph does.
//!
//! What every operation sends and returns is in the table in `operations.rs`,
//! and reading a message as plain text is in `chat_text.rs`. This file holds
//! the rest of what is particular to Teams.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Retry};
use socketkit_microsoft::models::{ChatType, CreateChat, Cursor, Paging, SendChatMessage};
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::{
    ALL_KINDS, CHANNEL, CHANNEL_PATH, CHAT, CHAT_PATH, answer, answering, body_of, chat, chat_message, graph_error,
    invoke, member, microsoft, only_request, prefer, query_of,
};

fn in_channel() -> Value {
    json!({ "team": "team-1", "channel": CHANNEL })
}

fn with(mut input: Value, extra: Value) -> Value {
    input
        .as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    input
}

// ── Reading ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn what_teams_itself_noted_is_told_apart_from_what_someone_wrote() {
    // Asked to name every kind, Graph says `systemEventMessage`; without the
    // header it would say `unknownFutureValue`. Such a message has no sender
    // and nothing to read.
    let system = json!({
        "id": "1616990171266", "messageType": "systemEventMessage", "from": null,
        "createdDateTime": "2026-10-09T08:16:11.266Z", "deletedDateTime": null,
        "body": { "contentType": "html", "content": "<systemEventMessage/>" },
        "attachments": [], "mentions": [], "reactions": [],
        "eventDetail": { "@odata.type": "#microsoft.graph.membersAddedEventMessageDetail", "members": [{ "id": "u-3", "displayName": "Alan Turing" }] }
    });
    let deleted = json!({
        "id": "1616990032099", "messageType": "message", "deletedDateTime": "2026-10-09T09:00:00Z",
        "from": { "user": { "id": "u-2", "displayName": "Grace Hopper", "userIdentityType": "aadUser" } },
        "body": { "contentType": "html", "content": "" }, "attachments": null, "mentions": null, "reactions": null
    });
    let bot = json!({
        "id": "1616990032100", "messageType": "message",
        "from": { "user": null, "device": null, "application": { "id": "bot-1", "displayName": "Deploy bot", "applicationIdentityType": "bot" } },
        "body": { "contentType": "text", "content": "Deployed 1.4.2 <ok>" }
    });
    let (server, socket, key) = answering(200, json!({ "value": [system, deleted, bot, chat_message()] })).await;
    let page = invoke(&socket, &key, "channel_messages.list", in_channel())
        .await
        .unwrap();
    assert_eq!(prefer(&only_request(&server).await), Some(ALL_KINDS));
    let items = page["items"].as_array().unwrap();
    assert_eq!(items[0]["messageType"], "systemEventMessage");
    assert_eq!(items[0]["from"], json!(null));
    assert_eq!(items[0]["text"], "");
    // What happened is in the detail, which is kept as Graph sent it.
    assert_eq!(
        items[0]["eventDetail"],
        json!({ "@odata.type": "#microsoft.graph.membersAddedEventMessageDetail", "members": [{ "id": "u-3", "displayName": "Alan Turing" }] })
    );
    assert_eq!(items[3]["eventDetail"], json!(null));
    assert_eq!(items[1]["deletedDateTime"], "2026-10-09T09:00:00Z");
    assert_eq!(items[1]["text"], "");
    assert_eq!(items[2]["from"]["application"]["applicationIdentityType"], "bot");
    assert_eq!(
        items[2]["text"], "Deployed 1.4.2 <ok>",
        "plain text is returned as it is"
    );
    assert_eq!(items[3]["text"], "@Ada Lovelace we shipped.\n[attachment: notes.docx]");
    assert_eq!(items[3]["body"]["contentType"], "html", "the HTML is kept beside it");
}

#[tokio::test]
async fn a_page_of_messages_or_chats_is_at_most_what_graph_allows() {
    let (server, socket, key) = answering(200, json!({ "value": [] })).await;
    for (name, input) in [
        ("channel_messages.list", with(in_channel(), json!({ "limit": 51 }))),
        (
            "channel_messages.replies",
            with(in_channel(), json!({ "message": "1", "limit": 51 })),
        ),
        ("chats.list", json!({ "limit": 51 })),
        ("chats.messages", json!({ "chat": CHAT, "limit": 100 })),
        ("chats.messages", json!({ "chat": CHAT, "limit": 0 })),
        ("teams.members", json!({ "team": "team-1", "limit": 1000 })),
        ("channels.members", with(in_channel(), json!({ "limit": 1000 }))),
        // These lists are not Graph's to size: it takes no page size for them.
        ("teams.list_joined", json!({ "limit": 10 })),
        ("channels.list", json!({ "team": "team-1", "limit": 10 })),
        ("chats.members", json!({ "chat": CHAT, "limit": 10 })),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert!(err.message().contains("limit"), "{name}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());

    for (name, input, top) in [
        (
            "channel_messages.list",
            with(in_channel(), json!({ "limit": 50 })),
            "50",
        ),
        ("chats.list", json!({ "limit": 50 }), "50"),
        ("teams.members", json!({ "team": "team-1", "limit": 999 }), "999"),
    ] {
        let (server, socket, key) = answering(200, json!({ "value": [] })).await;
        invoke(&socket, &key, name, input).await.unwrap();
        assert_eq!(query_of(&only_request(&server).await), json!({ "$top": top }), "{name}");
    }
}

#[tokio::test]
async fn the_next_page_of_a_conversation_is_asked_for_at_its_own_address() {
    let (server, socket, key) = microsoft().await;
    let next = format!(
        "{}/v1.0/teams('team-1')/channels('19:abc@thread.tacv2')/messages?$skiptoken=page-2",
        server.uri()
    );
    Mock::given(any())
        .respond_with(answer(
            200,
            &json!({ "value": [chat_message()], "@odata.nextLink": next }),
        ))
        .mount(&server)
        .await;
    let first = invoke(&socket, &key, "channel_messages.list", in_channel())
        .await
        .unwrap();
    assert_eq!(first["next_cursor"], next);

    server.reset().await;
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [chat_message()] })))
        .mount(&server)
        .await;
    let second = invoke(
        &socket,
        &key,
        "channel_messages.list",
        with(in_channel(), json!({ "cursor": next })),
    )
    .await
    .unwrap();
    assert_eq!(
        second["items"][0]["text"],
        "@Ada Lovelace we shipped.\n[attachment: notes.docx]"
    );
    let request = only_request(&server).await;
    assert_eq!(
        request.url.path(),
        format!("/v1.0/teams/team-1/channels/{CHANNEL_PATH}/messages")
    );
    assert_eq!(request.url.query(), Some("$skiptoken=page-2"));
    assert_eq!(prefer(&request), Some(ALL_KINDS));

    // A list with no page size is continued the same way.
    let (server, socket, key) = answering(200, json!({ "value": [member()] })).await;
    let cursor = format!("{}/v1.0/chats/x/members?$skiptoken=m-2", server.uri());
    invoke(
        &socket,
        &key,
        "chats.members",
        json!({ "chat": CHAT, "cursor": cursor }),
    )
    .await
    .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), format!("/v1.0/chats/{CHAT_PATH}/members"));
    assert_eq!(request.url.query(), Some("$skiptoken=m-2"));
}

#[tokio::test]
async fn throttling_on_a_channel_reaches_the_caller_with_the_wait() {
    // Graph allows one read of a channel's messages a second.
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(graph_error(429, "TooManyRequests", "Too many requests.").insert_header("retry-after", "30"))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "channel_messages.list", in_channel())
        .await
        .unwrap_err();
    assert_eq!(
        (err.kind(), err.retry()),
        (ErrorKind::RateLimited, Retry::After(Duration::from_secs(30)))
    );
}

#[tokio::test]
async fn a_permission_that_needs_an_administrator_is_a_refusal_with_graphs_reason() {
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(graph_error(
            403,
            "Forbidden",
            "Missing scope permissions on the request. API requires one of 'ChannelMessage.Read.All'.",
        ))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "channel_messages.list", in_channel())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().contains("ChannelMessage.Read.All"), "{}", err.message());
}

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error_and_never_repeats_what_was_said() {
    let secret = "CONFIDENTIAL: we are acquiring Contoso";
    for (name, input, response) in [
        (
            "teams.get",
            json!({ "team": "team-1" }),
            json!({ "displayName": secret }),
        ),
        ("teams.list_joined", json!({}), json!({ "teams": [] })),
        ("channels.get", in_channel(), json!({ "id": "", "description": secret })),
        (
            "channel_messages.get",
            with(in_channel(), json!({ "message": "1" })),
            json!({ "body": { "contentType": "html", "content": secret } }),
        ),
        (
            "channel_messages.list",
            in_channel(),
            json!({ "value": [{ "id": "1", "body": secret }] }),
        ),
        (
            "channel_messages.send",
            with(in_channel(), json!({ "body": { "content": "hello" } })),
            json!({ "body": { "content": secret } }),
        ),
        ("chats.get", json!({ "chat": CHAT }), json!({ "topic": secret })),
        (
            "chats.create",
            json!({ "chatType": "group", "members": ["u-1", "u-2", "u-3"] }),
            json!(null),
        ),
        (
            "chats.messages",
            json!({ "chat": CHAT }),
            json!({ "value": [{ "id": "1", "mentions": [{ "id": secret }] }] }),
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
    }
}

// ── Sending ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_message_is_sent_as_it_was_written_with_its_mentions() {
    let (server, socket, key) = answering(201, chat_message()).await;
    let mention = json!({ "id": 0, "mentionText": "Grace Hopper", "mentioned": { "user": { "id": "u-2", "displayName": "Grace Hopper", "userIdentityType": "aadUser" } } });
    let input = json!({
        "chat": CHAT,
        "body": { "contentType": "html", "content": "<at id=\"0\">Grace Hopper</at> can you look?" },
        "importance": "high",
        "mentions": [mention.clone()]
    });
    invoke(&socket, &key, "chats.send", input).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), format!("/v1.0/chats/{CHAT_PATH}/messages"));
    assert_eq!(
        body_of(&request),
        json!({
            "body": { "contentType": "html", "content": "<at id=\"0\">Grace Hopper</at> can you look?" },
            "importance": "high",
            "mentions": [mention]
        })
    );
}

#[tokio::test]
async fn a_message_with_nothing_in_it_is_not_sent() {
    let (server, socket, key) = answering(201, chat_message()).await;
    for body in [
        json!({}),
        json!({ "contentType": "html" }),
        json!({ "content": "" }),
        json!({ "contentType": "text", "content": " \n " }),
    ] {
        for (name, target) in [
            ("channel_messages.send", in_channel()),
            (
                "channel_messages.reply",
                with(in_channel(), json!({ "message": "1616990032035" })),
            ),
            ("chats.send", json!({ "chat": CHAT })),
        ] {
            let input = with(target, json!({ "body": body.clone() }));
            let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        }
    }
    // The body is required, and a field that is not known is refused, not dropped.
    for (input, field) in [
        (json!({ "chat": CHAT }), "body"),
        (
            json!({ "chat": CHAT, "body": { "content": "hi" }, "text": "hi" }),
            "text",
        ),
        (
            json!({ "chat": CHAT, "body": { "content": "hi", "content_type": "html" } }),
            "body.content_type",
        ),
    ] {
        let err = invoke(&socket, &key, "chats.send", input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(err.message().contains(field), "{}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_chat_is_created_among_the_people_named_and_nobody_else() {
    let (server, socket, key) = answering(201, chat()).await;
    let guest = "ada_fabrikam.com#EXT#@contoso.onmicrosoft.com";
    let members = json!([
        "u-1",
        " grace@contoso.example ",
        "it's-me",
        guest,
        "x%27)/manager?a=(%27"
    ]);
    let input = json!({ "chatType": "group", "topic": "Launch", "members": members });
    invoke(&socket, &key, "chats.create", input).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "POST");
    assert_eq!(request.url.path(), "/v1.0/chats");
    let users = format!("{}/v1.0/users", server.uri());
    let member = |user: &str| json!({ "@odata.type": "#microsoft.graph.aadUserConversationMember", "roles": ["owner"], "user@odata.bind": format!("{users}('{user}')") });
    assert_eq!(
        body_of(&request),
        json!({
            "chatType": "group",
            "topic": "Launch",
            // A name is written so that nothing in it is read as part of the
            // address: a quote is doubled, and then everything that is not a
            // letter or a digit is percent-encoded. A guest's sign-in name
            // holds `#`, which would otherwise end the address early.
            "members": [
                member("u-1"),
                member("grace%40contoso.example"),
                member("it%27%27s-me"),
                member("ada_fabrikam.com%23EXT%23%40contoso.onmicrosoft.com"),
                member("x%2527%29%2Fmanager%3Fa%3D%28%2527")
            ]
        })
    );

    let (server, socket, key) = answering(201, chat()).await;
    for input in [
        json!({ "chatType": "oneOnOne", "members": ["u-1"] }),
        json!({ "chatType": "oneOnOne", "members": ["u-1", "u-2", "u-3"] }),
        json!({ "chatType": "oneOnOne", "members": ["u-1", "u-2"], "topic": "Just us" }),
        json!({ "chatType": "group", "members": ["u-1"] }),
        json!({ "chatType": "group", "members": [] }),
        json!({ "chatType": "group", "members": ["u-1", " "] }),
        // A kind of chat that cannot be created, and one that is not a kind.
        json!({ "chatType": "meeting", "members": ["u-1", "u-2"] }),
        json!({ "chatType": "OneOnOne", "members": ["u-1", "u-2"] }),
        json!({ "members": ["u-1", "u-2"] }),
        json!({ "chatType": "group" }),
    ] {
        let err = invoke(&socket, &key, "chats.create", input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_message_is_sent_once_when_graph_fails() {
    let unavailable = || graph_error(503, "serviceNotAvailable", "The service is temporarily unavailable.");
    let hello = json!({ "body": { "content": "hello" } });
    for (name, input) in [
        ("channel_messages.send", with(in_channel(), hello.clone())),
        (
            "channel_messages.reply",
            with(in_channel(), with(hello.clone(), json!({ "message": "1" }))),
        ),
        ("chats.send", with(json!({ "chat": CHAT }), hello.clone())),
        (
            "chats.create",
            json!({ "chatType": "oneOnOne", "members": ["u-1", "u-2"] }),
        ),
    ] {
        let (server, socket, key) = microsoft().await;
        Mock::given(any()).respond_with(unavailable()).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            1,
            "{name}: it may have been posted, so it is not posted again"
        );
    }
}

#[tokio::test]
async fn an_id_is_one_path_segment_and_one_that_cannot_be_is_refused() {
    let (server, socket, key) = answering(200, chat_message()).await;
    let input = json!({ "team": "team/1", "channel": "19:a?b@thread.tacv2", "message": "../1" });
    invoke(&socket, &key, "channel_messages.get", input).await.unwrap();
    let request = only_request(&server).await;
    assert_eq!(
        request.url.path(),
        "/v1.0/teams/team%2F1/channels/19%3Aa%3Fb%40thread.tacv2/messages/..%2F1"
    );
    assert_eq!(request.url.query(), None);

    let (server, socket, key) = answering(200, chat_message()).await;
    for (name, input) in [
        ("teams.get", json!({ "team": ".." })),
        ("teams.members", json!({ "team": "" })),
        ("channels.list", json!({ "team": "." })),
        ("channels.get", json!({ "team": "team-1", "channel": " " })),
        ("channel_messages.get", with(in_channel(), json!({ "message": ".." }))),
        (
            "channel_messages.reply",
            with(in_channel(), json!({ "message": "", "body": { "content": "x" } })),
        ),
        ("chats.get", json!({ "chat": ".." })),
        ("chats.message_get", json!({ "chat": CHAT, "message": "" })),
        ("chats.send", json!({ "chat": "", "body": { "content": "x" } })),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let microsoft = Microsoft::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(microsoft.clone()), "eyJ.good").await;
    let connection = socket.connection(key).await.unwrap();
    Mock::given(any())
        .respond_with(answer(200, &json!({ "value": [chat_message()] })))
        .mount(&server)
        .await;

    let messages = microsoft
        .channel_messages(&connection)
        .list("team-1", CHANNEL, Paging::default())
        .await
        .unwrap();
    let first = &messages.items[0];
    assert_eq!(first.id, "1616990032035");
    assert_eq!(first.text, "@Ada Lovelace we shipped.\n[attachment: notes.docx]");
    assert_eq!(
        first
            .from
            .as_ref()
            .and_then(|from| from.user.as_ref())
            .and_then(|user| user.display_name.as_deref()),
        Some("Grace Hopper")
    );
    assert_eq!(first.mentions[0].id, Some(0));
    assert_eq!(first.attachments[0].name.as_deref(), Some("notes.docx"));
    assert_eq!(first.reactions[0].reaction_type.as_deref(), Some("like"));
    assert!(first.deleted_date_time.is_none() && first.last_edited_date_time.is_none());

    server.reset().await;
    Mock::given(any())
        .respond_with(answer(201, &chat_message()))
        .up_to_n_times(2)
        .mount(&server)
        .await;
    let sent = microsoft
        .chats(&connection)
        .send(CHAT, SendChatMessage::text("On my way."))
        .await
        .unwrap();
    assert_eq!(sent.id, "1616990032035");
    microsoft
        .channel_messages(&connection)
        .reply("team-1", CHANNEL, "1616990032035", SendChatMessage::html("<b>Done</b>"))
        .await
        .unwrap();
    let bodies: Vec<Value> = server.received_requests().await.unwrap().iter().map(body_of).collect();
    assert_eq!(
        bodies,
        [
            json!({ "body": { "contentType": "text", "content": "On my way." } }),
            json!({ "body": { "contentType": "html", "content": "<b>Done</b>" } }),
        ]
    );

    server.reset().await;
    Mock::given(any())
        .respond_with(answer(201, &chat()))
        .mount(&server)
        .await;
    let created = microsoft
        .chats(&connection)
        .create(CreateChat {
            chat_type: ChatType::OneOnOne,
            topic: None,
            members: vec!["u-1".into(), "u-2".into()],
        })
        .await
        .unwrap();
    assert_eq!(created.id, CHAT);
    assert_eq!(created.chat_type.as_deref(), Some("oneOnOne"));
    let joined = microsoft.teams(&connection).list_joined(Cursor::default()).await;
    assert_eq!(
        joined.unwrap_err().kind(),
        ErrorKind::Decode,
        "a chat is not a list of teams"
    );
}
