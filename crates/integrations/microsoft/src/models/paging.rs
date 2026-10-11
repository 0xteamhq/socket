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

/// Where to go on in a list whose page size is Graph's to choose.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Cursor {
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
}

impl From<Cursor> for Paging {
    fn from(place: Cursor) -> Self {
        Self {
            cursor: place.cursor,
            limit: None,
        }
    }
}
