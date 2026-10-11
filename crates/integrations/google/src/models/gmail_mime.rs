//! A message as Gmail sends it: a tree of MIME parts, each with its own
//! headers and its body in base64. Reading that tree into a [`GmailMessage`]
//! is the work of this file, so that no caller has to.

use std::borrow::Cow;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer};

use super::gmail_address::displayed;
use super::{GmailAttachment, GmailMessage, gmail_words};

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
/// An attachment that is fetched by itself arrives the same way.
pub(super) struct Bytes(pub(super) Vec<u8>);

impl<'de> Deserialize<'de> for Bytes {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        gmail_words::from_base64(&text)
            .map(Bytes)
            .ok_or_else(|| D::Error::custom("not base64"))
    }
}

/// What the parts of a message add up to.
///
/// What the message says is kept apart from what was found inside a part
/// marked as a file, however deep that part lies. The two are put together
/// only at the end, the message first, so that nothing marked as a file can
/// be chosen as the message among its forms.
#[derive(Default)]
struct Found {
    text: Option<String>,
    html: Option<String>,
    /// The text and the HTML found inside parts marked as files.
    filed_text: Option<String>,
    filed_html: Option<String>,
    attachments: Vec<GmailAttachment>,
}

/// `more` after `all`, on a line of its own.
fn after(all: &mut Option<String>, more: Option<String>) {
    match (all.as_mut(), more) {
        (Some(all), Some(more)) => {
            all.push('\n');
            all.push_str(&more);
        }
        (None, more @ Some(_)) => *all = more,
        (_, None) => {}
    }
}

impl Found {
    /// Adds what another part of the same message held, after what is here.
    fn add(&mut self, more: Found) {
        after(&mut self.text, more.text);
        after(&mut self.html, more.html);
        self.filed(more.filed_text, more.filed_html);
        self.attachments.extend(more.attachments);
    }

    /// Adds text that was found inside a part marked as a file.
    fn filed(&mut self, text: Option<String>, html: Option<String>) {
        after(&mut self.filed_text, text);
        after(&mut self.filed_html, html);
    }

    /// Everything a part marked as a file held, as what it is: none of it
    /// is what the message says.
    fn into_filed(mut self) -> Self {
        let (text, html) = (self.text.take(), self.html.take());
        let (filed_text, filed_html) = (self.filed_text.take(), self.filed_html.take());
        self.filed(text, html);
        self.filed(filed_text, filed_html);
        self
    }

    /// The text and the HTML of the whole message: what it says, and after
    /// it what was found in parts marked as files.
    fn whole(mut self) -> (Option<String>, Option<String>, Vec<GmailAttachment>) {
        after(&mut self.text, self.filed_text.take());
        after(&mut self.html, self.filed_html.take());
        (self.text, self.html, self.attachments)
    }
}

/// A header's value without the line breaks it was folded at.
///
/// This is done once, where a value leaves the message ([`Part::values`]),
/// and nowhere else. A value read folded in one place and unfolded in
/// another is two different texts: a backslash before a line break escapes
/// the break in the first and whatever follows it in the second, which is
/// enough for the two to find different addresses in one header.
fn unfolded(value: &str) -> Cow<'_, str> {
    if value.contains(['\r', '\n']) {
        Cow::Owned(value.replace(['\r', '\n'], ""))
    } else {
        Cow::Borrowed(value)
    }
}

/// A value as a person reads it: its encoded words read, and a space where
/// one was hiding a character that is not seen. `None` when nothing is left.
fn shown(value: &str) -> Option<String> {
    Some(gmail_words::readable(value)).filter(|text| !text.is_empty())
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
    /// Every value of the header `name`, unfolded. Mail writes header names
    /// in any case: `Message-ID`, `Message-Id`, `message-id`.
    ///
    /// Everything that reads a header reads it from here, so all of it reads
    /// the same text.
    fn values<'a>(&'a self, name: &str) -> impl Iterator<Item = Cow<'a, str>> {
        let named = move |header: &&Header| header.name.trim().eq_ignore_ascii_case(name);
        self.headers.iter().filter(named).map(|header| unfolded(&header.value))
    }

    /// The first value of a header that mail allows only once.
    fn one(&self, name: &str) -> Option<String> {
        self.values(name).find_map(|value| shown(&value))
    }

    /// The people a header names, as a person reads them. Some senders
    /// write a list as several headers; every one of them is shown.
    fn people(&self, name: &str) -> Option<String> {
        let shown: Vec<String> = self.values(name).filter_map(|value| displayed(&value)).collect();
        Some(shown.join(", ")).filter(|joined| !joined.is_empty())
    }

    /// Reads this part and the parts inside it.
    fn found(&self) -> Found {
        let kind = self
            .mime_type
            .as_deref()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        // A part made of other parts that its sender marked as a file. Mail
        // programs do not agree on such a part: some show what is inside it
        // as the message, some show a file. A reader that kept its text back
        // would be blind to what a person may be shown, so the text is read
        // with the rest; and the part is listed among the attachments, so
        // that it can be seen that some of the text came marked as a file.
        if self.marked_as_a_file() {
            let mut found = Found::default();
            self.leaf(&kind, &mut found);
            found.add(self.contents().into_filed());
            return found;
        }
        self.contents()
    }

    /// Whether this is a part made of other parts that its sender marked as
    /// a file.
    fn marked_as_a_file(&self) -> bool {
        let made_of_parts = self
            .mime_type
            .as_deref()
            .is_some_and(|kind| kind.trim().to_ascii_lowercase().starts_with("multipart/"));
        made_of_parts && self.attached()
    }

    /// Reads what this part holds, whatever it says of itself. The message
    /// as a whole is read this way: a message is never a file attached to
    /// itself, and one marked so would otherwise read as saying nothing.
    fn contents(&self) -> Found {
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
            // What a form holds inside a part marked as a file, at any
            // depth, is not a form of the message: it is kept beside them
            // all, and never chosen in place of what the message says.
            for form in self.parts.iter().map(Part::found) {
                found.text = form.text.or(found.text.take());
                found.html = form.html.or(found.html.take());
                found.filed(form.filed_text, form.filed_html);
                found.attachments.extend(form.attachments);
            }
        } else if kind.starts_with("multipart/") {
            self.parts.iter().for_each(|part| found.add(part.found()));
        } else {
            self.leaf(&kind, &mut found);
        }
        found
    }

    /// Whether the sender attached this part as a file: it is marked as an
    /// attachment, or has a file's name, or Gmail keeps it to be fetched.
    fn attached(&self) -> bool {
        let marked = self.values("Content-Disposition").next().is_some_and(|value| {
            value
                .split(';')
                .next()
                .unwrap_or_default()
                .trim()
                .eq_ignore_ascii_case("attachment")
        });
        let named = self.filename.as_deref().is_some_and(|name| !name.trim().is_empty());
        let kept = self
            .body
            .as_ref()
            .and_then(|body| body.attachment_id.as_deref())
            .is_some_and(|id| !id.is_empty());
        marked || named || kept
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
                .and_then(|value| parameter(&value, "charset"));
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
    /// The first value of one of the message's own headers that says
    /// anything, unfolded and otherwise as it was sent. It is the value
    /// `read` shows for a header mail allows once.
    pub(crate) fn header(&self, name: &str) -> Option<Cow<'_, str>> {
        let mut values = self.payload.as_ref()?.values(name);
        values.find(|value| shown(value).is_some())
    }

    /// The subject as a person reads it.
    pub(crate) fn subject(&self) -> Option<String> {
        self.payload.as_ref()?.one("Subject")
    }

    /// The message with its parts decoded. One that came without a payload,
    /// as `minimal` asks for, is its ids and labels and nothing more.
    pub(crate) fn read(self) -> GmailMessage {
        let top = self.payload.unwrap_or_default();
        let (text, html, attachments) = top.contents().whole();
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
            text,
            html,
            attachments,
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
    fn a_part_marked_as_a_file_is_listed_as_one_and_nothing_in_it_is_kept_from_the_reader() {
        // A part made of parts, marked by its sender as an attachment. Some
        // mail programs show its text as the message, so a reader that left
        // it out would not see what a person may be shown.
        let mut marked = multipart(
            "mixed",
            json!([
                part("text/plain", "Wire the money today."),
                part("text/html", "<p>Wire the money today.</p>"),
                file("application/pdf", "invoice.pdf", json!([]))
            ]),
        );
        marked["filename"] = json!("forwarded.eml");
        marked["headers"] = json!([{ "name": "Content-Disposition", "value": "ATTACHMENT; filename=forwarded.eml" }]);
        let message = read(multipart("mixed", json!([part("text/plain", "See attached."), marked])));
        assert_eq!(message.text.as_deref(), Some("See attached.\nWire the money today."));
        assert_eq!(message.html.as_deref(), Some("<p>Wire the money today.</p>"));
        // The part itself is not lost among what it holds: it is the first
        // of the files, before the one inside it.
        let files: Vec<(&str, Option<&str>)> = message
            .attachments
            .iter()
            .map(|file| (file.filename.as_str(), file.mime_type.as_deref()))
            .collect();
        assert_eq!(
            files,
            [
                ("forwarded.eml", Some("multipart/mixed")),
                ("invoice.pdf", Some("application/pdf"))
            ]
        );
    }

    #[test]
    fn a_part_marked_as_a_file_never_takes_the_place_of_what_the_message_says() {
        // Among the forms of one message, the last that has text is the
        // text. A part marked as a file is not a form of the message, so
        // what it holds cannot stand in for the body a person is shown.
        let mut marked = multipart(
            "mixed",
            json!([
                part("text/plain", "Wire the money today."),
                part("text/html", "<p>Wire the money today.</p>")
            ]),
        );
        marked["headers"] = json!([{ "name": "content-disposition", "value": "attachment" }]);
        let message = read(multipart(
            "alternative",
            json!([
                part("text/plain", "Lunch on Friday?"),
                part("text/html", "<p>Lunch on Friday?</p>"),
                marked
            ]),
        ));
        assert_eq!(message.text.as_deref(), Some("Lunch on Friday?\nWire the money today."));
        assert_eq!(
            message.html.as_deref(),
            Some("<p>Lunch on Friday?</p>\n<p>Wire the money today.</p>")
        );
        assert_eq!(
            message.attachments.len(),
            1,
            "and it is listed as the file it was marked as"
        );
        assert_eq!(message.attachments[0].mime_type.as_deref(), Some("multipart/mixed"));
    }

    #[test]
    fn however_deep_a_part_marked_as_a_file_lies_it_never_takes_the_place_of_the_message() {
        // The marked part is wrapped in one that is not marked, which stands
        // among the forms of the message. Its text is still not the message's.
        let mut marked = multipart("mixed", json!([part("text/plain", "Wire the money today.")]));
        marked["filename"] = json!("note.eml");
        let wrapped = multipart("related", json!([multipart("mixed", json!([marked]))]));
        let message = read(multipart(
            "alternative",
            json!([part("text/plain", "Lunch on Friday?"), wrapped]),
        ));
        assert_eq!(message.text.as_deref(), Some("Lunch on Friday?\nWire the money today."));
        assert_eq!(message.attachments.len(), 1);

        // And one marked part inside another: all of it is beside the message.
        let mut inner = multipart("alternative", json!([part("text/plain", "Send it to Eve.")]));
        inner["filename"] = json!("inner.eml");
        let mut outer = multipart("mixed", json!([part("text/plain", "Wire the money today."), inner]));
        outer["filename"] = json!("outer.eml");
        let message = read(multipart(
            "alternative",
            json!([part("text/plain", "Lunch on Friday?"), outer]),
        ));
        assert_eq!(
            message.text.as_deref(),
            Some("Lunch on Friday?\nWire the money today.\nSend it to Eve.")
        );
        // With nothing of its own to say, the message is what the files held.
        let mut only = multipart("mixed", json!([part("text/plain", "Wire the money today.")]));
        only["filename"] = json!("only.eml");
        let message = read(multipart("mixed", json!([only])));
        assert_eq!(message.text.as_deref(), Some("Wire the money today."));
    }

    #[test]
    fn a_message_marked_as_a_file_itself_still_says_what_it_says() {
        // A sender can mark the whole message as an attachment. Every mail
        // program shows its body all the same, so it is read all the same.
        let mut whole = multipart(
            "alternative",
            json!([
                part("text/plain", "Wire the money today."),
                part("text/html", "<p>Wire the money today.</p>")
            ]),
        );
        whole["filename"] = json!("message.eml");
        whole["headers"] = json!([{ "name": "Content-Disposition", "value": "attachment" }]);
        let message = read(whole);
        assert_eq!(message.text.as_deref(), Some("Wire the money today."));
        assert_eq!(message.html.as_deref(), Some("<p>Wire the money today.</p>"));
        assert!(message.attachments.is_empty());
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
    fn a_header_is_unfolded_once_so_every_reader_sees_the_same_people() {
        let from = |value: &str| read(json!({ "headers": [{ "name": "From", "value": value }] })).from;
        // A backslash before a line break. Read folded, it escapes the break
        // and the comment ends at the bracket after it, leaving the first
        // address; read unfolded, it escapes that bracket and the comment
        // runs on over the first address. One reader showed
        // `boss@corp.test` and another found `eve@evil.test` to answer.
        // There is one text now, the unfolded one, and one reader of it.
        for line_break in ["\r", "\n", "\r\n"] {
            let header = format!("(\\{line_break}) <eve@evil.test>, ) <boss@corp{line_break}.test>");
            assert_eq!(from(&header).as_deref(), Some("boss@corp.test"), "{header:?}");
            assert_eq!(from(&header), from("(\\) <eve@evil.test>, ) <boss@corp.test>"));
        }
        // The same in a quoted name: the quote the break stood before is
        // part of the name, and the name runs to the next one.
        let quoted = "\"Boss\\\r\" <eve@evil.test>, \" <boss@corp.test>";
        assert_eq!(
            from(quoted).as_deref(),
            Some("\"Boss\\\" <eve@evil.test>,\" <boss@corp.test>")
        );
        // A header folded as mail folds one reads as it always did.
        assert_eq!(
            from("Grace Hopper\r\n <grace@example.test>,\r\n\tada@example.test").as_deref(),
            Some("Grace Hopper <grace@example.test>, ada@example.test")
        );
        // Every other header is read from the same unfolded text.
        let message = read(json!({ "headers": [
            { "name": "Subject", "value": "Q3\r\n plan" },
            { "name": "Message-ID", "value": "<m1@mail\r.example.test>" },
            { "name": "Content-Type", "value": "text/plain;\r\n charset=\"iso-8859-1\"" }
        ], "mimeType": "text/plain", "body": { "data": URL_SAFE_NO_PAD.encode(b"Caf\xe9") } }));
        assert_eq!(message.subject.as_deref(), Some("Q3 plan"));
        assert_eq!(message.message_id.as_deref(), Some("<m1@mail.example.test>"));
        assert_eq!(message.text.as_deref(), Some("Café"));
    }

    #[test]
    fn what_is_not_seen_in_a_header_or_a_file_name_is_a_space() {
        let hidden = |text: &str| format!("=?UTF-8?B?{}?=", base64::engine::general_purpose::STANDARD.encode(text));
        let message = read(json!({
            "mimeType": "multipart/mixed",
            "headers": [
                { "name": "From", "value": format!("{} <eve@evil.test>", hidden("\u{202e}<tset.proc@ssob>\u{202c}")) },
                { "name": "Subject", "value": hidden("Invoice\u{200b}\u{2028}paid\u{202e}") },
                { "name": "Date", "value": "Fri,\u{feff} 9 Oct 2026" }
            ],
            "parts": [file("application/pdf", "plan\u{202e}fdp.exe", json!([]))]
        }));
        assert_eq!(message.from.as_deref(), Some("\"<tset.proc@ssob>\" <eve@evil.test>"));
        assert_eq!(message.subject.as_deref(), Some("Invoice  paid"));
        assert_eq!(message.date.as_deref(), Some("Fri,  9 Oct 2026"));
        // A name written to be read backwards, as `planexe.pdf`.
        assert_eq!(message.attachments[0].filename, "plan fdp.exe");
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
