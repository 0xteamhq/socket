//! Reminders. Needs a user token.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Result};

use super::Api;
use crate::models::Reminder;

/// Reminders. Needs a user token.
#[derive(Debug, Clone, Copy)]
pub struct Reminders<'a>(pub(crate) Api<'a>);

impl Reminders<'_> {
    /// Creates a reminder. `time` is a Unix timestamp, a number of seconds
    /// from now, or words such as `"in 15 minutes"` or `"every Thursday"`.
    pub async fn add(&self, text: &str, time: &str) -> Result<Reminder> {
        self.0.required("the reminder's text", text)?;
        self.0.required("a time", time)?;
        let body = self
            .0
            .post("reminders.add", json!({ "text": text, "time": time }))
            .await?;
        self.reminder(&body)
    }

    /// The reminders the token's user created or was sent.
    pub async fn list(&self) -> Result<Vec<Reminder>> {
        let body = self.0.get("reminders.list", json!({})).await?;
        self.0.field(&body, "reminders")
    }

    /// Deletes a reminder.
    pub async fn delete(&self, reminder: &str) -> Result<()> {
        self.0.required("a reminder", reminder)?;
        self.0
            .post("reminders.delete", json!({ "reminder": reminder }))
            .await
            .map(drop)
    }

    /// Marks a reminder as done.
    pub async fn complete(&self, reminder: &str) -> Result<()> {
        self.0.required("a reminder", reminder)?;
        self.0
            .post("reminders.complete", json!({ "reminder": reminder }))
            .await
            .map(drop)
    }

    fn reminder(&self, body: &Value) -> Result<Reminder> {
        let reminder: Reminder = self.0.field(body, "reminder")?;
        if reminder.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a reminder that has no id"));
        }
        Ok(reminder)
    }
}
