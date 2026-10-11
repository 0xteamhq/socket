//! Shared drives: the drives that belong to an organisation and not to one person.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A shared drive the account is a member of.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SharedDrive {
    /// Also the id of the drive's top folder: what is at the top of the
    /// drive has it as its parent.
    pub id: String,
    pub name: String,
    /// RFC 3339, in UTC.
    pub created_time: Option<String>,
    /// Whether the drive is hidden from the account's default view.
    pub hidden: bool,
}
