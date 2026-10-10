//! Who something belongs to, as Graph describes a person, an application or a device.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A person, an application or a device.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Identity {
    pub id: Option<String>,
    /// Graph often leaves this out; look the id up when a name is needed.
    pub display_name: Option<String>,
    pub tenant_id: Option<String>,
}

/// The identities behind one actor. Usually only `user` is set.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct IdentitySet {
    pub user: Option<Identity>,
    pub application: Option<Identity>,
    pub device: Option<Identity>,
}
