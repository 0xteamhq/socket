//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before; absent for the first page. It is
    /// the link Graph gave for the next page and already carries the page size.
    pub cursor: Option<String>,
    /// How many items to ask for on the first page (Graph's `$top`).
    pub top: Option<u32>,
}
