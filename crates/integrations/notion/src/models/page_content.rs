//! A page's content read whole.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How far to read into a page.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadPage {
    /// How many levels of nested blocks to read, from 1 to 50. 10 when not given.
    pub max_depth: Option<u32>,
    /// The most requests to spend on the page, from 1 to 500. 50 when not
    /// given. One reads the page itself, and each list of blocks takes one
    /// for every 100 blocks in it.
    pub max_requests: Option<u32>,
}

/// A page's content as Markdown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PageContent {
    pub id: String,
    pub title: Option<String>,
    /// The address that opens the page in Notion.
    pub url: Option<String>,
    /// The content, without the title. Where blocks were left unread, a line
    /// in square brackets says so in their place.
    pub markdown: String,
    /// Whether a limit stopped the reading before the whole page was read.
    pub truncated: bool,
    /// Which limit that was, when `truncated` is set.
    pub truncation: Option<String>,
    /// How many blocks were read.
    pub blocks: u32,
    /// How many requests were made to Notion.
    pub requests: u32,
}
