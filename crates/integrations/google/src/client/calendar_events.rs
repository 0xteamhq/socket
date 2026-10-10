//! Events on a calendar.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, set};
use crate::models::{DeleteEvent, Event, EventTime, InsertEvent, Instances, ListEvents, PatchEvent, Respond};

/// The answers Google takes from an attendee.
const ANSWERS: [&str; 4] = ["accepted", "declined", "tentative", "needsAction"];

/// Events on a calendar.
#[derive(Debug, Clone, Copy)]
pub struct CalendarEvents<'a>(pub(crate) Api<'a>);

impl CalendarEvents<'_> {
    /// A calendar's events. `primary` names the signed-in person's own calendar.
    ///
    /// Without `single_events` a recurring event comes back once, as the
    /// series; with it, each occurrence in the window comes back by itself.
    pub async fn list(&self, calendar_id: &str, options: ListEvents) -> Result<Page<Event>> {
        let path = self.events(calendar_id)?;
        self.page(self.0.get(path, &Value::Object(set(&options))).await?)
    }

    /// One event.
    pub async fn get(&self, calendar_id: &str, event_id: &str) -> Result<Event> {
        let path = self.one(calendar_id, event_id)?;
        self.event(self.0.get(path, &json!({})).await?)
    }

    /// The occurrences of a recurring event.
    pub async fn instances(&self, calendar_id: &str, event_id: &str, options: Instances) -> Result<Page<Event>> {
        let path = format!("{}/instances", self.one(calendar_id, event_id)?);
        self.page(self.0.get(path, &Value::Object(set(&options))).await?)
    }

    /// Creates an event, with a Google Meet link when `create_meet_link` is set.
    pub async fn insert(&self, calendar_id: &str, event: InsertEvent) -> Result<Event> {
        self.time("start", &event.start)?;
        self.time("end", &event.end)?;
        let path = self.events(calendar_id)?;
        let (arguments, body) = self.split(set(&event))?;
        let request = RawRequest::new("POST", path);
        self.event(self.0.send(request, &arguments, Some(Value::Object(body))).await?)
    }

    /// Changes an event. Only the fields that are set are changed.
    pub async fn patch(&self, calendar_id: &str, event_id: &str, changes: PatchEvent) -> Result<Event> {
        for (name, time) in [("start", &changes.start), ("end", &changes.end)] {
            if let Some(time) = time {
                self.time(name, time)?;
            }
        }
        let path = self.one(calendar_id, event_id)?;
        let (arguments, mut body) = self.split(set(&changes))?;
        if body.is_empty() {
            return Err(self.0.error(ErrorKind::InvalidInput, "no change was given"));
        }
        // Google merges a patch into the event it has. A new `dateTime` beside
        // the `date` of an all-day event would be refused, so whichever of
        // the two is not given is cleared.
        for name in ["start", "end"] {
            if let Some(Value::Object(time)) = body.get_mut(name) {
                for field in ["date", "dateTime"] {
                    time.entry(field).or_insert(Value::Null);
                }
            }
        }
        let request = RawRequest::new("PATCH", path);
        self.event(self.0.send(request, &arguments, Some(Value::Object(body))).await?)
    }

    /// Answers an invitation on a calendar. On `primary` that is the
    /// signed-in person's own answer; on a calendar they manage for someone
    /// else, it is that person's.
    ///
    /// Google has no call for this alone: an answer is a change to the
    /// event's guest list, and a change replaces the whole list. So the event
    /// is read, the answer is written into the entry Google marks as the
    /// calendar's own, and the list goes back with everyone else exactly as
    /// they were.
    pub async fn respond(&self, calendar_id: &str, event_id: &str, answer: Respond) -> Result<Event> {
        if !ANSWERS.contains(&answer.response_status.as_str()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "responseStatus must be accepted, declined, tentative or needsAction",
            ));
        }
        let path = self.one(calendar_id, event_id)?;
        let mut current = self.0.get(path.clone(), &json!({})).await?;
        if current["id"].as_str().is_none_or(str::is_empty) {
            return Err(self.no_id());
        }
        if current["attendeesOmitted"] == true {
            return Err(self.0.error(
                ErrorKind::Decode,
                "google left guests out of the event it returned, so an answer would remove them",
            ));
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
        // With the version that was read, Google refuses the change if the
        // event has changed since: a guest added in between is not dropped.
        let Some(etag) = current["etag"].as_str().filter(|etag| !etag.is_empty()) else {
            return Err(self.0.error(
                ErrorKind::Decode,
                "google returned the event without its version, so an answer could overwrite a change",
            ));
        };
        let request = RawRequest::new("PATCH", path).with_header("If-Match", etag);
        let arguments = json!({ "sendUpdates": answer.send_updates });
        let body = json!({ "attendees": attendees });
        self.event(self.0.send(request, &arguments, Some(body)).await?)
    }

    /// Deletes an event.
    pub async fn delete(&self, calendar_id: &str, event_id: &str, options: DeleteEvent) -> Result<()> {
        let path = self.one(calendar_id, event_id)?;
        let request = RawRequest::new("DELETE", path);
        self.0
            .send(request, &Value::Object(set(&options)), None)
            .await
            .map(drop)
    }

    fn events(&self, calendar_id: &str) -> Result<String> {
        let calendar = self.0.segment("a calendar id", calendar_id)?;
        Ok(format!("calendar/v3/calendars/{calendar}/events"))
    }

    fn one(&self, calendar_id: &str, event_id: &str) -> Result<String> {
        let event = self.0.segment("an event id", event_id)?;
        Ok(format!("{}/{event}", self.events(calendar_id)?))
    }

    /// Separates what Google takes in the query from what it takes in the body.
    ///
    /// `sendUpdates` is a query parameter. `createMeetLink` is ours: it
    /// becomes a request to create a conference, which Google ignores unless
    /// `conferenceDataVersion` says the caller understands conferences.
    fn split(&self, mut fields: Map<String, Value>) -> Result<(Value, Map<String, Value>)> {
        let mut arguments = Map::new();
        if let Some(send_updates) = fields.remove("sendUpdates") {
            arguments.insert("sendUpdates".into(), send_updates);
        }
        if fields.remove("createMeetLink") == Some(Value::Bool(true)) {
            let create =
                json!({ "requestId": self.request_id()?, "conferenceSolutionKey": { "type": "hangoutsMeet" } });
            fields.insert("conferenceData".into(), json!({ "createRequest": create }));
            arguments.insert("conferenceDataVersion".into(), json!(1));
        }
        Ok((Value::Object(arguments), fields))
    }

    /// An id Google has not seen. It ignores a request to create a conference
    /// whose id repeats the one before.
    fn request_id(&self) -> Result<String> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).map_err(|e| {
            self.0
                .error(ErrorKind::Unexpected, format!("the system random source failed: {e}"))
        })?;
        Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
    }

    /// A start or an end is a time or, for an all-day event, a date.
    fn time(&self, name: &str, time: &EventTime) -> Result<()> {
        let blank = |field: &Option<String>| field.as_deref().is_some_and(|text| text.trim().is_empty());
        if blank(&time.date) || blank(&time.date_time) || time.date.is_some() == time.date_time.is_some() {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                format!("{name} needs a dateTime, or a date for an all-day event, and not both"),
            ));
        }
        Ok(())
    }

    fn event(&self, body: Value) -> Result<Event> {
        let event: Event = self.0.decode(body, "an event")?;
        if event.id.is_empty() {
            return Err(self.no_id());
        }
        Ok(event)
    }

    fn page(&self, body: Value) -> Result<Page<Event>> {
        let page: Page<Event> = self.0.page(body, "calendar#events", "events")?;
        if page.items.iter().any(|event| event.id.is_empty()) {
            return Err(self.no_id());
        }
        Ok(page)
    }

    fn no_id(&self) -> socketkit_core::Error {
        self.0
            .error(ErrorKind::Decode, "google answered with an event that has no id")
    }
}
