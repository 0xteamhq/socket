//! The people and integrations of a workspace.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A person or an integration. Where Notion names only who did something,
/// as in `created_by`, only `id` is set.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct User {
    pub id: String,
    /// `person` or `bot`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Absent when the integration may not read user information.
    pub name: Option<String>,
    pub avatar_url: Option<String>,
    pub person: Option<Person>,
    pub bot: Option<Bot>,
}

/// What Notion says about a person.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Person {
    /// Absent unless the integration may read email addresses.
    pub email: Option<String>,
}

/// What Notion says about an integration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Bot {
    /// Who owns the integration: a person or the workspace, as Notion writes it.
    pub owner: Option<Value>,
    pub workspace_name: Option<String>,
    pub workspace_id: Option<String>,
}
