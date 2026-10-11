//! When calendars are busy.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::calendar_time::instant;
use super::{Api, set};
use crate::models::{FreeBusy, FreeBusyQuery};

/// When calendars are busy.
#[derive(Debug, Clone, Copy)]
pub struct CalendarFreebusy<'a>(pub(crate) Api<'a>);

impl CalendarFreebusy<'_> {
    /// Reads when each of `calendars` is busy inside `window`. Changes nothing.
    ///
    /// A calendar is named by its id, or `primary`. One that Google could
    /// not answer for comes back with `errors` set and no busy periods,
    /// which is not the same as being free.
    ///
    /// Google takes this only as a POST, so the transport does not repeat it
    /// after a server error.
    pub async fn query(&self, calendars: &[String], window: FreeBusyQuery) -> Result<FreeBusy> {
        if calendars.is_empty() || calendars.iter().any(|id| id.trim().is_empty()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`calendars` needs at least one calendar id, and none of them blank",
            ));
        }
        instant(&self.0, "timeMin", &window.time_min)?;
        instant(&self.0, "timeMax", &window.time_max)?;
        let items: Vec<Value> = calendars.iter().map(|id| json!({ "id": id.trim() })).collect();
        let mut body = set(&window);
        body.insert("items".into(), Value::Array(items));
        let answer = self
            .0
            .send(RawRequest::post("calendar/v3/freeBusy", Value::Object(body)))
            .await?;
        self.0.kind(&answer, "calendar#freeBusy", "the calendars' busy times")?;
        // Asked about at least one calendar, an answer that names none says
        // nothing, and must not be read as "free all day".
        if answer["calendars"].as_object().is_none_or(serde_json::Map::is_empty) {
            return Err(self
                .0
                .error(ErrorKind::Decode, "google answered without the calendars' busy times"));
        }
        self.0.decode(answer, "busy times")
    }
}
