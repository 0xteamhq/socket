//! Microsoft Graph as typed methods, one file per area of the API.
//!
//! What identifies the thing asked for (a meeting's id, a transcript's id) is
//! a plain argument. Options are structs from [`crate::models`]. Everything
//! here reads, and a read is sent as `GET`.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it.

mod attendance;
mod online_meetings;
mod recordings;
mod transcripts;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::de::DeserializeOwned;
use serde_json::Value;
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

pub use attendance::Attendance;
pub use online_meetings::OnlineMeetings;
pub use recordings::Recordings;
pub use transcripts::Transcripts;

use crate::models::Paging;

/// Everything outside letters, digits and `-._~` is encoded, in a path segment
/// and in a query value alike.
const ENCODED: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

/// One connection's access to Microsoft Graph.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// An identifier as one path segment. Graph's ids hold `*`, `@`, `=` and
    /// `:`; a `/` in one is encoded, so it stays one segment. `.` and `..` are
    /// refused, since as path segments they would reach a different endpoint.
    pub(super) fn segment(&self, what: &str, value: &str) -> Result<String> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        if value == "." || value == ".." {
            return Err(self.error(ErrorKind::InvalidInput, format!("{value:?} is not {what}")));
        }
        Ok(utf8_percent_encode(value, ENCODED).to_string())
    }

    /// Reads `path`, with `options` as OData query options such as `$filter`.
    ///
    /// The query is written here and not by the transport: Graph documents
    /// `%20` for the spaces in a filter, where a form encoder writes `+`.
    pub(super) async fn get(&self, path: &str, options: &[(&str, &str)]) -> Result<Value> {
        let query: Vec<String> = options
            .iter()
            .map(|(name, value)| format!("{name}={}", utf8_percent_encode(value, ENCODED)))
            .collect();
        let path = if query.is_empty() {
            path.to_owned()
        } else {
            format!("{path}?{}", query.join("&"))
        };
        Ok(self.connection.request(RawRequest::get(path)).await?.body)
    }

    /// Reads one page of the list at `path`, or the page `paging.cursor` points to.
    pub(super) async fn list<T: DeserializeOwned>(&self, path: &str, paging: &Paging, what: &str) -> Result<Page<T>> {
        let body = match &paging.cursor {
            Some(cursor) => {
                let next = self.next_link(cursor)?;
                self.connection.request(RawRequest::get(next)).await?.body
            }
            None => {
                let top = paging.top.map(|top| top.to_string());
                let options: Vec<(&str, &str)> = top.iter().map(|top| ("$top", top.as_str())).collect();
                self.get(path, &options).await?
            }
        };
        let Some(items) = body.get("value").filter(|items| items.is_array()) else {
            return Err(self.error(
                ErrorKind::Decode,
                format!("microsoft answered without a list of {what}"),
            ));
        };
        Ok(Page {
            items: self.decode(items.clone(), what)?,
            next_cursor: body["@odata.nextLink"]
                .as_str()
                .filter(|link| !link.is_empty())
                .map(str::to_owned),
        })
    }

    /// Reads a whole response body as `T`.
    pub(super) fn decode<T: DeserializeOwned>(&self, body: Value, what: &str) -> Result<T> {
        if body.is_null() {
            return Err(self.error(ErrorKind::Decode, format!("microsoft answered without {what}")));
        }
        serde_json::from_value(body).map_err(|e| {
            self.error(
                ErrorKind::Decode,
                format!("microsoft sent {what} that could not be read"),
            )
            .with_source(e)
        })
    }

    /// A cursor, checked to be a link into this connection's API.
    ///
    /// Graph's next link is a whole URL, and it is the caller who hands it
    /// back. The transport already refuses a host outside the provider's own;
    /// this also refuses another API on the same host, and the sign-in host.
    fn next_link(&self, cursor: &str) -> Result<String> {
        let base = &self.connection.provider().api_base;
        let prefix = format!("{}/", base.as_str().trim_end_matches('/'));
        let absolute = cursor.starts_with("https://") || cursor.starts_with("http://");
        // Parsing resolves any `..` first, so a link cannot climb out of the API.
        match base.join(cursor) {
            Ok(link) if absolute && link.as_str().starts_with(&prefix) => Ok(link.into()),
            _ => Err(self.error(
                ErrorKind::InvalidInput,
                "the cursor is not a next-page link from Microsoft Graph",
            )),
        }
    }
}
