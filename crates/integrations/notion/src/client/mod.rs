//! Notion's API as typed methods, one file per area of the API.
//!
//! Identifiers (a page id, a block id) are plain arguments, and each may be
//! given with or without dashes, or as the address of the page. Content and
//! optional filters are structs from [`crate::models`]. Reads are sent as
//! GET, so the transport may repeat them after a server error, except the two
//! Notion offers only as POST: a search and a query. Everything else goes out
//! with the verb Notion dictates. A POST or a PATCH is never repeated unless
//! Notion refused it outright; a DELETE still is, after a server error, until
//! the transport stops repeating anything but a read.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it.

mod blocks;
mod comments;
mod databases;
mod pages;
mod pages_read;
mod search;
mod users;

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

pub use blocks::Blocks;
pub use comments::Comments;
pub use databases::Databases;
pub use pages::Pages;
pub use search::Search;
pub use users::Users;

use crate::models::{Block, Paging};

/// The most items Notion returns in one page of a list.
const MOST_IN_A_PAGE: u32 = 100;

/// One connection's access to Notion's API.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
    /// The API version every request declares.
    pub(crate) version: &'a str,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Sends `request` with the API version and returns what Notion answered.
    pub(super) async fn send(&self, request: RawRequest) -> Result<Value> {
        let request = request.with_header("Notion-Version", self.version);
        Ok(self.connection.request(request).await?.body)
    }

    /// Reads a response as `T`. `what` names it in the error: "a page".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// is the content of someone's workspace. An error is logged and shown,
    /// so only the place of the value goes into it, and serde's error is not
    /// kept as its cause. The names in the place come from our own types.
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
                format!("notion sent {what} that could not be read{place}"),
            )
        })
    }

    /// Reads one object of the kind asked for: `page`, `block`, `data_source`.
    /// A success that carries anything else is not that object.
    pub(super) fn object<T: DeserializeOwned>(&self, body: Value, kind: &str) -> Result<T> {
        let named = kind.replace('_', " ");
        if body["object"] != kind || body["id"].as_str().is_none_or(str::is_empty) {
            return Err(self.error(ErrorKind::Decode, format!("notion answered without a {named}")));
        }
        self.decode(body, &format!("a {named}"))
    }

    /// Reads one page of a list. Every list Notion returns carries its items
    /// under `results`, and a cursor when there are more.
    pub(super) fn list<T: DeserializeOwned>(&self, mut body: Value, what: &str) -> Result<Page<T>> {
        if body["object"] != "list" || !body["results"].is_array() {
            return Err(self.error(ErrorKind::Decode, format!("notion answered without {what}")));
        }
        let more = body["has_more"].as_bool().unwrap_or(false);
        let next_cursor = body["next_cursor"]
            .as_str()
            .filter(|cursor| more && !cursor.is_empty())
            .map(str::to_owned);
        Ok(Page {
            items: self.decode(body["results"].take(), what)?,
            next_cursor,
        })
    }

    /// The part of a list that was asked for, as Notion names it: the cursor
    /// and the page size. A GET takes them in its query and a POST in its body.
    fn place(&self, paging: &Paging) -> Result<Vec<(&'static str, Value)>> {
        if paging.limit.is_some_and(|limit| !(1..=MOST_IN_A_PAGE).contains(&limit)) {
            return Err(self.error(
                ErrorKind::InvalidInput,
                format!("`limit` is from 1 to {MOST_IN_A_PAGE}"),
            ));
        }
        let cursor = paging.cursor.as_deref().map(str::trim).filter(|c| !c.is_empty());
        let mut place = Vec::new();
        place.extend(cursor.map(|cursor| ("start_cursor", json!(cursor))));
        place.extend(paging.limit.map(|limit| ("page_size", json!(limit))));
        Ok(place)
    }

    /// A GET for one page of a list.
    pub(super) fn listing(&self, path: String, paging: &Paging) -> Result<RawRequest> {
        let mut request = RawRequest::get(path);
        for (name, value) in self.place(paging)? {
            let written = value.as_str().map_or_else(|| value.to_string(), str::to_owned);
            request = request.with_query(name, written);
        }
        Ok(request)
    }

    /// A POST that reads one page of a list: `body` with the place in the list added.
    pub(super) fn asking(&self, path: String, mut body: Map<String, Value>, paging: &Paging) -> Result<RawRequest> {
        for (name, value) in self.place(paging)? {
            body.insert(name.to_owned(), value);
        }
        Ok(RawRequest::post(path, Value::Object(body)))
    }

    /// Reads an id and returns it with dashes, as it goes into a path.
    ///
    /// An id is a UUID, so what is returned holds only hex digits and dashes
    /// and can never add a segment, a query or a fragment to a path.
    pub(super) fn id(&self, what: &str, id: &str) -> Result<String> {
        crate::dashed(id).ok_or_else(|| {
            self.error(
                ErrorKind::InvalidInput,
                format!("{what} is a UUID, with or without dashes, or the address of a page in Notion"),
            )
        })
    }

    /// One page of the blocks directly inside a block or a page.
    pub(super) async fn children(&self, block: &str, paging: &Paging) -> Result<Page<Block>> {
        let path = format!("blocks/{}/children", self.id("a block id", block)?);
        let body = self.send(self.listing(path, paging)?).await?;
        self.list(body, "blocks")
    }
}

/// A PATCH, which Notion uses for every change to what exists.
pub(super) fn patch(path: String, body: Value) -> RawRequest {
    RawRequest::new("PATCH", path).with_body(body)
}
