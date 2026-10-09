//! The data Slack returns, and the content and options its methods take.
//!
//! Every field Slack may omit has a default, so a response that carries less
//! than these types describe still reads.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── What Slack returns ────────────────────────────────────────────────────────

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

/// A member of the workspace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct User {
    pub id: String,
    pub name: String,
    pub real_name: Option<String>,
    pub deleted: bool,
    pub is_bot: bool,
    pub is_admin: bool,
    pub tz: Option<String>,
    pub profile: Profile,
}

/// A member's profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Profile {
    pub real_name: Option<String>,
    pub display_name: Option<String>,
    /// Present only with the `users:read.email` scope.
    pub email: Option<String>,
    pub title: Option<String>,
    pub status_text: Option<String>,
    pub status_emoji: Option<String>,
    pub image_72: Option<String>,
}

/// Whether a member is active.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Presence {
    /// `"active"` or `"away"`.
    pub presence: String,
    pub online: Option<bool>,
}

/// A file shared in the workspace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct File {
    pub id: String,
    pub name: Option<String>,
    pub title: Option<String>,
    pub mimetype: Option<String>,
    pub filetype: Option<String>,
    pub size: Option<u64>,
    pub user: Option<String>,
    pub created: Option<i64>,
    pub permalink: Option<String>,
    /// Needs the token to download.
    pub url_private: Option<String>,
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

/// A reminder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Reminder {
    pub id: String,
    pub creator: Option<String>,
    pub user: Option<String>,
    pub text: String,
    pub time: Option<i64>,
    pub complete_ts: Option<i64>,
    pub recurring: bool,
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

/// A user group, such as `@engineering`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct UserGroup {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub description: Option<String>,
    pub user_count: Option<u32>,
    /// Present only when members were asked for.
    pub users: Vec<String>,
}

/// The workspace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub email_domain: Option<String>,
}

/// A member's Do Not Disturb state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DndStatus {
    pub dnd_enabled: Option<bool>,
    pub snooze_enabled: Option<bool>,
    pub snooze_endtime: Option<i64>,
    pub next_dnd_start_ts: Option<i64>,
    pub next_dnd_end_ts: Option<i64>,
}

/// One message found by a search.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SearchMatch {
    pub ts: String,
    pub text: String,
    pub user: Option<String>,
    pub username: Option<String>,
    pub permalink: Option<String>,
    pub channel: Option<SearchChannel>,
}

/// The channel a found message is in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SearchChannel {
    pub id: String,
    pub name: Option<String>,
}

/// The messages a search found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SearchResults {
    pub total: u32,
    pub matches: Vec<SearchMatch>,
}

/// The workspace's custom emoji: name to image URL, or to `alias:<name>`.
pub type Emoji = BTreeMap<String, String>;

// ── What a caller supplies ────────────────────────────────────────────────────

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

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before; absent for the first page.
    pub cursor: Option<String>,
    pub limit: Option<u32>,
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

/// Which scheduled messages to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListScheduled {
    pub channel: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

/// Which files to list. Slack pages this list by number, not by cursor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListFiles {
    pub channel: Option<String>,
    pub user: Option<String>,
    /// Comma-separated: `images`, `pdfs`, `snippets`, `gdocs`, `zips`, `spaces`, or `all`.
    pub types: Option<String>,
    pub count: Option<u32>,
    pub page: Option<u32>,
}

/// How to run a search. Slack pages search results by number.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Search {
    pub count: Option<u32>,
    pub page: Option<u32>,
    /// `"score"` or `"timestamp"`.
    pub sort: Option<String>,
    /// `"asc"` or `"desc"`.
    pub sort_dir: Option<String>,
}

/// Which user groups to list, and how much about each.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListUserGroups {
    pub include_users: Option<bool>,
    pub include_disabled: Option<bool>,
    pub include_count: Option<bool>,
}
