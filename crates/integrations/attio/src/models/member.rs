//! The people who work in a workspace.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// The id of a workspace member.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WorkspaceMemberId {
    pub workspace_id: String,
    pub workspace_member_id: String,
}

/// Someone who works in the workspace. A member is never deleted, only
/// suspended, so that what they did still has a name beside it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct WorkspaceMember {
    pub id: WorkspaceMemberId,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
    pub avatar_url: Option<String>,
    pub email_address: Option<String>,
    pub created_at: Option<String>,
    /// `admin`, `member` or `suspended`.
    pub access_level: Option<String>,
}
