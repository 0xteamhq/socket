//! Attachments: files and items carried by a message.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// Something attached to a message: what describes it, without the file.
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
}

/// An attachment that is text, as text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AttachmentText {
    /// The media type Graph served it as, such as `text/csv`.
    pub content_type: Option<String>,
    /// The attachment, exactly as it was written.
    pub text: String,
}
