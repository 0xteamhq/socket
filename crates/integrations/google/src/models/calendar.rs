//! The calendars on a person's calendar list.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A calendar as it appears on the signed-in person's calendar list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct CalendarListEntry {
    /// The calendar's id, which the event methods take. A person's own
    /// calendar has their email address as its id.
    pub id: String,
    /// The calendar's title.
    pub summary: Option<String>,
    /// The title this person gave the calendar, when they renamed it.
    pub summary_override: Option<String>,
    pub description: Option<String>,
    pub location: Option<String>,
    /// An IANA name such as `Europe/Zurich`.
    pub time_zone: Option<String>,
    /// `freeBusyReader`, `reader`, `writer` or `owner`.
    pub access_role: Option<String>,
    /// True for the person's own calendar, which `primary` also names.
    pub primary: bool,
    /// Whether the person has it switched on in Google Calendar.
    pub selected: bool,
    pub hidden: bool,
    pub deleted: bool,
    pub color_id: Option<String>,
    pub background_color: Option<String>,
    pub foreground_color: Option<String>,
}

/// Which calendars to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ListCalendars {
    /// Only calendars the person has at least this access to:
    /// `freeBusyReader`, `reader`, `writer` or `owner`.
    pub min_access_role: Option<String>,
    /// Include calendars the person has hidden.
    pub show_hidden: Option<bool>,
    /// Include calendars removed from the list.
    pub show_deleted: Option<bool>,
    /// How many calendars per page, at most 250. Google's default is 100.
    pub max_results: Option<u32>,
    /// The `next_cursor` of the page before; absent for the first page.
    pub page_token: Option<String>,
}
