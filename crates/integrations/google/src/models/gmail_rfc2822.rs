//! Writing a message the way mail is sent: headers, a blank line, the body,
//! by RFC 5322 and MIME. Gmail takes the whole of it in base64 as `raw`.
//!
//! Everything a caller wrote passes through [`header`] or through base64 on
//! its way in, and nothing else is written. A header is where a mistake
//! becomes someone else's mail: a line break in a subject would end the
//! subject and start a header of the attacker's choosing, such as `Bcc`.

use base64::Engine;
use base64::engine::general_purpose::{STANDARD, URL_SAFE};

use super::{GmailAddress, GmailSendMessage, gmail_words};

/// What separates the plain text of a message from its HTML.
///
/// Both are written in base64, whose lines are made of letters, digits, `+`,
/// `/` and `=`. A separator line starts with `--`, which base64 never
/// writes, so no content can be mistaken for one, whatever it says.
const BOUNDARY: &str = "=_socketkit_alternative_=";

/// The longest a line of a header is written, where it can be broken.
const LINE: usize = 78;

/// What ties a reply to the message it answers.
pub(crate) struct GmailThreading {
    /// The `Message-ID` of the message answered, in angle brackets.
    pub(crate) in_reply_to: String,
    /// The `Message-ID`s of the thread so far, that one last.
    pub(crate) references: Vec<String>,
}

/// The message ids in a header that was received, each in angle brackets.
///
/// Only what is an id is kept: printable ASCII between `<` and `>`. They are
/// someone else's text on their way into a header of ours.
pub(crate) fn message_ids(header: &str) -> Vec<String> {
    let id = |inside: &str| (1..=250).contains(&inside.len()) && inside.bytes().all(|b| b.is_ascii_graphic());
    header
        .split('<')
        .skip(1)
        .filter_map(|from| from.split_once('>'))
        .filter(|(inside, _)| id(inside))
        .map(|(inside, _)| format!("<{inside}>"))
        .collect()
}

/// Whether `content` may be sent at once: it has to reach someone and say
/// something. A draft may be empty; what cannot be taken back may not.
pub(crate) fn sendable(content: &GmailSendMessage) -> Result<(), String> {
    let lists = [&content.to, &content.cc, &content.bcc];
    if lists.into_iter().flatten().all(Vec::is_empty) {
        return Err("a message needs at least one recipient in `to`, `cc` or `bcc`".to_owned());
    }
    let said = |text: &Option<String>| text.as_deref().is_some_and(|text| !text.trim().is_empty());
    if !said(&content.subject) && !said(&content.text) && !said(&content.html) {
        return Err("a message needs a `subject`, `text` or `html`".to_owned());
    }
    Ok(())
}

/// The message as Gmail takes it in `raw`: base64 in the URL-safe alphabet.
pub(crate) fn raw(content: &GmailSendMessage, thread: Option<&GmailThreading>) -> Result<String, String> {
    Ok(URL_SAFE.encode(written(content, thread)?))
}

/// The message as it travels. The error names the field that cannot be
/// written, and never repeats it.
fn written(content: &GmailSendMessage, thread: Option<&GmailThreading>) -> Result<String, String> {
    let mut out = String::new();
    let lists = [
        ("To", "to", &content.to),
        ("Cc", "cc", &content.cc),
        ("Bcc", "bcc", &content.bcc),
    ];
    for (name, field, people) in lists {
        let people = people.as_deref().unwrap_or_default();
        if !people.is_empty() {
            header(&mut out, name, &addresses(field, people)?)?;
        }
    }
    let subject = content.subject.as_deref().map(str::trim).unwrap_or_default();
    if subject.chars().any(char::is_control) {
        return Err("`subject` has a line break or another control character".to_owned());
    }
    if !subject.is_empty() {
        header(&mut out, "Subject", &unstructured(subject))?;
    }
    if let Some(thread) = thread {
        header(&mut out, "In-Reply-To", std::slice::from_ref(&thread.in_reply_to))?;
        header(&mut out, "References", &thread.references)?;
    }
    out.push_str("MIME-Version: 1.0\r\n");
    // A body with nothing in it is no body.
    fn given(body: &Option<String>) -> Option<&str> {
        body.as_deref().filter(|body| !body.is_empty())
    }
    match (given(&content.text), given(&content.html)) {
        (Some(text), Some(html)) => {
            out.push_str(&format!(
                "Content-Type: multipart/alternative; boundary=\"{BOUNDARY}\"\r\n\r\n"
            ));
            // The plainest form first: a reader shows the last one it can.
            for (kind, body) in [("text/plain", text), ("text/html", html)] {
                out.push_str(&format!("--{BOUNDARY}\r\n"));
                part(&mut out, kind, body);
            }
            out.push_str(&format!("--{BOUNDARY}--\r\n"));
        }
        (None, Some(html)) => part(&mut out, "text/html", html),
        (text, None) => part(&mut out, "text/plain", text.unwrap_or_default()),
    }
    Ok(out)
}

/// A list of people as the pieces of one header, with a comma after each
/// but the last.
fn addresses(field: &str, people: &[GmailAddress]) -> Result<Vec<String>, String> {
    let mut pieces = Vec::new();
    for (at, person) in people.iter().enumerate() {
        let mut written = person
            .written()
            .map_err(|problem| format!("`{field}[{at}]` {problem}"))?;
        if let (true, Some(last)) = (at + 1 < people.len(), written.last_mut()) {
            last.push(',');
        }
        pieces.extend(written);
    }
    Ok(pieces)
}

/// Free text as the pieces of a header: its own words where they are plain
/// ASCII and short enough to break a line between, and encoded words for
/// anything else, in any language and of any length.
fn unstructured(text: &str) -> Vec<String> {
    let plain = |word: &str| !word.is_empty() && word.len() <= 60 && word.bytes().all(|b| b.is_ascii_graphic());
    if !text.contains("=?") && text.split(' ').all(plain) {
        text.split(' ').map(str::to_owned).collect()
    } else {
        gmail_words::encoded(text)
    }
}

/// Writes one header, breaking the line between pieces where it would pass
/// 78 characters.
///
/// This is the only place a header is written, and it refuses a piece that
/// is not printable ASCII whatever the code above let through: the line
/// breaks of a header are the ones written here and no others.
fn header(out: &mut String, name: &str, pieces: &[String]) -> Result<(), String> {
    let printable = |piece: &String| piece.bytes().all(|byte| (0x20..=0x7e).contains(&byte));
    if !pieces.iter().all(printable) {
        return Err(format!(
            "the `{name}` header cannot hold a line break or anything outside printable ASCII"
        ));
    }
    let mut line = format!("{name}:");
    for piece in pieces {
        if line.len() + 1 + piece.len() > LINE && line.len() > name.len() + 1 {
            out.push_str(&line);
            out.push_str("\r\n");
            line.clear();
        }
        line.push(' ');
        line.push_str(piece);
    }
    out.push_str(&line);
    out.push_str("\r\n");
    Ok(())
}

/// Writes one body with the headers that say what it is. In base64 any text
/// survives: a line of any length, a line that starts with `From ` or is a
/// single dot, bytes a mail server would otherwise rewrite.
fn part(out: &mut String, kind: &str, body: &str) {
    out.push_str(&format!(
        "Content-Type: {kind}; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n"
    ));
    // Mail ends every line of text with CR LF, whatever the text came with.
    let lines = body.replace("\r\n", "\n").replace('\r', "\n").replace('\n', "\r\n");
    let encoded = STANDARD.encode(lines);
    // base64 is ASCII, so any 76 bytes of it are 76 characters.
    for line in encoded.as_bytes().chunks(76) {
        out.extend(line.iter().copied().map(char::from));
        out.push_str("\r\n");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(content: &GmailSendMessage) -> String {
        written(content, None).unwrap()
    }

    fn to(email: &str) -> Option<Vec<GmailAddress>> {
        Some(vec![GmailAddress::new(email)])
    }

    /// The text a part holds, read back out of its base64 lines.
    fn body_of(part: &str) -> String {
        let (_, encoded) = part.split_once("\r\n\r\n").unwrap();
        String::from_utf8(STANDARD.decode(encoded.replace("\r\n", "")).unwrap()).unwrap()
    }

    #[test]
    fn a_plain_message_is_its_headers_a_blank_line_and_its_text() {
        let content = GmailSendMessage {
            to: Some(vec![
                GmailAddress::named("grace@example.test", "Grace Hopper"),
                GmailAddress::new("alan@example.test"),
            ]),
            cc: to("ada@example.test"),
            bcc: to("quiet@example.test"),
            subject: Some("  Monday  ".into()),
            text: Some("See you Monday.".into()),
            html: None,
        };
        assert_eq!(
            message(&content),
            "To: Grace Hopper <grace@example.test>, alan@example.test\r\n\
             Cc: ada@example.test\r\n\
             Bcc: quiet@example.test\r\n\
             Subject: Monday\r\n\
             MIME-Version: 1.0\r\n\
             Content-Type: text/plain; charset=UTF-8\r\n\
             Content-Transfer-Encoding: base64\r\n\
             \r\n\
             U2VlIHlvdSBNb25kYXku\r\n"
        );
        // An empty draft is still a message.
        assert_eq!(
            message(&GmailSendMessage::default()),
            "MIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n"
        );
    }

    #[test]
    fn no_field_can_start_a_header_of_its_own() {
        let injected = "x\r\nBcc: eve@example.test";
        type With = fn(String) -> GmailSendMessage;
        let fields: [(&str, With); 5] = [
            ("`subject`", |v| GmailSendMessage {
                subject: Some(v),
                ..Default::default()
            }),
            ("`to[0]`", |v| GmailSendMessage {
                to: Some(vec![GmailAddress::new(v)]),
                ..Default::default()
            }),
            ("`cc[0]`", |v| GmailSendMessage {
                cc: Some(vec![GmailAddress::named("a@example.test", v)]),
                ..Default::default()
            }),
            ("`bcc[1]`", |v| GmailSendMessage {
                bcc: Some(vec![GmailAddress::new("a@example.test"), GmailAddress::new(v)]),
                ..Default::default()
            }),
            ("`to[0]`", |v| GmailSendMessage {
                to: Some(vec![GmailAddress::new(format!("a@example.test{v}"))]),
                ..Default::default()
            }),
        ];
        for (field, with) in fields {
            for breaking in [
                injected,
                "x\nBcc: eve@example.test",
                "x\rBcc: eve@example.test",
                "x\u{0}",
                "x\u{85}y",
            ] {
                let refused = written(&with(breaking.to_owned()), None).unwrap_err();
                assert!(refused.starts_with(field), "{field} {breaking:?}: {refused}");
                assert!(!refused.contains("eve"), "the refusal does not repeat what was given");
            }
        }
        // The last gate holds by itself, whatever is handed to it.
        let mut out = String::new();
        for piece in [
            "<a@b>\r\nBcc: eve@example.test",
            "a\nb",
            "a\rb",
            "a\u{2028}b",
            "a\u{85}b",
            "a\tb",
            "é",
        ] {
            assert!(
                header(&mut out, "References", &[piece.to_owned()]).is_err(),
                "{piece:?}"
            );
        }
        assert_eq!(out, "", "nothing of a refused header is written");
        // In a body the same text is only text.
        let body = GmailSendMessage {
            text: Some(injected.into()),
            ..Default::default()
        };
        let sent = message(&body);
        assert!(!sent.contains("Bcc"), "{sent}");
        assert_eq!(body_of(&sent), injected);
    }

    #[test]
    fn whatever_a_subject_or_a_name_holds_the_headers_are_the_ones_written_here() {
        // Every pair of characters that means something to mail or to a
        // terminal, ahead of a header someone would like to add.
        let odd = [
            '\r', '\n', '\0', '\t', '\u{b}', '\u{c}', '\u{7f}', '\u{85}', '\u{2028}', '\u{2029}', '\u{feff}', '"',
            '\\', '<', '>', ',', ';', ':', '(', ')', '@', '=', '?', 'é', ' ',
        ];
        let known = [
            "To:",
            "Subject:",
            "MIME-Version:",
            "Content-Type:",
            "Content-Transfer-Encoding:",
            " ",
        ];
        for (first, second) in odd.iter().flat_map(|a| odd.iter().map(move |b| (a, b))) {
            let text = format!("x{first}{second}Bcc: eve@example.test");
            let content = GmailSendMessage {
                to: Some(vec![GmailAddress::named("grace@example.test", text.clone())]),
                subject: Some(text.clone()),
                ..Default::default()
            };
            let controls = text.chars().any(char::is_control);
            let Ok(sent) = written(&content, None) else {
                assert!(controls, "{text:?} was refused, and holds nothing to refuse");
                continue;
            };
            assert!(!controls, "{text:?} holds a control character, and was written");
            let (headers, _) = sent.split_once("\r\n\r\n").unwrap();
            assert!(!headers.replace("\r\n", "").contains(['\r', '\n']), "{headers:?}");
            for line in headers.split("\r\n") {
                assert!(known.iter().any(|start| line.starts_with(start)), "{text:?}: {line:?}");
                assert!(line.len() <= LINE && line.is_ascii(), "{text:?}: {line:?}");
            }
            // And both read back as exactly what was given.
            let unfolded = headers.replace("\r\n ", " ");
            let value = |name: &str| {
                let line = unfolded.split("\r\n").find(|line| line.starts_with(name)).unwrap();
                line[name.len() + 1..].to_owned()
            };
            assert_eq!(gmail_words::decoded(&value("Subject:")), text);
            let read = super::super::gmail_address::listed(&value("To:"));
            assert_eq!(
                read,
                [GmailAddress::named("grace@example.test", text.clone())],
                "{text:?}"
            );
        }
    }

    #[test]
    fn a_subject_in_any_language_and_of_any_length_reads_back_whole() {
        for subject in [
            "Grüße aus Wien",
            "日本語のとても長い件名、これは一行に収まらないので、いくつかの語に分けて書かれます",
            "Plan 🚀",
            "A subject that is plain ASCII and still much too long to be written on the one line that a header starts with",
            "https://example.test/a/very/long/address/that/has/no/space/to/break/a/line/at/anywhere/in/it",
            "two  spaces",
            "=?UTF-8?Q?not_an_encoded_word?=",
        ] {
            let sent = message(&GmailSendMessage {
                subject: Some(subject.into()),
                ..Default::default()
            });
            let (headers, _) = sent.split_once("\r\nMIME-Version").unwrap();
            assert!(headers.is_ascii(), "{headers}");
            assert!(headers.split("\r\n").all(|line| line.len() <= LINE), "{headers}");
            let value = headers.replace("\r\n", "").replacen("Subject: ", "", 1);
            assert_eq!(gmail_words::decoded(&value), subject);
        }
    }

    #[test]
    fn a_long_list_of_people_is_broken_between_them() {
        let people: Vec<_> = (0..6)
            .map(|n| GmailAddress::named(format!("person{n}@example.test"), "Zoë Müller"))
            .collect();
        let sent = message(&GmailSendMessage {
            to: Some(people),
            ..Default::default()
        });
        let (headers, _) = sent.split_once("\r\nMIME-Version").unwrap();
        let lines: Vec<&str> = headers.split("\r\n").collect();
        assert!(
            lines.len() > 1 && lines.iter().all(|line| line.len() <= LINE),
            "{headers}"
        );
        assert!(
            lines[1..].iter().all(|line| line.starts_with(' ')),
            "a line that continues starts with a space"
        );
        let unfolded = headers.replace("\r\n", "");
        assert_eq!(unfolded.matches("=?UTF-8?B?Wm/DqyBNw7xsbGVy?= <person").count(), 6);
        assert!(unfolded.ends_with("<person5@example.test>"), "no comma after the last");
    }

    #[test]
    fn text_and_html_are_two_forms_of_one_message() {
        let text = "--=_socketkit_alternative_=\nFrom here\n.\n";
        let html = "<p>Grüße</p>\r\n--=_socketkit_alternative_=--";
        let both = GmailSendMessage {
            text: Some(text.into()),
            html: Some(html.into()),
            ..Default::default()
        };
        let sent = message(&both);
        let (headers, body) = sent.split_once("\r\n\r\n").unwrap();
        assert_eq!(
            headers,
            "MIME-Version: 1.0\r\nContent-Type: multipart/alternative; boundary=\"=_socketkit_alternative_=\""
        );
        // The separator stands three times and nowhere else, though both bodies spell it out.
        let parts: Vec<&str> = body.split("--=_socketkit_alternative_=").collect();
        assert_eq!(parts.len(), 4, "{body}");
        assert_eq!((parts[0], parts[3]), ("", "--\r\n"));
        assert!(
            parts[1].starts_with(
                "\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n"
            )
        );
        assert!(
            parts[2]
                .starts_with("\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n")
        );
        assert_eq!(body_of(parts[1]), "--=_socketkit_alternative_=\r\nFrom here\r\n.\r\n");
        assert_eq!(body_of(parts[2]), html);

        // HTML alone is one part, and text that is empty is no text.
        let html_only = GmailSendMessage {
            text: Some(String::new()),
            html: Some("<p>x</p>".into()),
            ..Default::default()
        };
        assert!(message(&html_only).contains("MIME-Version: 1.0\r\nContent-Type: text/html; charset=UTF-8\r\n"));
    }

    #[test]
    fn a_long_body_is_written_in_lines_of_76_and_keeps_every_byte() {
        let text = format!("{}\r\n\tTab, NUL \u{0}, lone CR \r and 🚀", "long ".repeat(400));
        let sent = message(&GmailSendMessage {
            text: Some(text.clone()),
            ..Default::default()
        });
        let (_, encoded) = sent.split_once("\r\n\r\n").unwrap();
        let lines: Vec<&str> = encoded.trim_end_matches("\r\n").split("\r\n").collect();
        assert!(lines.len() > 30);
        assert!(lines[..lines.len() - 1].iter().all(|line| line.len() == 76));
        assert_eq!(body_of(&sent), text.replace(" \r ", " \r\n "));
    }

    #[test]
    fn a_reply_names_the_message_it_answers_and_the_thread_before_it() {
        let thread = GmailThreading {
            in_reply_to: "<m2@mail.example.test>".into(),
            references: vec!["<m1@mail.example.test>".into(), "<m2@mail.example.test>".into()],
        };
        let reply = GmailSendMessage {
            to: to("grace@example.test"),
            subject: Some("Re: Plan".into()),
            ..Default::default()
        };
        let sent = written(&reply, Some(&thread)).unwrap();
        assert!(sent.starts_with(
            "To: grace@example.test\r\nSubject: Re: Plan\r\nIn-Reply-To: <m2@mail.example.test>\r\n\
             References: <m1@mail.example.test> <m2@mail.example.test>\r\nMIME-Version: 1.0\r\n"
        ));
    }

    #[test]
    fn only_message_ids_are_taken_from_a_received_header() {
        assert_eq!(
            message_ids("<m1@mail.example.test>\r\n <m2@mail.example.test>"),
            ["<m1@mail.example.test>", "<m2@mail.example.test>"]
        );
        assert_eq!(message_ids("<a@b> junk <c d@e> <> <f@g>,<h@i"), ["<a@b>", "<f@g>"]);
        assert_eq!(message_ids("<a@b\r\nBcc: eve@example.test>"), Vec::<String>::new());
        assert_eq!(message_ids("<<a@b>"), ["<a@b>"]);
        assert_eq!(message_ids(&format!("<{}@b>", "a".repeat(250))), Vec::<String>::new());
        assert_eq!(message_ids("no ids here"), Vec::<String>::new());
    }

    #[test]
    fn what_is_sent_at_once_has_to_reach_someone_and_say_something() {
        let nobody = GmailSendMessage {
            subject: Some("x".into()),
            to: Some(vec![]),
            ..Default::default()
        };
        assert!(sendable(&nobody).unwrap_err().contains("recipient"));
        let nothing = GmailSendMessage {
            bcc: to("a@example.test"),
            subject: Some(" ".into()),
            text: Some("\n".into()),
            ..Default::default()
        };
        assert!(sendable(&nothing).unwrap_err().contains("`subject`, `text` or `html`"));
        for said in [
            GmailSendMessage {
                cc: to("a@example.test"),
                subject: Some("x".into()),
                ..Default::default()
            },
            GmailSendMessage {
                bcc: to("a@example.test"),
                html: Some("<p>x</p>".into()),
                ..Default::default()
            },
        ] {
            assert_eq!(sendable(&said), Ok(()));
        }
    }
}
