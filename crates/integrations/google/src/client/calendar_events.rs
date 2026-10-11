//! Events on a calendar: reading them.
//!
//! Creating, changing, answering and deleting are in
//! `calendar_events_write.rs`; they are methods of the same group, kept apart
//! only for the length of the file.

use serde_json::Value;
use socketkit_core::{Error, ErrorKind, Page, RawRequest, Result};

use super::calendar_time::instant;
use super::{Api, with_query};
use crate::models::{CalendarEvent, EventFilter, EventInstancesFilter, Paging};

/// The most events Google returns in one page.
const MOST: u32 = 2500;

/// Events on a calendar.
#[derive(Debug, Clone, Copy)]
pub struct CalendarEvents<'a>(pub(crate) Api<'a>);

impl CalendarEvents<'_> {
    /// Lists a calendar's events. `primary` names the signed-in person's own
    /// calendar.
    ///
    /// Without `single_events` a recurring event comes back once, as the
    /// series; with it, each occurrence in the window comes back by itself.
    pub async fn list(&self, calendar: &str, filter: EventFilter, paging: Paging) -> Result<Page<CalendarEvent>> {
        let path = self.events(calendar)?;
        let times = [
            ("timeMin", &filter.time_min),
            ("timeMax", &filter.time_max),
            ("updatedMin", &filter.updated_min),
        ];
        for (name, time) in times {
            if let Some(time) = time {
                instant(&self.0, name, time)?;
            }
        }
        // Google refuses this; a series has no one start to sort by.
        let by_start = filter.order_by.as_deref().map(str::trim) == Some("startTime");
        if by_start && filter.single_events != Some(true) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`orderBy` can be `startTime` only when `singleEvents` is true",
            ));
        }
        let request = with_query(RawRequest::get(path), &filter);
        let request = self.0.paged(request, &paging, "maxResults", MOST)?;
        self.page(self.0.send(request).await?)
    }

    /// Gets one event.
    pub async fn get(&self, calendar: &str, event: &str) -> Result<CalendarEvent> {
        let path = self.one(calendar, event)?;
        self.event(self.0.send(RawRequest::get(path)).await?)
    }

    /// Lists the occurrences of a recurring event. `event` is the id of the
    /// series.
    pub async fn instances(
        &self,
        calendar: &str,
        event: &str,
        filter: EventInstancesFilter,
        paging: Paging,
    ) -> Result<Page<CalendarEvent>> {
        let path = format!("{}/instances", self.one(calendar, event)?);
        for (name, time) in [("timeMin", &filter.time_min), ("timeMax", &filter.time_max)] {
            if let Some(time) = time {
                instant(&self.0, name, time)?;
            }
        }
        let request = with_query(RawRequest::get(path), &filter);
        let request = self.0.paged(request, &paging, "maxResults", MOST)?;
        self.page(self.0.send(request).await?)
    }

    /// The path of a calendar's events.
    pub(super) fn events(&self, calendar: &str) -> Result<String> {
        let calendar = self.0.segment("a calendar id", calendar)?;
        Ok(format!("calendar/v3/calendars/{calendar}/events"))
    }

    /// The path of one event.
    pub(super) fn one(&self, calendar: &str, event: &str) -> Result<String> {
        let events = self.events(calendar)?;
        let event = self.0.segment("an event id", event)?;
        Ok(format!("{events}/{event}"))
    }

    /// Reads an answer as the event it should be.
    pub(super) fn event(&self, body: Value) -> Result<CalendarEvent> {
        self.0.kind(&body, "calendar#event", "an event")?;
        let event: CalendarEvent = self.0.decode(body, "an event")?;
        if event.id.is_empty() {
            return Err(self.no_id());
        }
        Ok(event)
    }

    fn page(&self, body: Value) -> Result<Page<CalendarEvent>> {
        self.0.kind(&body, "calendar#events", "events")?;
        let page: Page<CalendarEvent> = self.0.page(body, "items", "events")?;
        if page.items.iter().any(|event| event.id.is_empty()) {
            return Err(self.no_id());
        }
        Ok(page)
    }

    pub(super) fn no_id(&self) -> Error {
        self.0
            .error(ErrorKind::Decode, "google answered with an event that has no id")
    }
}
