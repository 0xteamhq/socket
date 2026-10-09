//! Channels and direct messages, and what is attached to a channel.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A channel, private channel, direct message or group direct message.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Channel {
    pub id: String,
    /// Absent for a direct message.
    pub name: Option<String>,
    pub is_channel: bool,
    pub is_private: bool,
    pub is_im: bool,
    pub is_mpim: bool,
    pub is_member: bool,
    pub is_archived: bool,
    /// The other person, for a direct message.
    pub user: Option<String>,
    pub topic: Option<Described>,
    pub purpose: Option<Described>,
    pub num_members: Option<u32>,
    pub created: Option<i64>,
}

/// A channel's topic or purpose.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Described {
    pub value: String,
}

/// A bookmark at the top of a channel.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Bookmark {
    pub id: String,
    pub channel_id: String,
    pub title: String,
    pub link: Option<String>,
    pub emoji: Option<String>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
}

/// Which conversations to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListConversations {
    /// Comma-separated: `public_channel`, `private_channel`, `mpim`, `im`. Slack's default is public channels.
    pub types: Option<String>,
    pub exclude_archived: Option<bool>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// How to create a conversation.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateConversation {
    pub is_private: Option<bool>,
}
