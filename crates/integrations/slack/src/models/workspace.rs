//! The workspace itself, its emoji, and reminders.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A reminder.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Reminder {
    pub id: String,
    pub creator: Option<String>,
    pub user: Option<String>,
    pub text: String,
    pub time: Option<i64>,
    pub complete_ts: Option<i64>,
    pub recurring: bool,
}

/// The workspace.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Team {
    pub id: String,
    pub name: String,
    pub domain: String,
    pub email_domain: Option<String>,
}

/// The workspace's custom emoji: name to image URL, or to `alias:<name>`.
pub type Emoji = BTreeMap<String, String>;
