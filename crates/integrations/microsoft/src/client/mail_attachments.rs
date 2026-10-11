//! A person's mail: what is attached to a message.
//!
//! These are methods of [`Mail`], whose struct and reading methods are in
//! `mail.rs`.

use socketkit_core::{Content, ContentRequest, ErrorKind, Page, RawRequest, Result};

use super::mail::Mail;
use crate::models::{Attachment, AttachmentText, Download, Paging, TextLimit};

/// What describes an attachment, without the file itself. A list that
/// carried every file would be as large as all of them together.
const ATTACHMENT_FIELDS: &str = "id,name,contentType,size,isInline,lastModifiedDateTime";

impl Mail<'_> {
    /// Lists what is attached to a message: names, types and sizes, without
    /// the files themselves.
    pub async fn attachments_list(&self, message: &str, paging: Paging) -> Result<Page<Attachment>> {
        let request =
            RawRequest::get(format!("{}/attachments", self.item(message)?)).with_query("$select", ATTACHMENT_FIELDS);
        self.0.page(request, &paging, "attachments").await
    }

    /// Describes one attachment: its name, type and size. The file itself is
    /// [`Mail::attachment_content`], or [`Mail::attachment_text`] when it is text.
    pub async fn attachment_get(&self, message: &str, attachment: &str) -> Result<Attachment> {
        let request = RawRequest::get(self.attachment(message, attachment)?).with_query("$select", ATTACHMENT_FIELDS);
        let attachment: Attachment = self.0.decode(self.0.send(request).await?, "an attachment")?;
        if attachment.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "microsoft answered without an attachment"));
        }
        Ok(attachment)
    }

    /// The attachment itself: the bytes of a file, unchanged, with its type.
    /// An attached message, event or contact comes as Graph writes it out,
    /// in MIME. A link to a file kept elsewhere has no content, and is refused.
    ///
    /// Ten megabytes are read unless `limits` allows more, and a larger file
    /// is refused whole. This is a typed method only: an operation called by
    /// name never returns bytes.
    pub async fn attachment_content(&self, message: &str, attachment: &str, limits: Download) -> Result<Content> {
        let path = format!("{}/$value", self.attachment(message, attachment)?);
        self.0.fetch(path, &limits).await
    }

    /// An attachment that is text, as text: a CSV file, a text file, a
    /// calendar invitation. Anything Graph does not serve as text is refused
    /// with `unsupported`, and nothing of it is returned.
    ///
    /// One megabyte is read unless `limit` allows more, up to ten.
    pub async fn attachment_text(&self, message: &str, attachment: &str, limit: TextLimit) -> Result<AttachmentText> {
        let most = limit.max_bytes.unwrap_or(Content::MAX_INLINE_BYTES);
        if most > ContentRequest::DEFAULT_MAX_BYTES {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                format!(
                    "`maxBytes` can be at most {} for text",
                    ContentRequest::DEFAULT_MAX_BYTES
                ),
            ));
        }
        let limits = Download {
            max_bytes: Some(most),
            timeout_secs: None,
        };
        let content = self.attachment_content(message, attachment, limits).await?;
        let content_type = content.content_type.clone();
        let text = content.into_text(&self.0.connection.provider().id)?;
        Ok(AttachmentText { content_type, text })
    }

    /// The path of one attachment of one message.
    fn attachment(&self, message: &str, attachment: &str) -> Result<String> {
        let attachment = self.0.segment("an attachment id", attachment)?;
        Ok(format!("{}/attachments/{attachment}", self.item(message)?))
    }
}
