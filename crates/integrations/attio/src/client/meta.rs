//! What Attio says about the token itself.

use serde_json::Value;
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::models::TokenInfo;

/// What Attio says about the token itself.
#[derive(Debug, Clone, Copy)]
pub struct Meta<'a>(pub(crate) Api<'a>);

impl Meta<'_> {
    /// Describes the connection's token: the workspace it belongs to, the
    /// scopes it was given, and whether it acts as the workspace or as one member.
    ///
    /// Attio answers this with a success even for a token it no longer
    /// accepts, and says so in the answer. That is reported as what it is:
    /// the person has to connect again.
    pub async fn identify(&self) -> Result<TokenInfo> {
        let body = self.0.send(RawRequest::get("self")).await?;
        if body.get("active") == Some(&Value::Bool(false)) {
            return Err(self
                .0
                .error(ErrorKind::ReconnectRequired, "attio rejected the stored authorization"));
        }
        let token: TokenInfo = self.0.decode(body, "a token")?;
        if !token.active || token.workspace_id.trim().is_empty() {
            return Err(self.0.missing("a workspace"));
        }
        Ok(token.with_scopes())
    }
}
