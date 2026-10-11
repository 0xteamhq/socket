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
/// characters outside ASCII, and nothing that starts an encoded word (`=?`),
/// which a mail program that reads those words first would turn into other
/// text, and so into another address. What passes is safe to write in a
/// header as it is.
pub(super) fn mailbox(text: &str) -> Option<&str> {
    let text = text.trim();
    let (local, domain) = text.rsplit_once('@')?;
    let local_fits = (1..=64).contains(&local.len())
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && !local.contains("=?")
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
///
/// A name that holds either end of an encoded word (`=?`, `?=`) is written
/// as encoded words too: left as it is, it could be read as one, alone or
/// together with what stands beside it in the header.
fn phrase(name: &str) -> Vec<String> {
    let printable =
        name.bytes().all(|byte| (0x20..=0x7e).contains(&byte)) && !name.contains("=?") && !name.contains("?=");
    let bare = |word: &str| !word.is_empty() && word.len() <= 60 && word.bytes().all(atom);
    if printable && name.split(' ').all(bare) {
        name.split(' ').map(str::to_owned).collect()
    } else if printable && name.len() <= 60 {
        vec![format!("\"{}\"", name.replace('\\', "\\\\").replace('"', "\\\""))]
    } else {
        gmail_words::encoded(name)
    }
}

/// One entry of a header of people that was received.
enum Entry {
    /// One mailbox, with the name beside it.
    Mailbox(GmailAddress),
    /// What is not one mailbox: a group, a broken address, anything else.
    Text(String),
}

/// A header of people that was received, such as `From` or `To`, taken
/// apart. `header` is the value unfolded, as a message's headers are handed
/// out, and this is the one place such a value is read: whatever is shown of
/// it, or done with it, comes from these entries, so no two readers can take
/// the same header for different people.
///
/// A header that ends inside a quoted name or inside a comment is not taken
/// apart at all. Mail programs disagree on where such a header's addresses
/// are, and a comment that is never closed would hide the rest of the header
/// from a reader, so the whole of it is one piece of text.
fn entries(header: &str) -> Vec<Entry> {
    let text = |text: &str| Some(gmail_words::readable(text)).filter(|text| !text.is_empty());
    let Some(pieces) = pieces(header) else {
        return text(header).map(Entry::Text).into_iter().collect();
    };
    let entry = |piece: &String| match one(piece) {
        Some(address) => Some(Entry::Mailbox(address)),
        None => text(piece).map(Entry::Text),
    };
    pieces.iter().filter_map(entry).collect()
}

/// The mailboxes a header names. Only the tests read a header this way:
/// nothing a message says about people decides what is done with it.
#[cfg(test)]
pub(super) fn listed(header: &str) -> Vec<GmailAddress> {
    let mailbox = |entry| match entry {
        Entry::Mailbox(address) => Some(address),
        Entry::Text(_) => None,
    };
    entries(header).into_iter().filter_map(mailbox).collect()
}

/// A header of people as a person reads it: `Grace Hopper
/// <grace@example.test>, ada@example.test`. `None` when it names nobody.
///
/// The name beside an address is the sender's own text, and can be written
/// to look like an address itself: `boss@example.test <eve@example.test>`.
/// So the header is taken apart before its encoded words are read, and a
/// name that is anything but plain words goes back in quotes. The mailbox a
/// message really came from is then always the one in angle brackets, or the
/// one that stands alone. What is not a mailbox at all is shown as text in
/// quotes, never as an address.
pub(super) fn displayed(header: &str) -> Option<String> {
    let shown: Vec<String> = entries(header)
        .into_iter()
        .map(|entry| match entry {
            Entry::Mailbox(GmailAddress {
                email,
                name: Some(name),
            }) => format!("{} <{email}>", name_shown(&name)),
            Entry::Mailbox(GmailAddress { email, name: None }) => email,
            Entry::Text(text) => in_quotes(&text),
        })
        .collect();
    Some(shown.join(", ")).filter(|shown| !shown.is_empty())
}

/// A name as it is shown beside its address: as it is when it is plain
/// words, and in quotes otherwise.
///
/// Plain words are letters and digits of any writing, spaces, and `.`, `-`,
/// `'` and `_`. Anything else is in quotes: the characters an address is
/// written with, and every sign that only looks like one of them, as the
/// full-width `＠`, `＜` and `＞` do.
fn name_shown(name: &str) -> String {
    let plain = |character: char| character.is_alphanumeric() || matches!(character, ' ' | '.' | '-' | '\'' | '_');
    if name.chars().all(plain) {
        name.to_owned()
    } else {
        in_quotes(name)
    }
}

fn in_quotes(text: &str) -> String {
    format!("\"{}\"", text.replace('\\', "\\\\").replace('"', "\\\""))
}

/// The addresses of a header, split at the commas between them. A comma
/// inside a quoted name or a comment in parentheses separates nothing, and a
/// comment is left out. `None` when the header ends inside a quoted name or
/// inside a comment.
fn pieces(header: &str) -> Option<Vec<String>> {
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
    (!quoted && comment == 0).then_some(pieces)
}

fn one(piece: &str) -> Option<GmailAddress> {
    let piece = piece.trim();
    // Angle brackets outside a quoted name. A mailbox is in the one pair of
    // them, at the end. With more than one pair, mail programs disagree on
    // which is the address, so the piece is not read as one at all.
    let mut quoted = false;
    let mut escaped = false;
    let mut brackets = Vec::new();
    for (at, character) in piece.char_indices() {
        match character {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '<' | '>' if !quoted => brackets.push((at, character)),
            _ => {}
        }
    }
    let (name, email) = match brackets[..] {
        [] => ("", piece),
        [(open, '<'), (close, '>')] if close + 1 == piece.len() => (&piece[..open], &piece[open + 1..close]),
        _ => return None,
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
    // The name is someone else's text: its encoded words are read only now,
    // when it is known to be a name, and nothing that is not seen stays in it.
    let name = gmail_words::readable(unquoted.as_deref().unwrap_or(name));
    Some(GmailAddress {
        email: mailbox(email)?.to_owned(),
        name: Some(name).filter(|name| !name.is_empty()),
    })
}

#[cfg(test)]
mod tests {
    use base64::Engine;

    use super::*;

    #[test]
    fn a_name_written_to_look_like_an_address_is_never_shown_as_one() {
        // The name says one address and the brackets another. Encoded, so
        // that it is only a name until its words are read.
        let disguised = "=?UTF-8?Q?boss=40example.test_=3Cboss=40example.test=3E?= <eve@example.test>";
        assert_eq!(
            displayed(disguised).as_deref(),
            Some("\"boss@example.test <boss@example.test>\" <eve@example.test>")
        );
        assert_eq!(
            listed(disguised),
            [GmailAddress::named(
                "eve@example.test",
                "boss@example.test <boss@example.test>"
            )]
        );
        // The same thing already read, as Gmail may send it: quoted, the
        // mailbox is still the one outside the quotes.
        assert_eq!(
            listed("\"boss <boss@example.test>\" <eve@example.test>"),
            [GmailAddress::named("eve@example.test", "boss <boss@example.test>")]
        );
        // Two pairs of brackets: mail programs disagree on which is the
        // address, so it is nobody's, and is shown as the text it is.
        let two = "boss <boss@example.test> <eve@example.test>";
        assert_eq!(listed(two), []);
        assert_eq!(
            displayed(two).as_deref(),
            Some("\"boss <boss@example.test> <eve@example.test>\"")
        );
        // What is not a mailbox is text, in quotes, and never dropped unseen.
        assert_eq!(
            displayed("undisclosed-recipients:;").as_deref(),
            Some("\"undisclosed-recipients:;\"")
        );
        assert_eq!(displayed("grace@localhost").as_deref(), Some("\"grace@localhost\""));
    }

    #[test]
    fn a_header_of_people_is_shown_as_names_and_addresses() {
        assert_eq!(
            displayed("=?UTF-8?Q?Zo=C3=AB_M=C3=BCller?= <zoe@example.test>").as_deref(),
            Some("Zoë Müller <zoe@example.test>")
        );
        assert_eq!(
            displayed("ada@example.test, \"Hopper, Grace\" <grace@example.test> (her own)").as_deref(),
            Some("ada@example.test, \"Hopper, Grace\" <grace@example.test>")
        );
        // A line break an encoded word was hiding does not come out of a name.
        assert_eq!(
            displayed("=?UTF-8?B?R3JhY2UNCkJjYzogZXZlQGV4YW1wbGUudGVzdA==?= <grace@example.test>").as_deref(),
            Some("\"Grace  Bcc: eve@example.test\" <grace@example.test>")
        );
        assert_eq!(displayed(""), None);
        assert_eq!(displayed(" , "), None);
    }

    #[test]
    fn a_header_that_ends_inside_a_comment_or_a_quoted_name_is_text_and_names_no_mailbox() {
        // A comment that is never closed hid the rest of the header: this was
        // shown as `boss@corp.test` alone, with the address in brackets gone.
        for (header, shown) in [
            ("boss@corp.test (<eve@evil.test>", "\"boss@corp.test (<eve@evil.test>\""),
            (
                "boss@corp.test, (a (b) <eve@evil.test>",
                "\"boss@corp.test, (a (b) <eve@evil.test>\"",
            ),
            // A quoted name that is never closed, after an address that was whole.
            (
                "boss@corp.test, \"Eve <eve@evil.test>",
                "\"boss@corp.test, \\\"Eve <eve@evil.test>\"",
            ),
            ("\"boss@corp.test\\", "\"\\\"boss@corp.test\\\\\""),
            // A closing bracket that an escape took: the comment runs to the end.
            (
                "(\\) <eve@evil.test>, <boss@corp.test>",
                "\"(\\\\) <eve@evil.test>, <boss@corp.test>\"",
            ),
        ] {
            assert_eq!(displayed(header).as_deref(), Some(shown), "{header}");
            assert_eq!(listed(header), [], "{header}");
        }
        // Closed, a comment is left out and a quoted name is a name.
        assert_eq!(
            displayed("boss@corp.test (<eve@evil.test>)").as_deref(),
            Some("boss@corp.test")
        );
        assert_eq!(
            displayed("\"Eve (\" <eve@evil.test>").as_deref(),
            Some("\"Eve (\" <eve@evil.test>")
        );
    }

    #[test]
    fn a_name_is_shown_bare_only_when_it_is_plain_words() {
        let shown = |name: &str| {
            let word = base64::engine::general_purpose::STANDARD.encode(name);
            displayed(&format!("=?UTF-8?B?{word}?= <eve@evil.test>")).unwrap()
        };
        for plain in ["Grace Hopper", "Zoë Müller", "日本 語", "O'Brien-Smith Jr.", "ada_1"] {
            assert_eq!(shown(plain), format!("{plain} <eve@evil.test>"));
        }
        // Signs that only look like the ones an address is written with:
        // this was shown bare, as a name and an address before the real one.
        assert_eq!(
            shown("boss＠corp.test ＜boss＠corp.test＞"),
            "\"boss＠corp.test ＜boss＠corp.test＞\" <eve@evil.test>"
        );
        for odd in [
            "a@b",
            "a<b",
            "a>b",
            "a,b",
            "a;b",
            "a:b",
            "a(b",
            "a[b",
            "a\\b",
            "a\"b",
            "a/b",
            "a=b",
            "a!b",
            "a＠b",
            "a﹫b",
            "a‹b",
            "a\u{a0}b",
            "a\u{3000}b",
            "a🚀b",
        ] {
            let quoted = format!("\"{}\"", odd.replace('\\', "\\\\").replace('"', "\\\""));
            assert_eq!(shown(odd), format!("{quoted} <eve@evil.test>"), "{odd}");
        }
        // A mark that turns the writing around, so that the name reads as an
        // address from right to left, is a space by the time it is shown.
        assert_eq!(
            shown("\u{202e}<tset.proc@ssob>\u{202c}"),
            "\"<tset.proc@ssob>\" <eve@evil.test>"
        );
        assert_eq!(
            shown("Gra\u{200b}ce\u{feff} Hop\u{ad}per"),
            "Gra ce  Hop per <eve@evil.test>"
        );
        // A name made only of what is not seen is no name.
        assert_eq!(shown("\u{200b}\u{202e}"), "eve@evil.test");
    }

    #[test]
    fn an_encoded_word_is_never_part_of_an_address_read_or_written() {
        // As a mailbox this was accepted, and written bare into `To`. A mail
        // program that reads encoded words first sees `boss@corp.test,` in it.
        let disguised = "=?utf-8?q?boss=40corp.test=2C?=@evil.test";
        assert_eq!(mailbox(disguised), None);
        assert!(GmailAddress::new(disguised).written().is_err());
        assert_eq!(listed(disguised), []);
        assert_eq!(
            displayed(disguised).as_deref(),
            Some("\"boss@corp.test,@evil.test\""),
            "text in quotes, not a mailbox"
        );
        // A comma inside a word that was not encoded as it should be: this
        // was read as two mailboxes, the first of them `=?UTF-8?Q?boss@corp.test`.
        let split = "=?UTF-8?Q?boss@corp.test,?= <eve@evil.test>";
        assert_eq!(listed(split), [GmailAddress::named("eve@evil.test", "?=")]);
        assert_eq!(
            displayed(split).as_deref(),
            Some("\"=?UTF-8?Q?boss@corp.test\", \"?=\" <eve@evil.test>")
        );
        // Either end of a word in a name is written encoded, so that it
        // cannot join what stands beside it in the header into a word.
        for name in ["?=", "x ?= y", "=?", "a =?UTF-8?Q?b"] {
            let written = GmailAddress::named("grace@example.test", name).written().unwrap();
            let base64 = base64::engine::general_purpose::STANDARD.encode(name);
            assert_eq!(
                written,
                [format!("=?UTF-8?B?{base64}?="), "<grace@example.test>".to_owned()]
            );
            assert_eq!(
                listed(&written.join(" ")),
                [GmailAddress::named("grace@example.test", name)]
            );
        }
        assert_eq!(mailbox("a=b?c@example.test"), Some("a=b?c@example.test"));
    }

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
