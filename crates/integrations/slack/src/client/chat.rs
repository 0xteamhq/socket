//! Posting, changing and scheduling messages.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, Result};

use super::{Api, with};
use crate::models::{ListScheduled, PostMessage, PostedMessage, ScheduledMessage, UpdateMessage};

/// Posting, changing and scheduling messages.
#[derive(Debug, Clone, Copy)]
pub struct Chat<'a>(pub(crate) Api<'a>);

impl Chat<'_> {
    /// Posts a message to a channel, a direct message, or a thread.
    pub async fn post_message(&self, channel: &str, message: PostMessage) -> Result<PostedMessage> {
        self.0.required("a channel", channel)?;
        self.content(&message)?;
        let body = self
            .0
            .post("chat.postMessage", with(json!({ "channel": channel }), &message))
            .await?;
        self.posted(body)
    }

    /// Posts a message only `user` can see. Returns its timestamp.
    pub async fn post_ephemeral(&self, channel: &str, user: &str, message: PostMessage) -> Result<String> {
        self.0.required("a channel", channel)?;
        self.0.required("a user", user)?;
        self.content(&message)?;
        let arguments = with(json!({ "channel": channel, "user": user }), &message);
        let body = self.0.post("chat.postEphemeral", arguments).await?;
        self.0.field(&body, "message_ts")
    }

    /// Replaces the content of a message.
    pub async fn update(&self, channel: &str, ts: &str, message: UpdateMessage) -> Result<PostedMessage> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", ts)?;
        let body = self
            .0
            .post("chat.update", with(json!({ "channel": channel, "ts": ts }), &message))
            .await?;
        self.posted(body)
    }

    /// Deletes a message.
    pub async fn delete(&self, channel: &str, ts: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", ts)?;
        self.0
            .post("chat.delete", json!({ "channel": channel, "ts": ts }))
            .await
            .map(drop)
    }

    /// Schedules a message for `post_at`, in seconds since the Unix epoch.
    pub async fn schedule_message(
        &self,
        channel: &str,
        post_at: i64,
        message: PostMessage,
    ) -> Result<ScheduledMessage> {
        self.0.required("a channel", channel)?;
        self.content(&message)?;
        let arguments = with(json!({ "channel": channel, "post_at": post_at }), &message);
        let body = self.0.post("chat.scheduleMessage", arguments).await?;
        let mut scheduled: ScheduledMessage = serde_json::from_value(body).map_err(|e| {
            self.0
                .error(
                    ErrorKind::Decode,
                    "slack sent a scheduled message that could not be read",
                )
                .with_source(e)
        })?;
        if scheduled.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered without a scheduled message id"));
        }
        if scheduled.text.is_none() {
            scheduled.text = message.text;
        }
        Ok(scheduled)
    }

    /// Cancels a scheduled message.
    pub async fn delete_scheduled_message(&self, channel: &str, scheduled_message_id: &str) -> Result<()> {
        self.0.required("a channel", channel)?;
        self.0.required("a scheduled message id", scheduled_message_id)?;
        let arguments = json!({ "channel": channel, "scheduled_message_id": scheduled_message_id });
        self.0.post("chat.deleteScheduledMessage", arguments).await.map(drop)
    }

    /// Lists messages that are scheduled and not yet posted.
    pub async fn scheduled_messages(&self, options: ListScheduled) -> Result<Page<ScheduledMessage>> {
        let body = self
            .0
            .post("chat.scheduledMessages.list", with(json!({}), &options))
            .await?;
        self.0.page(&body, "scheduled_messages")
    }

    /// The permanent link to a message.
    pub async fn permalink(&self, channel: &str, message_ts: &str) -> Result<String> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", message_ts)?;
        let body = self
            .0
            .get(
                "chat.getPermalink",
                json!({ "channel": channel, "message_ts": message_ts }),
            )
            .await?;
        self.0.field(&body, "permalink")
    }

    fn content(&self, message: &PostMessage) -> Result<()> {
        if message.has_content() {
            Ok(())
        } else {
            Err(self
                .0
                .error(ErrorKind::InvalidInput, "a message needs text, blocks or attachments"))
        }
    }

    fn posted(&self, body: Value) -> Result<PostedMessage> {
        let posted: PostedMessage = serde_json::from_value(body).map_err(|e| {
            self.0
                .error(ErrorKind::Decode, "slack sent a message that could not be read")
                .with_source(e)
        })?;
        if posted.channel.is_empty() || posted.ts.is_empty() {
            return Err(self.0.error(
                ErrorKind::Decode,
                "slack answered without the message's channel and timestamp",
            ));
        }
        Ok(posted)
    }
}
