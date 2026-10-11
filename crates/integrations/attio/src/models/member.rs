//! Workspace members: the people who have access to a workspace.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// Someone with access to the workspace.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WorkspaceMember {
    pub id: WorkspaceMemberId,
    #[serde(deserialize_with = "nullable")]
    pub first_name: String,
    #[serde(deserialize_with = "nullable")]
    pub last_name: String,
    pub avatar_url: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub email_address: String,
    /// `admin`, `member` or `suspended`.
    pub access_level: Option<String>,
    pub created_at: Option<String>,
}

/// A workspace member's id, with the workspace they belong to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WorkspaceMemberId {
    #[serde(deserialize_with = "nullable")]
    pub workspace_id: String,
    #[serde(deserialize_with = "nullable")]
    pub workspace_member_id: String,
}
