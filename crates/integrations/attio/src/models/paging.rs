//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first
    /// page. Keep the filters and the limit the same from page to page.
    pub cursor: Option<String>,
    /// The most items to return.
    pub limit: Option<u32>,
}
