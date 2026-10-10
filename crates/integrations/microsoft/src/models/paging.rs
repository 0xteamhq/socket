//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first
    /// page. It is the address Graph gave for the next page, and carries the
    /// rest of the request with it.
    pub cursor: Option<String>,
    /// The most items to return in the first page. Later pages keep it.
    pub limit: Option<u32>,
}
