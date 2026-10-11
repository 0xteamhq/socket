//! The order of a list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which way a list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One key the rows of a data source are ordered by. Set `property` or
/// `timestamp`, not both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Sort {
    /// The name or id of a property to order by.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<String>,
    /// `created_time` or `last_edited_time`, to order by when a row was made or changed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timestamp: Option<String>,
    pub direction: SortDirection,
}
