//! What the Microsoft tests share: a local server that answers as Graph does,
//! and ways to read what reached it.

#![allow(dead_code)] // Each test file uses its own part of this.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::{ConnectionKey, Integration, Socket};
use socketkit_microsoft::{Microsoft, provider};
use socketkit_testkit::wiremock::matchers::any;
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

/// The header that asks Graph to write every time in UTC.
pub const IN_UTC: &str = "outlook.timezone=\"UTC\"";

/// The header that asks Graph for the body of a message as plain text, and as HTML.
pub const AS_TEXT: &str = "outlook.body-content-type=\"text\"";
pub const AS_HTML: &str = "outlook.body-content-type=\"html\"";

/// One operation's expected behaviour.
pub struct Case {
    pub name: &'static str,
    pub input: Value,
    pub verb: &'static str,
    /// The path below Graph's `v1.0`.
    pub path: &'static str,
    /// Exactly the query parameters that reach Graph.
    pub query: Value,
    /// Exactly the JSON body that reaches Graph; `null` when there is none.
    pub body: Value,
    /// Exactly the `Prefer` header that reaches Graph, if any.
    pub prefer: Option<&'static str>,
    /// The `Accept` header that reaches Graph: JSON, or the text format asked
    /// for. When it is not JSON, `response` is a string and is sent as that text.
    pub accept: &'static str,
    pub status: u16,
    pub response: Value,
    /// What the operation returns. Checked as a subset, so models may carry more fields.
    pub returns: Value,
}

/// True when every part of `expected` is present in `actual`.
pub fn contains(actual: &Value, expected: &Value) -> bool {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => e.iter().all(|(k, v)| a.get(k).is_some_and(|av| contains(av, v))),
        (Value::Array(a), Value::Array(e)) => a.len() == e.len() && a.iter().zip(e).all(|(av, ev)| contains(av, ev)),
        _ => actual == expected,
    }
}

pub fn answer(status: u16, body: &Value) -> ResponseTemplate {
    if body.is_null() {
        ResponseTemplate::new(status)
    } else {
        ResponseTemplate::new(status).set_body_json(body.clone())
    }
}

pub fn graph_error(status: u16, code: &str, message: &str) -> ResponseTemplate {
    ResponseTemplate::new(status).set_body_json(json!({ "error": { "code": code, "message": message } }))
}

pub fn query_of(request: &Request) -> Value {
    Value::Object(
        request
            .url
            .query_pairs()
            .map(|(k, v)| (k.into_owned(), Value::String(v.into_owned())))
            .collect(),
    )
}

pub fn body_of(request: &Request) -> Value {
    if request.body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&request.body).unwrap()
    }
}

pub fn prefer(request: &Request) -> Option<&str> {
    request.headers.get("prefer").map(|value| value.to_str().unwrap())
}

pub async fn microsoft() -> (MockServer, Socket, ConnectionKey) {
    let server = MockServer::start().await;
    let integration: Arc<dyn Integration> = Arc::new(Microsoft::with_spec(point_at(provider(), &server)));
    let (socket, key) = connect(integration, "eyJ.good").await;
    (server, socket, key)
}

/// A server that answers every request with `status` and `body`.
pub async fn answering(status: u16, body: Value) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = microsoft().await;
    Mock::given(any())
        .respond_with(answer(status, &body))
        .mount(&server)
        .await;
    (server, socket, key)
}

pub async fn invoke(socket: &Socket, key: &ConnectionKey, name: &str, input: Value) -> socketkit_core::Result<Value> {
    socket.invoke(key.clone(), format!("microsoft.{name}"), input).await
}

/// The one request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut received = server.received_requests().await.unwrap();
    assert_eq!(received.len(), 1, "one call to Graph");
    received.remove(0)
}

/// A received message as Graph returns it when asked for the body as text.
pub fn message() -> Value {
    json!({
        "id": "msg-1",
        "conversationId": "conv-1",
        "subject": "Q3 plan",
        "bodyPreview": "Attached is the plan",
        "body": { "contentType": "text", "content": "Attached is the plan for Q3." },
        "from": { "emailAddress": { "name": "Grace Hopper", "address": "grace@contoso.example" } },
        "sender": { "emailAddress": { "name": "Grace Hopper", "address": "grace@contoso.example" } },
        "toRecipients": [{ "emailAddress": { "name": "Ada Lovelace", "address": "ada@contoso.example" } }],
        "ccRecipients": [{ "emailAddress": { "name": "Alan Turing", "address": "alan@contoso.example" } }],
        "bccRecipients": [],
        "replyTo": [],
        "receivedDateTime": "2026-10-09T08:15:00Z",
        "sentDateTime": "2026-10-09T08:14:58Z",
        "isRead": false,
        "isDraft": false,
        "hasAttachments": true,
        "importance": "normal",
        "categories": ["Customer"],
        "flag": { "flagStatus": "notFlagged" },
        "parentFolderId": "folder-inbox",
        "internetMessageId": "<abc@contoso.example>",
        "isDeliveryReceiptRequested": null,
        "webLink": "https://outlook.office365.com/owa/?ItemID=msg-1"
    })
}

/// What an operation that returns `message()` must pass on.
pub fn message_returned() -> Value {
    json!({
        "id": "msg-1",
        "conversationId": "conv-1",
        "subject": "Q3 plan",
        "from": { "emailAddress": { "address": "grace@contoso.example" } },
        "toRecipients": [{ "emailAddress": { "address": "ada@contoso.example" } }],
        "ccRecipients": [{ "emailAddress": { "address": "alan@contoso.example" } }],
        "receivedDateTime": "2026-10-09T08:15:00Z",
        "bodyPreview": "Attached is the plan",
        "body": { "contentType": "text", "content": "Attached is the plan for Q3." },
        "isRead": false,
        "hasAttachments": true,
        "webLink": "https://outlook.office365.com/owa/?ItemID=msg-1"
    })
}

pub fn to(address: &str) -> Value {
    json!([{ "emailAddress": { "address": address } }])
}

/// A meeting's id as Graph writes it, with characters that must be encoded in a path.
pub const MEETING: &str = "MSpkYzE3Njc0Yy04MWQ5*MCoqMTk6bWVldGluZ18@thread.v2";
pub const MEETING_PATH: &str = "MSpkYzE3Njc0Yy04MWQ5%2AMCoqMTk6bWVldGluZ18%40thread.v2";
/// A transcript's id ends in base64 padding.
pub const TRANSCRIPT: &str = "MSMjMCMjNzU3ODc2ZDY=";
pub const TRANSCRIPT_PATH: &str = "MSMjMCMjNzU3ODc2ZDY%3D";
/// A join link as it appears on a calendar event: already percent-encoded once.
pub const MEETING_LINK: &str =
    "https://teams.microsoft.com/l/meetup-join/19%3ameeting_MGQ4%40thread.v2/0?context=%7b%22Tid%22%3a%22909c%22%7d";
pub const VTT: &str = "WEBVTT\n\n00:00:16.246 --> 00:00:17.726\n<v Ada Lovelace>We ship on Friday.</v>\n";

pub fn organizer() -> Value {
    json!({ "application": null, "device": null, "user": { "id": "u-1", "displayName": null, "tenantId": "t-1" } })
}

pub fn meeting() -> Value {
    json!({
        "id": MEETING,
        "subject": "Launch review",
        "startDateTime": "2026-09-29T22:35:31.389759Z",
        "endDateTime": "2026-09-29T23:35:31.389759Z",
        "joinWebUrl": MEETING_LINK,
        "allowTranscription": true,
        "chatInfo": { "threadId": "19:meeting_MGQ4@thread.v2", "messageId": "0", "replyChainMessageId": null },
        "participants": {
            "organizer": { "upn": "ada@example.test", "role": "presenter", "identity": organizer() },
            "attendees": null
        }
    })
}

pub fn transcript() -> Value {
    json!({
        "id": TRANSCRIPT,
        "meetingId": MEETING,
        "callId": "af630fe0",
        "contentCorrelationId": "bc842d7a-0",
        "createdDateTime": "2026-09-17T06:09:24.8968037Z",
        "endDateTime": "2026-09-17T06:27:25.2346000Z",
        "transcriptContentUrl": "https://graph.microsoft.com/v1.0/me/onlineMeetings/m/transcripts/t/content",
        "meetingOrganizer": organizer()
    })
}

/// The header that asks Graph to name the kinds it has added since v1.0 was fixed.
pub const ALL_KINDS: &str = "include-unknown-enum-members";

/// A channel's id and a chat's id as Graph writes them, and as they go into a path.
pub const CHANNEL: &str = "19:abc@thread.tacv2";
pub const CHANNEL_PATH: &str = "19%3Aabc%40thread.tacv2";
pub const CHAT: &str = "19:u-1_u-2@unq.gbl.spaces";
pub const CHAT_PATH: &str = "19%3Au-1_u-2%40unq.gbl.spaces";

pub fn team() -> Value {
    json!({ "id": "team-1", "displayName": "Launch crew", "description": "Ships things", "isArchived": false, "tenantId": "t-1",
            "visibility": null, "webUrl": null, "createdDateTime": null })
}

pub fn channel() -> Value {
    json!({ "id": CHANNEL, "displayName": "General", "description": "Everything", "membershipType": "standard", "isArchived": false,
            "webUrl": "https://teams.microsoft.com/l/channel/19%3Aabc%40thread.tacv2/General", "email": "", "createdDateTime": "2026-01-05T09:00:00Z" })
}

pub fn member() -> Value {
    json!({ "@odata.type": "#microsoft.graph.aadUserConversationMember", "id": "MCMjMSMj", "roles": ["owner"], "displayName": "Ada Lovelace",
            "userId": "u-1", "email": "ada@contoso.example", "tenantId": "t-1", "visibleHistoryStartDateTime": "0001-01-01T00:00:00Z" })
}

pub fn chat() -> Value {
    json!({ "id": CHAT, "topic": null, "chatType": "oneOnOne", "createdDateTime": "2026-10-01T09:00:00Z", "lastUpdatedDateTime": "2026-10-09T08:00:00Z",
            "webUrl": "https://teams.microsoft.com/l/chat/19%3Au-1_u-2%40unq.gbl.spaces/0", "tenantId": "t-1", "onlineMeetingInfo": null })
}

/// A message in a channel, with a mention, an attachment and a reaction.
pub fn chat_message() -> Value {
    json!({
        "id": "1616990032035", "replyToId": null, "etag": "1616990032035", "messageType": "message",
        "createdDateTime": "2026-10-09T08:13:52.035Z", "lastModifiedDateTime": "2026-10-09T08:20:00.000Z",
        "lastEditedDateTime": null, "deletedDateTime": null, "subject": "Release", "summary": null, "chatId": null,
        "importance": "normal", "locale": "en-us",
        "webUrl": "https://teams.microsoft.com/l/message/19%3Aabc%40thread.tacv2/1616990032035",
        "from": { "application": null, "device": null, "user": { "id": "u-2", "displayName": "Grace Hopper", "userIdentityType": "aadUser", "tenantId": "t-1" } },
        "body": { "contentType": "html", "content": "<div><at id=\"0\">Ada Lovelace</at>&nbsp;we shipped. <attachment id=\"a1\"></attachment></div>" },
        "channelIdentity": { "teamId": "team-1", "channelId": CHANNEL },
        "attachments": [{ "id": "a1", "contentType": "reference", "contentUrl": "https://contoso.sharepoint.com/notes.docx", "content": null, "name": "notes.docx", "thumbnailUrl": null, "teamsAppId": null }],
        "mentions": [{ "id": 0, "mentionText": "Ada Lovelace", "mentioned": { "application": null, "device": null, "conversation": null,
            "user": { "id": "u-1", "displayName": "Ada Lovelace", "userIdentityType": "aadUser" } } }],
        "reactions": [{ "reactionType": "like", "createdDateTime": "2026-10-09T08:20:00.000Z",
            "user": { "application": null, "device": null, "user": { "id": "u-1", "displayName": null, "userIdentityType": "aadUser" } } }],
        "eventDetail": null
    })
}

/// What an operation that returns `chat_message()` must pass on.
pub fn chat_message_returned() -> Value {
    json!({
        "id": "1616990032035", "replyToId": null, "messageType": "message", "createdDateTime": "2026-10-09T08:13:52.035Z",
        "lastEditedDateTime": null, "deletedDateTime": null,
        "from": { "user": { "id": "u-2", "displayName": "Grace Hopper", "userIdentityType": "aadUser" } },
        "body": { "contentType": "html" },
        // The body as a person would read it.
        "text": "@Ada Lovelace we shipped.\n[attachment: notes.docx]",
        "attachments": [{ "id": "a1", "name": "notes.docx", "contentUrl": "https://contoso.sharepoint.com/notes.docx" }],
        "mentions": [{ "id": 0, "mentionText": "Ada Lovelace", "mentioned": { "user": { "id": "u-1" } } }],
        "reactions": [{ "reactionType": "like", "user": { "user": { "id": "u-1" } } }]
    })
}
