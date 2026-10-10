//! A person's mail: writing drafts, and sending.
//!
//! These are methods of [`Mail`], whose struct and reading methods are in
//! `mail.rs`.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::mail::Mail;
use super::with;
use crate::models::{DraftMessage, Message, ReplyContent, SendMail};

/// Whether there is any text in what was given.
fn said(text: Option<&str>) -> bool {
    text.is_some_and(|text| !text.trim().is_empty())
}

impl Mail<'_> {
    /// Saves a new message in Drafts. Nothing is sent.
    pub async fn create_draft(&self, draft: DraftMessage) -> Result<Message> {
        let body = self
            .0
            .send(RawRequest::post("me/messages", self.draft(&draft)?))
            .await?;
        self.message(body)
    }

    /// Changes a draft. Only what is set in `changes` is replaced.
    pub async fn update_draft(&self, message: &str, changes: DraftMessage) -> Result<Message> {
        let path = self.item(message)?;
        let body = self.0.send(self.patch(path, self.draft(&changes)?)?).await?;
        self.message(body)
    }

    /// Saves a reply to the sender of a message as a draft. Nothing is sent.
    pub async fn create_reply(&self, message: &str, reply: ReplyContent) -> Result<Message> {
        self.answer_draft(message, "createReply", self.answer(&reply)?).await
    }

    /// Saves a reply to everyone on a message as a draft. Nothing is sent.
    pub async fn create_reply_all(&self, message: &str, reply: ReplyContent) -> Result<Message> {
        self.answer_draft(message, "createReplyAll", self.answer(&reply)?).await
    }

    /// Saves a forward of a message as a draft, addressed to
    /// `forward.message.to_recipients`. Nothing is sent.
    pub async fn create_forward(&self, message: &str, forward: ReplyContent) -> Result<Message> {
        if forward.message.to_recipients.as_ref().is_none_or(Vec::is_empty) {
            return Err(self.0.error(ErrorKind::InvalidInput, "a forward needs `toRecipients`"));
        }
        self.answer_draft(message, "createForward", self.answer(&forward)?)
            .await
    }

    /// Sends a message at once. It cannot be taken back.
    pub async fn send(&self, mail: SendMail) -> Result<()> {
        let message = &mail.message;
        let addressed = [&message.to_recipients, &message.cc_recipients, &message.bcc_recipients]
            .into_iter()
            .flatten()
            .any(|recipients| !recipients.is_empty());
        if !addressed {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "a message needs at least one recipient"));
        }
        // A draft may be empty. What is sent at once, and cannot be taken
        // back, has to say something.
        if !said(message.subject.as_deref()) && message.body.is_none() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "a message needs a `subject` or a `body`"));
        }
        let mut body = json!({ "message": self.draft(message)? });
        if let Some(keep) = mail.save_to_sent_items {
            body["saveToSentItems"] = json!(keep);
        }
        self.0.send(RawRequest::post("me/sendMail", body)).await.map(drop)
    }

    /// Sends a draft as it stands. It cannot be taken back.
    pub async fn send_draft(&self, message: &str) -> Result<()> {
        // Graph wants a length on this request. A POST with no body goes out
        // without one, so an empty object is sent: it has a length and says nothing.
        let request = RawRequest::post(format!("{}/send", self.item(message)?), json!({}));
        self.0.send(request).await.map(drop)
    }

    /// Replies to the sender of a message, and sends the reply at once. It
    /// cannot be taken back.
    pub async fn reply(&self, message: &str, reply: ReplyContent) -> Result<()> {
        if !said(reply.comment.as_deref()) && reply.message.body.is_none() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "a reply needs a `comment` or a `body`"));
        }
        let request = RawRequest::post(format!("{}/reply", self.item(message)?), self.answer(&reply)?);
        self.0.send(request).await.map(drop)
    }

    /// The content of a message as Graph takes it, once it is known that
    /// every recipient has an address and a body has its text.
    fn draft(&self, draft: &DraftMessage) -> Result<Value> {
        for recipients in [
            &draft.to_recipients,
            &draft.cc_recipients,
            &draft.bcc_recipients,
            &draft.reply_to,
        ] {
            self.0.recipients(recipients.as_deref())?;
        }
        self.0.body(draft.body.as_ref())?;
        Ok(with(json!({}), draft))
    }

    /// The body of a reply or a forward: a comment, or changes to the
    /// message, or neither.
    fn answer(&self, content: &ReplyContent) -> Result<Value> {
        if content.comment.is_some() && content.message.body.is_some() {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "give `comment` or `body`, not both: Graph refuses a reply that has the two",
            ));
        }
        let mut body = Map::new();
        if let Some(comment) = &content.comment {
            body.insert("comment".to_owned(), json!(comment));
        }
        let message = self.draft(&content.message)?;
        if message.as_object().is_some_and(|fields| !fields.is_empty()) {
            body.insert("message".to_owned(), message);
        }
        Ok(Value::Object(body))
    }

    async fn answer_draft(&self, message: &str, action: &str, content: Value) -> Result<Message> {
        let request = RawRequest::post(format!("{}/{action}", self.item(message)?), content);
        let body = self.0.send(request).await?;
        self.message(body)
    }
}
