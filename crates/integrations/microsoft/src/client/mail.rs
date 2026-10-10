//! A person's mail: reading it, and marking, moving and deleting what is there.
//!
//! Writing and sending are in `mail_compose.rs`; they are methods of the same
//! group, kept apart only for the length of the file.
//!
//! Every path here starts at `me`, the signed-in person's own mailbox. A
//! shared or delegated mailbox would start at `users/{id}` instead, and
//! [`Mail::item`] and [`Mail::messages`] are the two places that say which.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, with};
use crate::models::{Attachment, BodyType, GetMessage, ListMessages, Message, Paging, UpdateMessage};

/// What describes an attachment, without the file itself. A list that
/// carried every file would be as large as all of them together.
const ATTACHMENT_FIELDS: &str = "id,name,contentType,size,isInline,lastModifiedDateTime";

/// Asks Graph for bodies in one format: plain text unless HTML is asked for.
fn with_body_as(request: RawRequest, body_type: Option<BodyType>) -> RawRequest {
    let format = match body_type.unwrap_or(BodyType::Text) {
        BodyType::Text => "text",
        BodyType::Html => "html",
    };
    request.with_header("Prefer", format!("outlook.body-content-type=\"{format}\""))
}

/// A person's mail.
#[derive(Debug, Clone, Copy)]
pub struct Mail<'a>(pub(crate) Api<'a>);

impl Mail<'_> {
    /// Lists the messages of one folder, or of the whole mailbox, newest
    /// first unless `order_by` says otherwise.
    pub async fn list(&self, options: ListMessages) -> Result<Page<Message>> {
        let set = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        let mut request = with_body_as(
            RawRequest::get(self.messages(options.folder.as_deref())?),
            options.body_type,
        );
        if let Some(filter) = set(&options.filter) {
            request = request.with_query("$filter", filter);
        }
        if let Some(order) = set(&options.order_by) {
            request = request.with_query("$orderby", order);
        }
        if let Some(search) = set(&options.search) {
            // Graph takes the search as one phrase in double quotes. A quote
            // or a backslash inside it is escaped so that the phrase ends
            // where the caller's text does.
            let phrase = search.replace('\\', "\\\\").replace('"', "\\\"");
            request = request.with_query("$search", format!("\"{phrase}\""));
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.page(request, &paging, "messages").await
    }

    /// Gets one message, with its body as plain text unless HTML is asked for.
    pub async fn get(&self, message: &str, options: GetMessage) -> Result<Message> {
        let request = with_body_as(RawRequest::get(self.item(message)?), options.body_type);
        let body = self.0.send(request).await?;
        self.message(body)
    }

    /// Lists every message of one conversation, oldest first, with bodies as
    /// plain text. `conversation` is a message's `conversationId`.
    pub async fn conversation(&self, conversation: &str, paging: Paging) -> Result<Page<Message>> {
        self.0.required("a conversation id", conversation)?;
        // Graph sorts a filtered list only by what the filter names first, so
        // the filter names the received time, with a bound every message
        // meets. A quote in the id is doubled, as OData writes one in a string.
        let filter = format!(
            "receivedDateTime ge 1900-01-01T00:00:00Z and conversationId eq '{}'",
            conversation.replace('\'', "''")
        );
        let request = with_body_as(RawRequest::get(self.messages(None)?), None)
            .with_query("$filter", filter)
            .with_query("$orderby", "receivedDateTime asc");
        self.0.page(request, &paging, "messages").await
    }

    /// Lists what is attached to a message: names, types and sizes, without
    /// the files themselves.
    pub async fn attachments_list(&self, message: &str, paging: Paging) -> Result<Page<Attachment>> {
        let request =
            RawRequest::get(format!("{}/attachments", self.item(message)?)).with_query("$select", ATTACHMENT_FIELDS);
        self.0.page(request, &paging, "attachments").await
    }

    /// Gets one attachment. A file comes with its content, in base64.
    pub async fn attachment_get(&self, message: &str, attachment: &str) -> Result<Attachment> {
        let attachment = self.0.segment("an attachment id", attachment)?;
        let path = format!("{}/attachments/{attachment}", self.item(message)?);
        let attachment: Attachment = self
            .0
            .decode(self.0.send(RawRequest::get(path)).await?, "an attachment")?;
        if attachment.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "microsoft answered without an attachment"));
        }
        Ok(attachment)
    }

    /// Marks a message: read or unread, its categories, its follow-up flag.
    pub async fn update(&self, message: &str, changes: UpdateMessage) -> Result<Message> {
        let path = self.item(message)?;
        let body = self.0.send(self.patch(path, with(json!({}), &changes))?).await?;
        self.message(body)
    }

    /// Moves a message to another folder: a folder id, or a well-known name
    /// such as `archive` or `deleteditems`. The message that comes back has a
    /// new id; the old one no longer finds it.
    pub async fn move_to(&self, message: &str, folder: &str) -> Result<Message> {
        self.0.required("a folder", folder)?;
        let request = RawRequest::post(
            format!("{}/move", self.item(message)?),
            json!({ "destinationId": folder }),
        );
        let body = self.0.send(request).await?;
        self.message(body)
    }

    /// Deletes a message.
    pub async fn delete(&self, message: &str) -> Result<()> {
        self.0
            .send(RawRequest::new("DELETE", self.item(message)?))
            .await
            .map(drop)
    }

    /// The path of one message.
    pub(super) fn item(&self, message: &str) -> Result<String> {
        Ok(format!("me/messages/{}", self.0.segment("a message id", message)?))
    }

    /// The path of a folder's messages, or of every message in the mailbox.
    fn messages(&self, folder: Option<&str>) -> Result<String> {
        Ok(match folder {
            Some(folder) => format!("me/mailFolders/{}/messages", self.0.segment("a folder", folder)?),
            None => "me/messages".to_owned(),
        })
    }

    /// A change to the message at `path`, refused when it changes nothing.
    pub(super) fn patch(&self, path: String, changes: Value) -> Result<RawRequest> {
        if changes.as_object().is_none_or(serde_json::Map::is_empty) {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "a change needs at least one field"));
        }
        Ok(RawRequest::new("PATCH", path).with_body(changes))
    }

    pub(super) fn message(&self, body: Value) -> Result<Message> {
        let message: Message = self.0.decode(body, "a message")?;
        if message.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "microsoft answered without a message"));
        }
        Ok(message)
    }
}
