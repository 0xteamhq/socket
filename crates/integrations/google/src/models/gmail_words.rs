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
///
/// A word is `=?set?B?content?=` with no space in it, so each of its three
/// parts is looked for only as far as the next `?` or the next space. Text
/// that says `=?` again and again and never ends a word is then read once,
/// not once for every `=?` in it.
fn word(text: &str) -> Option<(Charset, Vec<u8>, usize)> {
    /// What stands before the next `?`, and what follows it. `None` when a
    /// space comes first, or no `?` at all.
    fn part(text: &str) -> Option<(&str, &str)> {
        let end = text.find(|c: char| c == '?' || c.is_whitespace())?;
        text[end..].strip_prefix('?').map(|rest| (&text[..end], rest))
    }
    let rest = text.strip_prefix("=?")?;
    let (name, rest) = part(rest)?;
    let (encoding, rest) = part(rest)?;
    let (content, rest) = part(rest)?;
    if !rest.starts_with('=') {
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

/// The bytes of a `Q` word: `_` is a space, and `=41` is the byte 0x41. What
/// follows `=` is two hexadecimal digits and nothing else: a sign is not one.
fn quoted(content: &str) -> Option<Vec<u8>> {
    let mut bytes = Vec::with_capacity(content.len());
    let mut rest = content.as_bytes();
    while let Some((&byte, after)) = rest.split_first() {
        rest = after;
        match byte {
            b'_' => bytes.push(b' '),
            b'=' => {
                let hex = rest.get(..2).filter(|hex| hex.iter().all(u8::is_ascii_hexdigit))?;
                let hex = std::str::from_utf8(hex).ok()?;
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

/// Whether a character shows as nothing, or changes how the text around it
/// is shown: a control character, a character that only formats (a soft
/// hyphen, a zero-width space, a mark that turns the direction of writing
/// around, a tag), or a separator of lines.
///
/// The two joiners (U+200C and U+200D) are judged by where they stand, in
/// [`readable`]: Persian and the scripts of India are written with them, and
/// so is an emoji made of several, and there they are kept.
///
/// Someone else's text can use these to hide what it says from a person and
/// not from a program, or to show an address written backwards.
fn unseen(character: char) -> bool {
    character.is_control()
        || matches!(
            character,
            '\u{ad}'
                | '\u{61c}'
                | '\u{180e}'
                | '\u{200b}'
                | '\u{200e}'..='\u{200f}'
                | '\u{2028}'..='\u{202e}'
                | '\u{2060}'..='\u{206f}'
                | '\u{feff}'
                | '\u{fff9}'..='\u{fffb}'
                | '\u{e0000}'..='\u{e007f}'
        )
}

/// Someone else's text as a person reads it: its encoded words read, and a
/// space where it held a character that is not seen. Whatever it hides in
/// an encoded word, no such character comes out of it.
pub(super) fn readable(value: &str) -> String {
    let written: Vec<char> = decoded(value).chars().collect();
    let read: String = written
        .iter()
        .enumerate()
        .map(|(at, character)| {
            let before = at.checked_sub(1).and_then(|before| written.get(before));
            let hidden = unseen(*character) || (joiner(*character) && !joins(before, written.get(at + 1)));
            if hidden { ' ' } else { *character }
        })
        .collect();
    read.trim().to_owned()
}

/// The two characters that change how the letters beside them join, and
/// show as nothing themselves (U+200C and U+200D).
fn joiner(character: char) -> bool {
    matches!(character, '\u{200c}' | '\u{200d}')
}

/// Whether a joiner between these two is doing what it is for: joining, or
/// keeping apart, letters of a script that is written with it, or the parts
/// of an emoji. That is never so beside a letter, a digit or a mark of
/// ASCII, where it could only make two words that read alike differ: a name
/// or an address that a person takes for one they know, and a program does not.
fn joins(before: Option<&char>, after: Option<&char>) -> bool {
    let beside = |neighbour: Option<&char>| neighbour.is_some_and(|c| !c.is_ascii() && !unseen(*c) && !joiner(*c));
    beside(before) && beside(after)
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

    #[test]
    fn a_sign_is_not_a_hexadecimal_digit_of_a_q_word() {
        // `=+A` was read as the byte 0x0A, a line break. It is not a word at all.
        for broken in [
            "=?UTF-8?Q?a=+Ab?=",
            "=?UTF-8?Q?a=+4b?=",
            "=?UTF-8?Q?=-1?=",
            "=?UTF-8?Q?=4?=",
            "=?UTF-8?Q?=G0?=",
            "=?UTF-8?Q?=?=",
        ] {
            assert_eq!(decoded(broken), broken, "{broken}");
        }
        assert_eq!(decoded("=?UTF-8?Q?=4a=4B?="), "JK");
    }

    #[test]
    fn a_word_has_no_space_in_it_and_ends_where_it_says() {
        for plain in [
            "=?UTF-8 ?Q?a?=",
            "=?UTF-8?Q ?a?=",
            "=?UTF-8?Q?a? =",
            "=?UTF-8?Q?a?b?=",
            "=?UTF-8?Q?a",
            "=?UTF-8?Q",
            "=?",
        ] {
            assert_eq!(decoded(plain), plain, "{plain}");
        }
        // A word with nothing in it is nothing, and what follows it is kept.
        assert_eq!(decoded("=?UTF-8?B??= x"), " x");
        assert_eq!(decoded("a=?UTF-8?Q?b?=c"), "abc");
    }

    #[test]
    fn text_that_starts_a_word_again_and_again_is_read_once() {
        // Each `=?` used to be read to the end of the text: 400 KB of this
        // took seconds, and a subject is a stranger's to write.
        for start in ["=?x?Q?a", "plain words and =? only "] {
            let subject = start.repeat(400 * 1024 / start.len());
            let began = std::time::Instant::now();
            assert_eq!(decoded(&subject), subject);
            let took = began.elapsed();
            assert!(took < std::time::Duration::from_secs(2), "{start:?} took {took:?}");
        }
    }

    #[test]
    fn the_joiners_a_script_or_an_emoji_is_written_with_are_kept() {
        // Persian "mi-khaham" with its non-joiner, and a family made of three people.
        for written in ["می\u{200c}خواهم", "👨\u{200d}👩\u{200d}👧"] {
            assert_eq!(readable(written), written);
        }
        // Beside the letters of an address or an English word a joiner joins
        // nothing. It only makes a word a person reads as one they know into
        // one a program does not.
        assert_eq!(readable("pay\u{200d}pal"), "pay pal");
        assert_eq!(readable("boss\u{200c}@example.test"), "boss @example.test");
        assert_eq!(readable("\u{200d}می"), "می", "at an end it joins nothing");
        assert_eq!(readable("می\u{200c}\u{200c}خواهم"), "می  خواهم", "nor beside another");
        assert_eq!(readable("a\u{200d}👩"), "a 👩");
    }

    #[test]
    fn what_is_not_seen_becomes_a_space_and_never_reaches_a_reader() {
        // A line break, a mark that turns the writing around, a zero-width
        // space, a soft hyphen, a separator of lines, a byte order mark, a tag.
        for hidden in [
            '\n',
            '\0',
            '\u{85}',
            '\u{ad}',
            '\u{61c}',
            '\u{180e}',
            '\u{200b}',
            '\u{200f}',
            '\u{2028}',
            '\u{2029}',
            '\u{202a}',
            '\u{202e}',
            '\u{2060}',
            '\u{2066}',
            '\u{2069}',
            '\u{206f}',
            '\u{feff}',
            '\u{fff9}',
            '\u{fffb}',
            '\u{e0001}',
            '\u{e0041}',
            '\u{e007f}',
        ] {
            assert_eq!(readable(&format!("a{hidden}b")), "a b", "{:?}", u32::from(hidden));
            let word = base64::engine::general_purpose::STANDARD.encode(format!("a{hidden}b"));
            assert_eq!(readable(&format!("=?UTF-8?B?{word}?=")), "a b", "encoded");
            assert_eq!(
                readable(&format!("{hidden}ab{hidden}")),
                "ab",
                "at the ends it is nothing"
            );
        }
        // What is seen is kept: letters of any writing, signs, a space that does not break.
        for seen in [
            "Zoë Müller",
            "日本語",
            "שלום",
            "a\u{a0}b",
            "Plan 🚀",
            "a\u{202f}b",
            "boss＠corp.test",
        ] {
            assert_eq!(readable(seen), seen);
        }
    }
}
