//! The values of a page's properties.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One property's value, or one item of it.
///
/// The value sits under a key named after the kind, as Notion writes it:
/// `{ "type": "select", "select": { "name": "Done" } }`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct PropertyValue {
    pub id: Option<String>,
    /// `title`, `rich_text`, `number`, `select`, `relation` and so on.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// The value, under the key named by `type`.
    #[serde(flatten)]
    pub value: BTreeMap<String, Value>,
}

/// One property of a page, read in full a page of items at a time.
///
/// A property with one value, such as a number or a select, has one item. A
/// title, a text, a list of people, a relation or a rollup has an item for
/// each of its parts, and `next_cursor` when there are more.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PropertyItems {
    pub id: Option<String>,
    /// The property's kind: `relation`, `people`, `title` and so on.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    pub items: Vec<PropertyValue>,
    pub next_cursor: Option<String>,
}
