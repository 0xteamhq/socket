//! Searching the pages and data sources shared with the integration.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::SortDirection;

/// What to search for.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchQuery {
    /// Words to look for in titles. Notion searches titles only, never the
    /// content. Everything shared with the integration when not given.
    pub query: Option<String>,
    pub filter: Option<SearchFilter>,
    /// The order of the results. By relevance when not given.
    pub sort: Option<SearchSort>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most results to return, from 1 to 100. Notion may return fewer.
    pub limit: Option<u32>,
}

/// Which of what was found to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchFilter {
    /// Only pages, or only data sources. Both when not given.
    pub value: Option<SearchObject>,
    /// Set to search the trash.
    pub in_trash: Option<bool>,
}

/// The kinds of object a search finds. A database is found as its data sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchObject {
    Page,
    DataSource,
}

/// Orders the results by when each was last edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SearchSort {
    pub direction: SortDirection,
}
