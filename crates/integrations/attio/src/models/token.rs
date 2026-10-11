//! What a token is, as Attio describes it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// The workspace a token belongs to, and what it may do.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct TokenInfo {
    /// False for a token Attio does not know, or one that was revoked or
    /// deleted. Nothing else is set then.
    #[serde(deserialize_with = "nullable")]
    pub active: bool,
    /// The scopes the token was given, separated by spaces, as Attio writes them.
    #[serde(deserialize_with = "nullable")]
    pub scope: String,
    /// The same scopes as a list. Attio does not send this; it is `scope`, split.
    pub scopes: Vec<String>,
    /// The app the token was issued to, or the id of a workspace's own token.
    pub client_id: Option<String>,
    /// `workspace` for a token that acts as the workspace, `user` for one
    /// that acts as one member with that member's permissions.
    pub token_level: Option<String>,
    /// The member who authorised the token. Absent for a token Attio made itself.
    pub authorized_by_workspace_member_id: Option<String>,
    pub workspace_id: String,
    pub workspace_name: Option<String>,
    pub workspace_slug: Option<String>,
    pub workspace_logo_url: Option<String>,
}

impl TokenInfo {
    /// This description with `scopes` filled in from `scope`.
    pub(crate) fn with_scopes(mut self) -> Self {
        self.scopes = self.scope.split_whitespace().map(str::to_owned).collect();
        self
    }
}
