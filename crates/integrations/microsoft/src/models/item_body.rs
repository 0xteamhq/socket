//! The text of an event or a message.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The text of an event or a message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ItemBody {
    /// `text` or `html`.
    pub content_type: Option<String>,
    /// The text itself. Always given when a body is sent: a body without it
    /// would blank the event's text.
    pub content: Option<String>,
}

impl ItemBody {
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            content_type: Some("text".into()),
            content: Some(content.into()),
        }
    }

    pub fn html(content: impl Into<String>) -> Self {
        Self {
            content_type: Some("html".into()),
            content: Some(content.into()),
        }
    }
}
