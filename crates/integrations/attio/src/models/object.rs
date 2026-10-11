//! Objects: the kinds of record a workspace keeps.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// A kind of record: people, companies, deals, or one the workspace defined.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Object {
    pub id: ObjectId,
    /// The name the API knows the object by, such as `people`. Every method
    /// that takes an object takes this, or the object's id.
    pub api_slug: Option<String>,
    pub singular_noun: Option<String>,
    pub plural_noun: Option<String>,
    pub created_at: Option<String>,
}

/// An object's id, with the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ObjectId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub object_id: String,
}
