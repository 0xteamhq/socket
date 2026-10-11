//! A message as Gmail sends it: a tree of MIME parts, each with its own
//! headers and its body in base64. Reading that tree into a [`GmailMessage`]
//! is the work of this file, so that no caller has to.

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::gmail_address::{displayed, listed};
use super::{GmailAddress, GmailAttachment, GmailMessage, gmail_words};

/// A message in Gmail's own shape. It has no `Debug`: it holds someone's mail.
#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct GmailWireMessage {
    pub(crate) id: String,
    pub(crate) thread_id: Option<String>,
    label_ids: Vec<String>,
    snippet: Option<String>,
    history_id: Option<String>,
    internal_date: Option<String>,
    size_estimate: Option<i64>,
    payload: Option<Part>,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Part {
    part_id: Option<String>,
    mime_type: Option<String>,
    filename: Option<String>,
    headers: Vec<Header>,
    body: Option<Body>,
    parts: Vec<Part>,
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct Header {
    name: String,
    value: String,
}

#[derive(Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct Body {
    attachment_id: Option<String>,
    size: Option<i64>,
    data: Option<Bytes>,
}

/// A body's bytes, taken out of base64 while the answer is read. What is
/// not base64 fails there, so the error names the part it was found in
/// (`payload.parts[1].body.data`) and says nothing of what the part held.
struct Bytes(Vec<u8>);

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        gmail_words::from_base64(&text)
            .map(Bytes)
            .ok_or_else(|| D::Error::custom("not base64"))
    }
}

/// What the parts of a message add up to.
#[derive(Default)]
struct Found {
    text: Option<String>,
    html: Option<String>,
    attachments: Vec<GmailAttachment>,
}

impl Found {
    /// Adds what another part of the same message held, after what is here.
    fn add(&mut self, more: Found) {
        for (all, more) in [(&mut self.text, more.text), (&mut self.html, more.html)] {
            match (all.as_mut(), more) {
                (Some(all), Some(more)) => {
                    all.push('\n');
                    all.push_str(&more);
                }
                (None, more @ Some(_)) => *all = more,
                (_, None) => {}
            }
        }
        self.attachments.extend(more.attachments);
    }
}

/// A header's value as a person reads it: unfolded, with its encoded words
/// read. A control character an encoded word was hiding becomes a space.
fn shown(value: &str) -> Option<String> {
    let unfolded = value.replace(['\r', '\n'], "");
    let text: String = gmail_words::decoded(&unfolded)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    Some(text.trim().to_owned()).filter(|text| !text.is_empty())
}

/// One parameter of a header such as `text/plain; charset="utf-8"`.
fn parameter(value: &str, name: &str) -> Option<String> {
    value.split(';').skip(1).find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        key.trim()
            .eq_ignore_ascii_case(name)
            .then(|| value.trim().trim_matches('"').to_owned())
    })
}

impl Part {
    /// Every value of the header `name`, as sent. Mail writes header names
    /// in any case: `Message-ID`, `Message-Id`, `message-id`.
    fn values<'a>(&'a self, name: &str) -> impl Iterator<Item = &'a str> {
        let named = move |header: &&Header| header.name.trim().eq_ignore_ascii_case(name);
        self.headers.iter().filter(named).map(|header| header.value.as_str())
    }

    /// The first value of a header that mail allows only once.
    fn one(&self, name: &str) -> Option<String> {
        self.values(name).find_map(shown)
    }

    /// The people a header names, as a person reads them. Some senders
    /// write a list as several headers; every one of them is shown.
    fn people(&self, name: &str) -> Option<String> {
        let joined = self.values(name).filter_map(displayed).collect::<Vec<_>>().join(", ");
        Some(joined).filter(|joined| !joined.is_empty())
    }

    /// Reads this part and the parts inside it.
    fn found(&self) -> Found {
        let kind = self
            .mime_type
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let mut found = Found::default();
        if kind == "multipart/alternative" {
            // The same content in several forms, the plainest first. The
            // last form that has text is the text, and likewise for HTML.
            for form in self.parts.iter().map(Part::found) {
                found.text = form.text.or(found.text.take());
                found.html = form.html.or(found.html.take());
                found.attachments.extend(form.attachments);
            }
        } else if kind.starts_with("multipart/") {
            self.parts.iter().for_each(|part| found.add(part.found()));
        } else {
            self.leaf(&kind, &mut found);
        }
        found
    }

    /// Reads a part that holds content of its own: a body, or a file.
    fn leaf(&self, kind: &str, found: &mut Found) {
        let body = self.body.as_ref();
        let data = body.and_then(|body| body.data.as_ref()).map(|data| data.0.as_slice());
        let attachment_id = body
            .and_then(|body| body.attachment_id.clone())
            .filter(|id| !id.is_empty());
        let filename = self.filename.as_deref().and_then(shown).unwrap_or_default();
        let disposition = self
            .values("Content-Disposition")
            .next()
            .map(|value| value.split(';').next().unwrap_or_default().trim().to_ascii_lowercase());
        // A text part that has a file name, or is marked as one, is a file
        // someone attached, not something the message says.
        let is_file = disposition.as_deref() == Some("attachment") || !filename.is_empty() || attachment_id.is_some();
        let said = match kind {
            "text/plain" if !is_file => Some(&mut found.text),
            "text/html" if !is_file => Some(&mut found.html),
            _ => None,
        };
        if let Some(said) = said {
            let charset = self
                .values("Content-Type")
                .next()
                .and_then(|value| parameter(value, "charset"));
            *said = data
                .filter(|data| !data.is_empty())
                .map(|data| gmail_words::text_in(charset.as_deref(), data));
            return;
        }
        // A part with no name, no id and nothing in it is not a file.
        if !is_file && data.is_none_or(<[u8]>::is_empty) {
            return;
        }
        let content_id = self.one("Content-ID").map(|id| {
            let id = id.strip_prefix('<').unwrap_or(&id);
            id.strip_suffix('>').unwrap_or(id).to_owned()
        });
        found.attachments.push(GmailAttachment {
            attachment_id,
            filename,
            mime_type: Some(kind.to_owned()).filter(|kind| !kind.is_empty()),
            size: body.and_then(|body| body.size),
            inline: match disposition.as_deref() {
                Some("attachment") => false,
                Some("inline") => true,
                _ => content_id.is_some(),
            },
            content_id,
            part_id: self.part_id.clone().filter(|id| !id.is_empty()),
        });
    }
}

impl GmailWireMessage {
    /// The first value of one of the message's own headers, as Gmail sent it.
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.payload.as_ref()?.values(name).next()
    }

    /// The mailboxes a header of the message names, in every place the
    /// header is written: the same people `read` shows for it.
    pub(crate) fn mailboxes(&self, name: &str) -> Vec<GmailAddress> {
        let headers = self.payload.iter().flat_map(|payload| payload.values(name));
        headers.flat_map(listed).collect()
    }

    /// The subject as a person reads it.
    pub(crate) fn subject(&self) -> Option<String> {
        self.payload.as_ref()?.one("Subject")
    }

    /// The message with its parts decoded. One that came without a payload,
    /// as `minimal` asks for, is its ids and labels and nothing more.
    pub(crate) fn read(self) -> GmailMessage {
        let top = self.payload.unwrap_or_default();
        let found = top.found();
        GmailMessage {
            id: self.id,
            thread_id: self.thread_id,
            label_ids: self.label_ids,
            snippet: self.snippet,
            history_id: self.history_id,
            internal_date: self.internal_date,
            size_estimate: self.size_estimate,
            from: top.people("From"),
            to: top.people("To"),
            cc: top.people("Cc"),
            bcc: top.people("Bcc"),
            reply_to: top.people("Reply-To"),
            subject: top.one("Subject"),
            date: top.one("Date"),
            message_id: top.one("Message-ID"),
            in_reply_to: top.one("In-Reply-To"),
            references: top.one("References"),
            text: found.text,
            html: found.html,
            attachments: found.attachments,
        }
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose::{URL_SAFE, URL_SAFE_NO_PAD};
    use serde_json::{Value, json};

    use super::*;

    fn read(payload: Value) -> GmailMessage {
        let wire: GmailWireMessage = serde_json::from_value(json!({ "id": "m1", "payload": payload })).unwrap();
        wire.read()
    }

    fn part(kind: &str, text: &str) -> Value {
        json!({ "mimeType": kind, "body": { "size": text.len(), "data": URL_SAFE_NO_PAD.encode(text) } })
    }

    fn file(kind: &str, name: &str, headers: Value) -> Value {
        json!({ "partId": "9", "mimeType": kind, "filename": name, "headers": headers, "body": { "attachmentId": "ANGjdJ8", "size": 4096 } })
    }

    fn multipart(kind: &str, parts: Value) -> Value {
        json!({ "mimeType": format!("multipart/{kind}"), "body": { "size": 0 }, "parts": parts })
    }

    #[test]
    fn a_message_of_one_part_is_its_text_or_its_html() {
        let plain = read(part("text/plain", "See you Monday."));
        assert_eq!(plain.text.as_deref(), Some("See you Monday."));
        assert_eq!((plain.html, plain.attachments.len()), (None, 0));
        let html = read(part("TEXT/HTML", "<p>See you Monday.</p>"));
        assert_eq!(html.html.as_deref(), Some("<p>See you Monday.</p>"));
        assert_eq!(html.text, None);
    }

    #[test]
    fn a_body_is_read_padded_or_not_and_in_the_character_set_it_names() {
        // 16 bytes, so the padded form ends in `==`.
        let text = "Grüße aus Wien";
        for data in [URL_SAFE.encode(text), URL_SAFE_NO_PAD.encode(text)] {
            let message = read(json!({ "mimeType": "text/plain", "body": { "data": data } }));
            assert_eq!(message.text.as_deref(), Some(text), "{data}");
        }
        let latin = json!({
            "mimeType": "text/plain",
            "headers": [{ "name": "content-type", "value": "text/plain; format=flowed; CHARSET=\"iso-8859-1\"" }],
            "body": { "data": URL_SAFE_NO_PAD.encode(b"Caf\xe9") }
        });
        assert_eq!(read(latin).text.as_deref(), Some("Café"));
        // Bytes that are not UTF-8, with nothing to say what they are.
        let unknown =
            json!({ "mimeType": "text/plain", "body": { "data": URL_SAFE_NO_PAD.encode(b"Caf\xe9 au lait") } });
        assert_eq!(read(unknown).text.as_deref(), Some("Caf\u{fffd} au lait"));
    }

    #[test]
    fn a_body_that_is_not_base64_fails_without_repeating_it() {
        let broken =
            json!({ "id": "m1", "payload": { "mimeType": "text/plain", "body": { "data": "CONFIDENTIAL!!" } } });
        let error = serde_json::from_value::<GmailWireMessage>(broken).err().unwrap();
        assert!(!error.to_string().contains("CONFIDENTIAL"), "{error}");
    }

    #[test]
    fn the_alternatives_of_a_message_are_one_text_and_one_html() {
        let message = read(multipart(
            "alternative",
            json!([
                part("text/plain", "Plan attached."),
                part("text/html", "<b>Plan</b> attached.")
            ]),
        ));
        assert_eq!(message.text.as_deref(), Some("Plan attached."));
        assert_eq!(message.html.as_deref(), Some("<b>Plan</b> attached."));
        assert!(message.attachments.is_empty());
    }

    #[test]
    fn attachments_and_inline_pictures_are_listed_and_never_read_as_the_body() {
        let attached = json!([{ "name": "Content-Disposition", "value": "attachment; filename=\"plan.pdf\"" }]);
        let pictured = json!([
            { "name": "Content-Disposition", "value": "inline; filename=\"logo.png\"" },
            { "name": "Content-ID", "value": "<logo@example.test>" }
        ]);
        let notes = json!([{ "name": "content-disposition", "value": "ATTACHMENT; filename=notes.txt" }]);
        let message = read(multipart(
            "mixed",
            json!([
                multipart(
                    "related",
                    json!([
                        multipart(
                            "alternative",
                            json!([
                                part("text/plain", "Plan attached."),
                                part("text/html", "<img src=\"cid:logo@example.test\">")
                            ])
                        ),
                        file("image/png", "logo.png", pictured)
                    ])
                ),
                file("application/pdf", "plan.pdf", attached),
                // A text file that was attached is a file, though it is text.
                file("text/plain", "notes.txt", notes),
                part("text/plain", "Sent from my phone")
            ]),
        ));
        assert_eq!(message.text.as_deref(), Some("Plan attached.\nSent from my phone"));
        assert_eq!(message.html.as_deref(), Some("<img src=\"cid:logo@example.test\">"));
        let listed: Vec<_> = message
            .attachments
            .iter()
            .map(|a| {
                (
                    a.filename.as_str(),
                    a.mime_type.as_deref().unwrap(),
                    a.inline,
                    a.content_id.as_deref(),
                )
            })
            .collect();
        assert_eq!(
            listed,
            [
                ("logo.png", "image/png", true, Some("logo@example.test")),
                ("plan.pdf", "application/pdf", false, None),
                ("notes.txt", "text/plain", false, None),
            ]
        );
        let pdf = &message.attachments[1];
        assert_eq!(
            (pdf.attachment_id.as_deref(), pdf.size, pdf.part_id.as_deref()),
            (Some("ANGjdJ8"), Some(4096), Some("9"))
        );
    }

    #[test]
    fn a_part_gmail_names_a_file_is_a_file_whatever_its_headers_say() {
        // Gmail's own attachments carry a Content-ID and are still attachments.
        let gmail = json!([
            { "name": "Content-Disposition", "value": "attachment; filename=\"plan.pdf\"" },
            { "name": "Content-ID", "value": "<f_abc123>" }
        ]);
        let listed = read(multipart(
            "mixed",
            json!([
                file("application/pdf", "plan.pdf", gmail),
                // No disposition: a Content-ID alone means the body shows it.
                file("image/gif", "", json!([{ "name": "Content-Id", "value": "<pixel@example.test>" }])),
                file("application/octet-stream", "=?UTF-8?B?UGzDpG5lLnBkZg==?=", json!([])),
                // A message that was attached is not opened and read as this one's body.
                { "mimeType": "message/rfc822", "filename": "", "body": { "attachmentId": "ANGjdJ9", "size": 900 },
                  "parts": [part("text/plain", "Another message entirely.")] },
                // A small part Gmail sent whole has no id to fetch it by.
                { "mimeType": "text/calendar", "body": { "size": 5, "data": "QkVHSU4" } },
                { "mimeType": "application/x-empty", "body": { "size": 0 } }
            ]),
        ))
        .attachments;
        let seen: Vec<_> = listed
            .iter()
            .map(|a| {
                (
                    a.mime_type.as_deref().unwrap(),
                    a.filename.as_str(),
                    a.inline,
                    a.attachment_id.is_some(),
                )
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("application/pdf", "plan.pdf", false, true),
                ("image/gif", "", true, true),
                ("application/octet-stream", "Pläne.pdf", false, true),
                ("message/rfc822", "", false, true),
                ("text/calendar", "", false, false),
            ]
        );
    }

    #[test]
    fn headers_are_found_in_any_case_and_a_repeated_one_is_not_lost() {
        let message = read(json!({
            "mimeType": "text/plain",
            "headers": [
                { "name": "FROM", "value": "=?UTF-8?Q?Zo=C3=AB_M=C3=BCller?= <zoe@example.test>" },
                { "name": "to", "value": "ada@example.test" },
                { "name": "To", "value": "Grace Hopper\r\n <grace@example.test>" },
                { "name": "message-id", "value": "<m1@mail.example.test>" },
                { "name": "Subject", "value": "=?UTF-8?B?UGzDpG5l?= for Q3" },
                { "name": "SUBJECT", "value": "A second subject" },
                { "name": "Date", "value": "Fri, 9 Oct 2026 08:15:00 +0000" },
                { "name": "X-Empty", "value": "" }
            ]
        }));
        assert_eq!(message.from.as_deref(), Some("Zoë Müller <zoe@example.test>"));
        assert_eq!(
            message.to.as_deref(),
            Some("ada@example.test, Grace Hopper <grace@example.test>")
        );
        assert_eq!(message.message_id.as_deref(), Some("<m1@mail.example.test>"));
        assert_eq!(
            message.subject.as_deref(),
            Some("Pläne for Q3"),
            "the first of a header mail allows once"
        );
        assert_eq!(message.date.as_deref(), Some("Fri, 9 Oct 2026 08:15:00 +0000"));
        assert_eq!((message.cc, message.reply_to, message.text), (None, None, None));
    }

    #[test]
    fn a_message_without_a_payload_is_its_ids_and_labels() {
        let wire: GmailWireMessage =
            serde_json::from_value(json!({ "id": "m1", "threadId": "t1", "labelIds": ["INBOX"], "snippet": "Plan" }))
                .unwrap();
        assert_eq!((wire.header("Subject"), wire.subject()), (None, None));
        let message = wire.read();
        assert_eq!((message.id.as_str(), message.thread_id.as_deref()), ("m1", Some("t1")));
        assert_eq!(message.label_ids, ["INBOX"]);
        assert_eq!((message.subject, message.text, message.html), (None, None, None));
        assert!(message.attachments.is_empty());
        // Parts that are there and hold nothing do no harm either.
        let hollow = read(
            json!({ "mimeType": "multipart/mixed", "parts": [{}, { "mimeType": "text/plain" }, { "mimeType": "text/html", "body": {} }] }),
        );
        assert_eq!((hollow.text, hollow.html, hollow.attachments.len()), (None, None, 0));
    }
}
