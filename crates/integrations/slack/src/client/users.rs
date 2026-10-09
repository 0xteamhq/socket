//! The members of the workspace.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, Result};

use super::{Api, with};
use crate::models::{Paging, Presence, Profile, User};

/// The members of the workspace.
#[derive(Debug, Clone, Copy)]
pub struct Users<'a>(pub(crate) Api<'a>);

impl Users<'_> {
    /// Lists the members of the workspace.
    pub async fn list(&self, paging: Paging) -> Result<Page<User>> {
        let body = self.0.get("users.list", with(json!({}), &paging)).await?;
        self.0.page(&body, "members")
    }

    /// One member.
    pub async fn info(&self, user: &str) -> Result<User> {
        self.0.required("a user", user)?;
        let body = self.0.get("users.info", json!({ "user": user })).await?;
        self.user(&body)
    }

    /// The member with this email address. Needs the `users:read.email` scope.
    pub async fn lookup_by_email(&self, email: &str) -> Result<User> {
        self.0.required("an email address", email)?;
        let body = self.0.get("users.lookupByEmail", json!({ "email": email })).await?;
        self.user(&body)
    }

    /// Whether a member is active.
    pub async fn presence(&self, user: &str) -> Result<Presence> {
        self.0.required("a user", user)?;
        let body = self.0.get("users.getPresence", json!({ "user": user })).await?;
        let presence: Presence = serde_json::from_value(body).map_err(|e| {
            self.0
                .error(ErrorKind::Decode, "slack sent a presence that could not be read")
                .with_source(e)
        })?;
        if presence.presence.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "slack answered without a presence"));
        }
        Ok(presence)
    }

    /// A member's profile.
    pub async fn profile(&self, user: &str) -> Result<Profile> {
        self.0.required("a user", user)?;
        let body = self.0.get("users.profile.get", json!({ "user": user })).await?;
        self.0.field(&body, "profile")
    }

    fn user(&self, body: &Value) -> Result<User> {
        let user: User = self.0.field(body, "user")?;
        if user.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a user that has no id"));
        }
        Ok(user)
    }
}
