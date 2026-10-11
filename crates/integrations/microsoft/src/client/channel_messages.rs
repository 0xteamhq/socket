//! The messages of a channel, and the replies under them.

use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{ChatMessage, Paging, SendChatMessage};

/// The messages of a channel, and the replies under them.
#[derive(Debug, Clone, Copy)]
pub struct ChannelMessages<'a>(pub(crate) Api<'a>);

impl ChannelMessages<'_> {
    /// Lists the messages that start a conversation in a channel, without
    /// their replies, the most recently active first. Graph returns 20 a page
    /// unless `limit` says otherwise, and at most 50.
    pub async fn list(&self, team: &str, channel: &str, paging: Paging) -> Result<Page<ChatMessage>> {
        let request = RawRequest::get(self.messages(team, channel)?);
        self.0.messages(request, &paging).await
    }

    /// Gets one message of a channel.
    pub async fn get(&self, team: &str, channel: &str, message: &str) -> Result<ChatMessage> {
        self.0
            .message(RawRequest::get(self.item(team, channel, message)?))
            .await
    }

    /// Lists the replies to a message, at most 50 a page.
    pub async fn replies(&self, team: &str, channel: &str, message: &str, paging: Paging) -> Result<Page<ChatMessage>> {
        let request = RawRequest::get(format!("{}/replies", self.item(team, channel, message)?));
        self.0.messages(request, &paging).await
    }

    /// Posts a new message to a channel, as the signed-in person.
    pub async fn send(&self, team: &str, channel: &str, message: SendChatMessage) -> Result<ChatMessage> {
        let request = RawRequest::post(self.messages(team, channel)?, self.0.outgoing(&message)?);
        self.0.message(request).await
    }

    /// Posts a reply under a message of a channel.
    pub async fn reply(&self, team: &str, channel: &str, message: &str, reply: SendChatMessage) -> Result<ChatMessage> {
        let path = format!("{}/replies", self.item(team, channel, message)?);
        self.0.message(RawRequest::post(path, self.0.outgoing(&reply)?)).await
    }

    fn messages(&self, team: &str, channel: &str) -> Result<String> {
        let team = self.0.segment("a team id", team)?;
        let channel = self.0.segment("a channel id", channel)?;
        Ok(format!("teams/{team}/channels/{channel}/messages"))
    }

    fn item(&self, team: &str, channel: &str, message: &str) -> Result<String> {
        let message = self.0.segment("a message id", message)?;
        Ok(format!("{}/{message}", self.messages(team, channel)?))
    }
}
