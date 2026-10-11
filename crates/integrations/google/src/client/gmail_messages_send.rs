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
    /// The reply goes to the people in `reply.to`, which has to name at
    /// least one, and to nobody else. Nothing in the original decides who it
    /// goes to. A person approves a reply by the input they are shown, so
    /// the recipients are in that input. What the original says about where
    /// its answers should go, its `From` and its `Reply-To`, is the sender's
    /// own text: mail that seems to come from a colleague can ask for its
    /// answers to go to a stranger. It is there to read with
    /// [`GmailMessages::get`] and decide on, never to act on unseen.
    ///
    /// The original's headers are read first, which is one more request.
    /// They give the reply what keeps it in the thread, for Gmail and for
    /// whoever receives it: the original's `threadId`, its `Message-ID` in
    /// `In-Reply-To`, and the thread so far in `References`. They give the
    /// subject too, when `reply.subject` is not set: the original's with
    /// `Re: ` before it, once. An original that carries no `Message-ID`
    /// cannot be answered in its thread, and is refused with `InvalidInput`.
    pub async fn reply(&self, message: &str, reply: GmailReply) -> Result<GmailMessageRef> {
        let invalid = |problem: String| self.0.error(ErrorKind::InvalidInput, problem);
        if reply.to.is_empty() {
            return Err(invalid(
                "a reply needs at least one recipient in `to`: the message it answers does not decide who it goes to"
                    .to_owned(),
            ));
        }
        let said = |body: &Option<String>| body.as_deref().is_some_and(|body| !body.trim().is_empty());
        if !said(&reply.text) && !said(&reply.html) {
            return Err(invalid("a reply needs `text` or `html`".to_owned()));
        }
        // What the caller wrote is checked before the original is read, so
        // that a reply that could never be sent costs Gmail nothing.
        let own = GmailSendMessage {
            to: Some(reply.to.clone()),
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
        let thread_id = original.thread_id.clone().filter(|id| !id.is_empty());
        let thread_id = thread_id.ok_or_else(|| {
            let said = "google answered without the thread of the message being answered";
            self.0.error(ErrorKind::Decode, said)
        })?;
        // Not Google's failure: the message itself has no id to answer.
        let threading = original.threading().ok_or_else(|| {
            invalid(
                "the message being answered carries no `Message-ID` a reply can name, so it cannot be answered in its \
                 thread; nothing was sent"
                    .to_owned(),
            )
        })?;
        let content = reply.into_message(&original);
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
