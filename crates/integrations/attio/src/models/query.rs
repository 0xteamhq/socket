//! Filtering and sorting, for the two lists Attio lets be queried: records and entries.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Which records of an object, or entries of a list, to return, and in what order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Query {
    /// A filter in Attio's own form, by attribute slug: `{ "name": "Ada
    /// Lovelace" }`, or with operators such as `$and`, `$or`, `$eq`,
    /// `$contains`, `$gte` and `$not_empty`. Not together with `filter_view_id`.
    pub filter: Option<Map<String, Value>>,
    /// The id of a saved view, to use that view's filter. Not together with `filter`.
    pub filter_view_id: Option<String>,
    /// What to sort by, the first given first.
    pub sorts: Option<Vec<Sort>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first
    /// page. Keep the filter, the sorts and the limit the same from page to page.
    pub cursor: Option<String>,
    /// The most to return, from 1 to 500. 50 when not given.
    pub limit: Option<u32>,
}

/// One thing to sort by: an attribute, or an attribute reached through a reference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Sort {
    pub direction: SortDirection,
    /// The slug or id of the attribute to sort by. Not together with `path`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attribute: Option<String>,
    /// Which part of the value to sort by, such as `last_name` of a name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub field: Option<String>,
    /// A way through references to the attribute to sort by: pairs of an
    /// object or list and an attribute on it, starting with the one queried.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<(String, String)>>,
}

impl Sort {
    /// A sort by one attribute of what is queried.
    pub fn by(attribute: impl Into<String>, direction: SortDirection) -> Self {
        Self {
            direction,
            attribute: Some(attribute.into()),
            field: None,
            path: None,
        }
    }
}

/// Which way to sort.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    Asc,
    Desc,
}
