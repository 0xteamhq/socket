//! The people a Gmail message is addressed to.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::gmail_words;

/// Someone a message goes to.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GmailAddress {
    /// The mailbox, such as `grace@example.test`. One address, in ASCII.
    pub email: String,
    /// The name shown beside it, such as `Grace Hopper`.
    #[serde(default)]
    pub name: Option<String>,
}

impl GmailAddress {
    /// A mailbox with no name beside it.
    pub fn new(email: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            name: None,
        }
    }

    /// A mailbox with the name to show beside it.
    pub fn named(email: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            name: Some(name.into()),
        }
    }

    /// The address as a header holds it, in pieces a line may be broken
    /// between: `Grace`, `Hopper`, `<grace@example.test>`. The error says
    /// what is wrong with it, without repeating it.
    ///
    /// Nothing the caller wrote reaches a header unchecked. The mailbox is
    /// one address made of the characters an address is made of, and the name
    /// has no control character, so neither can end the header and begin
    /// another.
    pub(super) fn written(&self) -> Result<Vec<String>, &'static str> {
        let email = mailbox(&self.email).ok_or("needs an `email` that is one mailbox, such as ada@example.test")?;
        let name = self.name.as_deref().map(str::trim).unwrap_or_default();
        if name.chars().any(char::is_control) {
            return Err("has a `name` with a line break or another control character");
        }
        if name.is_empty() {
            return Ok(vec![email.to_owned()]);
        }
        let mut pieces = phrase(name);
        pieces.push(format!("<{email}>"));
        Ok(pieces)
    }
}

/// The characters a word of a name, or of the part of an address before
/// the `@`, may be made of without being quoted.
fn atom(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-/=?^_`{|}~".contains(&byte)
}

/// `text` when it is one mailbox, with the space around it removed.
///
/// Stricter than mail allows: no quoted part, no address in brackets, no
/// characters outside ASCII. What passes is safe to write in a header as it is.
pub(super) fn mailbox(text: &str) -> Option<&str> {
    let text = text.trim();
    let (local, domain) = text.rsplit_once('@')?;
    let local_fits = (1..=64).contains(&local.len())
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local.bytes().all(|byte| atom(byte) || byte == b'.');
    let label = |label: &str| {
        (1..=63).contains(&label.len())
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    };
    let domain_fits = domain.len() <= 253 && domain.contains('.') && domain.split('.').all(label);
    (local_fits && domain_fits).then_some(text)
}

/// A name as a header holds it: bare words where they need nothing more,
/// in quotes where a character has a meaning of its own (`Hopper, Grace`),
/// and as encoded words where it leaves ASCII or would not fit on a line.
fn phrase(name: &str) -> Vec<String> {
    let printable = name.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) && !name.contains("=?");
    let bare = |word: &str| !word.is_empty() && word.len() <= 60 && word.bytes().all(atom);
    if printable && name.split(' ').all(bare) {
        name.split(' ').map(str::to_owned).collect()
    } else if printable && name.len() <= 60 {
        vec![format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))]
    } else {
        gmail_words::encoded(name)
    }
}

/// The mailboxes named in a header that was received, such as `From` or
/// `Reply-To`, as Gmail sent it. What is not one mailbox is left out.
pub(super) fn listed(header: &str) -> Vec<GmailAddress> {
    // Commas separate the addresses, except inside a quoted name, a comment
    // in parentheses, or the angle brackets around a mailbox.
    let mut pieces = vec![String::new()];
    let (mut quoted, mut comment, mut escaped) = (false, 0_u32, false);
    for character in header.chars() {
        match character {
            _ if escaped => escaped = false,
            '\\' if quoted || comment > 0 => escaped = true,
            '"' if comment == 0 => quoted = !quoted,
            '(' if !quoted => comment += 1,
            ')' if !quoted && comment > 0 => {
                comment -= 1;
                continue;
            }
            ',' if !quoted && comment == 0 => {
                pieces.push(String::new());
                continue;
            }
            _ => {}
        }
        if let (0, Some(piece)) = (comment, pieces.last_mut()) {
            piece.push(character);
        }
    }
    pieces.iter().filter_map(|piece| one(piece)).collect()
}

fn one(piece: &str) -> Option<GmailAddress> {
    let piece = piece.trim();
    let (name, email) = match piece.rfind('<') {
        Some(open) if piece.ends_with('>') => (&piece[..open], &piece[open + 1..piece.len() - 1]),
        _ => ("", piece),
    };
    let name = name.trim();
    // Inside quotes a backslash stands before a character that is meant as itself.
    let unquoted = name
        .strip_prefix('"')
        .and_then(|name| name.strip_suffix('"'))
        .map(|name| {
            let mut escaped = false;
            let kept = |c: &char| {
                escaped = !escaped && *c == '\\';
                !escaped
            };
            name.chars().filter(kept).collect::<String>()
        });
    // The name is someone else's text. Whatever it hides in an encoded word,
    // it is written out again by `written`, which lets no control character by.
    let name: String = gmail_words::decoded(unquoted.as_deref().unwrap_or(name))
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    Some(GmailAddress {
        email: mailbox(email)?.to_owned(),
        name: Some(name.trim().to_owned()).filter(|name| !name.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use base64::Engine;

    use super::*;

    #[test]
    fn a_mailbox_is_one_plain_address_and_nothing_else() {
        for good in [
            "ada@example.test",
            "  ada@example.test ",
            "ada.lovelace+notes@mail.example.test",
            "o'brien_1@sub-domain.example.test",
        ] {
            assert_eq!(mailbox(good), Some(good.trim()), "{good}");
        }
        for bad in [
            "",
            " ",
            "ada",
            "ada@",
            "@example.test",
            "ada@localhost",
            "ada@example..test",
            "ada@-example.test",
            ".ada@example.test",
            "ada.@example.test",
            "a..da@example.test",
            "ada@example.test, grace@example.test",
            "ada@example.test grace@example.test",
            "Ada <ada@example.test>",
            "<ada@example.test>",
            "\"ada lovelace\"@example.test",
            "ada@[192.0.2.1]",
            "ada@example.test\r\nBcc: eve@example.test",
            "ada@example.test\nBcc: eve@example.test",
            "ada\r@example.test",
            "ada@exämple.test",
            "adä@example.test",
            "ada@example.test>",
            "ada(comment)@example.test",
        ] {
            assert_eq!(mailbox(bad), None, "{bad:?}");
        }
        assert_eq!(mailbox(&format!("{}@example.test", "a".repeat(65))), None);
    }

    #[test]
    fn a_name_is_written_bare_quoted_or_encoded_as_it_needs() {
        let written = |name: &str| GmailAddress::named("grace@example.test", name).written().unwrap();
        assert_eq!(written("Grace Hopper"), ["Grace", "Hopper", "<grace@example.test>"]);
        assert_eq!(written("  "), ["grace@example.test"]);
        assert_eq!(written("Hopper, Grace"), ["\"Hopper, Grace\"", "<grace@example.test>"]);
        // A quote or a backslash cannot end the quoted name early.
        assert_eq!(
            written("Grace \"Amazing\" Hopper \\ <eve@example.test>"),
            [
                "\"Grace \\\"Amazing\\\" Hopper \\\\ <eve@example.test>\"",
                "<grace@example.test>"
            ]
        );
        assert_eq!(
            written("Zoë Müller"),
            ["=?UTF-8?B?Wm/DqyBNw7xsbGVy?=", "<grace@example.test>"]
        );
        // What looks like an encoded word is the caller's own text, and is kept as text.
        let disguised = written("=?UTF-8?Q?x?=");
        assert_eq!(gmail_words::decoded(&disguised[0]), "=?UTF-8?Q?x?=");
        assert_eq!(
            GmailAddress::new("ada@example.test").written().unwrap(),
            ["ada@example.test"]
        );
    }

    #[test]
    fn an_address_that_could_break_out_of_its_header_is_refused() {
        for name in [
            "Grace\r\nBcc: eve@example.test",
            "Grace\nHopper",
            "Grace\rHopper",
            "Grace\u{0}",
            "a\tb",
        ] {
            let refused = GmailAddress::named("grace@example.test", name).written().unwrap_err();
            assert!(refused.contains("`name`"), "{name:?}: {refused}");
            assert!(!refused.contains("eve"), "the refusal does not repeat the name");
        }
        for email in [
            "",
            "grace",
            "grace@example.test\r\nBcc: eve@example.test",
            "a@b.test, c@d.test",
        ] {
            let refused = GmailAddress::new(email).written().unwrap_err();
            assert!(refused.contains("`email`"), "{email:?}: {refused}");
            assert!(!refused.contains("eve"), "the refusal does not repeat the address");
        }
    }

    #[test]
    fn the_mailboxes_of_a_received_header_are_read_whatever_surrounds_them() {
        let read = |header: &str| -> Vec<(String, Option<String>)> {
            listed(header).into_iter().map(|a| (a.email, a.name)).collect()
        };
        let named = |email: &str, name: &str| (email.to_owned(), Some(name.to_owned()));
        let bare = |email: &str| (email.to_owned(), None);
        assert_eq!(read("grace@example.test"), [bare("grace@example.test")]);
        assert_eq!(read("<grace@example.test>"), [bare("grace@example.test")]);
        assert_eq!(
            read("Grace Hopper <grace@example.test>, alan@example.test"),
            [named("grace@example.test", "Grace Hopper"), bare("alan@example.test")]
        );
        // A comma, a bracket or an escaped quote inside a quoted name is part of the name.
        assert_eq!(
            read("\"Hopper, Grace <not@this.test>\" <grace@example.test>"),
            [named("grace@example.test", "Hopper, Grace <not@this.test>")]
        );
        assert_eq!(
            read("\"Grace \\\"G\\\" Hopper\" <grace@example.test>"),
            [named("grace@example.test", "Grace \"G\" Hopper")]
        );
        assert_eq!(
            read("=?UTF-8?B?Wm/DqyBNw7xsbGVy?= <zoe@example.test>"),
            [named("zoe@example.test", "Zoë Müller")]
        );
        assert_eq!(
            read("grace@example.test (Grace, at work), Alan (he) <alan@example.test>"),
            [bare("grace@example.test"), named("alan@example.test", "Alan")]
        );
        // What is not one mailbox is left out: a group, a broken address, nothing at all.
        assert_eq!(read("undisclosed-recipients:;"), []);
        assert_eq!(read("Grace <grace at example.test>, , <>"), []);
        assert_eq!(read(""), []);
        // A line break hidden in an encoded name does not survive as one.
        let word = base64::engine::general_purpose::STANDARD.encode("Grace\r\nBcc: eve@example.test");
        let hidden = listed(&format!("=?UTF-8?B?{word}?= <grace@example.test>"));
        assert_eq!(hidden[0].name.as_deref(), Some("Grace  Bcc: eve@example.test"));
        assert!(hidden[0].written().is_ok());
    }
}
