//! Calendars: the containers events live in.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::EmailAddress;
use super::nullable::nullable;

/// One of the account's calendars.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Calendar {
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub name: String,
    /// A named colour such as `lightBlue`, or `auto`.
    pub color: Option<String>,
    /// The colour as `#RRGGBB`; empty when none was ever set.
    pub hex_color: Option<String>,
    /// Graph leaves this out of some lists, so absent does not mean no.
    pub is_default_calendar: Option<bool>,
    #[serde(deserialize_with = "nullable")]
    pub can_edit: bool,
    #[serde(deserialize_with = "nullable")]
    pub can_share: bool,
    #[serde(deserialize_with = "nullable")]
    pub can_view_private_items: bool,
    /// Whose calendar it is: the account itself, or the person who shared it.
    pub owner: Option<EmailAddress>,
    /// What an online meeting in this calendar is, such as `teamsForBusiness`.
    pub default_online_meeting_provider: Option<String>,
    #[serde(deserialize_with = "nullable")]
    pub allowed_online_meeting_providers: Vec<String>,
}
