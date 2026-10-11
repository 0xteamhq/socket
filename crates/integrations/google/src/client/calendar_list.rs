//! The calendars on the signed-in person's calendar list.

use serde_json::Value;
use socketkit_core::{Error, ErrorKind, Page, RawRequest, Result};

use super::{Api, with_query};
use crate::models::{CalendarListEntry, CalendarListFilter, Paging};

const CALENDAR_LIST: &str = "calendar/v3/users/me/calendarList";

/// The most calendars Google returns in one page.
const MOST: u32 = 250;

/// The calendars on the signed-in person's calendar list.
#[derive(Debug, Clone, Copy)]
pub struct CalendarList<'a>(pub(crate) Api<'a>);

impl CalendarList<'_> {
    /// Lists the calendars the person has: their own, and those shared with
    /// or subscribed to by them.
    pub async fn list(&self, filter: CalendarListFilter, paging: Paging) -> Result<Page<CalendarListEntry>> {
        let request = with_query(RawRequest::get(CALENDAR_LIST), &filter);
        let request = self.0.paged(request, &paging, "maxResults", MOST)?;
        let body = self.0.send(request).await?;
        self.0.kind(&body, "calendar#calendarList", "calendars")?;
        let page: Page<CalendarListEntry> = self.0.page(body, "items", "calendars")?;
        if page.items.iter().any(|calendar| calendar.id.is_empty()) {
            return Err(self.no_id());
        }
        Ok(page)
    }

    /// Gets one calendar from the list. `primary` names the person's own.
    pub async fn get(&self, calendar: &str) -> Result<CalendarListEntry> {
        let calendar = self.0.segment("a calendar id", calendar)?;
        let body = self
            .0
            .send(RawRequest::get(format!("{CALENDAR_LIST}/{calendar}")))
            .await?;
        self.entry(body)
    }

    fn entry(&self, body: Value) -> Result<CalendarListEntry> {
        self.0.kind(&body, "calendar#calendarListEntry", "a calendar")?;
        let calendar: CalendarListEntry = self.0.decode(body, "a calendar")?;
        if calendar.id.is_empty() {
            return Err(self.no_id());
        }
        Ok(calendar)
    }

    fn no_id(&self) -> Error {
        self.0
            .error(ErrorKind::Decode, "google answered with a calendar that has no id")
    }
}
