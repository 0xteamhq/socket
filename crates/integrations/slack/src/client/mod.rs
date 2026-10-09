//! Slack's Web API as typed methods, one file per area of the API.
//!
//! Identifiers (a channel id, a user id, a message timestamp) are plain
//! arguments. Content and optional filters are structs from [`crate::models`].
//! Reads are sent as GET with query parameters; writes as POST with a JSON
//! body, so the transport never repeats a write that may have happened.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it.

mod bookmarks;
mod chat;
mod conversations;
mod files;
mod pins;
mod reactions;
mod reminders;
mod search;
mod usergroups;
mod users;
mod workspace;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Error, ErrorKind, Page, RawRequest, Result};

pub use bookmarks::Bookmarks;
pub use chat::Chat;
pub use conversations::Conversations;
pub use files::Files;
pub use pins::Pins;
pub use reactions::Reactions;
pub use reminders::Reminders;
pub use search::SearchApi;
pub use usergroups::UserGroups;
pub use users::Users;
pub use workspace::Workspace;

/// One connection's access to Slack's Web API.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Calls a method that reads, with `arguments` as query parameters.
    pub(super) async fn get(&self, method: &str, arguments: Value) -> Result<Value> {
        let mut request = RawRequest::get(method);
        for (name, value) in arguments.as_object().into_iter().flatten() {
            let text = match value {
                Value::Null => continue,
                Value::String(text) => text.clone(),
                other => other.to_string(),
            };
            request = request.with_query(name.as_str(), text);
        }
        Ok(self.connection.request(request).await?.body)
    }

    /// Calls a method that writes, with `arguments` as a JSON body.
    pub(super) async fn post(&self, method: &str, arguments: Value) -> Result<Value> {
        Ok(self.connection.request(RawRequest::post(method, arguments)).await?.body)
    }

    /// Reads one field of a response as `T`.
    pub(super) fn field<T: DeserializeOwned>(&self, body: &Value, name: &str) -> Result<T> {
        let value = body
            .get(name)
            .filter(|v| !v.is_null())
            .ok_or_else(|| self.error(ErrorKind::Decode, format!("slack answered without `{name}`")))?;
        serde_json::from_value(value.clone()).map_err(|e| {
            self.error(
                ErrorKind::Decode,
                format!("slack sent a `{name}` that could not be read"),
            )
            .with_source(e)
        })
    }

    /// Reads a list field and the cursor for the page after it.
    pub(super) fn page<T: DeserializeOwned>(&self, body: &Value, name: &str) -> Result<Page<T>> {
        let next_cursor = body["response_metadata"]["next_cursor"]
            .as_str()
            .filter(|c| !c.is_empty())
            .map(str::to_owned);
        Ok(Page {
            items: self.field(body, name)?,
            next_cursor,
        })
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }
}

/// `base` with the set fields of `options` added. Unset options are left out,
/// so Slack applies its own defaults.
pub(super) fn with(base: Value, options: &impl Serialize) -> Value {
    let mut merged = match base {
        Value::Object(map) => map,
        _ => Map::new(),
    };
    if let Ok(Value::Object(extra)) = serde_json::to_value(options) {
        merged.extend(extra.into_iter().filter(|(_, value)| !value.is_null()));
    }
    Value::Object(merged)
}
