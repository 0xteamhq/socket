//! What Google answers for Gmail, as the tests need it: fixtures and constants.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};
use serde_json::{Value, json};

/// Where Gmail's methods live, as a request reaches the local server.
pub const GMAIL_MESSAGES: &str = "/gmail/v1/users/me/messages";
pub const GMAIL_THREADS: &str = "/gmail/v1/users/me/threads";
pub const GMAIL_LABELS: &str = "/gmail/v1/users/me/labels";
pub const GMAIL_DRAFTS: &str = "/gmail/v1/users/me/drafts";
pub const GMAIL_PROFILE: &str = "/gmail/v1/users/me/profile";

pub const GMAIL_MESSAGE: &str = "18c1a2b3c4d5e6f7";
pub const GMAIL_THREAD: &str = "18c1a2b3c4d5e6f0";
pub const GMAIL_ATTACHMENT: &str = "ANGjdJ_q3plan-Zk0";
pub const GMAIL_DRAFT: &str = "r-7215003489218";

/// `content` as Gmail writes a body or takes a message: base64 in the
/// URL-safe alphabet, padded.
pub fn gmail_data(content: &str) -> String {
    URL_SAFE.encode(content)
}

fn header(name: &str, value: &str) -> Value {
    json!({ "name": name, "value": value })
}

/// The headers of the message below, as Gmail lists them.
fn gmail_headers() -> Value {
    json!([
        header("Delivered-To", "ada@example.test"),
        header(
            "Received",
            "by 2002:a05:6402:1a2b with SMTP id x7; Fri, 9 Oct 2026 01:15:02 -0700 (PDT)"
        ),
        header("MIME-Version", "1.0"),
        header("References", "<CAF0kickoff@mail.example.test>"),
        header("In-Reply-To", "<CAF0kickoff@mail.example.test>"),
        header("From", "Grace Hopper <grace@example.test>"),
        header("Date", "Fri, 9 Oct 2026 08:15:00 +0000"),
        header("Message-ID", "<CAF1plan@mail.example.test>"),
        header("Subject", "Q3 plan"),
        header("To", "Ada Lovelace <ada@example.test>"),
        header("Cc", "alan@example.test"),
        header("Content-Type", "multipart/mixed; boundary=\"000000000000a1b2c3\"")
    ])
}

/// A message as Gmail returns it in full: the same words as text and as
/// HTML, and a PDF.
pub fn gmail_message() -> Value {
    let (text, html) = (
        "Attached is the plan for Q3.\r\n",
        "<div>Attached is the plan for Q3.</div>\r\n",
    );
    json!({
        "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD,
        "labelIds": ["UNREAD", "IMPORTANT", "CATEGORY_PERSONAL", "INBOX"],
        "snippet": "Attached is the plan for Q3.",
        "historyId": "987654", "internalDate": "1791533700000", "sizeEstimate": 58213,
        "payload": {
            "partId": "", "mimeType": "multipart/mixed", "filename": "",
            "headers": gmail_headers(),
            "body": { "size": 0 },
            "parts": [
                {
                    "partId": "0", "mimeType": "multipart/alternative", "filename": "",
                    "headers": [header("Content-Type", "multipart/alternative; boundary=\"000000000000a1b2c1\"")],
                    "body": { "size": 0 },
                    "parts": [
                        { "partId": "0.0", "mimeType": "text/plain", "filename": "",
                          "headers": [header("Content-Type", "text/plain; charset=\"UTF-8\"")],
                          "body": { "size": text.len(), "data": gmail_data(text) } },
                        { "partId": "0.1", "mimeType": "text/html", "filename": "",
                          "headers": [header("Content-Type", "text/html; charset=\"UTF-8\"")],
                          "body": { "size": html.len(), "data": gmail_data(html) } }
                    ]
                },
                {
                    "partId": "1", "mimeType": "application/pdf", "filename": "q3-plan.pdf",
                    "headers": [
                        header("Content-Type", "application/pdf; name=\"q3-plan.pdf\""),
                        header("Content-Disposition", "attachment; filename=\"q3-plan.pdf\""),
                        header("Content-Transfer-Encoding", "base64"),
                        header("Content-ID", "<f_m1x2y3z40>"),
                        header("X-Attachment-Id", "f_m1x2y3z40")
                    ],
                    "body": { "attachmentId": GMAIL_ATTACHMENT, "size": 41230 }
                }
            ]
        }
    })
}

/// What an operation that returns `gmail_message()` must pass on.
pub fn gmail_message_returned() -> Value {
    json!({
        "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD,
        "labelIds": ["UNREAD", "IMPORTANT", "CATEGORY_PERSONAL", "INBOX"],
        "snippet": "Attached is the plan for Q3.",
        "historyId": "987654", "internalDate": "1791533700000", "sizeEstimate": 58213,
        "from": "Grace Hopper <grace@example.test>",
        "to": "Ada Lovelace <ada@example.test>",
        "cc": "alan@example.test",
        "bcc": null, "replyTo": null,
        "subject": "Q3 plan",
        "date": "Fri, 9 Oct 2026 08:15:00 +0000",
        "messageId": "<CAF1plan@mail.example.test>",
        "inReplyTo": "<CAF0kickoff@mail.example.test>",
        "references": "<CAF0kickoff@mail.example.test>",
        "text": "Attached is the plan for Q3.\r\n",
        "html": "<div>Attached is the plan for Q3.</div>\r\n",
        "attachments": [{
            "attachmentId": GMAIL_ATTACHMENT, "filename": "q3-plan.pdf", "mimeType": "application/pdf",
            "size": 41230, "inline": false, "contentId": "f_m1x2y3z40", "partId": "1"
        }]
    })
}

/// The same message as Gmail returns it for `format=metadata`: its headers,
/// and no part of its body.
pub fn gmail_metadata() -> Value {
    gmail_metadata_with(gmail_headers())
}

/// A message's metadata with headers of the test's choosing.
pub fn gmail_metadata_with(headers: Value) -> Value {
    json!({
        "id": GMAIL_MESSAGE, "threadId": GMAIL_THREAD, "labelIds": ["IMPORTANT", "INBOX"],
        "snippet": "Attached is the plan for Q3.", "historyId": "987654", "internalDate": "1791533700000",
        "sizeEstimate": 58213,
        "payload": { "partId": "", "mimeType": "multipart/mixed", "headers": headers }
    })
}

/// Headers as Gmail lists them, from pairs of a name and a value.
pub fn gmail_headers_of(pairs: &[(&str, &str)]) -> Value {
    pairs.iter().map(|(name, value)| header(name, value)).collect()
}

/// What Gmail answers when a message is sent or changed: its ids and labels.
pub fn gmail_ref(labels: &[&str]) -> Value {
    json!({ "id": "18c9f0e1d2c3b4a5", "threadId": GMAIL_THREAD, "labelIds": labels })
}

/// What Gmail answers when a draft is saved.
pub fn gmail_draft_ref() -> Value {
    json!({ "id": GMAIL_DRAFT, "message": { "id": "18c9f0e1d2c3b4a6", "threadId": "18c9f0e1d2c3b4a6", "labelIds": ["DRAFT"] } })
}

pub fn gmail_label() -> Value {
    json!({
        "id": "Label_12", "name": "Projects/Q3", "type": "user",
        "messageListVisibility": "show", "labelListVisibility": "labelShow",
        "messagesTotal": 42, "messagesUnread": 3, "threadsTotal": 17, "threadsUnread": 2,
        "color": { "textColor": "#ffffff", "backgroundColor": "#16a765" }
    })
}

/// The body of one part, as mail writes it: the headers that say what it
/// is, and the text in base64. For text short enough to fit one line.
pub fn gmail_part(kind: &str, text: &str) -> String {
    format!(
        "Content-Type: {kind}; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{}\r\n",
        STANDARD.encode(text)
    )
}

/// A message that reached Gmail in `raw`, read back out of it.
#[derive(Debug)]
pub struct GmailSent {
    /// Every header in the order written, with folded lines joined.
    pub headers: Vec<(String, String)>,
    /// The plain-text body, decoded.
    pub text: Option<String>,
    /// The HTML body, decoded.
    pub html: Option<String>,
}

impl GmailSent {
    /// The value of the one header called `name`.
    pub fn header(&self, name: &str) -> Option<&str> {
        let mut found = self.headers.iter().filter(|(n, _)| n.eq_ignore_ascii_case(name));
        let first = found.next().map(|(_, value)| value.as_str());
        assert!(found.next().is_none(), "{name} is written once");
        first
    }

    /// The names of the headers, in the order written.
    pub fn names(&self) -> Vec<&str> {
        self.headers.iter().map(|(name, _)| name.as_str()).collect()
    }
}

/// Splits a message, or one part of one, into its headers and what follows.
fn gmail_split(message: &str) -> (Vec<(String, String)>, &str) {
    let (head, body) = message
        .split_once("\r\n\r\n")
        .unwrap_or((message.trim_end_matches("\r\n"), ""));
    let headers = head
        .replace("\r\n ", " ")
        .split("\r\n")
        .map(|line| {
            let (name, value) = line
                .split_once(": ")
                .unwrap_or_else(|| panic!("not a header: {line:?}"));
            (name.to_owned(), value.to_owned())
        })
        .collect();
    (headers, body)
}

/// Reads the message a request body carries in `raw`: what Gmail would
/// send on. Every line of it must end in CR LF, and none may be over long.
pub fn gmail_sent(raw: &Value) -> GmailSent {
    let bytes = URL_SAFE
        .decode(raw.as_str().expect("`raw` is a string"))
        .expect("`raw` is base64url");
    let message = String::from_utf8(bytes).expect("a message is ASCII");
    assert!(message.is_ascii(), "everything outside ASCII is encoded");
    assert!(
        !message.replace("\r\n", "").contains(['\r', '\n']),
        "every line ends in CR LF"
    );
    assert!(
        message.split("\r\n").all(|line| line.len() <= 78),
        "no line is over long"
    );
    let decoded = |body: &str| String::from_utf8(STANDARD.decode(body.replace("\r\n", "")).unwrap()).unwrap();
    let (headers, body) = gmail_split(&message);
    let kind = |headers: &[(String, String)]| {
        let found = headers.iter().find(|(name, _)| name == "Content-Type");
        found.map(|(_, value)| value.clone()).unwrap_or_default()
    };
    let mut sent = GmailSent {
        headers,
        text: None,
        html: None,
    };
    let content_type = kind(&sent.headers);
    let mut put = |kind: &str, body: &str| match kind {
        "text/plain; charset=UTF-8" => sent.text = Some(decoded(body)),
        "text/html; charset=UTF-8" => sent.html = Some(decoded(body)),
        other => panic!("an unexpected part: {other}"),
    };
    match content_type.strip_prefix("multipart/alternative; boundary=") {
        Some(boundary) => {
            let separator = format!("--{}", boundary.trim_matches('"'));
            let pieces: Vec<&str> = body.split(separator.as_str()).collect();
            assert_eq!(
                (pieces[0], pieces[pieces.len() - 1]),
                ("", "--\r\n"),
                "the parts are fenced"
            );
            for piece in &pieces[1..pieces.len() - 1] {
                let (headers, body) = gmail_split(piece.strip_prefix("\r\n").expect("a part starts on its own line"));
                assert_eq!(
                    headers[1],
                    ("Content-Transfer-Encoding".to_owned(), "base64".to_owned())
                );
                put(&kind(&headers), body);
            }
        }
        None => put(&content_type, body),
    }
    sent
}
