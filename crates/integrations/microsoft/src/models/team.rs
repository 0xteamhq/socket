//! Teams.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// A team in Microsoft Teams.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub display_name: String,
    pub description: Option<String>,
    /// `private`, `public` or `hiddenMembership`. Graph leaves it out of a list of joined teams.
    pub visibility: Option<String>,
    /// The address that opens the team in Teams.
    pub web_url: Option<String>,
    pub is_archived: Option<bool>,
    pub tenant_id: Option<String>,
}
