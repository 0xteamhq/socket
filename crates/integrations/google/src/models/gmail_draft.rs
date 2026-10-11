//! Gmail drafts: messages that are written and not yet sent.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{GmailMessage, GmailMessageRef, GmailWireMessage};

/// A draft as Gmail names it, without its content: a row of a list, and
/// what Gmail answers when a draft is saved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailDraftRef {
    /// The draft's own id: what `get`, `update`, `delete` and `send_draft` take.
    pub id: String,
    /// The message the draft holds. Its id is not the draft's, and changes
    /// each time the draft is saved.
    pub message: GmailMessageRef,
}

/// A draft that was read, with its message decoded.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailDraft {
    /// The draft's own id.
    pub id: String,
    /// What the draft says so far.
    pub message: GmailMessage,
}

/// A draft in Gmail's own shape, with its message not yet decoded.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(crate) struct GmailWireDraft {
    id: String,
    message: GmailWireMessage,
}

impl GmailWireDraft {
    /// The draft with its message's parts decoded.
    pub(crate) fn read(self) -> GmailDraft {
        GmailDraft {
            id: self.id,
            message: self.message.read(),
        }
    }
}

/// Which drafts to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailListDrafts {
    /// A search in the words of Gmail's own search box: `to:grace`,
    /// `subject:plan`.
    pub q: Option<String>,
    /// Whether to include drafts in Spam and in the bin. Gmail leaves them
    /// out unless this is `true`.
    pub include_spam_trash: Option<bool>,
}
