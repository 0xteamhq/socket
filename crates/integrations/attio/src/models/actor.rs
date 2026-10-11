//! Who did something in a workspace.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Who created or changed something: a workspace member, an API token, an
/// application or Attio itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Actor {
    /// The actor's id: a workspace member's id when `type` is `workspace-member`.
    pub id: Option<String>,
    /// `workspace-member`, `api-token`, `app` or `system`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}
