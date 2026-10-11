//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
///
/// Pipedrive pages version 2 lists by a cursor and version 1 lists by an
/// offset. Both are one `cursor` here: pass back the `next_cursor` of the
/// page before, to the same method, and never build one by hand.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most items to return in a page, from 1 to 500. Pipedrive returns 100 when not given.
    pub limit: Option<u32>,
}
