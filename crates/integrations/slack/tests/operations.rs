//! Every Slack operation, called by name against a local server that answers as Slack does.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Effect, ErrorKind, Integration, Socket};
use socketkit_slack::models::{History, PostMessage};
use socketkit_slack::{Slack, provider};
use socketkit_testkit::wiremock::matchers::{method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// One operation's expected behaviour.
struct Case {
    name: &'static str,
    input: Value,
    verb: &'static str,
    /// Slack's own method name, which is the last part of the URL.
    slack_method: &'static str,
    /// Arguments that must reach Slack: in the query for a read, in the JSON body for a write.
    sent: Value,
    response: Value,
    /// What the operation returns. Checked as a subset, so models may carry more fields.
    returns: Value,
}

const TS: &str = "1712345678.000100";

fn message() -> Value {
    json!({ "ts": TS, "user": "U1", "text": "hello", "reactions": [{ "name": "tada", "count": 2, "users": ["U1", "U2"] }] })
}

fn channel() -> Value {
    json!({ "id": "C1", "name": "eng", "is_channel": true, "is_member": true, "num_members": 4, "topic": { "value": "ship it" } })
}

fn user() -> Value {
    json!({ "id": "U1", "name": "ada", "real_name": "Ada Lovelace", "profile": { "email": "ada@example.test", "display_name": "ada" } })
}

#[rustfmt::skip]
fn cases() -> Vec<Case> {
    let case = |name, input, verb, slack_method, sent, response, returns| Case { name, input, verb, slack_method, sent, response, returns };
    let cursor = json!({ "next_cursor": "page-2" });
    vec![
        // chat
        case("chat.post_message", json!({ "channel": "C1", "text": "hello", "thread_ts": TS }), "POST", "chat.postMessage",
            json!({ "channel": "C1", "text": "hello", "thread_ts": TS }),
            json!({ "ok": true, "channel": "C1", "ts": TS, "message": message() }), json!({ "channel": "C1", "ts": TS, "message": { "text": "hello" } })),
        case("chat.post_ephemeral", json!({ "channel": "C1", "user": "U1", "text": "psst" }), "POST", "chat.postEphemeral",
            json!({ "channel": "C1", "user": "U1", "text": "psst" }), json!({ "ok": true, "message_ts": TS }), json!(TS)),
        case("chat.update", json!({ "channel": "C1", "ts": TS, "text": "edited" }), "POST", "chat.update",
            json!({ "channel": "C1", "ts": TS, "text": "edited" }), json!({ "ok": true, "channel": "C1", "ts": TS, "text": "edited" }), json!({ "channel": "C1", "ts": TS })),
        case("chat.delete", json!({ "channel": "C1", "ts": TS }), "POST", "chat.delete",
            json!({ "channel": "C1", "ts": TS }), json!({ "ok": true, "channel": "C1", "ts": TS }), json!(null)),
        case("chat.schedule_message", json!({ "channel": "C1", "post_at": 1_900_000_000, "text": "later" }), "POST", "chat.scheduleMessage",
            json!({ "channel": "C1", "post_at": 1_900_000_000, "text": "later" }),
            json!({ "ok": true, "channel": "C1", "scheduled_message_id": "Q1", "post_at": 1_900_000_000 }),
            json!({ "id": "Q1", "channel_id": "C1", "post_at": 1_900_000_000, "text": "later" })),
        case("chat.delete_scheduled_message", json!({ "channel": "C1", "scheduled_message_id": "Q1" }), "POST", "chat.deleteScheduledMessage",
            json!({ "channel": "C1", "scheduled_message_id": "Q1" }), json!({ "ok": true }), json!(null)),
        case("chat.scheduled_messages", json!({ "channel": "C1", "limit": 5 }), "POST", "chat.scheduledMessages.list",
            json!({ "channel": "C1", "limit": 5 }),
            json!({ "ok": true, "scheduled_messages": [{ "id": "Q1", "channel_id": "C1", "post_at": 1_900_000_000, "text": "later" }], "response_metadata": cursor }),
            json!({ "items": [{ "id": "Q1", "channel_id": "C1" }], "next_cursor": "page-2" })),
        case("chat.permalink", json!({ "channel": "C1", "message_ts": TS }), "GET", "chat.getPermalink",
            json!({ "channel": "C1", "message_ts": TS }), json!({ "ok": true, "permalink": "https://acme.slack.com/archives/C1/p1" }), json!("https://acme.slack.com/archives/C1/p1")),

        // conversations
        case("conversations.list", json!({ "types": "public_channel,private_channel", "exclude_archived": true, "limit": 2 }), "GET", "conversations.list",
            json!({ "types": "public_channel,private_channel", "exclude_archived": "true", "limit": "2" }),
            json!({ "ok": true, "channels": [channel()], "response_metadata": cursor }), json!({ "items": [{ "id": "C1", "name": "eng" }], "next_cursor": "page-2" })),
        case("conversations.info", json!({ "channel": "C1" }), "GET", "conversations.info",
            json!({ "channel": "C1", "include_num_members": "true" }), json!({ "ok": true, "channel": channel() }), json!({ "id": "C1", "num_members": 4, "topic": { "value": "ship it" } })),
        case("conversations.history", json!({ "channel": "C1", "limit": 1, "oldest": "1712000000.000000" }), "GET", "conversations.history",
            json!({ "channel": "C1", "limit": "1", "oldest": "1712000000.000000" }),
            json!({ "ok": true, "messages": [message()], "has_more": true, "response_metadata": cursor }),
            json!({ "items": [{ "ts": TS, "text": "hello", "reactions": [{ "name": "tada", "count": 2 }] }], "next_cursor": "page-2" })),
        case("conversations.replies", json!({ "channel": "C1", "thread_ts": TS }), "GET", "conversations.replies",
            json!({ "channel": "C1", "ts": TS }), json!({ "ok": true, "messages": [message()] }), json!({ "items": [{ "ts": TS }], "next_cursor": null })),
        case("conversations.members", json!({ "channel": "C1" }), "GET", "conversations.members",
            json!({ "channel": "C1" }), json!({ "ok": true, "members": ["U1", "U2"], "response_metadata": { "next_cursor": "" } }), json!({ "items": ["U1", "U2"], "next_cursor": null })),
        case("conversations.create", json!({ "name": "launch", "is_private": true }), "POST", "conversations.create",
            json!({ "name": "launch", "is_private": true }), json!({ "ok": true, "channel": channel() }), json!({ "id": "C1" })),
        case("conversations.join", json!({ "channel": "C1" }), "POST", "conversations.join", json!({ "channel": "C1" }), json!({ "ok": true, "channel": channel() }), json!({ "id": "C1" })),
        case("conversations.leave", json!({ "channel": "C1" }), "POST", "conversations.leave", json!({ "channel": "C1" }), json!({ "ok": true }), json!(null)),
        case("conversations.invite", json!({ "channel": "C1", "users": ["U1", "U2"] }), "POST", "conversations.invite",
            json!({ "channel": "C1", "users": "U1,U2" }), json!({ "ok": true, "channel": channel() }), json!({ "id": "C1" })),
        case("conversations.kick", json!({ "channel": "C1", "user": "U2" }), "POST", "conversations.kick", json!({ "channel": "C1", "user": "U2" }), json!({ "ok": true }), json!(null)),
        case("conversations.archive", json!({ "channel": "C1" }), "POST", "conversations.archive", json!({ "channel": "C1" }), json!({ "ok": true }), json!(null)),
        case("conversations.unarchive", json!({ "channel": "C1" }), "POST", "conversations.unarchive", json!({ "channel": "C1" }), json!({ "ok": true }), json!(null)),
        case("conversations.rename", json!({ "channel": "C1", "name": "eng-core" }), "POST", "conversations.rename",
            json!({ "channel": "C1", "name": "eng-core" }), json!({ "ok": true, "channel": channel() }), json!({ "id": "C1" })),
        case("conversations.set_topic", json!({ "channel": "C1", "topic": "ship it" }), "POST", "conversations.setTopic", json!({ "channel": "C1", "topic": "ship it" }), json!({ "ok": true }), json!(null)),
        case("conversations.set_purpose", json!({ "channel": "C1", "purpose": "releases" }), "POST", "conversations.setPurpose", json!({ "channel": "C1", "purpose": "releases" }), json!({ "ok": true }), json!(null)),
        case("conversations.open", json!({ "users": ["U2"] }), "POST", "conversations.open",
            json!({ "users": "U2" }), json!({ "ok": true, "channel": { "id": "D1", "is_im": true, "user": "U2" } }), json!({ "id": "D1", "is_im": true, "user": "U2" })),
        case("conversations.mark", json!({ "channel": "C1", "ts": TS }), "POST", "conversations.mark", json!({ "channel": "C1", "ts": TS }), json!({ "ok": true }), json!(null)),

        // users
        case("users.list", json!({ "limit": 1, "cursor": "page-1" }), "GET", "users.list",
            json!({ "limit": "1", "cursor": "page-1" }), json!({ "ok": true, "members": [user()], "response_metadata": cursor }), json!({ "items": [{ "id": "U1", "name": "ada" }], "next_cursor": "page-2" })),
        case("users.info", json!({ "user": "U1" }), "GET", "users.info", json!({ "user": "U1" }), json!({ "ok": true, "user": user() }), json!({ "id": "U1", "profile": { "email": "ada@example.test" } })),
        case("users.lookup_by_email", json!({ "email": "ada@example.test" }), "GET", "users.lookupByEmail", json!({ "email": "ada@example.test" }), json!({ "ok": true, "user": user() }), json!({ "id": "U1" })),
        case("users.presence", json!({ "user": "U1" }), "GET", "users.getPresence", json!({ "user": "U1" }), json!({ "ok": true, "presence": "active", "online": true }), json!({ "presence": "active", "online": true })),
        case("users.profile", json!({ "user": "U1" }), "GET", "users.profile.get", json!({ "user": "U1" }),
            json!({ "ok": true, "profile": { "display_name": "ada", "status_text": "on call" } }), json!({ "display_name": "ada", "status_text": "on call" })),

        // reactions
        case("reactions.add", json!({ "channel": "C1", "timestamp": TS, "name": ":tada:" }), "POST", "reactions.add",
            json!({ "channel": "C1", "timestamp": TS, "name": "tada" }), json!({ "ok": true }), json!(null)),
        case("reactions.remove", json!({ "channel": "C1", "timestamp": TS, "name": "tada" }), "POST", "reactions.remove",
            json!({ "channel": "C1", "timestamp": TS, "name": "tada" }), json!({ "ok": true }), json!(null)),
        case("reactions.get", json!({ "channel": "C1", "timestamp": TS }), "GET", "reactions.get",
            json!({ "channel": "C1", "timestamp": TS, "full": "true" }), json!({ "ok": true, "type": "message", "message": message() }), json!([{ "name": "tada", "count": 2, "users": ["U1", "U2"] }])),

        // pins
        case("pins.add", json!({ "channel": "C1", "timestamp": TS }), "POST", "pins.add", json!({ "channel": "C1", "timestamp": TS }), json!({ "ok": true }), json!(null)),
        case("pins.remove", json!({ "channel": "C1", "timestamp": TS }), "POST", "pins.remove", json!({ "channel": "C1", "timestamp": TS }), json!({ "ok": true }), json!(null)),
        case("pins.list", json!({ "channel": "C1" }), "GET", "pins.list", json!({ "channel": "C1" }),
            json!({ "ok": true, "items": [{ "type": "message", "created": 1_712_345_678, "created_by": "U1", "message": message() }] }),
            json!([{ "type": "message", "created_by": "U1", "message": { "ts": TS } }])),

        // files
        case("files.info", json!({ "file": "F1" }), "GET", "files.info", json!({ "file": "F1" }),
            json!({ "ok": true, "file": { "id": "F1", "name": "plan.pdf", "mimetype": "application/pdf", "size": 1024 } }), json!({ "id": "F1", "name": "plan.pdf", "size": 1024 })),
        case("files.list", json!({ "channel": "C1", "types": "pdfs", "count": 10 }), "GET", "files.list",
            json!({ "channel": "C1", "types": "pdfs", "count": "10" }), json!({ "ok": true, "files": [{ "id": "F1", "name": "plan.pdf" }] }), json!([{ "id": "F1" }])),
        case("files.delete", json!({ "file": "F1" }), "POST", "files.delete", json!({ "file": "F1" }), json!({ "ok": true }), json!(null)),

        // search
        case("search.messages", json!({ "query": "in:#eng deploy", "count": 5, "sort": "timestamp" }), "GET", "search.messages",
            json!({ "query": "in:#eng deploy", "count": "5", "sort": "timestamp" }),
            json!({ "ok": true, "messages": { "total": 1, "matches": [{ "ts": TS, "text": "deploy done", "username": "ada", "permalink": "https://x", "channel": { "id": "C1", "name": "eng" } }] } }),
            json!({ "total": 1, "matches": [{ "ts": TS, "text": "deploy done", "channel": { "id": "C1" } }] })),

        // reminders
        case("reminders.add", json!({ "text": "stand-up", "time": "in 15 minutes" }), "POST", "reminders.add",
            json!({ "text": "stand-up", "time": "in 15 minutes" }), json!({ "ok": true, "reminder": { "id": "Rm1", "text": "stand-up", "time": 1_712_346_578 } }), json!({ "id": "Rm1", "text": "stand-up" })),
        case("reminders.list", json!({}), "GET", "reminders.list", json!({}), json!({ "ok": true, "reminders": [{ "id": "Rm1", "text": "stand-up" }] }), json!([{ "id": "Rm1" }])),
        case("reminders.delete", json!({ "reminder": "Rm1" }), "POST", "reminders.delete", json!({ "reminder": "Rm1" }), json!({ "ok": true }), json!(null)),
        case("reminders.complete", json!({ "reminder": "Rm1" }), "POST", "reminders.complete", json!({ "reminder": "Rm1" }), json!({ "ok": true }), json!(null)),

        // bookmarks
        case("bookmarks.add", json!({ "channel": "C1", "title": "Runbook", "link": "https://runbook.example.test", "emoji": ":book:" }), "POST", "bookmarks.add",
            json!({ "channel_id": "C1", "title": "Runbook", "type": "link", "link": "https://runbook.example.test", "emoji": ":book:" }),
            json!({ "ok": true, "bookmark": { "id": "Bk1", "channel_id": "C1", "title": "Runbook", "link": "https://runbook.example.test", "type": "link" } }),
            json!({ "id": "Bk1", "channel_id": "C1", "title": "Runbook", "type": "link" })),
        case("bookmarks.list", json!({ "channel": "C1" }), "POST", "bookmarks.list", json!({ "channel_id": "C1" }),
            json!({ "ok": true, "bookmarks": [{ "id": "Bk1", "channel_id": "C1", "title": "Runbook" }] }), json!([{ "id": "Bk1" }])),
        case("bookmarks.remove", json!({ "channel": "C1", "bookmark": "Bk1" }), "POST", "bookmarks.remove", json!({ "channel_id": "C1", "bookmark_id": "Bk1" }), json!({ "ok": true }), json!(null)),

        // user groups
        case("usergroups.list", json!({ "include_count": true }), "GET", "usergroups.list", json!({ "include_count": "true" }),
            json!({ "ok": true, "usergroups": [{ "id": "S1", "name": "Engineering", "handle": "engineering", "user_count": 12 }] }), json!([{ "id": "S1", "handle": "engineering", "user_count": 12 }])),
        case("usergroups.members", json!({ "usergroup": "S1" }), "GET", "usergroups.users.list", json!({ "usergroup": "S1" }), json!({ "ok": true, "users": ["U1", "U2"] }), json!(["U1", "U2"])),

        // workspace
        case("team.info", json!({}), "GET", "team.info", json!({}), json!({ "ok": true, "team": { "id": "T1", "name": "Acme", "domain": "acme" } }), json!({ "id": "T1", "name": "Acme", "domain": "acme" })),
        case("emoji.list", json!({}), "GET", "emoji.list", json!({}), json!({ "ok": true, "emoji": { "shipit": "https://emoji.example.test/shipit.png", "ship": "alias:shipit" } }), json!({ "ship": "alias:shipit" })),
        case("dnd.info", json!({ "user": "U1" }), "GET", "dnd.info", json!({ "user": "U1" }), json!({ "ok": true, "dnd_enabled": true, "snooze_enabled": false }), json!({ "dnd_enabled": true, "snooze_enabled": false })),
        case("dnd.set_snooze", json!({ "minutes": 30 }), "POST", "dnd.setSnooze", json!({ "num_minutes": "30" }), json!({ "ok": true, "snooze_enabled": true, "snooze_endtime": 1_712_347_478 }), json!({ "snooze_enabled": true })),
        case("dnd.end_snooze", json!({}), "POST", "dnd.endSnooze", json!({}), json!({ "ok": true, "dnd_enabled": false, "snooze_enabled": false }), json!({ "snooze_enabled": false })),
    ]
}

/// True when every part of `expected` is present in `actual`.
fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => e.iter().all(|(k, v)| a.get(k).is_some_and(|av| contains(av, v))),
        (Value::Array(a), Value::Array(e)) => a.len() == e.len() && a.iter().zip(e).all(|(av, ev)| contains(av, ev)),
        _ => actual == expected,
    }
}

async fn slack() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Slack::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, "xoxb-good").await;
    (server, socket, key)
}

#[tokio::test]
async fn the_table_below_covers_every_operation_slack_offers() {
    let listed: Vec<String> = Slack::new().operations().into_iter().map(|o| o.name).collect();
    let mut tested: Vec<String> = cases().iter().map(|c| format!("slack.{}", c.name)).collect();
    tested.extend(["slack.identity.get".to_owned(), "slack.resource.resolve".to_owned()]);
    for name in &listed {
        assert!(tested.contains(name), "{name} has no test case");
    }
    assert_eq!(
        listed.len(),
        tested.len(),
        "a test case names an operation that does not exist"
    );
    assert_eq!(listed.len(), 56);
}

#[tokio::test]
async fn every_operation_calls_the_right_slack_method_and_returns_what_slack_sent() {
    for case in cases() {
        let (server, socket, key) = slack().await;
        Mock::given(method(case.verb))
            .and(path(format!("/api/{}", case.slack_method)))
            .respond_with(ResponseTemplate::new(200).set_body_json(case.response.clone()))
            .mount(&server)
            .await;

        let output = socket
            .invoke(key, format!("slack.{}", case.name), case.input.clone())
            .await
            .unwrap_or_else(|e| panic!("{}: {e}", case.name));
        assert!(
            contains(&output, &case.returns),
            "{}: returned {output}, expected {}",
            case.name,
            case.returns
        );

        let received = server.received_requests().await.unwrap();
        assert_eq!(received.len(), 1, "{}: one call to Slack", case.name);
        let request = &received[0];
        assert_eq!(
            request.headers.get("authorization").unwrap(),
            "Bearer xoxb-good",
            "{}",
            case.name
        );
        let sent: Value = if request.url.query().is_some() {
            Value::Object(
                request
                    .url
                    .query_pairs()
                    .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
                    .collect(),
            )
        } else if request.body.is_empty() {
            json!({})
        } else {
            serde_json::from_slice(&request.body).unwrap()
        };
        assert!(
            contains(&sent, &case.sent),
            "{}: sent {sent}, expected {}",
            case.name,
            case.sent
        );
    }
}

#[tokio::test]
async fn every_operation_describes_its_input_and_marks_what_it_changes() {
    let operations = Slack::new().operations();
    let find = |name: &str| {
        operations
            .iter()
            .find(|o| o.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    };
    for operation in &operations {
        assert_eq!(operation.input_schema["type"], "object", "{}", operation.name);
        assert!(!operation.description.is_empty(), "{}", operation.name);
    }

    let post = find("slack.chat.post_message");
    assert_eq!(post.effect, Effect::Write);
    assert_eq!(post.required_scopes, ["chat:write"]);
    let required: Vec<&str> = post.input_schema["required"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(
        required,
        ["channel"],
        "the channel is required; the content is optional field by field"
    );
    for field in ["channel", "text", "blocks", "thread_ts"] {
        assert!(
            post.input_schema["properties"].get(field).is_some(),
            "{field} is described"
        );
    }
    assert!(post.output_schema["properties"].get("ts").is_some());

    assert_eq!(find("slack.conversations.history").effect, Effect::Read);
    for destructive in [
        "slack.chat.delete",
        "slack.conversations.archive",
        "slack.conversations.kick",
        "slack.files.delete",
    ] {
        assert_eq!(find(destructive).effect, Effect::Destructive, "{destructive}");
    }
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/chat.postMessage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": true, "channel": "C1", "ts": TS })))
        .mount(&server)
        .await;
    socket
        .invoke(
            key,
            "slack.chat.post_message".into(),
            json!({ "channel": "C1", "text": "hi" }),
        )
        .await
        .unwrap();
    let body: Value = serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(
        body,
        json!({ "channel": "C1", "text": "hi" }),
        "Slack applies its own defaults to everything else"
    );
}

#[tokio::test]
async fn input_of_the_wrong_shape_is_refused_by_name_without_calling_slack() {
    let (server, socket, key) = slack().await;
    let bad = [
        ("slack.chat.post_message", json!({ "text": "no channel" }), "channel"),
        (
            "slack.chat.post_message",
            json!({ "channel": 7, "text": "x" }),
            "channel",
        ),
        (
            "slack.chat.schedule_message",
            json!({ "channel": "C1", "post_at": "tomorrow", "text": "x" }),
            "post_at",
        ),
        (
            "slack.conversations.invite",
            json!({ "channel": "C1", "users": "U1" }),
            "users",
        ),
        ("slack.dnd.set_snooze", json!({ "minutes": -5 }), "minutes"),
    ];
    for (operation, input, field) in bad {
        let err = socket
            .invoke(key.clone(), operation.into(), input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation} {input}");
        assert!(
            err.message().contains(field),
            "{operation}: the message names the field: {}",
            err.message()
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn values_that_cannot_work_are_refused_before_slack_is_called() {
    let (server, socket, key) = slack().await;
    let bad = [
        ("slack.chat.post_message", json!({ "channel": "C1" })),
        ("slack.chat.post_message", json!({ "channel": "C1", "text": "   " })),
        ("slack.chat.post_message", json!({ "channel": "C1", "blocks": [] })),
        ("slack.chat.post_message", json!({ "channel": " ", "text": "hi" })),
        ("slack.chat.delete", json!({ "channel": "C1", "ts": "" })),
        ("slack.conversations.invite", json!({ "channel": "C1", "users": [] })),
        (
            "slack.conversations.invite",
            json!({ "channel": "C1", "users": ["U1,U2"] }),
        ),
        ("slack.conversations.open", json!({ "users": [" "] })),
        (
            "slack.reactions.add",
            json!({ "channel": "C1", "timestamp": TS, "name": "::" }),
        ),
        ("slack.dnd.set_snooze", json!({ "minutes": 0 })),
    ];
    for (operation, input) in bad {
        let err = socket
            .invoke(key.clone(), operation.into(), input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{operation} {input}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn slacks_error_codes_reach_the_caller_as_errors_they_can_act_on() {
    for (code, extra, kind) in [
        ("channel_not_found", json!({}), ErrorKind::NotFound),
        ("not_in_channel", json!({}), ErrorKind::AccessDenied),
        (
            "missing_scope",
            json!({ "needed": "chat:write" }),
            ErrorKind::AccessDenied,
        ),
        ("token_revoked", json!({}), ErrorKind::ReconnectRequired),
        ("msg_too_long", json!({}), ErrorKind::InvalidInput),
    ] {
        let (server, socket, key) = slack().await;
        let mut body = json!({ "ok": false, "error": code });
        body.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
        Mock::given(path("/api/chat.postMessage"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .mount(&server)
            .await;
        let input = json!({ "channel": "C1", "text": "hi" });
        let err = socket
            .invoke(key, "slack.chat.post_message".into(), input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), kind, "{code}");
    }
}

#[tokio::test]
async fn a_write_is_sent_once_even_when_slack_reports_a_failure_it_may_have_processed() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/chat.postMessage"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "ok": false, "error": "fatal_error" })))
        .mount(&server)
        .await;
    Mock::given(path("/api/chat.delete"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    let post = socket
        .invoke(
            key.clone(),
            "slack.chat.post_message".into(),
            json!({ "channel": "C1", "text": "hi" }),
        )
        .await;
    assert_eq!(post.unwrap_err().kind(), ErrorKind::Unexpected);
    socket
        .invoke(key, "slack.chat.delete".into(), json!({ "channel": "C1", "ts": TS }))
        .await
        .unwrap_err();
    let received = server.received_requests().await.unwrap();
    assert_eq!(
        received.len(),
        2,
        "one attempt each: a message must not be posted twice"
    );
}

#[tokio::test]
async fn a_success_that_lacks_what_was_asked_for_is_an_error() {
    for (operation, input, slack_method, response) in [
        (
            "slack.chat.post_message",
            json!({ "channel": "C1", "text": "hi" }),
            "chat.postMessage",
            json!({ "ok": true }),
        ),
        (
            "slack.conversations.info",
            json!({ "channel": "C1" }),
            "conversations.info",
            json!({ "ok": true }),
        ),
        (
            "slack.conversations.info",
            json!({ "channel": "C1" }),
            "conversations.info",
            json!({ "ok": true, "channel": { "name": "no-id" } }),
        ),
        (
            "slack.conversations.history",
            json!({ "channel": "C1" }),
            "conversations.history",
            json!({ "ok": true, "messages": "nope" }),
        ),
        (
            "slack.users.info",
            json!({ "user": "U1" }),
            "users.info",
            json!({ "ok": true, "user": {} }),
        ),
        (
            "slack.chat.permalink",
            json!({ "channel": "C1", "message_ts": TS }),
            "chat.getPermalink",
            json!({ "ok": true }),
        ),
    ] {
        let (server, socket, key) = slack().await;
        Mock::given(path(format!("/api/{slack_method}")))
            .respond_with(ResponseTemplate::new(200).set_body_json(response.clone()))
            .mount(&server)
            .await;
        let err = socket.invoke(key, operation.into(), input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{operation} given {response}");
    }
}

#[tokio::test]
async fn the_typed_methods_take_identifiers_as_arguments_and_content_as_structs() {
    let (server, socket, key) = slack().await;
    Mock::given(path("/api/chat.postMessage"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "ok": true, "channel": "C1", "ts": TS, "message": message() })),
        )
        .mount(&server)
        .await;
    Mock::given(path("/api/conversations.history"))
        .respond_with(ResponseTemplate::new(200).set_body_json(
            json!({ "ok": true, "messages": [message()], "response_metadata": { "next_cursor": "next" } }),
        ))
        .mount(&server)
        .await;

    let slack = Slack::new();
    let connection = socket.connection(key).await.unwrap();
    let posted = slack
        .chat(&connection)
        .post_message("C1", PostMessage::text("hello").in_thread(TS))
        .await
        .unwrap();
    assert_eq!((posted.channel.as_str(), posted.ts.as_str()), ("C1", TS));
    assert_eq!(posted.message.unwrap().text, "hello");

    let page = slack
        .conversations(&connection)
        .history(
            "C1",
            History {
                limit: Some(1),
                ..History::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].reactions[0].name, "tada");
    assert_eq!(page.next_cursor.as_deref(), Some("next"));

    let body: Value = serde_json::from_slice(&server.received_requests().await.unwrap()[0].body).unwrap();
    assert_eq!(body, json!({ "channel": "C1", "text": "hello", "thread_ts": TS }));
}
