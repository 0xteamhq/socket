//! The calendars on the signed-in person's calendar list.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, Result};

use super::{Api, set};
use crate::models::{CalendarListEntry, ListCalendars};

const CALENDAR_LIST: &str = "calendar/v3/users/me/calendarList";

/// The calendars on the signed-in person's calendar list.
#[derive(Debug, Clone, Copy)]
pub struct CalendarList<'a>(pub(crate) Api<'a>);

impl CalendarList<'_> {
    /// The calendars the person has: their own, and those shared with or subscribed to by them.
    pub async fn list(&self, options: ListCalendars) -> Result<Page<CalendarListEntry>> {
        let body = self
            .0
            .get(CALENDAR_LIST.to_owned(), &Value::Object(set(&options)))
            .await?;
        let page: Page<CalendarListEntry> = self.0.page(body, "calendar#calendarList", "calendars")?;
        if page.items.iter().any(|calendar| calendar.id.is_empty()) {
            return Err(self.no_id());
        }
        Ok(page)
    }

    /// One calendar from the list. `primary` names the person's own.
    pub async fn get(&self, calendar_id: &str) -> Result<CalendarListEntry> {
        let path = format!("{CALENDAR_LIST}/{}", self.0.segment("a calendar id", calendar_id)?);
        let calendar: CalendarListEntry = self.0.decode(self.0.get(path, &json!({})).await?, "a calendar")?;
        if calendar.id.is_empty() {
            return Err(self.no_id());
        }
        Ok(calendar)
    }

    fn no_id(&self) -> socketkit_core::Error {
        self.0
            .error(ErrorKind::Decode, "google answered with a calendar that has no id")
    }
}
