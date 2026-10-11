//! Comments on a page or a block.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Parent, RichText, User};

/// A comment.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Comment {
    pub id: String,
    /// The page or block the comment is on.
    pub parent: Option<Parent>,
    /// The thread the comment belongs to. A reply names it.
    pub discussion_id: Option<String>,
    pub created_time: Option<String>,
    pub last_edited_time: Option<String>,
    pub created_by: Option<User>,
    pub rich_text: Vec<RichText>,
    /// The name the comment is shown under, as Notion writes it.
    pub display_name: Option<Value>,
    /// Files attached to the comment, as Notion writes them. Their addresses expire.
    pub attachments: Vec<Value>,
}

/// A comment to add. It goes on a page or a block (`parent`), or into a
/// thread that exists (`discussion_id`): one of the two. Its text is
/// `rich_text` or `markdown`: one of the two.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateComment {
    /// The page (`page_id`) or the block (`block_id`) to comment on.
    pub parent: Option<Parent>,
    /// The thread to reply in.
    pub discussion_id: Option<String>,
    /// The comment as runs of text, 100 at most.
    pub rich_text: Option<Vec<RichText>>,
    /// The comment as Markdown. Bold, italic, strikethrough, inline code and
    /// links are kept; headings, lists and tables are not laid out.
    pub markdown: Option<String>,
}
