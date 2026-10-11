//! Google Docs: a document's tabs, what is written in it, and adding to it.
//!
//! A document is asked for in two ways. `get` names the few fields it wants,
//! so a long document answers in a few hundred bytes. `read` asks for
//! everything, tabs included, and returns it as text; turning Google's
//! structure into text is in `models/document_text.rs`.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, DOCS};
use crate::models::{DocsAppendText, DocsCreateDocument, Document, DocumentResource, DocumentText, DocumentUpdate};

/// How many tabs deep `get` asks for: a tab, and three inside one another
/// under it. A field mask cannot say "at every depth", and naming
/// `childTabs` alone would bring each child's whole content, so each depth
/// is written out. A tab deeper than this is not listed.
const TABS_DEEP: usize = 4;

/// The fields `get` asks for: what names a document, and what names each of
/// its tabs. Nothing that is written in them.
fn light() -> String {
    let mut tabs = "tabProperties".to_owned();
    for _ in 1..TABS_DEEP {
        tabs = format!("tabProperties,childTabs({tabs})");
    }
    format!("documentId,title,revisionId,tabs({tabs})")
}

/// Google Docs documents.
#[derive(Debug, Clone, Copy)]
pub struct DocsDocuments<'a>(pub(crate) Api<'a>);

impl DocsDocuments<'_> {
    /// Gets a document's title, its revision and its tabs, without what is
    /// written in it.
    pub async fn get(&self, document: &str) -> Result<Document> {
        // Google fills in `tabs` only when asked to. The mask keeps what is
        // written in them out of the answer.
        let request = RawRequest::get(self.path(document)?)
            .with_query("includeTabsContent", "true")
            .with_query("fields", light());
        self.document(self.0.send(request).await?)
    }

    /// Reads a document as plain text: the whole of it, and each tab's own.
    ///
    /// The document is read as it stands. What someone has only suggested
    /// adding is left out, and what someone has only suggested deleting is
    /// kept. Google's own default depends on whether the account may edit
    /// the document, so this asks for the one that does not.
    pub async fn read(&self, document: &str) -> Result<DocumentText> {
        let request = RawRequest::get(self.path(document)?)
            .with_query("includeTabsContent", "true")
            .with_query("suggestionsViewMode", "PREVIEW_WITHOUT_SUGGESTIONS");
        let body = self.0.send(request).await?;
        let document: DocumentResource = self.0.decode(body, "a document")?;
        let read = DocumentText::from(document);
        if read.document_id.is_empty() {
            return Err(self.missing());
        }
        Ok(read)
    }

    /// Creates a blank document with a title. Google takes nothing else
    /// here; text is added afterwards with [`DocsDocuments::append_text`].
    pub async fn create(&self, document: DocsCreateDocument) -> Result<Document> {
        self.0.required("`title`", &document.title)?;
        let request = RawRequest::post(self.0.on(DOCS, "v1/documents"), json!({ "title": document.title }))
            .with_query("fields", light());
        self.document(self.0.send(request).await?)
    }

    /// Adds text at the end of a document, or of one of its tabs. Nothing
    /// that was there is changed.
    pub async fn append_text(&self, document: &str, append: DocsAppendText) -> Result<DocumentUpdate> {
        if append.text.is_empty() {
            return Err(self.0.error(ErrorKind::InvalidInput, "`text` is required"));
        }
        // The end of the body, in the first tab unless another is named.
        // A tab that is named and blank is a mistake, not the first tab:
        // the text would go somewhere the caller did not mean.
        let mut end = Map::new();
        if let Some(tab) = append.tab_id.as_deref().map(str::trim) {
            self.0.required("`tabId`", tab)?;
            end.insert("tabId".to_owned(), Value::from(tab));
        }
        let insert = json!({ "insertText": { "text": append.text, "endOfSegmentLocation": end } });
        let path = format!("{}:batchUpdate", self.path(document)?);
        let body = self
            .0
            .send(RawRequest::post(path, json!({ "requests": [insert] })))
            .await?;
        let update: DocumentUpdate = self.0.decode(body, "a document")?;
        if update.document_id.is_empty() {
            return Err(self.missing());
        }
        Ok(update)
    }

    /// The path of one document.
    fn path(&self, document: &str) -> Result<String> {
        let document = self.0.segment("a document id", document)?;
        Ok(self.0.on(DOCS, &format!("v1/documents/{document}")))
    }

    fn document(&self, body: Value) -> Result<Document> {
        let document: DocumentResource = self.0.decode(body, "a document")?;
        let document = Document::from(document);
        if document.document_id.is_empty() {
            return Err(self.missing());
        }
        Ok(document)
    }

    fn missing(&self) -> socketkit_core::Error {
        self.0.error(ErrorKind::Decode, "google answered without a document")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_fields_name_every_tab_and_none_of_their_content() {
        assert_eq!(
            light(),
            "documentId,title,revisionId,tabs(tabProperties,childTabs(tabProperties,childTabs(tabProperties,childTabs(tabProperties))))"
        );
        assert!(!light().contains("documentTab"));
        assert!(!light().contains("body"));
    }
}
