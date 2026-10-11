//! Who did something in a workspace.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Who created or changed something: a person, an API token, an app, or
/// Attio itself.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Actor {
    /// The actor's id: a workspace member id when `type` is `workspace-member`.
    pub id: Option<String>,
    /// `workspace-member`, `api-token`, `app` or `system`.
    #[serde(rename = "type")]
    pub kind: Option<String>,
}
