//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
///
/// Attio pages most lists by an offset and a few by a cursor of its own.
/// Both are a `cursor` here: pass back the `next_cursor` of the page before
/// and the list goes on from there, whichever kind it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most items to return in one page. Each list says how many it
    /// returns when this is not given, and the most it takes.
    pub limit: Option<u32>,
}
