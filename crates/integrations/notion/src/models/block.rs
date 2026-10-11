//! Blocks: the pieces a page's content is made of.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Parent, User};

/// One block: a paragraph, a heading, a list item, a table and so on.
///
/// What the block holds sits under a key named after its kind, as Notion
/// writes it: `{ "type": "to_do", "to_do": { "rich_text": […], "checked": true } }`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Block {
    pub id: String,
    /// `paragraph`, `heading_1`, `to_do`, `table` and so on.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub parent: Option<Parent>,
    pub created_time: Option<String>,
    pub last_edited_time: Option<String>,
    pub created_by: Option<User>,
    pub last_edited_by: Option<User>,
    /// Whether blocks are nested inside this one. `blocks.children` lists them.
    pub has_children: bool,
    /// Whether the block is in the trash.
    pub in_trash: bool,
    /// The block's content, under the key named by `type`.
    #[serde(flatten)]
    pub content: BTreeMap<String, Value>,
}

impl Block {
    /// The block's kind, or `unknown` when Notion did not say.
    pub(crate) fn kind(&self) -> &str {
        self.kind.as_deref().unwrap_or("unknown")
    }

    /// What the block holds: the object under the key named by its kind.
    pub(crate) fn held(&self) -> &Value {
        self.content.get(self.kind()).unwrap_or(&Value::Null)
    }
}

/// Blocks to add inside a page or a block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AppendBlocks {
    /// From 1 to 100 blocks, as Notion writes them:
    /// `{ "paragraph": { "rich_text": [{ "text": { "content": "Hello" } }] } }`.
    /// A block may carry its own `children`, two levels deep at most.
    pub children: Vec<Value>,
    /// Where among the blocks already there. At the end when not given.
    pub position: Option<Position>,
}

/// Where new blocks go among those already there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Position {
    #[serde(rename = "type")]
    pub kind: PositionKind,
    /// The block to add after, when the kind is `after_block`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_block: Option<BlockRef>,
}

/// The places new blocks can go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PositionKind {
    Start,
    End,
    AfterBlock,
}

/// A block named by its id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BlockRef {
    pub id: String,
}

/// What to change in a block.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdateBlock {
    /// The block's kind and what to set in it, as Notion writes it:
    /// `{ "to_do": { "checked": true } }`. A field that is given is replaced
    /// whole; a field left out stays as it is. The kind itself cannot be
    /// changed, and neither can the blocks nested inside.
    pub content: BTreeMap<String, Value>,
}
