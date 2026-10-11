//! Pipedrive as typed methods, one file per group of its API.
//!
//! Identifiers are plain arguments: a number for a deal, a person, an
//! organisation, an activity, a note; a UUID for a lead. Content and optional
//! filters are structs from [`crate::models`]. Reads are sent as GET, so the
//! transport may repeat them after a server error. A create is a POST and an
//! update a PATCH, which it never repeats unless Pipedrive refused them
//! outright. A delete, and the PUT that version 1 changes a note with, are
//! still repeated after a server error, until the transport stops repeating
//! anything but a read.
//!
//! Pipedrive has two versions of its API side by side. Each method calls
//! version 2 where Pipedrive offers it and version 1 where it does not, and a
//! caller sees one shape either way: a path here starts with `v1/` or `v2/`
//! and nothing outside this module knows which.
//!
//! Paging is done in two ways as well, a cursor in version 2 and an offset in
//! version 1, and a caller sees one: [`Paging`] in, a [`Page`] with a
//! `next_cursor` out.
//!
//! This file holds only what every group shares: the access to the API, and
//! the re-exports. Two parts of that access are long enough to have files of
//! their own, `paging` and `request`; neither is a group. Each group's
//! methods are in the file named after it.

mod activities;
mod deals;
mod fields;
mod leads;
mod notes;
mod organizations;
mod paging;
mod persons;
mod pipelines;
mod request;
mod search;
mod users;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Error, ErrorKind, RawRequest, Result};

pub use activities::Activities;
pub use deals::Deals;
pub use fields::Fields;
pub use leads::Leads;
pub use notes::Notes;
pub use organizations::Organizations;
pub use persons::Persons;
pub use pipelines::Pipelines;
pub use search::Search;
pub use users::Users;

pub(crate) use request::is_uuid;
use request::{body, encoded, filtered};

use crate::models::{CustomFields, is_field_key};

/// One connection's access to Pipedrive.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    pub(super) fn invalid(&self, message: impl Into<String>) -> Error {
        self.error(ErrorKind::InvalidInput, message)
    }

    /// Sends `request` and returns Pipedrive's whole answer, once it says
    /// the request succeeded. `what` names what was asked for: "a deal".
    ///
    /// Every answer of Pipedrive's carries `success`. One without it is not
    /// Pipedrive confirming anything, whatever its status.
    ///
    /// The query is written here and not by the transport. The transport
    /// writes a space as `+`, as a form does; Pipedrive asks for a search
    /// term to be percent-encoded, which every server reads the same way.
    pub(super) async fn send(&self, mut request: RawRequest, what: &str) -> Result<Value> {
        if !request.query.is_empty() {
            let written: Vec<String> = request
                .query
                .drain(..)
                .map(|(name, value)| format!("{name}={}", encoded(&value)))
                .collect();
            request.path = format!("{}?{}", request.path, written.join("&"));
        }
        let body = self.connection.request(request).await?.body;
        if body["success"] != true {
            return Err(self.error(ErrorKind::Decode, format!("pipedrive answered without {what}")));
        }
        Ok(body)
    }

    /// Reads a value as `T`. `what` names it in the error: "a deal".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// is a customer's record. An error is logged and shown, so only the
    /// place of the value goes into it, and serde's error is not kept as its
    /// cause. The names in the place come from our own types.
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
                format!("pipedrive sent {what} that could not be read{place}"),
            )
        })
    }

    /// Sends `request` and returns the one record under `data`, as Pipedrive sent it.
    pub(super) async fn record(&self, request: RawRequest, what: &str) -> Result<Value> {
        let mut body = self.send(request, what).await?;
        match body["data"].take() {
            record @ Value::Object(_) => Ok(record),
            _ => Err(self.error(ErrorKind::Decode, format!("pipedrive answered without {what}"))),
        }
    }

    /// Sends `request` and reads the one record under `data`.
    pub(super) async fn one<T: DeserializeOwned>(&self, request: RawRequest, what: &str) -> Result<T> {
        self.decode(self.record(request, what).await?, what)
    }

    /// Sends a request whose answer carries nothing to return, such as a delete.
    pub(super) async fn done(&self, request: RawRequest, what: &str) -> Result<()> {
        self.send(request, what).await.map(|_| ())
    }

    /// A text a record cannot do without, such as a deal's title.
    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.invalid(format!("{what} is required")));
        }
        Ok(())
    }

    /// A search term Pipedrive takes: two characters or more, or one when
    /// only whole matches are asked for.
    pub(super) fn term(&self, term: &str, exact_match: Option<bool>) -> Result<()> {
        let least = if exact_match == Some(true) { 1 } else { 2 };
        if term.trim().chars().count() < least {
            return Err(self.invalid("`term` needs at least two characters, or one with `exact_match`"));
        }
        Ok(())
    }

    /// The request that changes the record at `path`: a PATCH that carries
    /// the set fields of `change`. A change that names nothing is refused
    /// here, and not sent for Pipedrive to refuse.
    pub(super) fn change(&self, path: String, change: &impl Serialize) -> Result<RawRequest> {
        let body = body(change);
        if body.as_object().is_none_or(Map::is_empty) {
            return Err(self.invalid("nothing to change: set at least one field"));
        }
        Ok(RawRequest::new("PATCH", path).with_body(body))
    }

    /// The custom fields a create or an update sets, checked: each is under
    /// the 40-character key of a custom field.
    ///
    /// The keys are the company's own, so the schema cannot list them. Their
    /// shape is still known, and anything else is refused: a name such as
    /// `status` or `is_deleted` is one of the record's own fields, and is
    /// not let through here to do what the operation does not say it does.
    pub(super) fn custom_values(&self, values: Option<&CustomFields>) -> Result<()> {
        if values.is_some_and(|values| !values.keys().all(|key| is_field_key(key))) {
            return Err(self.invalid(
                "`custom_fields` takes values under the 40-character keys of custom fields, which the `fields` operations list",
            ));
        }
        Ok(())
    }

    /// The keys of the custom fields a list is asked to return, checked.
    ///
    /// Pipedrive takes at most 15, joined by commas, so a key that holds a
    /// comma would be read as two.
    pub(super) fn custom_field_keys(&self, keys: Option<&[String]>) -> Result<()> {
        let keys = keys.unwrap_or_default();
        if keys.len() > 15 {
            return Err(self.invalid("`custom_fields` takes at most 15 keys"));
        }
        if keys.iter().any(|key| key.trim().is_empty() || key.contains(',')) {
            return Err(self.invalid("`custom_fields` takes the keys of custom fields, one in each entry"));
        }
        Ok(())
    }
}
