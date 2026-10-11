//! HubSpot's CRM as typed methods, one file per area of the API.
//!
//! Identifiers (an object type, a record id, a property's name) are plain
//! arguments. Content and optional filters are structs from
//! [`crate::models`]. Reads are sent as GET, so the transport may repeat
//! them after a server error. The two reads HubSpot takes only as POST, a
//! search and a batch read, are sent once, as every other POST and PATCH
//! is. A DELETE or a PUT still is repeated after a server error, until the
//! transport stops repeating anything but a read; archiving a record twice,
//! or associating two records twice, ends as doing it once would.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it. What
//! only the three files of `objects` need is in `objects.rs`, as methods of
//! that group.

mod associations;
mod objects;
mod objects_batch;
mod objects_search;
mod owners;
mod pipelines;
mod properties;

use serde::de::DeserializeOwned;
use serde_json::Value;
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

pub use associations::Associations;
pub use objects::Objects;
pub use owners::Owners;
pub use pipelines::Pipelines;
pub use properties::Properties;

use crate::models::Paging;

/// One connection's access to HubSpot's API.
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
    /// is what a company keeps about its customers. An error is logged and
    /// shown, so only the place of the value goes into it, and serde's error
    /// is not kept as its cause. Most names in the place come from our own
    /// types. The name of a property does not: it is the account's own word
    /// for a field, and comes from the answer. So a place is repeated only
    /// when it reads as one.
    pub(super) fn decode<T: DeserializeOwned>(&self, body: Value, what: &str) -> Result<T> {
        serde_path_to_error::deserialize(body).map_err(|e| {
            let path = e.path().to_string();
            let place = if is_place(&path) {
                format!(", at `{path}`")
            } else {
                String::new()
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
            .filter(|items| items.is_array())
            .ok_or_else(|| self.error(ErrorKind::Decode, format!("hubspot answered without {what}")))?;
        self.decode(items.clone(), what)
    }

    /// Refuses a page size HubSpot would. `most` is the largest it takes for
    /// this list, where its documentation says.
    pub(super) fn limit(&self, most: Option<u32>, limit: Option<u32>) -> Result<()> {
        match (limit, most) {
            (Some(limit), Some(most)) if !(1..=most).contains(&limit) => {
                Err(self.error(ErrorKind::InvalidInput, format!("`limit` is from 1 to {most}")))
            }
            (Some(0), None) => Err(self.error(ErrorKind::InvalidInput, "`limit` is at least 1")),
            _ => Ok(()),
        }
    }

    /// Reads one page of a list. `first` is the request for the first page.
    ///
    /// HubSpot pages with a token: each page names the next in
    /// `paging.next.after`, and that token is the cursor. Given back, it is
    /// sent as the `after` parameter of the same request, so a cursor can
    /// say where in the list to go on and nothing else.
    pub(super) async fn page<T: DeserializeOwned>(
        &self,
        most: Option<u32>,
        first: RawRequest,
        paging: &Paging,
        what: &str,
    ) -> Result<Page<T>> {
        self.limit(most, paging.limit)?;
        let mut request = first;
        if let Some(limit) = paging.limit {
            request = request.with_query("limit", limit.to_string());
        }
        if let Some(cursor) = cursor(paging.cursor.as_deref()) {
            request = request.with_query("after", cursor);
        }
        let body = self.send(request).await?;
        Ok(Page {
            items: self.results(&body, what)?,
            next_cursor: next_cursor(&body),
        })
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }

    /// Checks an object type and returns it as one segment of a path.
    ///
    /// An object type is a name (`contacts`, `line_items`), a type id
    /// (`0-1`, `2-3465404`) or a custom object's full name (`p12345_cars`).
    /// All of them are letters, digits, `_` and `-`, so anything else is
    /// refused and nothing is encoded: a type can never add a segment, a
    /// query or a fragment to the address. The value is not repeated in the
    /// error, since it is refused for what it holds.
    pub(super) fn object_type(&self, what: &str, object_type: &str) -> Result<String> {
        self.required(what, object_type)?;
        if !is_object_type(object_type) {
            return Err(self.error(
                ErrorKind::InvalidInput,
                format!("{what} is a name such as `contacts`, or a type id such as `2-3465404`"),
            ));
        }
        Ok(object_type.to_owned())
    }

    /// Writes an id as one segment of a path.
    ///
    /// A record id is a number, but with `idProperty` it is the value of a
    /// unique property, such as an email address. So everything outside the
    /// characters a URL leaves alone is percent-encoded, and an id cannot
    /// add a query or a fragment. One that holds a slash, or is made only of
    /// dots, is refused outright: a server may read an encoded slash as a
    /// slash, and dots are resolved away. A record whose unique value holds
    /// a slash is read with `objects.batch_read`, where the value travels in
    /// the body.
    pub(super) fn segment(&self, what: &str, id: &str) -> Result<String> {
        self.required(what, id)?;
        // Space around an id is never part of it.
        let id = id.trim();
        let unsafe_in_a_path = id == "." || id == ".." || id.chars().any(|c| matches!(c, '/' | '\\') || c.is_control());
        if unsafe_in_a_path {
            return Err(self.error(
                ErrorKind::InvalidInput,
                format!("{what} is not valid: it cannot hold a slash"),
            ));
        }
        Ok(encoded(id))
    }
}

/// True for what HubSpot accepts as an object type: a name, a type id, or a
/// custom object's full name.
pub(crate) fn is_object_type(object_type: &str) -> bool {
    (1..=100).contains(&object_type.len())
        && object_type
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
}

/// True when `path` reads as the place of a value: names, indexes and the
/// dots between them, and not too long. Anything else is some of what
/// HubSpot sent, and is not repeated.
fn is_place(path: &str) -> bool {
    path != "."
        && (1..=120).contains(&path.len())
        && path
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '[' | ']'))
}

/// The cursor a caller gave, unless it is blank: a blank one is the first page.
pub(super) fn cursor(cursor: Option<&str>) -> Option<&str> {
    cursor.map(str::trim).filter(|cursor| !cursor.is_empty())
}

/// The token of the next page, when HubSpot named one.
pub(super) fn next_cursor(body: &Value) -> Option<String> {
    match &body["paging"]["next"]["after"] {
        Value::String(after) if !after.is_empty() => Some(after.clone()),
        Value::Number(after) => Some(after.to_string()),
        _ => None,
    }
}

/// `request` with `name` set to `value`, when there is one.
pub(super) fn with_query(request: RawRequest, name: &str, value: Option<impl Into<String>>) -> RawRequest {
    match value {
        Some(value) => request.with_query(name, value),
        None => request,
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn an_object_type_is_a_name_a_type_id_or_a_custom_objects_full_name() {
        for object_type in [
            "contacts",
            "line_items",
            "feedback_submissions",
            "partner-clients",
            "0-1",
            "2-3465404",
            "p12345_cars",
            "Contact",
        ] {
            assert!(is_object_type(object_type), "{object_type}");
        }
        for bad in [
            "",
            " ",
            "contacts ",
            "contacts/1",
            "contacts/../owners",
            "..",
            ".",
            "contacts?archived=true",
            "contacts#",
            "contacts%2F1",
            "contacts\\1",
            "contacts\n",
            "cöntacts",
        ] {
            assert!(!is_object_type(bad), "{bad:?}");
        }
        assert!(is_object_type(&"a".repeat(100)));
        assert!(!is_object_type(&"a".repeat(101)));
    }

    #[test]
    fn a_place_is_repeated_only_when_it_reads_as_one() {
        for place in [
            "id",
            "[1].properties",
            "properties.hs_lead_status",
            "stages[0].metadata.probability",
        ] {
            assert!(is_place(place), "{place}");
        }
        // The root is no place, and a property's name is HubSpot's to send.
        for not_a_place in [
            ".",
            "",
            "properties.ada@example.com",
            "properties.4111 1111",
            "properties.`x`",
        ] {
            assert!(!is_place(not_a_place), "{not_a_place:?}");
        }
        assert!(!is_place(&"a".repeat(121)));
    }

    #[test]
    fn the_next_page_is_named_only_by_a_token_that_says_something() {
        assert_eq!(
            next_cursor(&json!({ "paging": { "next": { "after": "394", "link": "https://api.hubapi.com/x" } } })),
            Some("394".into())
        );
        assert_eq!(
            next_cursor(&json!({ "paging": { "next": { "after": 20 } } })),
            Some("20".into())
        );
        for last in [
            json!({}),
            json!({ "paging": null }),
            json!({ "paging": {} }),
            json!({ "paging": { "next": null } }),
            json!({ "paging": { "next": { "after": "" } } }),
            json!({ "paging": { "next": { "after": null } } }),
            json!({ "paging": { "prev": { "before": "10" } } }),
        ] {
            assert_eq!(next_cursor(&last), None, "{last}");
        }
    }

    #[test]
    fn a_blank_cursor_is_no_cursor() {
        assert_eq!(cursor(None), None);
        assert_eq!(cursor(Some("")), None);
        assert_eq!(cursor(Some("  ")), None);
        assert_eq!(cursor(Some(" 394 ")), Some("394"));
    }

    #[test]
    fn everything_a_url_would_read_is_encoded() {
        assert_eq!(encoded("ada@example.com"), "ada%40example.com");
        assert_eq!(encoded("a?b#c d%2F"), "a%3Fb%23c%20d%252F");
        assert_eq!(encoded("AZaz09-._~"), "AZaz09-._~");
    }
}
