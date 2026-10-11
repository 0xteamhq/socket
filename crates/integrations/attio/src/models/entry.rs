//! List entries: a record's place on a list, with the list's own attributes.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::nullable::nullable;
use super::{Sort, Values};

/// The id of a list entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EntryId {
    pub workspace_id: String,
    pub list_id: String,
    pub entry_id: String,
}

/// One entry of a list, with everything Attio holds for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Entry {
    pub id: EntryId,
    /// The record this entry puts on the list.
    pub parent_record_id: Option<String>,
    /// The object that record belongs to, such as `people`.
    pub parent_object: Option<String>,
    pub created_at: Option<String>,
    /// The values of the list's own attributes, as Attio lists them. The
    /// record's values are the record's: read them with `records.get`.
    #[serde(deserialize_with = "nullable")]
    pub entry_values: Values,
    /// Each of those attributes' current value by itself: `null` when it has
    /// none, the value when it has one, a list when it has several. Attio
    /// does not send this; it is read from `entry_values`.
    pub current: BTreeMap<String, Value>,
}

impl Entry {
    /// This entry with `current` filled in from `entry_values`.
    pub(crate) fn with_current(mut self) -> Self {
        self.current = self.entry_values.summary();
        self
    }
}

/// An entry as a query returns it: what it is, and each attribute's current
/// value by itself. `entries.get` returns the values in full.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EntryRow {
    pub id: EntryId,
    pub parent_record_id: Option<String>,
    pub parent_object: Option<String>,
    pub created_at: Option<String>,
    /// Each attribute's current value by itself, as in [`Entry::current`].
    pub current: BTreeMap<String, Value>,
}

impl EntryRow {
    /// The row for `entry`, with only the attributes named in `only` when
    /// any are. A list that names none asks for nothing in particular, and
    /// is read as no list. A name the entry has no attribute by is left out
    /// of the row, which is how it is told from an attribute with no value,
    /// which is `null`.
    pub(crate) fn of(entry: Entry, only: Option<&[String]>) -> Self {
        let mut current = entry.entry_values.summary();
        if let Some(only) = only.filter(|only| !only.is_empty()) {
            current = only
                .iter()
                .filter_map(|attribute| current.remove_entry(attribute))
                .collect();
        }
        Self {
            id: entry.id,
            parent_record_id: entry.parent_record_id,
            parent_object: entry.parent_object,
            created_at: entry.created_at,
            current,
        }
    }
}

/// Which entries of a list to return, and in what order.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QueryEntries {
    /// Attio's filter, as its documentation writes one, over the list's own
    /// attributes: `{ "stage": "Won" }`. Every entry when not given.
    pub filter: Option<Map<String, Value>>,
    pub sorts: Option<Vec<Sort>>,
    /// The slugs of the list's attributes each row should carry. Every
    /// attribute when not given, or when the list is empty. A slug the entry
    /// has no attribute by, a misspelt one for instance, is left out of the
    /// row; an attribute with no value is there as `null`.
    pub attributes: Option<Vec<String>>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most entries to return in one page, from 1 to 500. 25 when not given.
    pub limit: Option<u32>,
}

/// The values to write to a list entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WriteEntry {
    /// For each of the list's attributes, by its slug or id, the value to
    /// write, as Attio's documentation writes one for the attribute's type.
    /// A new entry may have none.
    #[serde(default)]
    pub entry_values: Map<String, Value>,
}
