//! Searching messages.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One message found by a search.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SearchMatch {
    pub ts: String,
    pub text: String,
    pub user: Option<String>,
    pub username: Option<String>,
    pub permalink: Option<String>,
    pub channel: Option<SearchChannel>,
}

/// The channel a found message is in.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SearchChannel {
    pub id: String,
    pub name: Option<String>,
}

/// The messages a search found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SearchResults {
    pub total: u32,
    pub matches: Vec<SearchMatch>,
}

/// How to run a search. Slack pages search results by number.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Search {
    pub count: Option<u32>,
    pub page: Option<u32>,
    /// `"score"` or `"timestamp"`.
    pub sort: Option<String>,
    /// `"asc"` or `"desc"`.
    pub sort_dir: Option<String>,
}
