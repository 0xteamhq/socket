//! Several records in one call: reading, creating and changing them.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::text::nullable;
use super::{CreateObject, Object};

/// Records to read in one call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchRead {
    /// The ids of the records, at most 100. With `idProperty`, the values of
    /// that property.
    pub ids: Vec<String>,
    /// The properties to return of each record.
    pub properties: Option<Vec<String>>,
    /// The name of a property whose values are unique, such as `email` for a
    /// contact. The records are then found by that value and not by their ids.
    pub id_property: Option<String>,
    /// Look among the archived records and no others.
    pub archived: Option<bool>,
}

/// Records to create in one call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BatchCreate {
    /// The records, at most 100.
    pub inputs: Vec<CreateObject>,
}

/// Records to change in one call.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BatchUpdate {
    /// The changes, at most 100, one for each record.
    pub inputs: Vec<BatchUpdateInput>,
}

/// The properties to change on one record of a batch.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BatchUpdateInput {
    /// The record's id. With `idProperty`, the value of that property.
    pub id: String,
    /// The properties to set, each value as text. An empty text clears the property.
    pub properties: BTreeMap<String, String>,
    /// The name of a property whose values are unique.
    pub id_property: Option<String>,
}

/// What a batch call did.
///
/// A batch read that finds only some of its records still succeeds: the
/// ones found are in `results`, and `errors` says which were not.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct BatchResult {
    /// `COMPLETE` once HubSpot has finished.
    pub status: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub results: Vec<Object>,
    #[serde(deserialize_with = "nullable")]
    pub num_errors: u32,
    #[serde(deserialize_with = "nullable")]
    pub errors: Vec<BatchError>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
}

/// Why part of a batch was not done.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct BatchError {
    pub status: Option<String>,
    /// HubSpot's name for the kind of failure, such as `OBJECT_NOT_FOUND`.
    pub category: Option<String>,
    pub message: Option<String>,
    /// What the failure is about: `ids` lists the records that were not found.
    #[serde(deserialize_with = "nullable")]
    pub context: BTreeMap<String, Vec<String>>,
}
