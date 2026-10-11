//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first
    /// page. It is what HubSpot gave as `paging.next.after`.
    pub cursor: Option<String>,
    /// The most items to return in this page. HubSpot chooses when not given.
    pub limit: Option<u32>,
}
