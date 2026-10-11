//! The Gmail account a connection reads and writes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A mailbox: whose it is, and how much is in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailProfile {
    /// The address mail is sent from and arrives at.
    pub email_address: String,
    pub messages_total: Option<i64>,
    pub threads_total: Option<i64>,
    /// Where the mailbox's record of changes stands now. Every message and
    /// thread carries the `historyId` of its own last change.
    pub history_id: Option<String>,
}
