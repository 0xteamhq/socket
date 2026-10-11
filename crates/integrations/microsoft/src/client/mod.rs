//! Microsoft Graph as typed methods, one file per area of the API.
//!
//! Identifiers (a calendar id, an event id) are plain arguments. Content and
//! optional filters are structs from [`crate::models`]. Reads are sent as
//! GET, so the transport may repeat them after a server error. Everything
//! else goes out with the verb Graph dictates. A POST or a PATCH is never
//! repeated unless Graph refused it outright; a DELETE still is, after a
//! server error, until the transport stops repeating anything but a read.
//!
//! This file holds only what every area shares: the access to the API, and
//! the re-exports. Each area's methods are in the file named after it.

mod attendance;
mod calendars;
mod channel_messages;
mod channels;
mod chats;
mod events;
mod mail;
mod mail_attachments;
mod mail_compose;
mod mail_folders;
mod online_meetings;
mod recordings;
mod teams;
mod transcripts;

use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use socketkit_core::{Connection, Content, ContentRequest, Error, ErrorKind, Page, RawRequest, Result};
use url::Url;

pub use attendance::Attendance;
pub use calendars::Calendars;
pub use channel_messages::ChannelMessages;
pub use channels::Channels;
pub use chats::Chats;
pub use events::Events;
pub use mail::Mail;
pub use mail_folders::MailFolders;
pub use online_meetings::OnlineMeetings;
pub use recordings::Recordings;
pub use teams::Teams;
pub use transcripts::Transcripts;

use crate::models::{ChatMessage, Download, ItemBody, Paging, Recipient, SendChatMessage};

/// One connection's access to Microsoft Graph.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Api<'a> {
    pub(crate) connection: &'a Connection,
}

impl Api<'_> {
    pub(super) fn error(&self, kind: ErrorKind, message: impl Into<String>) -> Error {
        Error::new(kind, message).with_provider(self.connection.provider().id.clone())
    }

    /// Sends `request` and returns what Graph answered. A request that
    /// answers with no content, as most actions do, returns `null`.
    ///
    /// The query is written here and not by the transport. The transport
    /// writes a space as `+`, as a form does; Graph documents its filters
    /// with `%20`, which every server reads the same way.
    pub(super) async fn send(&self, mut request: RawRequest) -> Result<Value> {
        if !request.query.is_empty() {
            let written: Vec<String> = request
                .query
                .drain(..)
                .map(|(name, value)| format!("{name}={}", encoded(&value)))
                .collect();
            request.path = format!("{}?{}", request.path, written.join("&"));
        }
        Ok(self.connection.request(request).await?.body)
    }

    /// Fetches what is at `path` as it is served: a recording, an attachment.
    /// The bytes are not read as anything. `limits` are the caller's own.
    pub(super) async fn fetch(&self, path: String, limits: &Download) -> Result<Content> {
        let mut request = ContentRequest::get(path);
        if let Some(most) = limits.max_bytes {
            request = request.with_max_bytes(most);
        }
        if let Some(secs) = limits.timeout_secs {
            request = request.with_timeout(std::time::Duration::from_secs(secs));
        }
        self.connection.fetch(request).await
    }

    /// Reads a response as `T`. `what` names it in the error: "an event".
    ///
    /// serde's own message quotes the value it could not read, and here that
    /// is the content of someone's calendar. An error is logged and shown, so
    /// only the place of the value goes into it, and serde's error is not
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
                format!("microsoft sent {what} that could not be read{place}"),
            )
        })
    }

    /// Reads the list every Graph collection carries under `value`.
    pub(super) fn list<T: DeserializeOwned>(&self, body: &Value, what: &str) -> Result<Vec<T>> {
        let items = body
            .get("value")
            .ok_or_else(|| self.error(ErrorKind::Decode, format!("microsoft answered without {what}")))?;
        self.decode(items.clone(), what)
    }

    /// Reads one page of a list. `first` is the request for the first page.
    ///
    /// Graph gives the whole address of the next page, which is the cursor.
    /// Given back, its query replaces the parameters of `first`: it already
    /// carries every one of them, the page size included, with Graph's place
    /// in the list. See [`Api::next_page`] for what else is asked of it.
    pub(super) async fn page<T: DeserializeOwned>(
        &self,
        first: RawRequest,
        paging: &Paging,
        what: &str,
    ) -> Result<Page<T>> {
        // Graph takes a page of 1 to 1000, where it does not say less.
        self.page_up_to(1000, first, paging, what).await
    }

    /// [`Api::page`] for a list whose pages Graph keeps to `most` items.
    pub(super) async fn page_up_to<T: DeserializeOwned>(
        &self,
        most: u32,
        first: RawRequest,
        paging: &Paging,
        what: &str,
    ) -> Result<Page<T>> {
        if paging.limit.is_some_and(|limit| !(1..=most).contains(&limit)) {
            return Err(self.error(ErrorKind::InvalidInput, format!("`limit` is from 1 to {most}")));
        }
        let cursor = paging.cursor.as_deref().map(str::trim).filter(|c| !c.is_empty());
        let request = match (cursor, paging.limit) {
            (Some(cursor), _) => RawRequest {
                path: self.next_page(cursor, &first.path)?,
                query: Vec::new(),
                ..first
            },
            (None, Some(limit)) => first.with_query("$top", limit.to_string()),
            (None, None) => first,
        };
        let body = self.send(request).await?;
        let next_cursor = body["@odata.nextLink"]
            .as_str()
            .filter(|link| !link.is_empty())
            .map(str::to_owned);
        Ok(Page {
            items: self.list(&body, what)?,
            next_cursor,
        })
    }

    /// The request path for the page a cursor points to: `listing`, with the
    /// cursor's query.
    ///
    /// A cursor comes back from the caller, so it is not trusted to be what
    /// Graph sent. Used as it stood, it would let a listing read any address
    /// that answers a GET, and would send the token to any host the provider
    /// allows, the sign-in host among them. So nothing in a cursor is ever
    /// used as a host or a path: the page is requested at the address this
    /// crate built for the list, and the cursor supplies only the query,
    /// where Graph keeps its place.
    ///
    /// The query is used whole. Graph does not say which parameters a next
    /// page carries, so none is taken out; a cursor that was written by hand
    /// can therefore filter, sort or skip within the list, as the caller
    /// could by other means, and cannot leave it.
    ///
    /// The cursor's path is not compared with the list's. Graph writes the
    /// same list in more than one way (`me/events('id')`, `users('id')/…`),
    /// and since the path is never used, a comparison could only refuse a
    /// good cursor. Its host is checked, so that what is plainly not from
    /// this API is refused and not read for a query.
    fn next_page(&self, cursor: &str, listing: &str) -> Result<String> {
        let base = &self.connection.provider().api_base;
        let place = Url::parse(cursor).ok().and_then(|next| {
            let from_here = next.origin() == base.origin() && next.username().is_empty() && next.password().is_none();
            let query = next.query().filter(|query| !query.is_empty())?;
            from_here.then(|| query.to_owned())
        });
        match place {
            Some(query) => Ok(format!("{listing}?{query}")),
            None => Err(self.error(
                ErrorKind::InvalidInput,
                "`cursor` is not the address of a next page; pass back `next_cursor` unchanged",
            )),
        }
    }

    /// A body that is sent replaces the text that was there, so it has to
    /// carry some, even if empty.
    pub(super) fn body(&self, body: Option<&ItemBody>) -> Result<()> {
        if body.is_some_and(|body| body.content.is_none()) {
            return Err(self.error(ErrorKind::InvalidInput, "a body needs `content`"));
        }
        Ok(())
    }

    /// Every recipient that is named has to have an address.
    pub(super) fn recipients(&self, recipients: Option<&[Recipient]>) -> Result<()> {
        let blank = |recipient: &Recipient| recipient.email_address.address.trim().is_empty();
        if recipients.unwrap_or_default().iter().any(blank) {
            return Err(self.error(ErrorKind::InvalidInput, "every recipient needs `emailAddress.address`"));
        }
        Ok(())
    }

    /// One page of messages, each with its body also as plain text. Graph
    /// keeps a page of messages to 50.
    pub(super) async fn messages(&self, first: RawRequest, paging: &Paging) -> Result<Page<ChatMessage>> {
        let page: Page<ChatMessage> = self.page_up_to(50, named(first), paging, "messages").await?;
        Ok(Page {
            items: page.items.into_iter().map(ChatMessage::rendered).collect(),
            next_cursor: page.next_cursor,
        })
    }

    /// One message, with its body also as plain text. `request` reads it or
    /// sends it; either way Graph answers with the message.
    pub(super) async fn message(&self, request: RawRequest) -> Result<ChatMessage> {
        let message: ChatMessage = self.decode(self.send(named(request)).await?, "a message")?;
        if message.id.is_empty() {
            return Err(self.error(ErrorKind::Decode, "microsoft answered without a message"));
        }
        Ok(message.rendered())
    }

    /// The body of a message to send, once it is known to say something.
    pub(super) fn outgoing(&self, message: &SendChatMessage) -> Result<Value> {
        if message
            .body
            .content
            .as_deref()
            .is_none_or(|content| content.trim().is_empty())
        {
            return Err(self.error(ErrorKind::InvalidInput, "a message needs `body.content`"));
        }
        Ok(with(serde_json::json!({}), message))
    }

    pub(super) fn required(&self, what: &str, value: &str) -> Result<()> {
        if value.trim().is_empty() {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is required")));
        }
        Ok(())
    }

    /// Writes an id as one segment of a path, whatever it contains.
    ///
    /// Graph's ids may hold `/`, `+` and `=`. Everything outside the
    /// characters a URL leaves alone is percent-encoded, so an id can never
    /// add a segment, a query or a fragment. A segment made only of dots
    /// would be resolved away and address something else, so it is refused.
    pub(super) fn segment(&self, what: &str, id: &str) -> Result<String> {
        self.required(what, id)?;
        if id == "." || id == ".." {
            return Err(self.error(ErrorKind::InvalidInput, format!("{what} is not valid")));
        }
        Ok(encoded(id))
    }
}

/// Asks Graph to name the kinds it has added since v1.0 was fixed: a system
/// message, a shared channel, a co-organiser. Without this it writes each of
/// them as `unknownFutureValue`.
pub(super) fn named(request: RawRequest) -> RawRequest {
    request.with_header("Prefer", "include-unknown-enum-members")
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
/// at every depth, so Graph applies its own defaults and a change touches
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
