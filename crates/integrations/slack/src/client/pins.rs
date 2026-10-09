//! Items pinned to a channel.

use serde_json::{Value, json};
use socketkit_core::Result;

use super::Api;
use crate::models::Pin;

/// Items pinned to a channel.
#[derive(Debug, Clone, Copy)]
pub struct Pins<'a>(pub(crate) Api<'a>);

impl Pins<'_> {
    /// Pins a message to its channel.
    pub async fn add(&self, channel: &str, timestamp: &str) -> Result<()> {
        self.0
            .post("pins.add", self.target(channel, timestamp)?)
            .await
            .map(drop)
    }

    /// Unpins a message.
    pub async fn remove(&self, channel: &str, timestamp: &str) -> Result<()> {
        self.0
            .post("pins.remove", self.target(channel, timestamp)?)
            .await
            .map(drop)
    }

    /// What is pinned to a channel.
    pub async fn list(&self, channel: &str) -> Result<Vec<Pin>> {
        self.0.required("a channel", channel)?;
        let body = self.0.get("pins.list", json!({ "channel": channel })).await?;
        self.0.field(&body, "items")
    }

    fn target(&self, channel: &str, timestamp: &str) -> Result<Value> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", timestamp)?;
        Ok(json!({ "channel": channel, "timestamp": timestamp }))
    }
}
