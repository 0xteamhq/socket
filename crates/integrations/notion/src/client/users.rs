//! Users: the people and integrations of a workspace.

use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{Paging, User};

/// Users: the people and integrations of a workspace.
#[derive(Debug, Clone, Copy)]
pub struct Users<'a>(pub(crate) Api<'a>);

impl Users<'_> {
    /// Lists the workspace's members and integrations, in no promised order.
    /// Guests are not listed.
    pub async fn list(&self, paging: Paging) -> Result<Page<User>> {
        let body = self.0.send(self.0.listing("users".to_owned(), &paging)?).await?;
        self.0.list(body, "users")
    }

    /// Gets one person or integration, a guest included.
    pub async fn get(&self, user: &str) -> Result<User> {
        let path = format!("users/{}", self.0.id("a user id", user)?);
        let body = self.0.send(RawRequest::get(path)).await?;
        self.0.object(body, "user")
    }
}
