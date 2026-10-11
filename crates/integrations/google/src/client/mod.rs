//! Google's APIs as typed methods, one file per group of methods.
//!
//! Identifiers (a message id, a calendar id, a file id) are plain arguments.
//! Content and optional filters are structs from [`crate::models`]. Reads are
//! sent as GET, so the transport may repeat them after a server error, except
//! the few Google offers only as POST. Everything else goes out with the verb
//! Google dictates. A POST or a PATCH is never repeated unless Google refused
//! it outright; a PUT or a DELETE still is, after a server error, until the
//! transport stops repeating anything but a read.
//!
//! This file holds only what every group shares: the access to the APIs, and
//! the re-exports. Each group's methods are in the file named after it.

// ── gmail: modules ──

// ── calendar: modules ──

// ── meet: modules ──

// ── drive: modules ──

// ── docs and sheets: modules ──

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};
use url::Url;

// ── gmail: groups ──

// ── calendar: groups ──

// ── meet: groups ──

// ── drive: groups ──

// ── docs and sheets: groups ──

use crate::models::Paging;

/// The host Calendar and Drive are served from. It is the provider's API
/// base, so their paths are written relative to it: `calendar/v3/…`.
const SHARED: &str = "www.googleapis.com";

/// The hosts of the APIs that have one of their own. See [`Api::on`].
pub(super) const GMAIL: &str = "gmail.googleapis.com";
pub(super) const MEET: &str = "meet.googleapis.com";
pub(super) const DOCS: &str = "docs.googleapis.com";
pub(super) const SHEETS: &str = "sheets.googleapis.com";

/// One connection's access to Google's APIs.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// The address of `path` on `host`, one of the hosts above:
    /// `self.on(GMAIL, "gmail/v1/users/me/profile")`.
    ///
    /// When the definition has been pointed somewhere else, at a test server
    /// or a gateway, that address stands in for every one of Google's hosts
    /// and each path is kept, as it is for the API base itself.
    pub(super) fn on(&self, host: &str, path: &str) -> String {
        address(&self.connection.provider().api_base, host, path)
    }

    /// Sends `request` and returns what Google answered. A request that
    /// answers with no content, as a delete does, returns `null`.
    pub(super) async fn send(&self, request: RawRequest) -> Result<Value> {
        Ok(self.connection.request(request).await?.body)
    }

    /// Reads a response as `T`. `what` names it in the error: "an event".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// is the content of someone's mail or calendar. An error is logged and
    /// shown, so only the place of the value goes into it, and serde's error
    /// is not kept as its cause. The names in the place come from our own types.
    pub(super) fn decode<T: DeserializeOwned>(&self, body: Value, what: &str) -> Result<T> {
        serde_path_to_error::deserialize(body).map_err(|e| {
            let path = e.path().to_string();
            let place = if path == "." {
                String::new()
            } else {
                format!(", at `{path}`")
            };
            self.error(
                ErrorKind::Decode,
                format!("google sent {what} that could not be read{place}"),
            )
        })
    }

    /// Checks that an answer is what was asked for, where Google says what
    /// it is: `calendar#events`, `drive#fileList`. `what` names it in the
    /// error: "events".
    pub(super) fn kind(&self, body: &Value, kind: &str, what: &str) -> Result<()> {
        if body["kind"] == kind {
            return Ok(());
        }
        Err(self.error(ErrorKind::Decode, format!("google answered without {what}")))
    }

    /// `request` for one page of a list: the cursor as Google's `pageToken`,
    /// and the limit under the name this API gives the page size
    /// (`maxResults` or `pageSize`), which takes from 1 to `most`.
    pub(super) fn paged(&self, request: RawRequest, paging: &Paging, size: &str, most: u32) -> Result<RawRequest> {
        if paging.limit.is_some_and(|limit| !(1..=most).contains(&limit)) {
            return Err(self.error(ErrorKind::InvalidInput, format!("`limit` is from 1 to {most}")));
        }
        let cursor = paging.cursor.as_deref().map(str::trim).filter(|c| !c.is_empty());
        let request = match paging.limit {
            Some(limit) => request.with_query(size, limit.to_string()),
            None => request,
        };
        Ok(match cursor {
            Some(cursor) => request.with_query("pageToken", cursor),
            None => request,
        })
    }

    /// Reads one page of a list: the items under `field`, and the token of
    /// the page after it as the cursor. `what` names the items in an error:
    /// "messages".
    ///
    /// Google leaves a list out of its answer when the list is empty, so an
    /// answer without `field` is an empty page. An answer that is not an
    /// object is not a list at all. Where Google names what it sent, check
    /// that first with [`Api::kind`].
    pub(super) fn page<T: DeserializeOwned>(&self, mut body: Value, field: &str, what: &str) -> Result<Page<T>> {
        let Some(fields) = body.as_object_mut() else {
            return Err(self.error(ErrorKind::Decode, format!("google answered without {what}")));
        };
        let next_cursor = fields
            .get("nextPageToken")
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_owned);
        let items = match fields.remove(field) {
            None | Some(Value::Null) => Vec::new(),
            Some(items) => self.decode(items, what)?,
        };
        Ok(Page { items, next_cursor })
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }

    /// Writes an id as one segment of a path, whatever it contains.
    ///
    /// A calendar's id is an email address and may hold `#`; other ids are
    /// the caller's own text. Everything outside the characters a URL leaves
    /// alone is percent-encoded, so an id can never add a segment, a query or
    /// a fragment. A segment made only of dots would be resolved away and
    /// address something else, so it is refused.
    pub(super) fn segment(&self, what: &str, id: &str) -> Result<String> {
        self.required(what, id)?;
        let id = id.trim();
        if id == "." || id == ".." {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is not valid")));
        }
        Ok(encoded(id))
    }
}

/// See [`Api::on`].
fn address(base: &Url, host: &str, path: &str) -> String {
    if base.host_str() == Some(SHARED) {
        format!("https://{host}/{path}")
    } else {
        path.to_owned()
    }
}

/// `text` with everything percent-encoded but the characters a URL always
/// leaves alone: letters, digits and `-._~`.
pub(super) fn encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The fields of `options` that are set, as a JSON object. Unset fields are
/// left out at every depth, so Google applies its own defaults and a change
/// touches only what was named.
pub(super) fn set(options: &impl Serialize) -> Map<String, Value> {
    match without_nulls(serde_json::to_value(options).unwrap_or(Value::Null)) {
        Value::Object(fields) => fields,
        _ => Map::new(),
    }
}

fn without_nulls(value: Value) -> Value {
    match value {
        Value::Object(map) => Value::Object(
            map.into_iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(name, value)| (name, without_nulls(value)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.into_iter().map(without_nulls).collect()),
        other => other,
    }
}

/// `request` with the set fields of `options` as query parameters, under the
/// names they are written with in JSON. A list is written as Google reads
/// one: the parameter once for each item.
pub(super) fn with_query(mut request: RawRequest, options: &impl Serialize) -> RawRequest {
    let text = |value: &Value| match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    for (name, value) in set(options) {
        match value {
            Value::Array(items) => {
                for item in &items {
                    request = request.with_query(name.as_str(), text(item));
                }
            }
            value => request = request.with_query(name.as_str(), text(&value)),
        }
    }
    request
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn each_api_is_addressed_on_its_own_host_unless_the_definition_points_elsewhere() {
        let google = crate::provider().api_base;
        assert_eq!(
            address(&google, GMAIL, "gmail/v1/users/me/profile"),
            "https://gmail.googleapis.com/gmail/v1/users/me/profile"
        );
        assert_eq!(
            address(&google, MEET, "v2/conferenceRecords"),
            "https://meet.googleapis.com/v2/conferenceRecords"
        );
        for host in [GMAIL, MEET, DOCS, SHEETS] {
            let url = Url::parse(&address(&google, host, "v1/x")).unwrap();
            assert!(crate::provider().allows_host(&url), "{host} may receive the token");
        }
        let elsewhere = Url::parse("http://127.0.0.1:4010/").unwrap();
        assert_eq!(address(&elsewhere, SHEETS, "v4/spreadsheets/s1"), "v4/spreadsheets/s1");
    }

    #[test]
    fn an_id_is_written_so_that_it_stays_one_segment() {
        assert_eq!(encoded("ada@example.test"), "ada%40example.test");
        assert_eq!(
            encoded("en.usa#holiday@group.v.calendar.google.com"),
            "en.usa%23holiday%40group.v.calendar.google.com"
        );
        assert_eq!(encoded("a/b?c=d"), "a%2Fb%3Fc%3Dd");
        assert_eq!(encoded("AbC-12_3.~"), "AbC-12_3.~");
    }

    #[test]
    fn unset_options_are_left_out_at_every_depth_and_a_list_repeats_its_parameter() {
        let options = json!({
            "q": "from:ada", "includeSpamTrash": false, "maxResults": null,
            "labelIds": ["INBOX", "UNREAD"], "start": { "dateTime": "2026-10-12T09:00:00Z", "timeZone": null }
        });
        assert_eq!(
            Value::Object(set(&options)),
            json!({ "q": "from:ada", "includeSpamTrash": false, "labelIds": ["INBOX", "UNREAD"], "start": { "dateTime": "2026-10-12T09:00:00Z" } }),
            "false is a value; only what is unset is left out"
        );
        let request = with_query(
            RawRequest::get("x"),
            &json!({ "q": "from:ada", "includeSpamTrash": false, "labelIds": ["INBOX", "UNREAD"], "pageToken": null }),
        );
        let mut query = request.query;
        query.sort();
        assert_eq!(
            query,
            [
                ("includeSpamTrash", "false"),
                ("labelIds", "INBOX"),
                ("labelIds", "UNREAD"),
                ("q", "from:ada")
            ]
            .map(|(name, value)| (name.to_owned(), value.to_owned()))
        );
    }
}
