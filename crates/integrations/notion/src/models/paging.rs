//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most items to return, from 1 to 100. Notion may return fewer.
    pub limit: Option<u32>,
}
