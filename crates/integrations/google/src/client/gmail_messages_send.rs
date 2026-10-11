//! Gmail messages: sending one, answering one, and sending a draft.
//!
//! These are methods of [`GmailMessages`], whose struct and reading methods
//! are in `gmail_messages.rs`.

use serde_json::json;
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::GMAIL;
use super::gmail_messages::{GmailMessages, MESSAGES};
use crate::models::{GmailMessageRef, GmailReply, GmailSendMessage, gmail_raw, gmail_sendable};

impl GmailMessages<'_> {
    /// Sends a message at once, from the account's own address. It cannot
    /// be taken back.
    ///
    /// The message is written here, as mail travels, from the content given:
    /// nothing in `subject`, in an address or in a name can add a header of
    /// its own, and a field that tries is refused. People in `bcc` are sent
    /// to Gmail with the rest; leaving them out of what the others receive
    /// is Gmail's part.
    pub async fn send(&self, content: GmailSendMessage) -> Result<GmailMessageRef> {
        let invalid = |problem: String| self.0.error(ErrorKind::InvalidInput, problem);
        gmail_sendable(&content).map_err(invalid)?;
        let raw = gmail_raw(&content, None).map_err(invalid)?;
        let request = RawRequest::post(self.0.on(GMAIL, &format!("{MESSAGES}/send")), json!({ "raw": raw }));
        let body = self.0.send(request).await?;
        self.reference(body)
    }

    /// Answers a message, in its thread, and sends the answer at once. It
    /// cannot be taken back.
    ///
    /// The original's headers are read first, which is one more request. The
    /// reply goes to the address the original asks replies to go to
    /// (`Reply-To`), or else to its sender (`From`), unless `reply.to` says
    /// otherwise; nobody else on the original is added. Its subject is the
    /// original's with `Re: ` before it, once. It names the original in
    /// `In-Reply-To` and the thread so far in `References`, and is filed
    /// under the original's `threadId`, which together are what keeps it in
    /// the thread for Gmail and for whoever receives it.
    pub async fn reply(&self, message: &str, reply: GmailReply) -> Result<GmailMessageRef> {
        let invalid = |problem: String| self.0.error(ErrorKind::InvalidInput, problem);
        let said = |body: &Option<String>| body.as_deref().is_some_and(|body| !body.trim().is_empty());
        if !said(&reply.text) && !said(&reply.html) {
            return Err(invalid("a reply needs `text` or `html`".to_owned()));
        }
        // What the caller wrote is checked before the original is read, so
        // that a reply that could never be sent costs Gmail nothing.
        let own = GmailSendMessage {
            to: reply.to.clone(),
            cc: reply.cc.clone(),
            bcc: reply.bcc.clone(),
            subject: reply.subject.clone(),
            ..GmailSendMessage::default()
        };
        gmail_raw(&own, None).map_err(invalid)?;
        // Every header is asked for, not a chosen few: a reply that missed
        // the original's `Message-ID` over its spelling would leave the thread.
        let read = RawRequest::get(self.item(message)?).with_query("format", "metadata");
        let original = self.wire(self.0.send(read).await?)?;
        let missing = |what: &str| {
            let said = format!("google answered without the {what} of the message being answered");
            self.0.error(ErrorKind::Decode, said)
        };
        let thread_id = original.thread_id.clone().filter(|id| !id.is_empty());
        let thread_id = thread_id.ok_or_else(|| missing("thread"))?;
        let threading = original.threading().ok_or_else(|| missing("`Message-ID`"))?;
        let content = reply.into_message(&original).map_err(invalid)?;
        gmail_sendable(&content).map_err(invalid)?;
        let raw = gmail_raw(&content, Some(&threading)).map_err(invalid)?;
        let request = RawRequest::post(
            self.0.on(GMAIL, &format!("{MESSAGES}/send")),
            json!({ "raw": raw, "threadId": thread_id }),
        );
        let body = self.0.send(request).await?;
        self.reference(body)
    }

    /// Sends a draft as it stands, to the people it is addressed to. It
    /// cannot be taken back. Gmail deletes the draft and answers with the
    /// message that was sent, which has an id of its own.
    pub async fn send_draft(&self, draft: &str) -> Result<GmailMessageRef> {
        self.0.required("a draft id", draft)?;
        let request = RawRequest::post(
            self.0.on(GMAIL, "gmail/v1/users/me/drafts/send"),
            json!({ "id": draft.trim() }),
        );
        let body = self.0.send(request).await?;
        self.reference(body)
    }
}
