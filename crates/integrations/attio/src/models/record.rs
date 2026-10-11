//! Records: the people, companies, deals and whatever else a workspace keeps.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::nullable::nullable;
use super::{Sort, Values};

/// The id of a record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RecordId {
    pub workspace_id: String,
    pub object_id: String,
    pub record_id: String,
}

/// One record, with everything Attio holds for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Record {
    pub id: RecordId,
    pub created_at: Option<String>,
    /// The address that opens the record in Attio.
    pub web_url: Option<String>,
    /// Each attribute's values as Attio lists them, with when each became
    /// active and who set it. See [`Values`] for reading one attribute.
    #[serde(deserialize_with = "nullable")]
    pub values: Values,
    /// Each attribute's current value by itself: `null` when it has none,
    /// the value when it has one, a list when it has several. Attio does not
    /// send this; it is read from `values`.
    pub current: BTreeMap<String, Value>,
}

impl Record {
    /// This record with `current` filled in from `values`.
    pub(crate) fn with_current(mut self) -> Self {
        self.current = self.values.summary();
        self
    }
}

/// A record as a query returns it: what it is, and each attribute's current
/// value by itself. `records.get` returns the values in full.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RecordRow {
    pub id: RecordId,
    pub created_at: Option<String>,
    pub web_url: Option<String>,
    /// Each attribute's current value by itself, as in [`Record::current`].
    pub current: BTreeMap<String, Value>,
}

impl RecordRow {
    /// The row for `record`, with only the attributes named in `only` when
    /// any are. A list that names none asks for nothing in particular, and
    /// is read as no list. A name the record has no attribute by is left out
    /// of the row, which is how it is told from an attribute with no value,
    /// which is `null`.
    pub(crate) fn of(record: Record, only: Option<&[String]>) -> Self {
        let mut current = record.values.summary();
        if let Some(only) = only.filter(|only| !only.is_empty()) {
            current = only
                .iter()
                .filter_map(|attribute| current.remove_entry(attribute))
                .collect();
        }
        Self {
            id: record.id,
            created_at: record.created_at,
            web_url: record.web_url,
            current,
        }
    }
}

/// Which records of an object to return, and in what order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueryRecords {
    /// Attio's filter, as its documentation writes one:
    /// `{ "name": "Ada Lovelace" }`, or with operators,
    /// `{ "email_addresses": { "email_domain": { "$eq": "example.com" } } }`.
    /// Every record when not given.
    pub filter: Option<Map<String, Value>>,
    pub sorts: Option<Vec<Sort>>,
    /// The slugs of the attributes each row should carry. Every attribute
    /// when not given, or when the list is empty. Attio returns them all
    /// either way; this keeps a row small. A slug the record has no
    /// attribute by, a misspelt one for instance, is left out of the row; an
    /// attribute with no value is there as `null`.
    pub attributes: Option<Vec<String>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most records to return in one page, from 1 to 500. 25 when not given.
    pub limit: Option<u32>,
}

/// The values to write to a record.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WriteRecord {
    /// For each attribute, by its slug or id, the value to write, as Attio's
    /// documentation writes one for the attribute's type: `"Ada Lovelace"`,
    /// `["ada@example.com"]`, `{ "currency_value": 100 }`. An attribute that
    /// holds several values takes a list.
    pub values: Map<String, Value>,
}

/// One list a record is on.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RecordEntry {
    pub list_id: String,
    pub list_api_slug: Option<String>,
    /// The record's entry on that list. `entries.get` returns its values.
    pub entry_id: String,
    pub created_at: Option<String>,
}
