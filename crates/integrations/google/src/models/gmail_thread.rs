//! Gmail threads: a conversation and the messages in it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{GmailFormat, GmailMessage, GmailWireMessage};

/// A conversation. In a list it is its id and a snippet; read by itself it
/// carries its messages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailThread {
    pub id: String,
    /// A short part of the text of its latest message.
    pub snippet: Option<String>,
    pub history_id: Option<String>,
    /// The messages of the thread, each decoded. Empty in a list.
    pub messages: Vec<GmailMessage>,
}

/// A thread in Gmail's own shape, with its messages not yet decoded.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct GmailWireThread {
    id: String,
    snippet: Option<String>,
    history_id: Option<String>,
    messages: Vec<GmailWireMessage>,
}

impl GmailWireThread {
    /// The thread with every message's parts decoded.
    pub(crate) fn read(self) -> GmailThread {
        GmailThread {
            id: self.id,
            snippet: self.snippet,
            history_id: self.history_id,
            messages: self.messages.into_iter().map(GmailWireMessage::read).collect(),
        }
    }
}

/// Which threads to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailListThreads {
    /// A search in the words of Gmail's own search box: `from:grace
    /// is:unread`, `subject:(q3 plan) newer_than:7d`.
    pub q: Option<String>,
    /// Only threads that carry every one of these labels: `INBOX`,
    /// `UNREAD`, `STARRED`, or a label's id.
    pub label_ids: Option<Vec<String>>,
    /// Whether to include what is in Spam and in the bin. Gmail leaves them
    /// out unless this is `true`.
    pub include_spam_trash: Option<bool>,
}

/// How to return one thread.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailGetThread {
    /// How much of each message to return: `full` when not given.
    /// `metadata` leaves out the bodies and attachments, which keeps a long
    /// thread small; `minimal` leaves out the headers too.
    pub format: Option<GmailFormat>,
}
