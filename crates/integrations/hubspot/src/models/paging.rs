//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first
    /// page. It is the value HubSpot gave as `paging.next.after`.
    pub cursor: Option<String>,
    /// The most items to return in a page. Each list has its own largest
    /// page, and a larger number is refused.
    pub limit: Option<u32>,
}
