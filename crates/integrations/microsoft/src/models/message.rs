//! Messages: what Graph returns for them, and the content and filters used to read, write and send them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;
use super::{ItemBody, Recipient};

/// A message in a mailbox: received, sent, or a draft.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Message {
    /// Changes when the message is moved to another folder.
    pub id: String,
    /// The same on every message of one thread.
    pub conversation_id: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub subject: String,
    /// Whose mailbox it was sent from.
    pub from: Option<Recipient>,
    /// Who sent it, when that is someone acting for `from`.
    pub sender: Option<Recipient>,
    #[serde(deserialize_with = "nullable")]
    pub to_recipients: Vec<Recipient>,
    #[serde(deserialize_with = "nullable")]
    pub cc_recipients: Vec<Recipient>,
    #[serde(deserialize_with = "nullable")]
    pub bcc_recipients: Vec<Recipient>,
    /// Where a reply should go, when that is not `from`.
    #[serde(deserialize_with = "nullable")]
    pub reply_to: Vec<Recipient>,
    /// When it arrived, in UTC: `2026-10-09T08:15:00Z`.
    pub received_date_time: Option<String>,
    pub sent_date_time: Option<String>,
    /// The first 255 characters of the body, as plain text.
    #[serde(deserialize_with = "nullable")]
    pub body_preview: String,
    pub body: Option<ItemBody>,
    #[serde(deserialize_with = "nullable")]
    pub is_read: bool,
    #[serde(deserialize_with = "nullable")]
    pub is_draft: bool,
    /// Attachments shown inline in the body are not counted.
    #[serde(deserialize_with = "nullable")]
    pub has_attachments: bool,
    /// The address that opens the message in Outlook on the web.
    pub web_link: Option<String>,
    /// `low`, `normal` or `high`.
    pub importance: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub categories: Vec<String>,
    pub flag: Option<FollowupFlag>,
    pub parent_folder_id: Option<String>,
    /// The id the message carries between mail systems, in angle brackets.
    pub internet_message_id: Option<String>,
}

/// Whether a message is marked to be followed up.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct FollowupFlag {
    /// `notFlagged`, `flagged` or `complete`.
    pub flag_status: Option<String>,
}

/// The format a message's body is returned in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum BodyType {
    Text,
    Html,
}

/// Which messages to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListMessages {
    /// A folder id, or a well-known name such as `inbox`, `drafts`,
    /// `sentitems`, `deleteditems`, `archive` or `junkemail`. The whole
    /// mailbox when not given.
    pub folder: Option<String>,
    /// Graph's `$filter`, such as `isRead eq false`.
    pub filter: Option<String>,
    /// Words to search for, in Graph's search syntax: `pizza`, `from:grace
    /// subject:plan`. Graph returns at most 1,000 results, sorted by when
    /// they were sent. Not to be given with `filter` or `orderBy`.
    pub search: Option<String>,
    /// Graph's `$orderby`, such as `receivedDateTime desc`. With a filter,
    /// what is sorted by has to come first in the filter.
    pub order_by: Option<String>,
    /// The format of each body: plain text when not given.
    pub body_type: Option<BodyType>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most messages to return in the first page, from 1 to 1000. Graph returns 10 when not given.
    pub limit: Option<u32>,
}

/// How to return one message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetMessage {
    /// The format of the body: plain text when not given.
    pub body_type: Option<BodyType>,
}

/// The content of a message that is being written. Everything is optional:
/// a draft can be filled in over several steps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DraftMessage {
    pub subject: Option<String>,
    pub body: Option<ItemBody>,
    pub to_recipients: Option<Vec<Recipient>>,
    pub cc_recipients: Option<Vec<Recipient>>,
    pub bcc_recipients: Option<Vec<Recipient>>,
    /// Where replies should go, when that is not the sender.
    pub reply_to: Option<Vec<Recipient>>,
    /// `low`, `normal` or `high`.
    pub importance: Option<String>,
}

/// What a reply or a forward adds to the message it answers.
///
/// Give `comment` for a few words above the quoted message, or a `body` to
/// write the whole text. Graph refuses both together.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ReplyContent {
    /// Text to put above the quoted message.
    pub comment: Option<String>,
    /// What to set on the reply itself: more recipients, another subject, its
    /// whole body. A forward takes the people it goes to from `toRecipients`.
    #[serde(flatten)]
    pub message: DraftMessage,
}

/// A message to send at once, without keeping a draft.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SendMail {
    #[serde(flatten)]
    pub message: DraftMessage,
    /// Whether a copy is kept in Sent Items. Graph keeps one unless this is `false`.
    pub save_to_sent_items: Option<bool>,
}

/// Marks to change on a message. They are the person's own and can be set back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMessage {
    pub is_read: Option<bool>,
    /// The whole list of categories: one left out is removed.
    pub categories: Option<Vec<String>>,
    pub flag: Option<FollowupFlag>,
}
