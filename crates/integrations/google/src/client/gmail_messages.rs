//! Gmail messages: finding and reading them, and labelling them and moving
//! them to the bin.
//!
//! Sending is in `gmail_messages_send.rs`; those are methods of the same
//! group, kept apart only for the length of the file.
//!
//! Every path here starts at `users/me`, the mailbox of the account that
//! connected. [`GmailMessages::item`] is the one place that says so.

use serde_json::{Value, json};
use socketkit_core::{Content, ContentRequest, ErrorKind, Page, RawRequest, Result};

use super::{Api, GMAIL, set, with_query};
use crate::models::{
    Download, GmailAttachmentText, GmailGetMessage, GmailListMessages, GmailMessage, GmailMessageRef,
    GmailModifyMessage, GmailWireAttachment, GmailWireMessage, Paging, TextLimit,
};

pub(super) const MESSAGES: &str = "gmail/v1/users/me/messages";

/// The most that Gmail writes around a file when it answers with one: the
/// names and punctuation of its JSON, and the attachment's id, which can be
/// a thousand characters long.
const ATTACHMENT_ENVELOPE: usize = 8 * 1024;

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
    /// as HTML, and what is attached, without the files. A file is read
    /// with [`GmailMessages::attachment_content`], or with
    /// [`GmailMessages::attachment_text`] when it is text.
    pub async fn get(&self, message: &str, options: GmailGetMessage) -> Result<GmailMessage> {
        let request = with_query(RawRequest::get(self.item(message)?), &options);
        Ok(self.wire(self.0.send(request).await?)?.read())
    }

    /// The attachment itself: the bytes of the file, unchanged.
    ///
    /// Gmail hands a file over inside JSON, in base64, and does not say what
    /// the file is: `content_type` is always `None`. Its type and its name
    /// are on the message, in the entry of `attachments` this id was read
    /// from.
    ///
    /// Ten megabytes are accepted unless `limits` says otherwise, and a
    /// larger file is refused whole with `too_large`, never cut short. The
    /// limit is on the file. What is fetched is larger, by the third that
    /// base64 adds and by what Gmail writes around it, and the fetch is
    /// limited to that, so a file over the limit is not read to its end.
    ///
    /// This is a typed method only: an operation called by name never
    /// returns bytes. [`GmailMessages::attachment_text`] is the one for a
    /// file that is text.
    pub async fn attachment_content(&self, message: &str, attachment: &str, limits: Download) -> Result<Content> {
        let attachment = self.0.segment("an attachment id", attachment)?;
        let path = format!("{}/attachments/{attachment}", self.item(message)?);
        let most = limits.max_bytes.unwrap_or(ContentRequest::DEFAULT_MAX_BYTES);
        let too_large = || {
            let said = format!("google has an attachment larger than the limit of {most} bytes set for this request");
            self.0.error(ErrorKind::TooLarge, said)
        };
        // base64 writes four characters for every three bytes, the last
        // three padded to four.
        let encoded = most.div_ceil(3).saturating_mul(4);
        let fetched = Download {
            max_bytes: Some(encoded.saturating_add(ATTACHMENT_ENVELOPE)),
            timeout_secs: limits.timeout_secs,
        };
        let answer = match self.0.fetch(ContentRequest::get(path), &fetched).await {
            // The limit the caller set is the one to name, not the one derived from it.
            Err(refused) if refused.kind() == ErrorKind::TooLarge => return Err(too_large()),
            answer => answer?,
        };
        // Nothing of an answer that cannot be read goes into the error: it
        // is someone's file. An answer that is not an object is no better
        // read than one that is not JSON.
        let unread = || {
            self.0
                .error(ErrorKind::Decode, "google sent an attachment that could not be read")
        };
        let body: Option<Value> = serde_json::from_slice(&answer.bytes).ok();
        let body = body.filter(Value::is_object).ok_or_else(unread)?;
        drop(answer);
        let found: GmailWireAttachment = self.0.decode(body, "an attachment")?;
        let Some(bytes) = found.into_bytes() else {
            return Err(self.0.error(ErrorKind::Decode, "google answered without an attachment"));
        };
        // The envelope leaves room for a file a little over the limit to
        // arrive whole. It is refused all the same.
        if bytes.len() > most {
            return Err(too_large());
        }
        Ok(Content {
            bytes,
            content_type: None,
        })
    }

    /// An attachment that is text, as text: a CSV file, a text file, a
    /// calendar invitation.
    ///
    /// Gmail does not say what a file is, so the file has to be text by its
    /// own bytes: UTF-8, with no control character but a tab and the ends
    /// of a line or a page. Anything else, such as a PDF, a picture, a file
    /// with a NUL or an escape character in it, or text in another
    /// encoding, is refused with `unsupported`, and nothing of it is
    /// returned. A byte order mark at the start is left out of the text.
    ///
    /// One megabyte is accepted unless `limit` allows more, up to ten, and a
    /// longer file is refused with `too_large`.
    pub async fn attachment_text(
        &self,
        message: &str,
        attachment: &str,
        limit: TextLimit,
    ) -> Result<GmailAttachmentText> {
        let limits = self.0.text_limits(&limit)?;
        let file = self.attachment_content(message, attachment, limits).await?;
        let size = file.len();
        GmailAttachmentText::read(file.bytes).ok_or_else(|| {
            let said = format!(
                "google returned an attachment of {size} bytes, which is not text; an operation called by name returns text only"
            );
            self.0.error(ErrorKind::Unsupported, said)
        })
    }

    /// Adds labels to a message and removes others. Archiving removes
    /// `INBOX`, marking as read removes `UNREAD`, starring adds `STARRED`.
    ///
    /// `TRASH` and `SPAM` cannot be added. Adding `TRASH` is moving the
    /// message to the bin, which is [`GmailMessages::trash`] and asks more of
    /// whoever approves it than a change of labels does. Marking a message as
    /// spam is not offered. Either can be removed.
    pub async fn modify(&self, message: &str, changes: GmailModifyMessage) -> Result<GmailMessageRef> {
        let adds = |label: &str| {
            let mut added = changes.add_label_ids.iter().flatten();
            added.any(|id| id.trim().eq_ignore_ascii_case(label))
        };
        if adds("TRASH") {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`addLabelIds` cannot hold `TRASH`: use gmail_messages.trash to move a message to the bin",
            ));
        }
        if adds("SPAM") {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`addLabelIds` cannot hold `SPAM`: marking a message as spam is not offered",
            ));
        }
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
