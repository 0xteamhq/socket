//! Records: what Attio returns for them, and the values used to create and change them.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::Values;
use super::nullable::nullable;
use super::value::current;

/// One record of an object: a person, a company, a deal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Record {
    pub id: RecordId,
    pub created_at: Option<String>,
    /// The address that opens the record in Attio.
    pub web_url: Option<String>,
    /// Every attribute's values as Attio keeps them: a list for each
    /// attribute, each value with its kind and the time it has held since.
    #[serde(deserialize_with = "nullable")]
    pub values: Values,
    /// What each attribute holds now, in its plainest form: nothing, one
    /// value, or a list of them. Worked out from `values`.
    pub current: BTreeMap<String, Value>,
}

impl Record {
    /// The record with `current` filled in from its values.
    pub(crate) fn summarised(mut self) -> Self {
        self.current = current(&self.values);
        self
    }
}

/// A record's id, with the object and the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RecordId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub object_id: String,
    #[serde(deserialize_with = "nullable")]
    pub record_id: String,
}

/// The values to give a record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WriteRecord {
    /// Values by attribute slug, in the forms Attio takes: a plain value,
    /// such as `"name": "Ada Lovelace"` or `"employees": 42`, or a list of
    /// values for an attribute that holds several. `attributes.list` says
    /// which attributes an object has and which may be written.
    pub values: Map<String, Value>,
}

impl WriteRecord {
    /// One attribute's value, to which more can be added with [`WriteRecord::and`].
    pub fn with(attribute: impl Into<String>, value: impl Into<Value>) -> Self {
        Self::default().and(attribute, value)
    }

    /// Adds another attribute's value.
    pub fn and(mut self, attribute: impl Into<String>, value: impl Into<Value>) -> Self {
        self.values.insert(attribute.into(), value.into());
        self
    }
}

/// A list a record is on: which list, and the record's entry in it. The
/// entry's own values are read with `entries.get`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct RecordEntry {
    #[serde(deserialize_with = "nullable")]
    pub list_id: String,
    pub list_api_slug: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub entry_id: String,
    pub created_at: Option<String>,
}
