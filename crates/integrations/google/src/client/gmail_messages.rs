//! Gmail messages: finding and reading them, and labelling them and moving
//! them to the bin.
//!
//! Sending is in `gmail_messages_send.rs`; those are methods of the same
//! group, kept apart only for the length of the file.
//!
//! Every path here starts at `users/me`, the mailbox of the account that
//! connected. [`GmailMessages::item`] is the one place that says so.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, GMAIL, set, with_query};
use crate::models::{
    GmailAttachmentBody, GmailGetMessage, GmailListMessages, GmailMessage, GmailMessageRef, GmailModifyMessage,
    GmailWireMessage, Paging,
};

pub(super) const MESSAGES: &str = "gmail/v1/users/me/messages";

/// The messages of a Gmail mailbox.
#[derive(Debug, Clone, Copy)]
pub struct GmailMessages<'a>(pub(crate) Api<'a>);

impl GmailMessages<'_> {
    /// Lists the messages a search finds, or every message when nothing is
    /// asked for.
    ///
    /// The list is light: Gmail returns each message's id and its thread's
    /// id and nothing else, and so does this. Nothing is fetched for each
    /// row. Read the ones that matter with [`GmailMessages::get`].
    pub async fn list(&self, options: GmailListMessages, paging: Paging) -> Result<Page<GmailMessageRef>> {
        let options = GmailListMessages {
            q: options.q.filter(|q| !q.trim().is_empty()),
            ..options
        };
        if options.label_ids.iter().flatten().any(|id| id.trim().is_empty()) {
            return Err(self.0.error(ErrorKind::InvalidInput, "`labelIds` has a blank id"));
        }
        let request = with_query(RawRequest::get(self.0.on(GMAIL, MESSAGES)), &options);
        let request = self.0.paged(request, &paging, "maxResults", 500)?;
        let page: Page<GmailMessageRef> = self.0.page(self.0.send(request).await?, "messages", "messages")?;
        if page.items.iter().any(|message| message.id.is_empty()) {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a message"));
        }
        Ok(page)
    }

    /// Gets one message, decoded: its headers, its body as plain text and
    /// as HTML, and what is attached, without the files.
    pub async fn get(&self, message: &str, options: GmailGetMessage) -> Result<GmailMessage> {
        let request = with_query(RawRequest::get(self.item(message)?), &options);
        Ok(self.wire(self.0.send(request).await?)?.read())
    }

    /// Gets the content of one attachment, as Gmail sends it: base64 in the
    /// URL-safe alphabet, with the size of the file it holds.
    ///
    /// The transport reads an answer of at most 10 MB, and base64 is a third
    /// larger than the file, so a file of more than about 7 MB cannot be
    /// fetched this way yet and fails with `Decode`. Larger files wait for a
    /// request that returns content as it is (issue #6).
    pub async fn attachment_get(&self, message: &str, attachment: &str) -> Result<GmailAttachmentBody> {
        let attachment = self.0.segment("an attachment id", attachment)?;
        let path = format!("{}/attachments/{attachment}", self.item(message)?);
        let body = self.0.send(RawRequest::get(path)).await?;
        // An empty file has a size and no data. An answer with neither is not an attachment.
        let named = body.get("size").is_some() || body.get("data").is_some();
        let content: GmailAttachmentBody = self.0.decode(body, "an attachment")?;
        if !named || (content.data.is_empty() && content.size != 0) {
            return Err(self.0.error(ErrorKind::Decode, "google answered without an attachment"));
        }
        Ok(content)
    }

    /// Adds labels to a message and removes others. Archiving removes
    /// `INBOX`, marking as read removes `UNREAD`, starring adds `STARRED`.
    pub async fn modify(&self, message: &str, changes: GmailModifyMessage) -> Result<GmailMessageRef> {
        let lists = [
            ("addLabelIds", &changes.add_label_ids),
            ("removeLabelIds", &changes.remove_label_ids),
        ];
        for (field, ids) in lists {
            let ids = ids.as_deref().unwrap_or_default();
            if ids.iter().any(|id| id.trim().is_empty()) {
                return Err(self
                    .0
                    .error(ErrorKind::InvalidInput, format!("`{field}` has a blank id")));
            }
            if ids.len() > 100 {
                return Err(self
                    .0
                    .error(ErrorKind::InvalidInput, format!("`{field}` takes at most 100 labels")));
            }
        }
        if lists.iter().all(|(_, ids)| ids.as_ref().is_none_or(Vec::is_empty)) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "a change needs `addLabelIds` or `removeLabelIds`",
            ));
        }
        let path = format!("{}/modify", self.item(message)?);
        // A list with nothing in it asks for nothing, and is not sent.
        let changes = GmailModifyMessage {
            add_label_ids: changes.add_label_ids.filter(|ids| !ids.is_empty()),
            remove_label_ids: changes.remove_label_ids.filter(|ids| !ids.is_empty()),
        };
        let request = RawRequest::post(path, Value::Object(set(&changes)));
        let body = self.0.send(request).await?;
        self.reference(body)
    }

    /// Moves a message to the bin. [`GmailMessages::untrash`] brings it back,
    /// for as long as Gmail keeps what is in the bin.
    pub async fn trash(&self, message: &str) -> Result<GmailMessageRef> {
        self.act(message, "trash").await
    }

    /// Takes a message out of the bin.
    pub async fn untrash(&self, message: &str) -> Result<GmailMessageRef> {
        self.act(message, "untrash").await
    }

    /// Asks Gmail to do to a message something that takes no content.
    async fn act(&self, message: &str, action: &str) -> Result<GmailMessageRef> {
        // Google wants a length on a POST. One with no body goes out without
        // a length, so an empty object is sent: it has one and says nothing.
        let request = RawRequest::post(format!("{}/{action}", self.item(message)?), json!({}));
        let body = self.0.send(request).await?;
        self.reference(body)
    }

    /// The address of one message.
    pub(super) fn item(&self, message: &str) -> Result<String> {
        let message = self.0.segment("a message id", message)?;
        Ok(self.0.on(GMAIL, &format!("{MESSAGES}/{message}")))
    }

    /// A message in Gmail's own shape, once it is known to be one.
    pub(super) fn wire(&self, body: Value) -> Result<GmailWireMessage> {
        let message: GmailWireMessage = self.0.decode(body, "a message")?;
        if message.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a message"));
        }
        Ok(message)
    }

    /// What Gmail answers after sending or changing a message: its ids and labels.
    pub(super) fn reference(&self, body: Value) -> Result<GmailMessageRef> {
        let message: GmailMessageRef = self.0.decode(body, "a message")?;
        if message.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a message"));
        }
        Ok(message)
    }
}
