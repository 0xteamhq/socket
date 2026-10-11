//! Searching the records of an object type.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Object;

/// What to search for. Everything is optional: a search with nothing set
/// returns every record, oldest first.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Search {
    /// Words to look for in the object type's default text properties: a
    /// contact's names, email and phone, a deal's name, a note's body. At
    /// most 3,000 characters.
    pub query: Option<String>,
    /// Conditions on properties. A record matches when it meets every filter
    /// of any one group. HubSpot takes at most 18 filters in all.
    pub filter_groups: Option<Vec<FilterGroup>>,
    /// The order of the results. HubSpot applies one sort and no more.
    pub sorts: Option<Vec<Sort>>,
    /// The properties to return of each record.
    pub properties: Option<Vec<String>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most records to return, from 1 to 200. HubSpot returns 10 when not given.
    pub limit: Option<u32>,
}

/// Filters a record has to meet all of.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FilterGroup {
    pub filters: Vec<Filter>,
}

/// One condition on one property.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Filter {
    /// The property's internal name. `associations.contact` and the like
    /// match records associated with the record whose id is the value.
    pub property_name: String,
    pub operator: FilterOperator,
    /// What to compare with. A date is in milliseconds since 1970, as text.
    pub value: Option<String>,
    /// The upper end of the range, for `BETWEEN`; `value` is the lower.
    pub high_value: Option<String>,
    /// The list to match against, for `IN` and `NOT_IN`.
    pub values: Option<Vec<String>>,
}

/// How a filter compares a property with its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FilterOperator {
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    /// Between `value` and `highValue`.
    Between,
    /// One of `values`. For a text property they have to be in lowercase.
    In,
    NotIn,
    /// Has any value at all.
    HasProperty,
    NotHasProperty,
    /// Holds the word in `value`, where `*` stands for any characters.
    ContainsToken,
    NotContainsToken,
}

/// What to sort the results by.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Sort {
    pub property_name: String,
    /// Ascending when not given.
    pub direction: Option<SortDirection>,
}

/// Which way a sort runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// One page of what a search found.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SearchResults {
    /// How many records match in all. A search returns the first 10,000 of
    /// them and no more, so a larger number means the filters are too wide
    /// to read every match.
    pub total: u64,
    pub items: Vec<Object>,
    /// `None` on the last page, and on the page that reaches the 10,000th
    /// match: there is no page after it to ask for.
    pub next_cursor: Option<String>,
}
