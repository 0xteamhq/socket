//! Attachments: files and items carried by a message.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// Something attached to a message.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Attachment {
    /// What kind it is: `#microsoft.graph.fileAttachment` for a file,
    /// `…itemAttachment` for another message, event or contact, and
    /// `…referenceAttachment` for a link to a file kept elsewhere.
    #[serde(rename = "@odata.type")]
    pub kind: Option<String>,
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    /// The media type, such as `application/pdf`. Graph leaves it empty for an attached item.
    pub content_type: Option<String>,
    /// The size in bytes.
    pub size: Option<i64>,
    /// Whether it is shown in the body, as a picture in a signature is.
    #[serde(deserialize_with = "nullable")]
    pub is_inline: bool,
    pub last_modified_date_time: Option<String>,
    /// The name the body refers to an inline attachment by.
    pub content_id: Option<String>,
    /// The file itself, in base64. Present for a file attachment that was
    /// asked for by itself, never in a list.
    pub content_bytes: Option<String>,
}
