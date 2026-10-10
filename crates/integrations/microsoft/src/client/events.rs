//! Events in a calendar, the answers to them, and when people are free.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, with};
use crate::models::{
    Attendee, CancelEvent, CreateEvent, DateTimeTimeZone, Event, EventResponse, FindMeetingTimes, GetSchedule,
    MeetingTimeSuggestions, Paging, RespondToEvent, ScheduleInformation, UpdateEvent,
};

/// The host of an online meeting when the caller asks for one and names none.
const TEAMS: &str = "teamsForBusiness";

/// Asks Graph to write every time in the answer in UTC, whatever zone the
/// event was made in. The event keeps its own zone beside the times.
fn in_utc(request: RawRequest) -> RawRequest {
    request.with_header("Prefer", "outlook.timezone=\"UTC\"")
}

/// `content` without each attendee's `status`. Graph fills that in from the
/// answers it receives; a list read from Graph carries it, and is the
/// natural list to send back with one attendee added.
fn invitable(mut content: Value) -> Value {
    let attendees = content.get_mut("attendees").and_then(Value::as_array_mut);
    for attendee in attendees.into_iter().flatten() {
        if let Some(attendee) = attendee.as_object_mut() {
            attendee.remove("status");
        }
    }
    content
}

/// Events in a calendar, the answers to them, and when people are free.
#[derive(Debug, Clone, Copy)]
pub struct Events<'a>(pub(crate) Api<'a>);

impl Events<'_> {
    /// Lists what is in a calendar between two times, with each occurrence
    /// of a repeating event as an event of its own.
    ///
    /// `start` and `end` are ISO 8601, such as `2026-10-12T00:00:00Z`; a time
    /// without an offset is in UTC. `calendar` is a calendar id, or `None`
    /// for the account's default calendar.
    pub async fn list_between(
        &self,
        start: &str,
        end: &str,
        calendar: Option<&str>,
        paging: Paging,
    ) -> Result<Page<Event>> {
        let view = match calendar {
            Some(calendar) => format!(
                "me/calendars/{}/calendarView",
                self.0.segment("a calendar id", calendar)?
            ),
            None => "me/calendarView".to_owned(),
        };
        self.0.page(self.between(view, start, end)?, &paging, "events").await
    }

    /// Gets one event.
    pub async fn get(&self, event: &str) -> Result<Event> {
        let event = self.0.segment("an event id", event)?;
        let body = self
            .0
            .send(in_utc(RawRequest::get(format!("me/events/{event}"))))
            .await?;
        self.event(body)
    }

    /// Lists the occurrences of a repeating event between two times. `event`
    /// is the id of the series.
    pub async fn instances(&self, event: &str, start: &str, end: &str, paging: Paging) -> Result<Page<Event>> {
        let event = self.0.segment("an event id", event)?;
        let instances = self.between(format!("me/events/{event}/instances"), start, end)?;
        self.0.page(instances, &paging, "events").await
    }

    /// Suggests times when a meeting could be held. Changes nothing.
    ///
    /// Work and school accounts only.
    pub async fn find_meeting_times(&self, meeting: FindMeetingTimes) -> Result<MeetingTimeSuggestions> {
        let slots = meeting.time_constraint.iter().flat_map(|c| &c.time_slots);
        for slot in slots {
            self.time("the start of a time slot", &slot.start)?;
            self.time("the end of a time slot", &slot.end)?;
        }
        self.attendees(meeting.attendees.as_deref())?;
        let request = RawRequest::post("me/findMeetingTimes", invitable(with(json!({}), &meeting)));
        let body = self.0.send(in_utc(request)).await?;
        self.0.decode(body, "meeting times")
    }

    /// Reads when people, distribution lists and rooms are free and busy.
    /// Changes nothing.
    ///
    /// Work and school accounts only.
    pub async fn schedule(&self, schedule: GetSchedule) -> Result<Vec<ScheduleInformation>> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        if schedule.schedules.is_empty() || schedule.schedules.iter().any(|s| s.trim().is_empty()) {
            return Err(invalid(
                "`schedules` needs at least one address, and none of them blank",
            ));
        }
        self.time("a start time", &schedule.start_time)?;
        self.time("an end time", &schedule.end_time)?;
        if schedule
            .availability_view_interval
            .is_some_and(|minutes| !(5..=1440).contains(&minutes))
        {
            return Err(invalid("`availabilityViewInterval` is from 5 to 1440 minutes"));
        }
        let request = RawRequest::post("me/calendar/getSchedule", with(json!({}), &schedule));
        let body = self.0.send(in_utc(request)).await?;
        self.0.list(&body, "schedules")
    }

    /// Creates an event, and sends an invitation to each attendee.
    ///
    /// `calendar` is a calendar id, or `None` for the account's default
    /// calendar. Ask for an online meeting and the event that comes back
    /// carries the link to join it.
    pub async fn create(&self, calendar: Option<&str>, event: CreateEvent) -> Result<Event> {
        let events = match calendar {
            Some(calendar) => format!("me/calendars/{}/events", self.0.segment("a calendar id", calendar)?),
            None => "me/events".to_owned(),
        };
        self.time("a start time", &event.start)?;
        self.time("an end time", &event.end)?;
        self.attendees(event.attendees.as_deref())?;
        self.0.body(event.body.as_ref())?;
        let mut content = invitable(with(json!({}), &event));
        if event.is_online_meeting == Some(true) && event.online_meeting_provider.is_none() {
            content["onlineMeetingProvider"] = json!(TEAMS);
        }
        let body = self.0.send(in_utc(RawRequest::post(events, content))).await?;
        self.event(body)
    }

    /// Changes an event. Only what is set in `changes` is touched.
    pub async fn update(&self, event: &str, changes: UpdateEvent) -> Result<Event> {
        let event = self.0.segment("an event id", event)?;
        for (what, time) in [("a start time", &changes.start), ("an end time", &changes.end)] {
            if let Some(time) = time {
                self.time(what, time)?;
            }
        }
        self.attendees(changes.attendees.as_deref())?;
        self.0.body(changes.body.as_ref())?;
        let content = invitable(with(json!({}), &changes));
        if content.as_object().is_none_or(serde_json::Map::is_empty) {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "an update needs at least one field to change"));
        }
        let request = RawRequest::new("PATCH", format!("me/events/{event}")).with_body(content);
        let body = self.0.send(in_utc(request)).await?;
        self.event(body)
    }

    /// Answers an invitation: accepts it, accepts it tentatively, or declines it.
    pub async fn respond(&self, event: &str, response: EventResponse, options: RespondToEvent) -> Result<()> {
        let event = self.0.segment("an event id", event)?;
        let action = match response {
            EventResponse::Accept => "accept",
            EventResponse::TentativelyAccept => "tentativelyAccept",
            EventResponse::Decline => "decline",
        };
        if let Some(proposed) = &options.proposed_new_time {
            if response == EventResponse::Accept {
                return Err(self.0.error(
                    ErrorKind::InvalidInput,
                    "another time can be proposed only with a decline or a tentative acceptance",
                ));
            }
            self.time("the start of the proposed time", &proposed.start)?;
            self.time("the end of the proposed time", &proposed.end)?;
        }
        let request = RawRequest::post(format!("me/events/{event}/{action}"), with(json!({}), &options));
        self.0.send(request).await.map(drop)
    }

    /// Cancels a meeting the account organised, and tells its attendees.
    pub async fn cancel(&self, event: &str, options: CancelEvent) -> Result<()> {
        let event = self.0.segment("an event id", event)?;
        let request = RawRequest::post(format!("me/events/{event}/cancel"), with(json!({}), &options));
        self.0.send(request).await.map(drop)
    }

    /// Deletes an event from the account's calendar. When the account
    /// organised it, Graph tells the attendees it is cancelled.
    pub async fn delete(&self, event: &str) -> Result<()> {
        let event = self.0.segment("an event id", event)?;
        self.0
            .send(RawRequest::new("DELETE", format!("me/events/{event}")))
            .await
            .map(drop)
    }

    /// The request for what lies between two times at `path`.
    fn between(&self, path: String, start: &str, end: &str) -> Result<RawRequest> {
        self.0.required("a start time", start)?;
        self.0.required("an end time", end)?;
        Ok(in_utc(RawRequest::get(path))
            .with_query("startDateTime", start)
            .with_query("endDateTime", end))
    }

    fn time(&self, what: &str, time: &DateTimeTimeZone) -> Result<()> {
        if time.is_set() {
            Ok(())
        } else {
            Err(self.0.error(
                ErrorKind::InvalidInput,
                format!("{what} needs both `dateTime` and `timeZone`"),
            ))
        }
    }

    fn attendees(&self, attendees: Option<&[Attendee]>) -> Result<()> {
        let blank = |attendee: &Attendee| attendee.email_address.address.trim().is_empty();
        if attendees.unwrap_or_default().iter().any(blank) {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "every attendee needs `emailAddress.address`"));
        }
        Ok(())
    }

    fn event(&self, body: Value) -> Result<Event> {
        let event: Event = self.0.decode(body, "an event")?;
        if event.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "microsoft answered without an event"));
        }
        Ok(event)
    }
}
