//! Emoji reactions on messages.

use serde_json::{Value, json};
use socketkit_core::Result;

use super::Api;
use crate::models::{Message, Reaction};

/// Emoji reactions on messages.
#[derive(Debug, Clone, Copy)]
pub struct Reactions<'a>(pub(crate) Api<'a>);

impl Reactions<'_> {
    /// Adds an emoji reaction to a message. `name` is the emoji's name without colons.
    pub async fn add(&self, channel: &str, timestamp: &str, name: &str) -> Result<()> {
        self.0
            .post("reactions.add", self.target(channel, timestamp, Some(name))?)
            .await
            .map(drop)
    }

    /// Removes the token's own reaction from a message.
    pub async fn remove(&self, channel: &str, timestamp: &str, name: &str) -> Result<()> {
        self.0
            .post("reactions.remove", self.target(channel, timestamp, Some(name))?)
            .await
            .map(drop)
    }

    /// The reactions on a message.
    pub async fn get(&self, channel: &str, timestamp: &str) -> Result<Vec<Reaction>> {
        let mut arguments = self.target(channel, timestamp, None)?;
        arguments["full"] = json!(true);
        let body = self.0.get("reactions.get", arguments).await?;
        let message: Message = self.0.field(&body, "message")?;
        Ok(message.reactions)
    }

    fn target(&self, channel: &str, timestamp: &str, name: Option<&str>) -> Result<Value> {
        self.0.required("a channel", channel)?;
        self.0.required("a message timestamp", timestamp)?;
        let mut arguments = json!({ "channel": channel, "timestamp": timestamp });
        if let Some(name) = name {
            let name = name.trim().trim_matches(':');
            self.0.required("an emoji name", name)?;
            arguments["name"] = json!(name);
        }
        Ok(arguments)
    }
}
