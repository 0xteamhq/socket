//! Attio as typed methods, one file per area of the API.
//!
//! Identifiers (an object, a list, a record id) are plain arguments. Content
//! and optional filters are structs from [`crate::models`]. Reads are sent
//! as GET, so the transport may repeat them after a server error; the two
//! queries are the exception, because Attio takes them as POST. Everything
//! else goes out with the verb Attio dictates.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Paging, which every list shares too, is in `paging.rs`.
//! Each area's methods are in the file named after it.

mod attributes;
mod call_recordings;
mod entries;
mod lists;
mod meetings;
mod meta;
mod notes;
mod objects;
mod paging;
mod records;
mod tasks;
mod threads;
mod workspace_members;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Error, ErrorKind, RawRequest, Result};

pub use attributes::Attributes;
pub use call_recordings::CallRecordings;
pub use entries::Entries;
pub use lists::Lists;
pub use meetings::Meetings;
pub use meta::Meta;
pub use notes::Notes;
pub use objects::Objects;
pub use records::Records;
pub use tasks::Tasks;
pub use threads::Threads;
pub use workspace_members::WorkspaceMembers;

use paging::{A_PAGE, MOST, next_cursor};

use crate::models::Sort;

/// One connection's access to Attio.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Sends `request` and returns what Attio answered.
    pub(super) async fn send(&self, request: RawRequest) -> Result<Value> {
        Ok(self.connection.request(request).await?.body)
    }

    /// Reads a response as `T`. `what` names it in the error: "a record".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// is what a workspace knows about a customer. An error is logged and
    /// shown, so only the place of the value goes into it, and serde's error
    /// is not kept as its cause. The names in the place come from our own
    /// types and from the workspace's attribute slugs, never from a value.
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
                format!("attio sent {what} that could not be read{place}"),
            )
        })
    }

    /// Reads what Attio wraps every answer in, `data`, as `T`. An answer
    /// without it is not the thing that was asked for.
    pub(super) fn data<T: DeserializeOwned>(&self, mut body: Value, what: &str) -> Result<T> {
        match body.get_mut("data").map(Value::take) {
            Some(data) if !data.is_null() => self.decode(data, what),
            _ => Err(self.missing(what)),
        }
    }

    /// Sends `request` and reads the `data` of the answer as `T`.
    pub(super) async fn one<T: DeserializeOwned>(&self, request: RawRequest, what: &str) -> Result<T> {
        self.data(self.send(request).await?, what)
    }

    /// The error for an answer that was read but does not carry the thing
    /// itself: a record without an id.
    pub(super) fn missing(&self, what: &str) -> Error {
        self.error(ErrorKind::Decode, format!("attio answered without {what}"))
    }

    /// The filter and the sorts of a query, as its body.
    pub(super) fn search(
        &self,
        filter: Option<Map<String, Value>>,
        sorts: Option<&[Sort]>,
    ) -> Result<Map<String, Value>> {
        let named = |name: &Option<String>| name.as_deref().is_some_and(|name| !name.trim().is_empty());
        let mut search = Map::new();
        if let Some(filter) = filter {
            search.insert("filter".to_owned(), Value::Object(filter));
        }
        if let Some(sorts) = sorts {
            for sort in sorts {
                let through = sort.path.as_deref().unwrap_or_default();
                let blank = through.iter().flatten().any(|part| part.trim().is_empty());
                match (named(&sort.attribute), sort.path.is_some()) {
                    (true, false) => {}
                    (false, true) if !through.is_empty() && !blank => {}
                    (false, true) => {
                        return Err(self.error(
                            ErrorKind::InvalidInput,
                            "the `path` of a sort is pairs of an object or list and one of its attributes",
                        ));
                    }
                    _ => {
                        return Err(self.error(
                            ErrorKind::InvalidInput,
                            "a sort needs `attribute` or `path`, and not both",
                        ));
                    }
                }
            }
            search.insert("sorts".to_owned(), written(&sorts));
        }
        Ok(search)
    }

    /// Checks an id or a slug before it is written into a path.
    ///
    /// Attio writes a slug in snake case and an id as a UUID, so letters,
    /// digits, `_` and `-` cover both. Nothing else is let through, so what
    /// a caller passes as an object, a list or a record can only ever be one
    /// segment of the path: it cannot add a segment, a query or a fragment,
    /// or step out of the one it is in. What was passed is not repeated in
    /// the error.
    pub(super) fn id<'v>(&self, what: &str, value: &'v str) -> Result<&'v str> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        let plain = |byte: u8| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-');
        if value.len() > 128 || !value.bytes().all(plain) {
            return Err(self.error(
                ErrorKind::InvalidInput,
                format!("{what} is not valid; it is a slug or an id, made of letters, digits, `_` and `-`"),
            ));
        }
        Ok(value)
    }

    /// [`Api::id`] for an id that is optional.
    pub(super) fn optional_id<'v>(&self, what: &str, value: Option<&'v str>) -> Result<Option<&'v str>> {
        value.map(|value| self.id(what, value)).transpose()
    }
}

/// `base` with the set fields of `options` added. Unset fields are left out
/// at every depth, so Attio applies its own defaults. Not for the values of
/// a record: there a `null` is something to write.
pub(super) fn with(base: Value, options: &impl Serialize) -> Value {
    let mut merged = match base {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Value::Object(extra) = written(options) {
        merged.extend(extra);
    }
    without_nulls(Value::Object(merged))
}

/// `content` as JSON, without the fields that are not set.
pub(super) fn written(content: &impl Serialize) -> Value {
    without_nulls(serde_json::to_value(content).unwrap_or(Value::Null))
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn unset_fields_are_left_out_at_every_depth_and_a_list_stays_a_list() {
        let merged = with(
            json!({ "format": "plaintext" }),
            &json!({ "title": "Call", "meeting_id": null, "author": { "id": "m-1", "note": null } }),
        );
        assert_eq!(
            merged,
            json!({ "format": "plaintext", "title": "Call", "author": { "id": "m-1" } })
        );
        let sorts = written(&json!([{ "direction": "asc", "attribute": "name", "path": null }]));
        assert_eq!(sorts, json!([{ "direction": "asc", "attribute": "name" }]));
    }
}
