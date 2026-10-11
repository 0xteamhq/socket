//! What is attached to a Gmail message: how a message lists it, and the
//! file itself as Gmail hands it over.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gmail_mime::Bytes;

/// A file carried by a message, without its content.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailAttachment {
    /// What `gmail_messages.attachment_text` and the typed
    /// `attachment_content` take. Absent when Gmail sent the content inside
    /// the message and not as a file to fetch.
    pub attachment_id: Option<String>,
    /// The file's name. Empty when the part has none.
    pub filename: String,
    /// The media type, such as `application/pdf`.
    pub mime_type: Option<String>,
    /// The size in bytes.
    pub size: Option<i64>,
    /// Whether the sender marked it to be shown in the body, as a picture in
    /// a signature is, and not offered as a file.
    pub inline: bool,
    /// The name the HTML body refers to an inline part by, as `cid:…`.
    pub content_id: Option<String>,
    /// Where the part sits in the message: `1`, `0.1`.
    pub part_id: Option<String>,
}

/// An attachment that is text, as text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct GmailAttachmentText {
    /// The size of the file in bytes.
    pub size: usize,
    /// What the file says. A byte order mark at its start is left out.
    pub text: String,
}

impl GmailAttachmentText {
    /// The file as text, when it is text. `None` when it is not, and then
    /// nothing of it is kept.
    ///
    /// Gmail hands a file over without saying what it is, so the bytes have
    /// to say it themselves. Text is UTF-8, and holds no control character
    /// but the ones a text file is written with: a tab, a line feed, a
    /// carriage return, a form feed. A NUL or any other of them is what a
    /// small binary file looks like when its bytes happen to read as UTF-8,
    /// and an escape character is how text takes over the screen it is
    /// shown on. A file with one is not text here.
    pub(crate) fn read(file: Vec<u8>) -> Option<Self> {
        let size = file.len();
        let text = String::from_utf8(file).ok()?;
        let text = match text.strip_prefix('\u{feff}') {
            Some(after) => after.to_owned(),
            None => text,
        };
        let written = |character: char| matches!(character, '\t' | '\n' | '\r' | '\u{c}');
        let binary = |character: char| character.is_ascii_control() && !written(character);
        (!text.contains(binary)).then_some(Self { size, text })
    }
}

/// An attachment's content in Gmail's own shape: the file in base64, inside
/// JSON. It has no `Debug`: it holds someone's file.
#[derive(Default, Deserialize)]
#[serde(default)]
pub(crate) struct GmailWireAttachment {
    size: Option<i64>,
    data: Option<Bytes>,
}

impl GmailWireAttachment {
    /// The file, out of its base64. `None` when the answer holds none.
    ///
    /// Gmail leaves out what is empty, so an empty file is an answer with a
    /// size of nothing and no data. An answer that names a size and carries
    /// nothing of it, or carries neither, is not a file.
    pub(crate) fn into_bytes(self) -> Option<Vec<u8>> {
        match (self.data, self.size) {
            (Some(data), size) if !data.0.is_empty() || size.unwrap_or(0) == 0 => Some(data.0),
            (None, Some(0)) => Some(Vec::new()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn file(answer: Value) -> Option<Vec<u8>> {
        serde_json::from_value::<GmailWireAttachment>(answer)
            .ok()
            .and_then(GmailWireAttachment::into_bytes)
    }

    #[test]
    fn a_file_is_taken_out_of_base64_padded_or_not_and_in_either_alphabet() {
        // 0xFB 0xFF is `-_8` in the URL-safe alphabet and `+/8` in the standard one.
        for data in ["-_8", "-_8=", "+/8", "+/8="] {
            let answer = json!({ "attachmentId": "a1", "size": 2, "data": data });
            assert_eq!(file(answer), Some(vec![0xfb, 0xff]), "{data}");
        }
        // What Gmail says of the size is not what decides how much there is.
        assert_eq!(file(json!({ "data": "JVBERg" })), Some(b"%PDF".to_vec()));
        assert_eq!(file(json!({ "size": 9000, "data": "JVBERg" })), Some(b"%PDF".to_vec()));
    }

    #[test]
    fn an_empty_file_is_a_file_and_an_answer_with_nothing_in_it_is_not() {
        for empty in [
            json!({ "size": 0 }),
            json!({ "size": 0, "data": "" }),
            json!({ "data": "" }),
        ] {
            assert_eq!(file(empty.clone()), Some(Vec::new()), "{empty}");
        }
        for nothing in [
            json!({}),
            json!({ "attachmentId": "a1" }),
            json!({ "size": 41230 }),
            json!({ "size": 41230, "data": "" }),
            json!({ "size": 4, "data": "not base64!" }),
            json!({ "size": 4, "data": 7 }),
        ] {
            assert_eq!(file(nothing.clone()), None, "{nothing}");
        }
    }

    #[test]
    fn text_is_utf8_without_the_control_characters_of_a_binary_file() {
        let read = |file: &[u8]| GmailAttachmentText::read(file.to_vec());
        let text = |file: &[u8]| read(file).map(|read| read.text);
        assert_eq!(
            text("date,total\r\n2026-10-09,42\r\n".as_bytes()).as_deref(),
            Some("date,total\r\n2026-10-09,42\r\n")
        );
        assert_eq!(
            text("Grüße\t日本語\n\u{c}🚀".as_bytes()).as_deref(),
            Some("Grüße\t日本語\n\u{c}🚀")
        );
        assert_eq!(text(b"").as_deref(), Some(""));
        // A byte order mark says how the text is written, and is not part
        // of it. The size is the file's, mark and all.
        let marked = read(b"\xef\xbb\xbfid,name\n").unwrap();
        assert_eq!((marked.text.as_str(), marked.size), ("id,name\n", 11));
        assert_eq!(
            text("a\u{feff}b".as_bytes()).as_deref(),
            Some("a\u{feff}b"),
            "only at the start"
        );

        // Not UTF-8: a PDF, a picture, text in another encoding.
        for binary in [
            &b"%PDF-1.7\n%\xe2\xe3\xcf\xd3\n"[..],
            &b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"[..],
            &b"Caf\xe9"[..],
            &b"\xff\xfeh\0i\0"[..],
        ] {
            assert_eq!(read(binary), None, "{binary:?}");
        }
        // UTF-8 as far as its bytes go, and still not text: a NUL, an
        // escape, a bell, a delete.
        for control in [
            "PK\u{3}\u{4}",
            "a\0b",
            "\u{1b}[2J\u{1b}]0;title\u{7}",
            "a\u{8}b",
            "a\u{7f}b",
        ] {
            assert_eq!(read(control.as_bytes()), None, "{control:?}");
        }
    }
}
