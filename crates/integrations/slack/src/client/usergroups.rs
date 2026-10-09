//! User groups such as `@engineering`.

use serde_json::json;
use socketkit_core::Result;

use super::{Api, with};
use crate::models::{ListUserGroups, UserGroup};

/// User groups such as `@engineering`.
#[derive(Debug, Clone, Copy)]
pub struct UserGroups<'a>(pub(crate) Api<'a>);

impl UserGroups<'_> {
    /// Lists user groups.
    pub async fn list(&self, options: ListUserGroups) -> Result<Vec<UserGroup>> {
        let body = self.0.get("usergroups.list", with(json!({}), &options)).await?;
        self.0.field(&body, "usergroups")
    }

    /// The ids of a user group's members.
    pub async fn members(&self, usergroup: &str) -> Result<Vec<String>> {
        self.0.required("a user group", usergroup)?;
        let body = self
            .0
            .get("usergroups.users.list", json!({ "usergroup": usergroup }))
            .await?;
        self.0.field(&body, "users")
    }
}
