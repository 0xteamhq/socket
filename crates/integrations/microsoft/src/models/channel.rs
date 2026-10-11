//! Channels of a team.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// A channel of a team.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub display_name: String,
    pub description: Option<String>,
    /// `standard`, `private` or `shared`.
    pub membership_type: Option<String>,
    /// The address that opens the channel in Teams.
    pub web_url: Option<String>,
    /// The address that posts to the channel by mail.
    pub email: Option<String>,
    pub created_date_time: Option<String>,
    pub is_archived: Option<bool>,
}
