//! Lists: a workspace's own groupings of records, such as a sales pipeline.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::Actor;
use super::nullable::nullable;

/// The id of a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ListId {
    pub workspace_id: String,
    pub list_id: String,
}

/// A list. Its entries are records of the objects in `parent_object`, each
/// with the list's own attributes beside it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct List {
    pub id: ListId,
    /// The name the list goes by in a path. It is what the `list` argument
    /// of the other methods takes.
    pub api_slug: Option<String>,
    pub name: Option<String>,
    /// The objects whose records the list may hold. A list made today has one.
    #[serde(deserialize_with = "nullable")]
    pub parent_object: Vec<String>,
    /// What the whole workspace may do with the list: `full-access`,
    /// `read-and-write` or `read-only`. Absent for a private list.
    pub workspace_access: Option<String>,
    /// The members given access of their own.
    #[serde(deserialize_with = "nullable")]
    pub workspace_member_access: Vec<MemberAccess>,
    pub created_by_actor: Option<Actor>,
    pub created_at: Option<String>,
}

/// One member's access to a list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct MemberAccess {
    pub workspace_member_id: String,
    /// `full-access`, `read-and-write` or `read-only`.
    pub level: Option<String>,
}
