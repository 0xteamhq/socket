//! Searching the records of an object type.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What to search for. With nothing set, every record of the type matches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    /// Words to look for in the type's searchable properties, such as a
    /// name, an email address or a phone number. At most 3,000 characters.
    pub query: Option<String>,
    /// A record matches when it passes every filter of any one group. At
    /// most 5 groups, 6 filters in a group and 18 filters in all.
    pub filter_groups: Option<Vec<FilterGroup>>,
    /// The order of the results. HubSpot applies one sort.
    pub sorts: Option<Vec<Sort>>,
    /// The properties to return, by their internal names.
    pub properties: Option<Vec<String>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most records to return in a page, from 1 to 200. HubSpot returns 10 when not given.
    pub limit: Option<u32>,
}

/// Filters a record has to pass all of.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FilterGroup {
    pub filters: Vec<Filter>,
}

/// One condition on one property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    /// The property's internal name.
    pub property_name: String,
    pub operator: Operator,
    /// What to compare with. For `BETWEEN`, the lower bound.
    pub value: Option<String>,
    /// The upper bound of `BETWEEN`.
    pub high_value: Option<String>,
    /// What to compare with, for `IN` and `NOT_IN`.
    pub values: Option<Vec<String>>,
}

impl Filter {
    /// A filter that compares a property with one value.
    pub fn new(property_name: impl Into<String>, operator: Operator, value: impl Into<String>) -> Self {
        Self {
            property_name: property_name.into(),
            operator,
            value: Some(value.into()),
            high_value: None,
            values: None,
        }
    }
}

/// How a filter compares a property with its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Operator {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    /// From `value` to `highValue`.
    Between,
    /// Any of `values`.
    In,
    NotIn,
    /// The property has a value. Takes none itself.
    HasProperty,
    NotHasProperty,
    /// The property holds this word.
    ContainsToken,
    NotContainsToken,
}

/// The order of the results of a search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    /// The property to sort by, by its internal name.
    pub property_name: String,
    /// Ascending when not given.
    pub direction: Option<Direction>,
}

/// Which way a search is sorted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Direction {
    Ascending,
    Descending,
}
