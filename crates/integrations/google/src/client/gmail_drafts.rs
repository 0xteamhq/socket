//! Gmail drafts: messages that are written and not yet sent.
//!
//! Sending a draft is `gmail_messages.send_draft`: it is what puts a
//! message in someone's inbox, and it sits with the other ways of doing that.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, GMAIL, with_query};
use crate::models::{
    GmailDraft, GmailDraftRef, GmailGetMessage, GmailListDrafts, GmailSendMessage, GmailWireDraft, Paging, gmail_raw,
};

const DRAFTS: &str = "gmail/v1/users/me/drafts";

/// The drafts of a Gmail mailbox.
#[derive(Debug, Clone, Copy)]
pub struct GmailDrafts<'a>(pub(crate) Api<'a>);

impl GmailDrafts<'_> {
    /// Lists the drafts of the mailbox.
    ///
    /// The list is light: each draft is its id and the ids of the message
    /// it holds, and nothing is fetched for each row. Read one with
    /// [`GmailDrafts::get`].
    pub async fn list(&self, options: GmailListDrafts, paging: Paging) -> Result<Page<GmailDraftRef>> {
        let options = GmailListDrafts {
            q: options.q.filter(|q| !q.trim().is_empty()),
            ..options
        };
        let request = with_query(RawRequest::get(self.0.on(GMAIL, DRAFTS)), &options);
        let request = self.0.paged(request, &paging, "maxResults", 500)?;
        let page: Page<GmailDraftRef> = self.0.page(self.0.send(request).await?, "drafts", "drafts")?;
        if page.items.iter().any(|draft| draft.id.is_empty()) {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a draft"));
        }
        Ok(page)
    }

    /// Gets one draft, with its message decoded as a message that was
    /// received is.
    pub async fn get(&self, draft: &str, options: GmailGetMessage) -> Result<GmailDraft> {
        let request = with_query(RawRequest::get(self.item(draft)?), &options);
        let draft = self
            .0
            .decode::<GmailWireDraft>(self.0.send(request).await?, "a draft")?
            .read();
        if draft.id.is_empty() || draft.message.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a draft"));
        }
        Ok(draft)
    }

    /// Saves a new draft. Nothing is sent, and nothing is required: a draft
    /// can be begun empty and filled in with [`GmailDrafts::update`].
    pub async fn create(&self, content: GmailSendMessage) -> Result<GmailDraftRef> {
        let request = RawRequest::post(self.0.on(GMAIL, DRAFTS), self.draft(&content)?);
        let body = self.0.send(request).await?;
        self.reference(body)
    }

    /// Replaces everything a draft says with `content`. What is not given
    /// again is gone: Gmail does not change part of a draft.
    pub async fn update(&self, draft: &str, content: GmailSendMessage) -> Result<GmailDraftRef> {
        let request = RawRequest::new("PUT", self.item(draft)?).with_body(self.draft(&content)?);
        let body = self.0.send(request).await?;
        self.reference(body)
    }

    /// Deletes a draft for good. It does not go to the bin.
    pub async fn delete(&self, draft: &str) -> Result<()> {
        self.0
            .send(RawRequest::new("DELETE", self.item(draft)?))
            .await
            .map(drop)
    }

    /// The address of one draft.
    fn item(&self, draft: &str) -> Result<String> {
        let draft = self.0.segment("a draft id", draft)?;
        Ok(self.0.on(GMAIL, &format!("{DRAFTS}/{draft}")))
    }

    /// `content` as Gmail takes a draft: the whole message, written as mail
    /// travels, inside a draft resource.
    fn draft(&self, content: &GmailSendMessage) -> Result<Value> {
        let raw = gmail_raw(content, None).map_err(|problem| self.0.error(ErrorKind::InvalidInput, problem))?;
        Ok(json!({ "message": { "raw": raw } }))
    }

    fn reference(&self, body: Value) -> Result<GmailDraftRef> {
        let draft: GmailDraftRef = self.0.decode(body, "a draft")?;
        if draft.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a draft"));
        }
        Ok(draft)
    }
}
