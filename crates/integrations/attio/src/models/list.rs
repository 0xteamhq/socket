//! Lists: the processes a workspace runs its records through.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// A list: a pipeline, a set of leads, anything a workspace tracks records
/// through. Each record on it is an entry, with the list's own attributes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct List {
    pub id: ListId,
    /// The name the API knows the list by. Every method that takes a list
    /// takes this, or the list's id.
    #[serde(deserialize_with = "nullable")]
    pub api_slug: String,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    /// The objects whose records may be on the list.
    #[serde(deserialize_with = "nullable")]
    pub parent_object: Vec<String>,
    /// What every member of the workspace may do: `full-access`,
    /// `read-and-write` or `read-only`. Absent when the list is not shared
    /// with the whole workspace.
    pub workspace_access: Option<String>,
    /// What particular members may do.
    #[serde(deserialize_with = "nullable")]
    pub workspace_member_access: Vec<MemberAccess>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// A list's id, with the workspace it belongs to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ListId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub list_id: String,
}

/// What one member may do with a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MemberAccess {
    #[serde(deserialize_with = "nullable")]
    pub workspace_member_id: String,
    /// `full-access`, `read-and-write` or `read-only`.
    pub level: Option<String>,
}
