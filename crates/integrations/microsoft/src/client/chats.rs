//! Chats: one-to-one, group and meeting conversations outside a channel.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, encoded, named};
use crate::models::{Chat, ChatMessage, ChatType, ConversationMember, CreateChat, Cursor, Paging, SendChatMessage};

/// Chats: one-to-one, group and meeting conversations outside a channel.
#[derive(Debug, Clone, Copy)]
pub struct Chats<'a>(pub(crate) Api<'a>);

impl Chats<'_> {
    /// Lists the chats the account is in, at most 50 a page.
    pub async fn list(&self, paging: Paging) -> Result<Page<Chat>> {
        let request = named(RawRequest::get("me/chats"));
        self.0.page_up_to(50, request, &paging, "chats").await
    }

    /// Gets one chat.
    pub async fn get(&self, chat: &str) -> Result<Chat> {
        let body = self.0.send(named(RawRequest::get(self.path(chat)?))).await?;
        self.chat(body)
    }

    /// Lists who is in a chat.
    pub async fn members(&self, chat: &str, place: Cursor) -> Result<Page<ConversationMember>> {
        let request = RawRequest::get(format!("{}/members", self.path(chat)?));
        self.0.page(request, &place.into(), "members").await
    }

    /// Lists a chat's messages, the most recently changed first, at most 50 a page.
    pub async fn messages(&self, chat: &str, paging: Paging) -> Result<Page<ChatMessage>> {
        let request = RawRequest::get(format!("{}/messages", self.path(chat)?));
        self.0.messages(request, &paging).await
    }

    /// Gets one message of a chat.
    pub async fn message_get(&self, chat: &str, message: &str) -> Result<ChatMessage> {
        let message = self.0.segment("a message id", message)?;
        let request = RawRequest::get(format!("{}/messages/{message}", self.path(chat)?));
        self.0.message(request).await
    }

    /// Sends a message to a chat, as the signed-in person.
    pub async fn send(&self, chat: &str, message: SendChatMessage) -> Result<ChatMessage> {
        let path = format!("{}/messages", self.path(chat)?);
        self.0.message(RawRequest::post(path, self.0.outgoing(&message)?)).await
    }

    /// Creates a chat between two people, or among several. Graph answers
    /// with the chat that already exists between two people, when there is one.
    ///
    /// Everyone in the chat is named in `members`, the account that creates
    /// it included.
    pub async fn create(&self, chat: CreateChat) -> Result<Chat> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        if chat.members.iter().any(|member| member.trim().is_empty()) {
            return Err(invalid("every member needs an id or a sign-in name"));
        }
        match (chat.chat_type, chat.members.len()) {
            (ChatType::OneOnOne, 2) | (ChatType::Group, 2..) => {}
            (ChatType::OneOnOne, _) => return Err(invalid("a one-to-one chat has exactly two members")),
            (ChatType::Group, _) => return Err(invalid("a group chat has at least two members")),
        }
        if chat.chat_type == ChatType::OneOnOne && chat.topic.is_some() {
            return Err(invalid("a one-to-one chat takes no `topic`"));
        }
        // Graph is told who each member is by the address of the user, in
        // this API. A quote in a name is doubled, as OData writes one, and
        // the name is then percent-encoded, so that nothing in it is read as
        // part of the address: a guest's sign-in name holds a `#`.
        let users = self.0.connection.provider().api_base.as_str().trim_end_matches('/');
        let members: Vec<Value> = chat
            .members
            .iter()
            .map(|member| {
                json!({
                    "@odata.type": "#microsoft.graph.aadUserConversationMember",
                    "roles": ["owner"],
                    "user@odata.bind": format!("{users}/users('{}')", encoded(&member.trim().replace('\'', "''"))),
                })
            })
            .collect();
        let mut body = json!({ "chatType": chat.chat_type, "members": members });
        if let Some(topic) = &chat.topic {
            body["topic"] = json!(topic);
        }
        self.chat(self.0.send(RawRequest::post("chats", body)).await?)
    }

    fn path(&self, chat: &str) -> Result<String> {
        Ok(format!("chats/{}", self.0.segment("a chat id", chat)?))
    }

    fn chat(&self, body: Value) -> Result<Chat> {
        let chat: Chat = self.0.decode(body, "a chat")?;
        if chat.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "microsoft answered without a chat"));
        }
        Ok(chat)
    }
}
