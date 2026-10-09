//! Channels, direct messages, and what is in them.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, Result};

use super::{Api, with};
use crate::models::{Channel, CreateConversation, History, ListConversations, Message, Paging};

/// Channels, direct messages, and what is in them.
#[derive(Debug, Clone, Copy)]
pub struct Conversations<'a>(pub(crate) Api<'a>);

impl Conversations<'_> {
    /// Lists conversations the token can see.
    pub async fn list(&self, options: ListConversations) -> Result<Page<Channel>> {
        let body = self.0.get("conversations.list", with(json!({}), &options)).await?;
        self.0.page(&body, "channels")
    }

    /// One conversation, with its member count.
    pub async fn info(&self, channel: &str) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        let arguments = json!({ "channel": channel, "include_num_members": true });
        let body = self.0.get("conversations.info", arguments).await?;
        self.channel(&body)
    }

    /// The messages of a conversation, newest first.
    pub async fn history(&self, channel: &str, options: History) -> Result<Page<Message>> {
        self.0.required("a channel", channel)?;
        let body = self
            .0
            .get("conversations.history", with(json!({ "channel": channel }), &options))
            .await?;
        self.0.page(&body, "messages")
    }

    /// The messages of one thread, starting with the message that began it.
    pub async fn replies(&self, channel: &str, thread_ts: &str, options: History) -> Result<Page<Message>> {
        self.0.required("a channel", channel)?;
        self.0.required("a thread timestamp", thread_ts)?;
        let arguments = with(json!({ "channel": channel, "ts": thread_ts }), &options);
        let body = self.0.get("conversations.replies", arguments).await?;
        self.0.page(&body, "messages")
    }

    /// The ids of a conversation's members.
    pub async fn members(&self, channel: &str, paging: Paging) -> Result<Page<String>> {
        self.0.required("a channel", channel)?;
        let body = self
            .0
            .get("conversations.members", with(json!({ "channel": channel }), &paging))
            .await?;
        self.0.page(&body, "members")
    }

    /// Creates a channel.
    pub async fn create(&self, name: &str, options: CreateConversation) -> Result<Channel> {
        self.0.required("a channel name", name)?;
        let body = self
            .0
            .post("conversations.create", with(json!({ "name": name }), &options))
            .await?;
        self.channel(&body)
    }

    /// Joins a public channel.
    pub async fn join(&self, channel: &str) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        let body = self.0.post("conversations.join", json!({ "channel": channel })).await?;
        self.channel(&body)
    }

    /// Leaves a conversation.
    pub async fn leave(&self, channel: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.leave", json!({ "channel": channel }))
            .await
            .map(drop)
    }

    /// Invites members to a channel.
    pub async fn invite(&self, channel: &str, users: &[String]) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        let body = self
            .0
            .post(
                "conversations.invite",
                json!({ "channel": channel, "users": self.ids(users)? }),
            )
            .await?;
        self.channel(&body)
    }

    /// Removes a member from a channel.
    pub async fn kick(&self, channel: &str, user: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a user", user)?;
        self.0
            .post("conversations.kick", json!({ "channel": channel, "user": user }))
            .await
            .map(drop)
    }

    /// Archives a channel.
    pub async fn archive(&self, channel: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.archive", json!({ "channel": channel }))
            .await
            .map(drop)
    }

    /// Restores an archived channel.
    pub async fn unarchive(&self, channel: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.unarchive", json!({ "channel": channel }))
            .await
            .map(drop)
    }

    /// Renames a channel.
    pub async fn rename(&self, channel: &str, name: &str) -> Result<Channel> {
        self.0.required("a channel", channel)?;
        self.0.required("a channel name", name)?;
        let body = self
            .0
            .post("conversations.rename", json!({ "channel": channel, "name": name }))
            .await?;
        self.channel(&body)
    }

    /// Sets a channel's topic.
    pub async fn set_topic(&self, channel: &str, topic: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post("conversations.setTopic", json!({ "channel": channel, "topic": topic }))
            .await
            .map(drop)
    }

    /// Sets a channel's purpose.
    pub async fn set_purpose(&self, channel: &str, purpose: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0
            .post(
                "conversations.setPurpose",
                json!({ "channel": channel, "purpose": purpose }),
            )
            .await
            .map(drop)
    }

    /// Opens a direct message with one member, or a group direct message with several.
    pub async fn open(&self, users: &[String]) -> Result<Channel> {
        let body = self
            .0
            .post(
                "conversations.open",
                json!({ "users": self.ids(users)?, "return_im": true }),
            )
            .await?;
        self.channel(&body)
    }

    /// Marks a conversation as read up to a message.
    pub async fn mark(&self, channel: &str, ts: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", ts)?;
        self.0
            .post("conversations.mark", json!({ "channel": channel, "ts": ts }))
            .await
            .map(drop)
    }

    fn channel(&self, body: &Value) -> Result<Channel> {
        let channel: Channel = self.0.field(body, "channel")?;
        if channel.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a channel that has no id"));
        }
        Ok(channel)
    }

    /// Slack takes several ids as one comma-separated string.
    fn ids(&self, users: &[String]) -> Result<String> {
        let ids: Vec<&str> = users.iter().map(|u| u.trim()).filter(|u| !u.is_empty()).collect();
        if ids.is_empty() || ids.iter().any(|id| id.contains(',')) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "at least one user id is required, one per entry",
            ));
        }
        Ok(ids.join(","))
    }
}
