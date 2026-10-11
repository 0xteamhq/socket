//! Gmail labels: what marks a message as in the inbox, unread, starred, or
//! filed under a name of the person's own.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A label. Gmail's own have fixed ids, such as `INBOX`, `UNREAD`,
/// `STARRED`, `SENT`, `DRAFT`, `TRASH`, `SPAM`, `IMPORTANT` and the
/// `CATEGORY_…` ids of the inbox's tabs; a person's own have ids such as
/// `Label_12`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailLabel {
    /// What a list of messages is filtered by, and what `modify` adds and removes.
    pub id: String,
    /// The name a person sees. A label inside another is written `Parent/Child`.
    pub name: String,
    /// `system` for Gmail's own labels, `user` for a person's.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// `show` or `hide`: whether its messages appear in Gmail's message list.
    pub message_list_visibility: Option<String>,
    /// `labelShow`, `labelShowIfUnread` or `labelHide`.
    pub label_list_visibility: Option<String>,
    /// How many messages carry it. The four counts come with one label
    /// read by itself, never in a list.
    pub messages_total: Option<i64>,
    pub messages_unread: Option<i64>,
    pub threads_total: Option<i64>,
    pub threads_unread: Option<i64>,
    /// The colours a person gave one of their own labels.
    pub color: Option<GmailLabelColor>,
}

/// The colours of a label, each as `#rrggbb`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailLabelColor {
    pub text_color: Option<String>,
    pub background_color: Option<String>,
}
