//! Members of the workspace, their profiles and presence, and user groups.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A member of the workspace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct User {
    pub id: String,
    pub name: String,
    pub real_name: Option<String>,
    pub deleted: bool,
    pub is_bot: bool,
    pub is_admin: bool,
    pub tz: Option<String>,
    pub profile: Profile,
}

/// A member's profile.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Profile {
    pub real_name: Option<String>,
    pub display_name: Option<String>,
    /// Present only with the `users:read.email` scope.
    pub email: Option<String>,
    pub title: Option<String>,
    pub status_text: Option<String>,
    pub status_emoji: Option<String>,
    pub image_72: Option<String>,
}

/// Whether a member is active.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Presence {
    /// `"active"` or `"away"`.
    pub presence: String,
    pub online: Option<bool>,
}

/// A user group, such as `@engineering`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct UserGroup {
    pub id: String,
    pub name: String,
    pub handle: String,
    pub description: Option<String>,
    pub user_count: Option<u32>,
    /// Present only when members were asked for.
    pub users: Vec<String>,
}

/// A member's Do Not Disturb state.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct DndStatus {
    pub dnd_enabled: Option<bool>,
    pub snooze_enabled: Option<bool>,
    pub snooze_endtime: Option<i64>,
    pub next_dnd_start_ts: Option<i64>,
    pub next_dnd_end_ts: Option<i64>,
}

/// Which user groups to list, and how much about each.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ListUserGroups {
    pub include_users: Option<bool>,
    pub include_disabled: Option<bool>,
    pub include_count: Option<bool>,
}
