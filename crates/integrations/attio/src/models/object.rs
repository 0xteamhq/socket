//! Objects: the kinds of record a workspace keeps, such as people, companies and deals.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The id of an object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ObjectId {
    pub workspace_id: String,
    pub object_id: String,
}

/// A kind of record. Attio defines some (`people`, `companies`); a workspace
/// adds its own.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Object {
    pub id: ObjectId,
    /// The name the object goes by in a path, such as `people`. It is what
    /// the `object` argument of the other methods takes.
    pub api_slug: Option<String>,
    pub singular_noun: Option<String>,
    pub plural_noun: Option<String>,
    pub created_at: Option<String>,
}
