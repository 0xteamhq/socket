//! Google's APIs as typed methods, one file per group of methods.
//!
//! What identifies the thing acted on is a plain argument: a calendar id, an
//! event id. Content and optional filters are structs from [`crate::models`].
//! A read is sent as `GET`, except the free/busy query, which Google only
//! takes as a `POST`. A write uses the verb Google requires and is never
//! repeated after a failure that may have been processed.
//!
//! This file holds only what every group shares: the access to the API, and
//! the re-exports. Each group's methods are in the file named after it.

mod calendar_events;
mod calendar_freebusy;
mod calendar_list;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

pub use calendar_events::CalendarEvents;
pub use calendar_freebusy::CalendarFreebusy;
pub use calendar_list::CalendarList;

/// Everything outside letters, digits and `-._~` is encoded in a path segment.
const SEGMENT: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// One connection's access to Google's APIs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Calls an endpoint that reads, with `arguments` as query parameters.
    pub(super) async fn get(&self, path: String, arguments: &Value) -> Result<Value> {
        let request = with_query(RawRequest::get(path), arguments);
        Ok(self.connection.request(request).await?.body)
    }

    /// Calls an endpoint with the verb Google gives it, `arguments` as query
    /// parameters and `body` as JSON when there is one.
    pub(super) async fn send(&self, request: RawRequest, arguments: &Value, body: Option<Value>) -> Result<Value> {
        let mut request = with_query(request, arguments);
        if let Some(body) = body {
            request = request.with_body(body);
        }
        Ok(self.connection.request(request).await?.body)
    }

    /// Reads a whole response body as `T`.
    pub(super) fn decode<T: DeserializeOwned>(&self, body: Value, what: &str) -> Result<T> {
        if body.is_null() {
            return Err(self.error(ErrorKind::Decode, format!("google answered without {what}")));
        }
        serde_json::from_value(body).map_err(|e| {
            self.error(ErrorKind::Decode, format!("google sent {what} that could not be read"))
                .with_source(e)
        })
    }

    /// Reads a list response and the token for the page after it.
    ///
    /// `kind` is what Google calls the collection, such as `calendar#events`.
    /// It is what tells a list with nothing in it, which may arrive without
    /// `items`, from a response that is not the list at all.
    pub(super) fn page<T: DeserializeOwned>(&self, mut body: Value, kind: &str, what: &str) -> Result<Page<T>> {
        if body["kind"] != kind {
            return Err(self.error(ErrorKind::Decode, format!("google answered without a list of {what}")));
        }
        let next_cursor = body["nextPageToken"]
            .as_str()
            .filter(|token| !token.is_empty())
            .map(str::to_owned);
        let items = match body["items"].take() {
            Value::Null => Vec::new(),
            items => self.decode(items, what)?,
        };
        Ok(Page { items, next_cursor })
    }

    /// An id as one path segment. Calendar ids are email addresses and may
    /// hold `#`, so everything that could end or extend the path is encoded.
    /// `.` and `..` are refused, since as segments they would reach a
    /// different endpoint.
    pub(super) fn segment(&self, what: &str, value: &str) -> Result<String> {
        let value = value.trim();
        self.required(what, value)?;
        if value == "." || value == ".." {
            return Err(self.error(ErrorKind::InvalidInput, format!("{value:?} is not {what}")));
        }
        Ok(utf8_percent_encode(value, SEGMENT).to_string())
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }
}

fn with_query(mut request: RawRequest, arguments: &Value) -> RawRequest {
    for (name, value) in arguments.as_object().into_iter().flatten() {
        let text = match value {
            Value::Null => continue,
            Value::String(text) => text.clone(),
            other => other.to_string(),
        };
        request = request.with_query(name.as_str(), text);
    }
    request
}

/// The fields of `options` that are set, as a JSON object. Unset fields are
/// left out at every depth, so Google applies its own defaults and a change
/// touches nothing that was not named.
pub(super) fn set(options: &impl Serialize) -> Map<String, Value> {
    fn prune(value: &mut Value) {
        match value {
            Value::Object(map) => {
                map.retain(|_, field| !field.is_null());
                map.values_mut().for_each(prune);
            }
            Value::Array(items) => items.iter_mut().for_each(prune),
            _ => {}
        }
    }
    let mut value = serde_json::to_value(options).unwrap_or(Value::Null);
    prune(&mut value);
    match value {
        Value::Object(map) => map,
        _ => Map::new(),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::models::{EventTime, InsertEvent, ListEvents};

    #[test]
    fn unset_options_are_left_out_at_every_depth() {
        let listing = ListEvents {
            q: Some("design".into()),
            single_events: Some(false),
            ..ListEvents::default()
        };
        assert_eq!(
            Value::Object(set(&listing)),
            json!({ "q": "design", "singleEvents": false }),
            "false is a value; only what is unset is left out"
        );

        let event = InsertEvent::new(EventTime::at("2026-10-12T09:00:00Z"), EventTime::all_day("2026-10-13"))
            .invite("grace@example.test");
        assert_eq!(
            Value::Object(set(&event)),
            json!({
                "start": { "dateTime": "2026-10-12T09:00:00Z" },
                "end": { "date": "2026-10-13" },
                "attendees": [{ "email": "grace@example.test" }]
            })
        );
    }
}
