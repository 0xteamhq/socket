//! A reply to a Gmail message: what the caller writes, and what is taken
//! from the message that is answered.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gmail_address::listed;
use super::gmail_rfc2822::message_ids;
use super::{GmailAddress, GmailSendMessage, GmailThreading, GmailWireMessage};

/// The content of a reply. Only the body is needed: who it goes to and its
/// subject are taken from the message that is answered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailReply {
    /// Who the reply goes to. When not given, the address the original asks
    /// replies to go to (`Reply-To`), or else its sender (`From`).
    pub to: Option<Vec<GmailAddress>>,
    /// Who receives a copy. Nobody when not given: the people in copy on
    /// the original are not added.
    pub cc: Option<Vec<GmailAddress>>,
    /// Who receives a copy the others are not told about.
    pub bcc: Option<Vec<GmailAddress>>,
    /// The subject. When not given, the original's with `Re: ` before it.
    /// Gmail keeps a reply in its thread only while the subjects match.
    pub subject: Option<String>,
    /// The body as plain text.
    pub text: Option<String>,
    /// The body as HTML.
    pub html: Option<String>,
}

impl GmailReply {
    /// The message to send in answer to `original`. The error says what the
    /// caller has to give because the original does not.
    pub(crate) fn into_message(self, original: &GmailWireMessage) -> Result<GmailSendMessage, String> {
        let to = match self.to.filter(|to| !to.is_empty()) {
            Some(to) => to,
            None => {
                let asked = original.header("Reply-To").map(listed).unwrap_or_default();
                let to = if asked.is_empty() {
                    original.header("From").map(listed).unwrap_or_default()
                } else {
                    asked
                };
                if to.is_empty() {
                    return Err("the message being answered names no one to reply to; give `to`".to_owned());
                }
                to
            }
        };
        let subject = match self.subject.filter(|subject| !subject.trim().is_empty()) {
            Some(subject) => subject,
            None => {
                let subject = original.subject().unwrap_or_default();
                let answered = subject.get(..3).is_some_and(|start| start.eq_ignore_ascii_case("re:"));
                if answered {
                    subject
                } else {
                    format!("Re: {subject}").trim_end().to_owned()
                }
            }
        };
        Ok(GmailSendMessage {
            to: Some(to),
            cc: self.cc,
            bcc: self.bcc,
            subject: Some(subject),
            text: self.text,
            html: self.html,
        })
    }
}

impl GmailWireMessage {
    /// What a reply to this message carries so that every mail program
    /// files it in the same thread: this message's id, after the ids of the
    /// thread before it. `None` when the message has no `Message-ID`.
    pub(crate) fn threading(&self) -> Option<GmailThreading> {
        let in_reply_to = message_ids(self.header("Message-ID")?).into_iter().next()?;
        let mut references = self.header("References").map(message_ids).unwrap_or_default();
        if references.is_empty() {
            // A message that answers another without listing the thread.
            references = self.header("In-Reply-To").map(message_ids).unwrap_or_default();
        }
        if references.last() != Some(&in_reply_to) {
            references.push(in_reply_to.clone());
        }
        Some(GmailThreading {
            in_reply_to,
            references,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn original(headers: Value) -> GmailWireMessage {
        let headers: Vec<Value> = headers
            .as_object()
            .unwrap()
            .iter()
            .map(|(name, value)| json!({ "name": name, "value": value }))
            .collect();
        serde_json::from_value(json!({ "id": "m2", "threadId": "t1", "payload": { "headers": headers } })).unwrap()
    }

    fn answer(headers: Value, reply: GmailReply) -> GmailSendMessage {
        reply.into_message(&original(headers)).unwrap()
    }

    #[test]
    fn a_reply_goes_where_the_original_asks_or_else_to_its_sender() {
        let sender = json!({ "From": "Grace Hopper <grace@example.test>", "Subject": "Plan" });
        let sent = answer(sender.clone(), GmailReply::default());
        assert_eq!(
            sent.to,
            Some(vec![GmailAddress::named("grace@example.test", "Grace Hopper")])
        );
        assert_eq!((sent.cc, sent.bcc), (None, None));

        let list = json!({ "From": "grace@example.test", "reply-to": "Team <team@example.test>, lead@example.test" });
        assert_eq!(
            answer(list, GmailReply::default()).to,
            Some(vec![
                GmailAddress::named("team@example.test", "Team"),
                GmailAddress::new("lead@example.test")
            ])
        );
        // A Reply-To with no mailbox in it does not leave the reply without one.
        let empty = json!({ "From": "grace@example.test", "Reply-To": "undisclosed-recipients:;" });
        assert_eq!(
            answer(empty, GmailReply::default()).to,
            Some(vec![GmailAddress::new("grace@example.test")])
        );

        let chosen = GmailReply {
            to: Some(vec![GmailAddress::new("alan@example.test")]),
            cc: Some(vec![GmailAddress::new("ada@example.test")]),
            ..GmailReply::default()
        };
        let sent = answer(sender, chosen);
        assert_eq!(sent.to, Some(vec![GmailAddress::new("alan@example.test")]));
        assert_eq!(sent.cc, Some(vec![GmailAddress::new("ada@example.test")]));

        for nobody in [json!({}), json!({ "From": "not an address" })] {
            let refused = GmailReply::default().into_message(&original(nobody)).unwrap_err();
            assert!(refused.contains("`to`"), "{refused}");
        }
    }

    #[test]
    fn a_reply_takes_the_subject_with_one_re_before_it() {
        for (subject, expected) in [
            ("Plan", "Re: Plan"),
            ("Re: Plan", "Re: Plan"),
            ("RE: Plan", "RE: Plan"),
            ("re:Plan", "re:Plan"),
            ("Report", "Re: Report"),
            ("=?UTF-8?B?UGzDpG5l?=", "Re: Pläne"),
            ("Ré", "Re: Ré"),
        ] {
            let sent = answer(
                json!({ "From": "grace@example.test", "Subject": subject }),
                GmailReply::default(),
            );
            assert_eq!(sent.subject.as_deref(), Some(expected), "{subject}");
        }
        let untitled = answer(json!({ "From": "grace@example.test" }), GmailReply::default());
        assert_eq!(untitled.subject.as_deref(), Some("Re:"));
        let own = GmailReply {
            subject: Some("Another matter".into()),
            ..GmailReply::default()
        };
        let sent = answer(json!({ "From": "grace@example.test", "Subject": "Plan" }), own);
        assert_eq!(sent.subject.as_deref(), Some("Another matter"));
    }

    #[test]
    fn a_reply_lists_the_thread_before_it_and_then_the_message_it_answers() {
        let read = |headers: Value| {
            let thread = original(headers).threading().unwrap();
            (thread.in_reply_to, thread.references.join(" "))
        };
        assert_eq!(
            read(json!({ "Message-ID": "<m2@x.test>", "References": "<m0@x.test>\r\n <m1@x.test>" })),
            (
                "<m2@x.test>".to_owned(),
                "<m0@x.test> <m1@x.test> <m2@x.test>".to_owned()
            )
        );
        // The first message of a thread, written with another spelling of the header.
        assert_eq!(
            read(json!({ "Message-Id": "<m0@x.test>" })),
            ("<m0@x.test>".to_owned(), "<m0@x.test>".to_owned())
        );
        assert_eq!(
            read(json!({ "message-id": " <m2@x.test> ", "In-Reply-To": "<m1@x.test>" })),
            ("<m2@x.test>".to_owned(), "<m1@x.test> <m2@x.test>".to_owned())
        );
        for missing in [
            json!({}),
            json!({ "Message-ID": "m2@x.test" }),
            json!({ "Message-ID": "<m2@x.test\r\nBcc: eve@x.test>" }),
        ] {
            assert!(original(missing).threading().is_none());
        }
    }
}
