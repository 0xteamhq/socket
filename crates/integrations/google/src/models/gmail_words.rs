//! Text as mail carries it: bytes in a named character set, base64, and the
//! "encoded words" of RFC 2047 that put text outside ASCII in a header.

use base64::Engine;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

const LENIENT: GeneralPurposeConfig = GeneralPurposeConfig::new()
    .with_decode_padding_mode(DecodePaddingMode::Indifferent)
    .with_decode_allow_trailing_bits(true);
const URL_SAFE: GeneralPurpose = GeneralPurpose::new(&alphabet::URL_SAFE, LENIENT);
const STANDARD: GeneralPurpose = GeneralPurpose::new(&alphabet::STANDARD, LENIENT);

/// The most bytes of text in one encoded word: with its frame the word is 68
/// characters, which fits on a line of 78 beside `Subject:`.
const WORD_BYTES: usize = 42;

/// Reads base64 as Gmail writes it: the URL-safe alphabet, padded or not.
/// The standard alphabet is read too, since Google's reference names both.
pub(super) fn from_base64(text: &str) -> Option<Vec<u8>> {
    let engine = if text.contains(['+', '/']) { STANDARD } else { URL_SAFE };
    engine.decode(text).ok()
}

/// The character sets that are read without a table of their own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Charset {
    /// UTF-8, and ASCII, which is the same bytes.
    Utf8,
    /// Windows-1252, and ISO-8859-1, which mail uses as another name for it.
    Western,
}

/// What Windows-1252 puts where ISO-8859-1 has control characters, from 0x80.
const WESTERN: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’',
    '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

fn charset(name: &str) -> Option<Charset> {
    // A name may carry a language after `*`, as `utf-8*en` does.
    let name = name.split('*').next().unwrap_or_default().trim().to_ascii_lowercase();
    match name.as_str() {
        "utf-8" | "utf8" | "us-ascii" | "ascii" => Some(Charset::Utf8),
        "iso-8859-1" | "latin1" | "windows-1252" | "cp1252" => Some(Charset::Western),
        _ => None,
    }
}

fn text(charset: Charset, bytes: &[u8]) -> String {
    match charset {
        Charset::Utf8 => String::from_utf8_lossy(bytes).into_owned(),
        Charset::Western => bytes
            .iter()
            .map(|&byte| match byte {
                0x80..=0x9f => WESTERN[usize::from(byte - 0x80)],
                _ => char::from(byte),
            })
            .collect(),
    }
}

/// The text `bytes` hold in the character set a part names.
///
/// A set that is not known here, or none, is read as UTF-8, and whatever is
/// not UTF-8 becomes the replacement character: most of a message is better
/// than none of it.
pub(super) fn text_in(named: Option<&str>, bytes: &[u8]) -> String {
    text(named.and_then(charset).unwrap_or(Charset::Utf8), bytes)
}

/// The encoded word `text` starts with: its character set, the bytes it
/// holds, and its length. `None` when it is not one, or names a character
/// set that is not known, and so is left as it stands.
fn word(text: &str) -> Option<(Charset, Vec<u8>, usize)> {
    let rest = text.strip_prefix("=?")?;
    let (name, rest) = rest.split_once('?')?;
    let (encoding, rest) = rest.split_once('?')?;
    let (content, _) = rest.split_once("?=")?;
    if content.contains(|c: char| c.is_whitespace() || c == '?') {
        return None;
    }
    let bytes = match encoding {
        "B" | "b" => STANDARD.decode(content).ok()?,
        "Q" | "q" => quoted(content)?,
        _ => return None,
    };
    let length = "=?".len() + name.len() + 1 + encoding.len() + 1 + content.len() + "?=".len();
    Some((charset(name)?, bytes, length))
}

/// The bytes of a `Q` word: `_` is a space, and `=41` is the byte 0x41.
fn quoted(content: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(content.len());
    let mut rest = content.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        rest = after;
        match byte {
            b'_' => bytes.push(b' '),
            b'=' => {
                let hex = std::str::from_utf8(rest.get(..2)?).ok()?;
                bytes.push(u8::from_str_radix(hex, 16).ok()?);
                rest = &rest[2..];
            }
            other => bytes.push(other),
        }
    }
    Some(bytes)
}

/// A header's value with its encoded words read: `=?UTF-8?B?…?=` and
/// `=?iso-8859-1?Q?…?=` become the text they stand for.
///
/// Google's reference does not say whether header values arrive decoded.
/// Reading them is harmless when they do: text without encoded words comes
/// back as it was. Words that follow one another are one text, so a
/// character split between two is still read, and the space between them is
/// not part of it.
pub(super) fn decoded(value: &str) -> String {
    fn flush(out: &mut String, held: &mut Option<(Charset, Vec<u8>)>) {
        if let Some((charset, bytes)) = held.take() {
            out.push_str(&text(charset, &bytes));
        }
    }
    let mut out = String::with_capacity(value.len());
    let mut held: Option<(Charset, Vec<u8>)> = None;
    let mut rest = value;
    while let Some(at) = rest.find("=?") {
        let (before, from) = rest.split_at(at);
        let Some((charset, bytes, length)) = word(from) else {
            flush(&mut out, &mut held);
            out.push_str(before);
            out.push_str("=?");
            rest = &from[2..];
            continue;
        };
        if held.is_none() || !before.chars().all(char::is_whitespace) {
            flush(&mut out, &mut held);
            out.push_str(before);
        }
        match &mut held {
            Some((same, all)) if *same == charset => all.extend(bytes),
            _ => {
                flush(&mut out, &mut held);
                held = Some((charset, bytes));
            }
        }
        rest = &from[length..];
    }
    flush(&mut out, &mut held);
    out.push_str(rest);
    out
}

/// `text` as encoded words in UTF-8, to be written with a space or a line
/// break between them. Each holds whole characters.
pub(super) fn encoded(text: &str) -> Vec<String> {
    let word = |chunk: &str| {
        format!(
            "=?UTF-8?B?{}?=",
            base64::engine::general_purpose::STANDARD.encode(chunk)
        )
    };
    let mut words = Vec::new();
    let mut chunk = String::new();
    for character in text.chars() {
        if chunk.len() + character.len_utf8() > WORD_BYTES {
            words.push(word(&chunk));
            chunk.clear();
        }
        chunk.push(character);
    }
    if !chunk.is_empty() {
        words.push(word(&chunk));
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_is_read_padded_or_not_and_in_either_alphabet() {
        // "ab?>" is `YWI_Pg` in the URL-safe alphabet and `YWI/Pg` in the standard one.
        for written in ["YWI_Pg", "YWI_Pg==", "YWI/Pg==", "YWI/Pg"] {
            assert_eq!(from_base64(written).as_deref(), Some(&b"ab?>"[..]), "{written}");
        }
        assert_eq!(from_base64("").as_deref(), Some(&b""[..]));
        for broken in ["YWI_Pg=x", "not base64!", "YWI_/g", "Y"] {
            assert_eq!(from_base64(broken), None, "{broken}");
        }
    }

    #[test]
    fn encoded_words_are_read_in_both_encodings_and_the_usual_character_sets() {
        for (written, read) in [
            ("=?UTF-8?B?R3LDvMOfZQ==?=", "Grüße"),
            ("=?utf-8?q?Gr=C3=BC=C3=9Fe_aus_Wien?=", "Grüße aus Wien"),
            ("=?ISO-8859-1?Q?Caf=E9?= menu", "Café menu"),
            ("=?windows-1252?Q?=93quoted=94_=80?=", "“quoted” €"),
            ("=?us-ascii*en?Q?plain?=", "plain"),
            ("Re: =?UTF-8?B?5pel5pys6Kqe?= (fwd)", "Re: 日本語 (fwd)"),
            // Words that follow one another are one text; the space between them is not.
            ("=?UTF-8?Q?a?= =?UTF-8?Q?b?=", "ab"),
            ("=?UTF-8?Q?a?=\t =?ISO-8859-1?Q?=E9?= c", "aé c"),
            // One character, 0xE6 0x97 0xA5, split between two words.
            ("=?UTF-8?B?5pc=?= =?UTF-8?B?pQ==?=", "日"),
            ("=?UTF-8?Q?a?= and =?UTF-8?Q?b?=", "a and b"),
        ] {
            assert_eq!(decoded(written), read, "{written}");
        }
    }

    #[test]
    fn what_is_not_an_encoded_word_is_left_as_it_stands() {
        for plain in [
            "",
            "Q3 plan",
            "2 + 2 =? 4",
            "=?UTF-8?B?not base64!?=",
            "=?UTF-8?Q?bad=ZZ?=",
            "=?UTF-8?Q?cut=4",
            "=?UTF-8?X?abc?=",
            "=?shift_jis?B?k/qWe4zq?=",
            "=?UTF-8?Q?two words?=",
            "Grüße, already decoded",
        ] {
            assert_eq!(decoded(plain), plain, "{plain}");
        }
        // Bytes that are not the text they claim to be are replaced, not passed on.
        assert_eq!(decoded("=?UTF-8?Q?=FF=FE?="), "\u{fffd}\u{fffd}");
    }

    #[test]
    fn bytes_are_read_in_the_character_set_a_part_names() {
        assert_eq!(text_in(Some("UTF-8"), "Grüße".as_bytes()), "Grüße");
        assert_eq!(text_in(None, "Grüße".as_bytes()), "Grüße");
        assert_eq!(text_in(Some("ISO-8859-1"), b"Caf\xe9"), "Café");
        assert_eq!(text_in(Some(" Windows-1252 "), b"\x93x\x94"), "“x”");
        // Not UTF-8, and nothing says what it is: the rest is still read.
        assert_eq!(text_in(None, b"Caf\xe9 au lait"), "Caf\u{fffd} au lait");
        assert_eq!(text_in(Some("koi8-r"), b"ok \xf0"), "ok \u{fffd}");
    }

    #[test]
    fn text_is_written_as_words_that_read_back_and_fit_on_a_line() {
        for text in [
            "Grüße",
            "日本語のとても長い件名、これは一行に収まらないので、いくつかの語に分けて書かれます",
            "A plain subject that is simply too long to share one line with the name of its header",
            "x",
        ] {
            let words = encoded(text);
            assert!(
                words.iter().all(|word| word.len() <= 68 && word.is_ascii()),
                "{words:?}"
            );
            assert_eq!(decoded(&words.join("\r\n ").replace("\r\n", "")), text);
        }
        assert!(encoded("").is_empty());
    }
}
