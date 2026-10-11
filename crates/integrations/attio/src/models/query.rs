//! How a query of records or of list entries is sorted.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Which way a sort runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Asc,
    Desc,
}

/// One sort of a query. It names an attribute of what is queried, or a path
/// to an attribute of a related record, and never both.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Sort {
    pub direction: Direction,
    /// The slug or id of the attribute to sort by.
    pub attribute: Option<String>,
    /// The way to an attribute of a related record: pairs of an object or
    /// list and one of its attributes, such as
    /// `[["people", "company"], ["companies", "name"]]`. The first pair
    /// names what is queried.
    pub path: Option<Vec<[String; 2]>>,
    /// Which part of the value to sort by, for a value with several: `last_name` of a name.
    pub field: Option<String>,
}

impl Sort {
    /// A sort by one attribute of what is queried.
    pub fn by(attribute: impl Into<String>, direction: Direction) -> Self {
        Self {
            direction,
            attribute: Some(attribute.into()),
            path: None,
            field: None,
        }
    }
}
