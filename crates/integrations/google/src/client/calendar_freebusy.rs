//! When calendars are busy.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, set};
use crate::models::{FreeBusy, FreeBusyQuery};

/// When calendars are busy.
#[derive(Debug, Clone, Copy)]
pub struct CalendarFreebusy<'a>(pub(crate) Api<'a>);

impl CalendarFreebusy<'_> {
    /// When each of `calendar_ids` is busy inside `window`.
    ///
    /// A calendar Google could not answer for comes back with `errors` set
    /// and no busy periods, which is not the same as being free.
    ///
    /// This reads and changes nothing, but Google only takes it as a `POST`.
    /// The transport therefore does not repeat it after a server error.
    pub async fn query(&self, calendar_ids: &[String], window: FreeBusyQuery) -> Result<FreeBusy> {
        self.0.required("timeMin", &window.time_min)?;
        self.0.required("timeMax", &window.time_max)?;
        let items: Vec<Value> = calendar_ids
            .iter()
            .map(|id| id.trim())
            .filter(|id| !id.is_empty())
            .map(|id| json!({ "id": id }))
            .collect();
        if items.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "at least one calendar id is required"));
        }
        let mut body = set(&window);
        body.insert("items".into(), Value::Array(items));
        let request = RawRequest::new("POST", "calendar/v3/freeBusy");
        let answer = self.0.send(request, &json!({}), Some(Value::Object(body))).await?;
        // Asked about at least one calendar, an answer that names none says nothing.
        if answer["kind"] != "calendar#freeBusy"
            || answer["calendars"].as_object().is_none_or(serde_json::Map::is_empty)
        {
            return Err(self
                .0
                .error(ErrorKind::Decode, "google answered without the calendars' busy times"));
        }
        self.0.decode(answer, "busy times")
    }
}
