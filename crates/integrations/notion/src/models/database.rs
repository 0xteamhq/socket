//! Databases, and the data sources that hold their rows.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Parent, RichText, Sort, User};

/// A database: a container for one or more data sources. Its rows and their
/// schema belong to the data sources, which are named in `data_sources`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Database {
    pub id: String,
    pub title: Vec<RichText>,
    pub description: Vec<RichText>,
    pub parent: Option<Parent>,
    /// Whether the database sits inside a page and not on a page of its own.
    pub is_inline: Option<bool>,
    /// Whether the database is in the trash.
    pub in_trash: bool,
    pub is_locked: Option<bool>,
    pub created_time: Option<String>,
    pub last_edited_time: Option<String>,
    /// The database's data sources. Most databases have one.
    pub data_sources: Vec<DataSourceRef>,
    pub icon: Option<Value>,
    pub cover: Option<Value>,
    /// The address that opens the database in Notion.
    pub url: Option<String>,
    pub public_url: Option<String>,
}

/// A data source as a database names it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DataSourceRef {
    /// The id `databases.data_source` and `databases.query` take.
    pub id: String,
    pub name: Option<String>,
}

/// A data source: one table of a database, with its schema.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DataSource {
    pub id: String,
    pub title: Vec<RichText>,
    pub description: Vec<RichText>,
    /// The database the data source belongs to.
    pub parent: Option<Parent>,
    /// Where that database lives.
    pub database_parent: Option<Parent>,
    /// The schema: each property by name, with its id, its kind and the
    /// kind's settings, as Notion writes it. A filter or a sort names these.
    pub properties: BTreeMap<String, Value>,
    /// Whether the data source is in the trash.
    pub in_trash: bool,
    pub created_time: Option<String>,
    pub last_edited_time: Option<String>,
    pub created_by: Option<User>,
    pub last_edited_by: Option<User>,
    pub icon: Option<Value>,
    pub cover: Option<Value>,
    pub url: Option<String>,
    pub public_url: Option<String>,
}

/// Which rows of a data source to return, and in what order.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct QueryDataSource {
    /// A filter, as Notion writes it: one condition on a property or a
    /// timestamp, such as `{ "property": "Status", "status": { "equals": "Done" } }`,
    /// or several joined under `and` or `or`. Every row when not given.
    pub filter: Option<Value>,
    /// What to order the rows by. An earlier key outranks a later one.
    pub sorts: Option<Vec<Sort>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most rows to return, from 1 to 100. Notion may return fewer.
    pub limit: Option<u32>,
}
