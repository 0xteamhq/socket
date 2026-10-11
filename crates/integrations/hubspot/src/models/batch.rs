//! Records read, created and changed many at a time.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::nullable::nullable;
use super::{CreateObject, Record};

/// Which records to read at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRead {
    /// The ids of the records, at most 100.
    pub ids: Vec<String>,
    /// The properties to return, by their internal names.
    pub properties: Option<Vec<String>>,
    /// The name of a property with unique values, such as `email` for a
    /// contact, when `ids` holds those values and not record ids.
    pub id_property: Option<String>,
    /// Read the records from the recycling bin.
    pub archived: Option<bool>,
}

/// Records to create at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BatchCreate {
    /// The new records, at most 100.
    pub inputs: Vec<CreateObject>,
}

/// Records to change at once.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BatchUpdate {
    /// The changes, one for each record, at most 100.
    pub inputs: Vec<RecordUpdate>,
}

/// What to change on one record of a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecordUpdate {
    /// The record's id, or its value of `idProperty`.
    pub id: String,
    /// The properties to set, each as a string. An empty string clears a property.
    pub properties: BTreeMap<String, String>,
    /// The name of a property with unique values, when `id` holds that value.
    pub id_property: Option<String>,
}

/// What a batch did: the records it read or wrote, and what went wrong with the rest.
///
/// HubSpot answers a batch it carried out only in part with success, and
/// lists the inputs that failed under `errors`. A record that does not exist
/// is such an error, not a failure of the call.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct BatchResult {
    /// `COMPLETE` once HubSpot has finished.
    pub status: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub results: Vec<Record>,
    /// How many inputs failed. Absent when none did.
    pub num_errors: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    pub errors: Vec<BatchError>,
}

/// Why some inputs of a batch failed.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct BatchError {
    pub status: Option<String>,
    /// HubSpot's name for the kind of error, such as `OBJECT_NOT_FOUND`.
    pub category: Option<String>,
    pub message: Option<String>,
    /// What the error is about, as HubSpot sent it: the ids that were not found, for one.
    pub context: Option<Value>,
}
