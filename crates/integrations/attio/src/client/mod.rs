//! Attio's REST API as typed methods, one file per area of the API.
//!
//! Identifiers (an object's slug, a record id) are plain arguments. Content
//! and optional filters are structs from [`crate::models`]. Reads are sent as
//! GET, so the transport may repeat them after a server error, except the
//! two queries Attio takes only as POST. Everything else goes out with the
//! verb Attio dictates: POST to create, PATCH to change, PUT to create or
//! change by a matching attribute, DELETE to delete. A POST or a PATCH is
//! never repeated unless Attio refused it outright; a PUT and a DELETE still
//! are, after a server error. Both leave the same state when repeated, but a
//! delete that had gone through before the error is then answered "not found".
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it.

mod attributes;
mod call_recordings;
mod entries;
mod lists;
mod meetings;
mod notes;
mod objects;
mod records;
mod tasks;
mod threads;
mod workspace_members;

use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

use crate::models::Query;

pub use attributes::Attributes;
pub use call_recordings::CallRecordings;
pub use entries::Entries;
pub use lists::Lists;
pub use meetings::Meetings;
pub use notes::Notes;
pub use objects::Objects;
pub use records::Records;
pub use tasks::Tasks;
pub use threads::Threads;
pub use workspace_members::WorkspaceMembers;

/// Which part of a list that Attio pages by position was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Window {
    pub(super) limit: u32,
    pub(super) offset: u64,
}

impl Window {
    /// `items` as a page. Attio does not say whether more follow; its rule
    /// is that a page shorter than the limit is the last one, so a full page
    /// is given a cursor, and the page after the last full one is empty.
    pub(super) fn page<T>(self, items: Vec<T>) -> Page<T> {
        let full = u64::try_from(items.len()).is_ok_and(|count| count >= u64::from(self.limit));
        // The offset is the caller's cursor, so it may be any number at all.
        let next = self.offset.checked_add(u64::from(self.limit)).filter(|_| full);
        Page {
            items,
            next_cursor: next.map(|offset| offset.to_string()),
        }
    }
}

/// One connection's access to Attio.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Sends `request` and returns the whole of what Attio answered.
    pub(super) async fn send(&self, request: RawRequest) -> Result<Value> {
        Ok(self.connection.request(request).await?.body)
    }

    /// Reads a value as `T`. `what` names it in the error: "a record".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// is a customer's details. An error is logged and shown, so only the
    /// place of the value goes into it, and serde's error is not kept as its
    /// cause. The names in the place come from our own types and from the
    /// workspace's attribute slugs.
    pub(super) fn decode<T: DeserializeOwned>(&self, value: Value, what: &str) -> Result<T> {
        serde_path_to_error::deserialize(value).map_err(|e| {
            let path = e.path().to_string();
            let place = if path == "." {
                String::new()
            } else {
                format!(", at `{path}`")
            };
            self.error(
                ErrorKind::Decode,
                format!("attio sent {what} that could not be read{place}"),
            )
        })
    }

    /// What Attio put under `data`, which is where every answer carries its content.
    fn data(&self, mut body: Value, what: &str) -> Result<Value> {
        match body.get_mut("data").map(Value::take) {
            Some(data) if !data.is_null() => Ok(data),
            _ => Err(self.error(ErrorKind::Decode, format!("attio answered without {what}"))),
        }
    }

    /// Sends `request` and reads the one thing it answers with.
    ///
    /// Everything Attio returns carries an `id`. A success without one is
    /// not the thing asked for, and is reported as that and not as a thing
    /// with blank fields.
    pub(super) async fn one<T: DeserializeOwned>(&self, request: RawRequest, what: &str) -> Result<T> {
        let data = self.data(self.send(request).await?, what)?;
        self.identified(data, what)
    }

    fn identified<T: DeserializeOwned>(&self, data: Value, what: &str) -> Result<T> {
        if data["id"].as_object().is_none_or(serde_json::Map::is_empty) {
            return Err(self.error(ErrorKind::Decode, format!("attio answered without {what}")));
        }
        self.decode(data, what)
    }

    /// [`Api::one`] for a thing that comes a part at a time: the thing, and
    /// the cursor for the rest of it.
    pub(super) async fn one_of_many<T: DeserializeOwned>(
        &self,
        request: RawRequest,
        what: &str,
    ) -> Result<(T, Option<String>)> {
        let body = self.send(request).await?;
        let next_cursor = next_cursor(&body);
        Ok((self.identified(self.data(body, what)?, what)?, next_cursor))
    }

    /// Sends `request` and reads the list it answers with.
    pub(super) async fn all<T: DeserializeOwned>(&self, request: RawRequest, what: &str) -> Result<Vec<T>> {
        let data = self.data(self.send(request).await?, what)?;
        self.decode(data, what)
    }

    /// Sends `request` and reads one page of a list Attio pages by cursor.
    pub(super) async fn cursor_page<T: DeserializeOwned>(&self, request: RawRequest, what: &str) -> Result<Page<T>> {
        let body = self.send(request).await?;
        let next_cursor = next_cursor(&body);
        Ok(Page {
            items: self.decode(self.data(body, what)?, what)?,
            next_cursor,
        })
    }

    /// Sends a request that answers with nothing worth keeping, as a delete does.
    pub(super) async fn done(&self, request: RawRequest) -> Result<()> {
        self.send(request).await.map(drop)
    }

    /// The part of a list paged by position that `cursor` and `limit` ask
    /// for. `usual` is the page size when none is asked for, and `most` the
    /// largest that may be.
    ///
    /// The cursor is the position of the next item, which is what Attio
    /// calls `offset`. It comes back from the caller, so it is read as a
    /// number and nothing else.
    pub(super) fn window(&self, cursor: Option<&str>, limit: Option<u32>, usual: u32, most: u32) -> Result<Window> {
        self.limit(limit, most)?;
        let offset = match cursor.map(str::trim).filter(|cursor| !cursor.is_empty()) {
            None => 0,
            Some(cursor) => cursor.parse().map_err(|_| {
                self.error(
                    ErrorKind::InvalidInput,
                    "`cursor` is not a place in this list; pass back `next_cursor` unchanged",
                )
            })?,
        };
        Ok(Window {
            limit: limit.unwrap_or(usual),
            offset,
        })
    }

    /// The cursor and the page size for a list Attio pages by cursor, as query parameters.
    pub(super) fn after(
        &self,
        request: RawRequest,
        cursor: Option<&str>,
        limit: Option<u32>,
        most: u32,
    ) -> Result<RawRequest> {
        self.limit(limit, most)?;
        let request = match limit {
            Some(limit) => request.with_query("limit", limit.to_string()),
            None => request,
        };
        Ok(match cursor.map(str::trim).filter(|cursor| !cursor.is_empty()) {
            Some(cursor) => request.with_query("cursor", cursor),
            None => request,
        })
    }

    fn limit(&self, limit: Option<u32>, most: u32) -> Result<()> {
        if limit.is_some_and(|limit| !(1..=most).contains(&limit)) {
            let range = if most == u32::MAX {
                "`limit` is at least 1".to_owned()
            } else {
                format!("`limit` is from 1 to {most}")
            };
            return Err(self.error(ErrorKind::InvalidInput, range));
        }
        Ok(())
    }

    /// The body of a query for records or entries, and the part of the list it asks for.
    ///
    /// Attio returns 500 at a time unless told otherwise, each with every
    /// value it holds. 50 is asked for when the caller names no limit, so
    /// that a page stays a size that can be read.
    pub(super) fn query(&self, query: &Query) -> Result<(Value, Window)> {
        let invalid = |message: &str| self.error(ErrorKind::InvalidInput, message);
        if query.filter.is_some() && query.filter_view_id.is_some() {
            return Err(invalid("`filter` and `filter_view_id` cannot be used together"));
        }
        let sorts = query.sorts.as_deref().unwrap_or_default();
        if sorts.iter().any(|sort| sort.attribute.is_some() == sort.path.is_some()) {
            return Err(invalid("every sort needs `attribute` or `path`, and not both"));
        }
        let window = self.window(query.cursor.as_deref(), query.limit, 50, 500)?;
        let mut body = Map::new();
        set(&mut body, "filter", query.filter.clone());
        set(&mut body, "filter_view_id", query.filter_view_id.clone());
        if let Some(sorts) = &query.sorts {
            let sorts = serde_json::to_value(sorts)
                .map_err(|_| self.error(ErrorKind::Unexpected, "could not encode the sorts"))?;
            body.insert("sorts".to_owned(), sorts);
        }
        body.insert("limit".to_owned(), window.limit.into());
        body.insert("offset".to_owned(), window.offset.into());
        Ok((Value::Object(body), window))
    }

    /// Two filters that only mean something together are given together or not at all.
    pub(super) fn together(&self, first: (&str, Option<&str>), second: (&str, Option<&str>)) -> Result<()> {
        if filled(first.1).is_some() == filled(second.1).is_some() {
            return Ok(());
        }
        Err(self.error(
            ErrorKind::InvalidInput,
            format!("`{}` and `{}` are given together", first.0, second.0),
        ))
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }

    /// Writes a slug or an id as one segment of a path, whatever it contains.
    ///
    /// Everything outside the characters a URL leaves alone is
    /// percent-encoded, so a value can never add a segment, a query or a
    /// fragment. A segment made only of dots would be resolved away and
    /// address something else, so it is refused.
    pub(super) fn segment(&self, what: &str, id: &str) -> Result<String> {
        self.required(what, id)?;
        let id = id.trim();
        if id == "." || id == ".." {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is not valid")));
        }
        Ok(encoded(id))
    }

    /// The values to write, which have to name at least one attribute.
    pub(super) fn values(&self, values: &Map<String, Value>) -> Result<()> {
        if values.is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, "`values` needs at least one attribute"));
        }
        Ok(())
    }
}

/// The cursor of the page after this one, where Attio pages by cursor.
fn next_cursor(body: &Value) -> Option<String> {
    body["pagination"]["next_cursor"]
        .as_str()
        .filter(|cursor| !cursor.is_empty())
        .map(str::to_owned)
}

/// `text` with everything percent-encoded but the characters a URL always
/// leaves alone: letters, digits and `-._~`.
fn encoded(text: &str) -> String {
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

/// Adds `value` to `body` under `name` when it is set, so that what was not
/// said is not sent and Attio applies its own default.
pub(super) fn set(body: &mut Map<String, Value>, name: &str, value: Option<impl Into<Value>>) {
    if let Some(value) = value {
        body.insert(name.to_owned(), value.into());
    }
}

/// `request` with the query parameter `name` when `value` is set.
pub(super) fn asking(request: RawRequest, name: &str, value: Option<impl ToString>) -> RawRequest {
    match value {
        Some(value) => request.with_query(name, value.to_string()),
        None => request,
    }
}

/// A filter that says something, without the space around it.
pub(super) fn filled(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}
