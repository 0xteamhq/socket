//! List entries: what Attio returns for them, and the values used to add and change them.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::Values;
use super::nullable::nullable;
use super::value::current;

/// A record's place on a list, with the values of the list's own attributes.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Entry {
    pub id: EntryId,
    /// The record the entry is for. Its own values are read with `records.get`.
    #[serde(deserialize_with = "nullable")]
    pub parent_record_id: String,
    /// The slug of the object that record belongs to.
    #[serde(deserialize_with = "nullable")]
    pub parent_object: String,
    pub created_at: Option<String>,
    /// The values of the list's attributes as Attio keeps them: a list for
    /// each attribute, each value with its kind and the time it has held since.
    #[serde(deserialize_with = "nullable")]
    pub entry_values: Values,
    /// What each of the list's attributes holds now, in its plainest form.
    /// Worked out from `entry_values`.
    pub current: BTreeMap<String, Value>,
}

impl Entry {
    /// The entry with `current` filled in from its values.
    pub(crate) fn summarised(mut self) -> Self {
        self.current = current(&self.entry_values);
        self
    }
}

/// An entry's id, with the list and the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct EntryId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub list_id: String,
    #[serde(deserialize_with = "nullable")]
    pub entry_id: String,
}

/// A record to put on a list.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreateEntry {
    /// The slug or id of the object the record belongs to, such as `people`.
    pub parent_object: String,
    /// The id of the record to add.
    pub parent_record_id: String,
    /// Values of the list's own attributes, by slug, in the forms Attio
    /// takes. None when not given.
    pub entry_values: Option<Map<String, Value>>,
}

impl CreateEntry {
    /// The record `parent_record_id` of `parent_object`, with no values of the list's own.
    pub fn of(parent_object: impl Into<String>, parent_record_id: impl Into<String>) -> Self {
        Self {
            parent_object: parent_object.into(),
            parent_record_id: parent_record_id.into(),
            entry_values: None,
        }
    }
}

/// The values to give an entry.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WriteEntry {
    /// Values of the list's own attributes, by slug, in the forms Attio
    /// takes. `attributes.list` says which attributes a list has.
    pub entry_values: Map<String, Value>,
}

impl WriteEntry {
    /// One attribute's value, to which more can be added with [`WriteEntry::and`].
    pub fn with(attribute: impl Into<String>, value: impl Into<Value>) -> Self {
        Self::default().and(attribute, value)
    }

    /// Adds another attribute's value.
    pub fn and(mut self, attribute: impl Into<String>, value: impl Into<Value>) -> Self {
        self.entry_values.insert(attribute.into(), value.into());
        self
    }
}
