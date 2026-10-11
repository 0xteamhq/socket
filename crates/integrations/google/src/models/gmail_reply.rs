//! A reply to a Gmail message: what the caller writes, and what is taken
//! from the message that is answered.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gmail_rfc2822::message_ids;
use super::{GmailAddress, GmailSendMessage, GmailThreading, GmailWireMessage};

/// The content of a reply: who it goes to and what it says. The thread it
/// stays in, and its subject when none is given, are taken from the message
/// that is answered. Nothing else is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct GmailReply {
    /// Who the reply goes to: at least one person, always given. Nothing in
    /// the message that is answered decides it. A reply is approved by the
    /// input a person is shown, so the recipients are in that input. What
    /// the message says about where its answers should go, its `from` and
    /// its `replyTo`, is the sender's own text: read it with
    /// `gmail_messages.get`, decide, and name the people here.
    pub to: Vec<GmailAddress>,
    /// Who receives a copy. Nobody when not given: the people in copy on
    /// the original are not added.
    #[serde(default)]
    pub cc: Option<Vec<GmailAddress>>,
    /// Who receives a copy the others are not told about.
    #[serde(default)]
    pub bcc: Option<Vec<GmailAddress>>,
    /// The subject. When not given, the original's with `Re: ` before it.
    /// Gmail keeps a reply in its thread only while the subjects match.
    #[serde(default)]
    pub subject: Option<String>,
    /// The body as plain text.
    #[serde(default)]
    pub text: Option<String>,
    /// The body as HTML.
    #[serde(default)]
    pub html: Option<String>,
}

impl GmailReply {
    /// The message to send in answer to `original`. The original gives the
    /// subject when the caller gave none, and nothing else: who the message
    /// goes to is what the caller wrote.
    pub(crate) fn into_message(self, original: &GmailWireMessage) -> GmailSendMessage {
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
        GmailSendMessage {
            to: Some(self.to),
            cc: self.cc,
            bcc: self.bcc,
            subject: Some(subject),
            text: self.text,
            html: self.html,
        }
    }
}

impl GmailWireMessage {
    /// What a reply to this message carries so that every mail program
    /// files it in the same thread: this message's id, after the ids of the
    /// thread before it. `None` when the message carries no `Message-ID`
    /// that can be written into a header.
    ///
    /// The id is read from the `Message-ID` that `read` shows: the first
    /// that says anything.
    pub(crate) fn threading(&self) -> Option<GmailThreading> {
        let in_reply_to = message_ids(&self.header("Message-ID")?).into_iter().next()?;
        let listed = |name: &str| self.header(name).map(|value| message_ids(&value));
        let mut references = listed("References").unwrap_or_default();
        if references.is_empty() {
            // A message that answers another without listing the thread.
            references = listed("In-Reply-To").unwrap_or_default();
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

    fn to_alan() -> GmailReply {
        GmailReply {
            to: vec![GmailAddress::new("alan@example.test")],
            ..GmailReply::default()
        }
    }

    fn answer(headers: Value, reply: GmailReply) -> GmailSendMessage {
        reply.into_message(&original(headers))
    }

    #[test]
    fn a_reply_goes_to_the_people_the_caller_named_whatever_the_original_says() {
        // Each of these once decided, or could have decided, who a reply
        // went to: the sender, a second sender, an address to answer to, and
        // a sender written so that two readers found two different people.
        for headers in [
            json!({ "From": "Grace Hopper <grace@example.test>" }),
            json!({ "From": "Grace <grace@corp.test>, eve@evil.test" }),
            json!({ "From": "grace@example.test", "Reply-To": "eve@evil.test" }),
            json!({ "From": "(\\\r) <eve@evil.test>, ) <boss@corp\r.test>" }),
            json!({ "From": "boss@corp.test (<eve@evil.test>", "Sender": "eve@evil.test", "Mail-Followup-To": "eve@evil.test" }),
            json!({}),
        ] {
            let chosen = GmailReply {
                cc: Some(vec![GmailAddress::new("ada@example.test")]),
                ..to_alan()
            };
            let sent = answer(headers.clone(), chosen);
            assert_eq!(sent.to, Some(vec![GmailAddress::new("alan@example.test")]), "{headers}");
            assert_eq!(sent.cc, Some(vec![GmailAddress::new("ada@example.test")]), "{headers}");
            assert_eq!(sent.bcc, None, "{headers}");
        }
    }

    #[test]
    fn who_a_reply_goes_to_is_a_field_that_has_to_be_given() {
        let read = |input: Value| serde_json::from_value::<GmailReply>(input);
        let missing = read(json!({ "text": "Agreed." })).unwrap_err();
        assert!(missing.to_string().contains("missing field `to`"), "{missing}");
        assert!(read(json!({ "to": null, "text": "Agreed." })).is_err());
        let given = read(json!({ "to": [{ "email": "alan@example.test" }] })).unwrap();
        assert_eq!(given, to_alan(), "and nothing else has to be");
        let schema = serde_json::to_value(schemars::schema_for!(GmailReply)).unwrap();
        assert_eq!(schema["required"], json!(["to"]));
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
            // What is not seen in the original's subject is not carried into the reply's.
            ("Pl\u{202e}an\u{200b}", "Re: Pl an"),
        ] {
            let sent = answer(json!({ "From": "grace@example.test", "Subject": subject }), to_alan());
            assert_eq!(sent.subject.as_deref(), Some(expected), "{subject}");
        }
        let untitled = answer(json!({ "From": "grace@example.test" }), to_alan());
        assert_eq!(untitled.subject.as_deref(), Some("Re:"));
        let own = GmailReply {
            subject: Some("Another matter".into()),
            ..to_alan()
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
            json!({ "Message-ID": " \r\n " }),
        ] {
            assert!(original(missing).threading().is_none());
        }
    }

    #[test]
    fn a_reply_names_the_message_id_that_reading_the_message_shows() {
        // A blank `Message-ID` before the real one: reading showed the
        // second, and a reply looked only at the first and found none.
        let wire = |headers: Value| -> GmailWireMessage {
            serde_json::from_value(json!({ "id": "m2", "threadId": "t1", "payload": { "headers": headers } })).unwrap()
        };
        let twice = || {
            wire(json!([
                { "name": "Message-ID", "value": " " },
                { "name": "message-id", "value": "<m2@x.test>" },
                { "name": "References", "value": "" },
                { "name": "references", "value": "<m1@x.test>" }
            ]))
        };
        let thread = twice().threading().unwrap();
        assert_eq!(thread.in_reply_to, "<m2@x.test>");
        assert_eq!(thread.references, ["<m1@x.test>", "<m2@x.test>"]);
        assert_eq!(twice().read().message_id.as_deref(), Some("<m2@x.test>"));

        // A value is unfolded once, for both: a line break inside an id is
        // gone from what is shown and from what a reply names alike.
        let folded = || wire(json!([{ "name": "Message-ID", "value": "<m2@x\r.test>" }]));
        assert_eq!(folded().threading().unwrap().in_reply_to, "<m2@x.test>");
        assert_eq!(folded().read().message_id.as_deref(), Some("<m2@x.test>"));
    }
}
