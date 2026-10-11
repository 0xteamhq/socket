//! Who can see a file in Drive, and in what role.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One grant of access to a file: to a person, a group, a whole domain, or anyone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DrivePermission {
    pub id: String,
    /// Who it is granted to: `user`, `group`, `domain` or `anyone`.
    #[serde(rename = "type")]
    pub kind: String,
    /// What they may do: `owner`, `organizer`, `fileOrganizer`, `writer`,
    /// `commenter` or `reader`.
    pub role: String,
    /// The address of the person or the group. Absent for a domain and for anyone.
    pub email_address: Option<String>,
    /// The domain, when it is granted to everyone in one.
    pub domain: Option<String>,
    /// The name of the person, the group or the domain. Absent for anyone.
    pub display_name: Option<String>,
    /// Whether the account it was granted to has since been deleted.
    pub deleted: bool,
    /// For a domain or anyone: whether the file can be found by searching.
    /// When `false`, only someone who has the link can open it.
    pub allow_file_discovery: Option<bool>,
    /// When the grant ends, in RFC 3339. Absent when it does not.
    pub expiration_time: Option<String>,
    /// Where the access comes from: the file itself, or something above it.
    /// One grant may have several sources, as when a member of a shared
    /// drive was also given the file.
    pub permission_details: Vec<DrivePermissionDetail>,
}

/// One source of a grant: set on the file, or inherited.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DrivePermissionDetail {
    /// `file` for a grant on a file or a folder, `member` for membership of
    /// the shared drive.
    pub permission_type: Option<String>,
    /// The id of the folder or the shared drive it is inherited from.
    pub inherited_from: Option<String>,
    /// The role this source gives.
    pub role: Option<String>,
    /// Whether it comes from above and not from the file itself.
    pub inherited: bool,
}
