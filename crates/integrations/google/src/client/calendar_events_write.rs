//! Events on a calendar: creating, changing, answering and deleting them.
//!
//! These are methods of [`CalendarEvents`], whose struct and reading methods
//! are in `calendar_events.rs`.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::calendar_events::CalendarEvents;
use super::calendar_time::has_offset;
use super::{set, with_query};
use crate::models::{CalendarEvent, EventDelete, EventInsert, EventInvitee, EventPatch, EventResponse, EventTime};

/// The answers Google takes from an attendee.
const ANSWERS: [&str; 4] = ["accepted", "declined", "tentative", "needsAction"];

impl CalendarEvents<'_> {
    /// Creates an event, with a Google Meet link when `create_meet_link` is set.
    pub async fn insert(&self, calendar: &str, event: EventInsert) -> Result<CalendarEvent> {
        let path = self.events(calendar)?;
        self.time("start", &event.start)?;
        self.time("end", &event.end)?;
        self.invitees(event.attendees.as_deref())?;
        let (request, content) = self.split(RawRequest::new("POST", path), set(&event))?;
        self.event(self.0.send(request.with_body(Value::Object(content))).await?)
    }

    /// Changes an event. Only what is set in `changes` is touched; a guest
    /// list, when given, replaces the one the event had.
    pub async fn patch(&self, calendar: &str, event: &str, changes: EventPatch) -> Result<CalendarEvent> {
        let path = self.one(calendar, event)?;
        for (name, time) in [("start", &changes.start), ("end", &changes.end)] {
            if let Some(time) = time {
                self.time(name, time)?;
            }
        }
        self.invitees(changes.attendees.as_deref())?;
        let (request, mut content) = self.split(RawRequest::new("PATCH", path), set(&changes))?;
        if content.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "a patch needs at least one field to change"));
        }
        // Google merges a patch into the event it has. A new `dateTime` beside
        // the `date` of an all-day event would be refused, so whichever of
        // the two is not given is cleared.
        for name in ["start", "end"] {
            if let Some(Value::Object(time)) = content.get_mut(name) {
                for field in ["date", "dateTime"] {
                    time.entry(field).or_insert(Value::Null);
                }
            }
        }
        self.event(self.0.send(request.with_body(Value::Object(content))).await?)
    }

    /// Answers an invitation on a calendar. On `primary` that is the
    /// signed-in person's own answer; on a calendar they manage for someone
    /// else, it is that person's.
    ///
    /// Google has no call for this alone: an answer is a change to the
    /// event's guest list, and a change replaces the whole list. So the event
    /// is read, the answer is written into the entry Google marks as the
    /// calendar's own, and the list goes back with everyone else exactly as
    /// they were. A list Google showed only part of never goes back as the
    /// list: then the answer is sent alone, marked as partial.
    pub async fn respond(&self, calendar: &str, event: &str, answer: EventResponse) -> Result<CalendarEvent> {
        if !ANSWERS.contains(&answer.response_status.as_str()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`responseStatus` is accepted, declined, tentative or needsAction",
            ));
        }
        let path = self.one(calendar, event)?;
        let mut current = self.0.send(RawRequest::get(path.clone())).await?;
        self.0.kind(&current, "calendar#event", "an event")?;
        if current["id"].as_str().is_none_or(str::is_empty) {
            return Err(self.no_id());
        }
        let mut attendees = match current["attendees"].take() {
            Value::Array(attendees) => attendees,
            _ => Vec::new(),
        };
        let Some(own) = attendees.iter_mut().find(|attendee| attendee["self"] == true) else {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "this calendar's owner is not invited to this event",
            ));
        };
        own["responseStatus"] = json!(answer.response_status);
        if let Some(comment) = answer.comment {
            own["comment"] = json!(comment);
        }
        let request = with_query(
            RawRequest::new("PATCH", path),
            &json!({ "sendUpdates": answer.send_updates }),
        );
        // The list that was read is not the whole of it when the organiser
        // hid the guests from each other, or when Google cut it short. Sent
        // back as it is, it would stand for the whole list. `attendeesOmitted`
        // is how Google is told that it does not: only the answer is changed.
        let partial = current["attendeesOmitted"] == true || current["guestsCanSeeOtherGuests"] == false;
        let request = if partial {
            let own = own.take();
            request.with_body(json!({ "attendeesOmitted": true, "attendees": [own] }))
        } else {
            // With the version that was read, Google refuses the change if
            // the event has changed since: a guest added in between is not
            // dropped.
            let Some(etag) = current["etag"].as_str().filter(|etag| !etag.is_empty()) else {
                return Err(self.0.error(
                    ErrorKind::Decode,
                    "google returned the event without its version, so an answer could overwrite a change",
                ));
            };
            request
                .with_header("If-Match", etag)
                .with_body(json!({ "attendees": attendees }))
        };
        self.event(self.0.send(request).await?)
    }

    /// Deletes an event. One that is already gone is `NotFound`.
    pub async fn delete(&self, calendar: &str, event: &str, options: EventDelete) -> Result<()> {
        let path = self.one(calendar, event)?;
        let request = with_query(RawRequest::new("DELETE", path), &options);
        self.0.send(request).await.map(drop)
    }

    /// Moves what Google takes in the query out of `content`, which is the
    /// rest: the body.
    ///
    /// `sendUpdates` is a query parameter. `createMeetLink` is ours: it
    /// becomes a request to create a conference, which Google ignores unless
    /// `conferenceDataVersion` says the caller understands conferences.
    fn split(
        &self,
        mut request: RawRequest,
        mut content: Map<String, Value>,
    ) -> Result<(RawRequest, Map<String, Value>)> {
        if let Some(Value::String(send_updates)) = content.remove("sendUpdates") {
            request = request.with_query("sendUpdates", send_updates);
        }
        if content.remove("createMeetLink") == Some(Value::Bool(true)) {
            let create =
                json!({ "requestId": self.request_id()?, "conferenceSolutionKey": { "type": "hangoutsMeet" } });
            content.insert("conferenceData".into(), json!({ "createRequest": create }));
            request = request.with_query("conferenceDataVersion", "1");
        }
        Ok((request, content))
    }

    /// An id Google has not seen: 128 bits from the system's random source.
    /// Google ignores a request to create a conference whose id repeats one
    /// it was given before.
    fn request_id(&self) -> Result<String> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| {
            self.0
                .error(ErrorKind::Unexpected, format!("the system's random source failed: {e}"))
        })?;
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// A start or an end is a time or, for an all-day event, a date. Google
    /// needs the time's offset, unless a time zone is named beside it.
    fn time(&self, name: &str, time: &EventTime) -> Result<()> {
        let invalid = |message: String| Err(self.0.error(ErrorKind::InvalidInput, message));
        let blank = |field: &Option<String>| field.as_deref().is_some_and(|text| text.trim().is_empty());
        if blank(&time.date) || blank(&time.date_time) || time.date.is_some() == time.date_time.is_some() {
            return invalid(format!(
                "`{name}` needs a `dateTime`, or a `date` for an all-day event, and not both"
            ));
        }
        let zoned = time.time_zone.as_deref().is_some_and(|zone| !zone.trim().is_empty());
        if time.date_time.as_deref().is_some_and(|at| !zoned && !has_offset(at)) {
            return invalid(format!(
                "`{name}.dateTime` needs its offset, such as 2026-10-12T09:00:00-07:00, or a `timeZone` beside it"
            ));
        }
        Ok(())
    }

    fn invitees(&self, attendees: Option<&[EventInvitee]>) -> Result<()> {
        if attendees
            .unwrap_or_default()
            .iter()
            .any(|attendee| attendee.email.trim().is_empty())
        {
            return Err(self.0.error(ErrorKind::InvalidInput, "every attendee needs an `email`"));
        }
        Ok(())
    }
}
