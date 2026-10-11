//! Users. Pipedrive offers them in version 1 of its API only.

use serde_json::Value;
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::models::User;

/// The users of the company's Pipedrive.
#[derive(Debug, Clone, Copy)]
pub struct Users<'a>(pub(crate) Api<'a>);

impl Users<'_> {
    /// Lists every user of the company. Pipedrive returns them all at once.
    pub async fn list(&self) -> Result<Vec<User>> {
        let mut body = self.0.send(RawRequest::get("v1/users"), "users").await?;
        match body["data"].take() {
            users @ Value::Array(_) => self.0.decode(users, "users"),
            _ => Err(self.0.error(ErrorKind::Decode, "pipedrive answered without users")),
        }
    }

    /// The signed-in user, with the company the connection is to.
    pub async fn me(&self) -> Result<User> {
        self.0.one(RawRequest::get("v1/users/me"), "a user").await
    }
}
