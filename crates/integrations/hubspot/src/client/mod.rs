//! The HubSpot CRM as typed methods, one file per area of the API.
//!
//! HubSpot's CRM is one API over many object types, so the object type is a
//! plain argument beside the ids: `contacts`, `deals`, a custom object's type
//! id such as `2-12345`. Content and optional filters are structs from
//! [`crate::models`]. Reads are sent as GET, so the transport may repeat them
//! after a server error. Everything else goes out with the verb HubSpot
//! dictates. A POST or a PATCH is never repeated unless HubSpot refused it
//! outright; a PUT and a DELETE still are, after a server error, until the
//! transport stops repeating anything but a read.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it.

mod associations;
mod objects;
mod objects_batch;
mod objects_search;
mod owners;
mod pipelines;
mod properties;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

pub use associations::Associations;
pub use objects::Objects;
pub use owners::Owners;
pub use pipelines::Pipelines;
pub use properties::Properties;

use crate::API_VERSION;
use crate::models::Paging;

/// One connection's access to the HubSpot API.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Sends `request` and returns what HubSpot answered. A request that
    /// answers with no content, as archiving does, returns `null`.
    pub(super) async fn send(&self, request: RawRequest) -> Result<Value> {
        Ok(self.connection.request(request).await?.body)
    }

    /// Reads a response as `T`. `what` names it in the error: "a record".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// is the content of someone's CRM. An error is logged and shown, so only
    /// the place of the value goes into it, and serde's error is not kept as
    /// its cause. The names in the place come from our own types.
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
                format!("hubspot sent {what} that could not be read{place}"),
            )
        })
    }

    /// Reads the list every HubSpot collection carries under `results`.
    pub(super) fn results<T: DeserializeOwned>(&self, body: &Value, what: &str) -> Result<Vec<T>> {
        let items = body
            .get("results")
            .ok_or_else(|| self.error(ErrorKind::Decode, format!("hubspot answered without {what}")))?;
        self.decode(items.clone(), what)
    }

    /// Reads one page of a list that HubSpot keeps to `most` items a page.
    /// `first` is the request for the first page.
    ///
    /// The cursor is HubSpot's `after` value. It goes back as a query
    /// parameter and nowhere else, so whatever a caller puts in it cannot
    /// change the address that is read.
    pub(super) async fn page<T: DeserializeOwned>(
        &self,
        most: u32,
        first: RawRequest,
        paging: &Paging,
        what: &str,
    ) -> Result<Page<T>> {
        self.limit(most, paging.limit)?;
        let mut request = first;
        if let Some(limit) = paging.limit {
            request = request.with_query("limit", limit.to_string());
        }
        if let Some(cursor) = place(paging.cursor.as_deref()) {
            request = request.with_query("after", cursor);
        }
        let body = self.send(request).await?;
        self.paged(&body, what)
    }

    /// The items of a page HubSpot sent, and where the next one begins.
    pub(super) fn paged<T: DeserializeOwned>(&self, body: &Value, what: &str) -> Result<Page<T>> {
        // HubSpot writes the place as a string. A number is read as well, as
        // its ids are: taken for no cursor, it would end the list early.
        let next_cursor = match &body["paging"]["next"]["after"] {
            Value::String(after) if !after.is_empty() => Some(after.clone()),
            Value::Number(after) => Some(after.to_string()),
            _ => None,
        };
        Ok(Page {
            items: self.results(body, what)?,
            next_cursor,
        })
    }

    pub(super) fn limit(&self, most: u32, limit: Option<u32>) -> Result<()> {
        if limit.is_some_and(|limit| !(1..=most).contains(&limit)) {
            return Err(self.error(ErrorKind::InvalidInput, format!("`limit` is from 1 to {most}")));
        }
        Ok(())
    }

    /// A list of names as HubSpot takes one in a query: joined with commas.
    /// An empty list is no list. A name with a comma in it would be read as two.
    pub(super) fn names(&self, what: &str, names: Option<&[String]>) -> Result<Option<String>> {
        let Some(names) = names.filter(|names| !names.is_empty()) else {
            return Ok(None);
        };
        if names.iter().any(|name| name.trim().is_empty() || name.contains(',')) {
            return Err(self.error(
                ErrorKind::InvalidInput,
                format!("`{what}` holds a name that is blank or has a comma in it"),
            ));
        }
        Ok(Some(names.join(",")))
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }

    /// Writes an id or an object type as one segment of a path, whatever it
    /// contains.
    ///
    /// A record may be named by a property's value, such as an email address,
    /// and a caller's text may hold anything. Everything outside the
    /// characters a URL leaves alone is percent-encoded, so it can never add
    /// a segment, a query or a fragment. A segment made only of dots would be
    /// resolved away and address something else, so it is refused.
    pub(super) fn segment(&self, what: &str, id: &str) -> Result<String> {
        self.required(what, id)?;
        if id == "." || id == ".." {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is not valid")));
        }
        Ok(encoded(id))
    }

    /// The address of the records of one object type.
    pub(super) fn records(&self, object_type: &str) -> Result<String> {
        Ok(format!(
            "{}/{}",
            area("objects"),
            self.segment("an object type", object_type)?
        ))
    }
}

/// The address of one area of the CRM, in the version of the API this crate
/// is written against: `crm/objects/2026-09`.
pub(super) fn area(name: &str) -> String {
    format!("crm/{name}/{API_VERSION}")
}

/// A cursor that names a place in a list. A blank one is the first page.
pub(super) fn place(cursor: Option<&str>) -> Option<&str> {
    cursor.map(str::trim).filter(|cursor| !cursor.is_empty())
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

/// `base` with the set fields of `options` added. Unset fields are left out
/// at every depth, so HubSpot applies its own defaults and a change touches
/// only what was named.
pub(super) fn with(base: Value, options: &impl Serialize) -> Value {
    let mut merged = match base {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Ok(Value::Object(extra)) = serde_json::to_value(options) {
        merged.extend(extra);
    }
    without_nulls(Value::Object(merged))
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
