//! Paging through a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which part of a list to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Paging {
    /// The `next_cursor` of the page before, unchanged; absent for the first
    /// page. It is the page token Google gave. Send the rest of the request
    /// as it was: Google refuses a token used with other filters.
    pub cursor: Option<String>,
    /// The most items to return in one page.
    pub limit: Option<u32>,
}
