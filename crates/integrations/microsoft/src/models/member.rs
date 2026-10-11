//! Who belongs to a team, a channel or a chat.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// One member of a team, a channel or a chat.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ConversationMember {
    /// The id of the membership, which is not the person's id.
    pub id: Option<String>,
    pub display_name: Option<String>,
    /// The person's id in the directory.
    pub user_id: Option<String>,
    pub email: Option<String>,
    /// `owner` for an owner, `guest` for a guest; empty for an ordinary member.
    #[serde(deserialize_with = "nullable")]
    pub roles: Vec<String>,
    pub tenant_id: Option<String>,
}
