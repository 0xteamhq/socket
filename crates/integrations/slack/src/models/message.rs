//! Messages: what Slack returns for them, and the content and filters used to post and read them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::File;

/// A message in a channel or thread.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Message {
    /// The message's timestamp, which is also its id within the channel.
    pub ts: String,
    pub user: Option<String>,
    pub bot_id: Option<String>,
    pub text: String,
    /// Set on every message of a thread, to the timestamp of its first message.
    pub thread_ts: Option<String>,
    pub reply_count: Option<u32>,
    pub subtype: Option<String>,
    pub reactions: Vec<Reaction>,
    pub files: Vec<File>,
    /// Block Kit content, as Slack sent it.
    pub blocks: Option<Value>,
}

/// An emoji reaction on a message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Reaction {
    pub name: String,
    pub count: u32,
    pub users: Vec<String>,
}

/// A message that was posted or changed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct PostedMessage {
    pub channel: String,
    pub ts: String,
    pub message: Option<Message>,
}

/// A message set to be posted later.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ScheduledMessage {
    /// Slack calls this `scheduled_message_id` when scheduling and `id` when listing.
    #[serde(alias = "scheduled_message_id")]
    pub id: String,
    #[serde(alias = "channel")]
    pub channel_id: String,
    pub post_at: i64,
    pub text: Option<String>,
}

/// A pinned item.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Pin {
    /// `"message"` or `"file"`.
    #[serde(rename = "type")]
    pub kind: String,
    pub created: Option<i64>,
    pub created_by: Option<String>,
    pub message: Option<Message>,
    pub file: Option<File>,
}

/// The content of a message. Give `text`, `blocks` or `attachments`; Slack
/// refuses a message with none of them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PostMessage {
    pub text: Option<String>,
    /// Block Kit blocks, as a JSON array.
    pub blocks: Option<Value>,
    /// Legacy attachments, as a JSON array.
    pub attachments: Option<Value>,
    /// Reply in the thread that starts at this timestamp.
    pub thread_ts: Option<String>,
    /// Also show a thread reply in the channel.
    pub reply_broadcast: Option<bool>,
    pub mrkdwn: Option<bool>,
    pub unfurl_links: Option<bool>,
    pub unfurl_media: Option<bool>,
}

impl PostMessage {
    /// A plain text message.
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }

    /// Makes this a reply in the thread that starts at `thread_ts`.
    pub fn in_thread(mut self, thread_ts: impl Into<String>) -> Self {
        self.thread_ts = Some(thread_ts.into());
        self
    }

    pub(crate) fn has_content(&self) -> bool {
        let filled = |value: &Option<Value>| {
            value
                .as_ref()
                .is_some_and(|v| v.as_array().is_some_and(|a| !a.is_empty()))
        };
        self.text.as_ref().is_some_and(|t| !t.trim().is_empty()) || filled(&self.blocks) || filled(&self.attachments)
    }
}

/// The new content of an existing message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateMessage {
    pub text: Option<String>,
    pub blocks: Option<Value>,
    pub attachments: Option<Value>,
}

impl UpdateMessage {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: Some(text.into()),
            ..Self::default()
        }
    }
}

/// Which messages of a channel or thread to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct History {
    pub cursor: Option<String>,
    pub limit: Option<u32>,
    /// Only messages after this timestamp.
    pub oldest: Option<String>,
    /// Only messages before this timestamp.
    pub latest: Option<String>,
    /// Include the messages at `oldest` and `latest` themselves.
    pub inclusive: Option<bool>,
}

/// Which scheduled messages to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListScheduled {
    pub channel: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}
