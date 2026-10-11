//! The order of a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which way a list is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    /// Smallest or oldest first. Pipedrive's default.
    Asc,
    /// Largest or newest first.
    Desc,
}
