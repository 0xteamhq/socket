//! The workspace itself, its emoji, and Do Not Disturb.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::models::{DndStatus, Emoji, Team};

/// The workspace itself, its emoji, and Do Not Disturb.
#[derive(Debug, Clone, Copy)]
pub struct Workspace<'a>(pub(crate) Api<'a>);

impl Workspace<'_> {
    /// The workspace's name and domain.
    pub async fn info(&self) -> Result<Team> {
        let body = self.0.get("team.info", json!({})).await?;
        let team: Team = self.0.field(&body, "team")?;
        if team.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a workspace that has no id"));
        }
        Ok(team)
    }

    /// The workspace's custom emoji.
    pub async fn emoji(&self) -> Result<Emoji> {
        let body = self.0.get("emoji.list", json!({})).await?;
        self.0.field(&body, "emoji")
    }

    /// A member's Do Not Disturb state.
    pub async fn dnd_info(&self, user: &str) -> Result<DndStatus> {
        self.0.required("a user", user)?;
        let body = self.0.get("dnd.info", json!({ "user": user })).await?;
        self.dnd(body)
    }

    /// Turns on Do Not Disturb for the token's user for `minutes`.
    pub async fn dnd_set_snooze(&self, minutes: u32) -> Result<DndStatus> {
        if minutes == 0 {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "the number of minutes must be at least 1"));
        }
        // This method takes its argument as a query parameter even though it writes.
        let request = RawRequest::new("POST", "dnd.setSnooze").with_query("num_minutes", minutes.to_string());
        let body = self.0.connection.request(request).await?.body;
        self.dnd(body)
    }

    /// Turns off the token's user's Do Not Disturb snooze.
    pub async fn dnd_end_snooze(&self) -> Result<DndStatus> {
        let body = self.0.post("dnd.endSnooze", json!({})).await?;
        self.dnd(body)
    }

    fn dnd(&self, body: Value) -> Result<DndStatus> {
        serde_json::from_value(body).map_err(|e| {
            self.0
                .error(
                    ErrorKind::Decode,
                    "slack sent a Do Not Disturb state that could not be read",
                )
                .with_source(e)
        })
    }
}
