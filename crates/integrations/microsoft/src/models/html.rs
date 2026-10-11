//! Reading the HTML of a Teams message as the text a person sees.
//!
//! This is not a browser. It reads the HTML Teams writes: paragraphs and
//! breaks, mentions, attachments, pictures, links, tables. It is read once
//! from end to end, whatever it holds, because a message may be ten megabytes
//! and is written by someone else.

/// Tags that end a line where they open or close.
const BLOCKS: [&str; 16] = [
    "p",
    "div",
    "li",
    "ul",
    "ol",
    "tr",
    "table",
    "blockquote",
    "pre",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "hr",
];

/// `html` as plain text.
///
/// A mention becomes `@` and the name, an attachment `[attachment: name]` on
/// a line of its own with the name `attachment_name` gives for its id, a
/// picture or an emoji what it stands for, a link its words with its address
/// after them, and the cells of a table are kept apart. Paragraphs and breaks
/// become lines; every other tag is dropped and its text kept. What a tag
/// holds in its attributes, a comment, a script and a style are not text.
pub(super) fn text(html: &str, attachment_name: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    // An open link: its address, and where its words begin in `out`.
    let mut link: Option<(String, usize)> = None;
    // Whether the next cell is the first of its row.
    let mut first_cell = true;
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        out.push_str(&unescaped(&rest[..open]));
        let after = &rest[open + 1..];
        let close = match tag_end(after) {
            TagEnd::At(close) => close,
            TagEnd::Comment(end) => {
                rest = &after[end..];
                continue;
            }
            // A `<` that opens no tag is something that was said, and so is
            // what was read while finding that out. Reading goes on from
            // where it stopped, so nothing is read twice.
            TagEnd::NotATag(read) => {
                out.push('<');
                out.push_str(&unescaped(&after[..read]));
                rest = &after[read..];
                continue;
            }
        };
        let tag = &after[..close];
        rest = &after[close + 1..];
        let closing = tag.starts_with('/');
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| c.is_whitespace() || c == '/')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match name.as_str() {
            // What a script or a style holds is not what was said.
            "script" | "style" if !closing => rest = after_closing(rest, &name),
            "br" => out.push('\n'),
            "at" if !closing => out.push('@'),
            "attachment" if !closing => {
                let name = attribute(tag, "id")
                    .and_then(|id| attachment_name(&id))
                    .filter(|name| !name.trim().is_empty());
                match name {
                    Some(name) => out.push_str(&format!("\n[attachment: {name}]\n")),
                    None => out.push_str("\n[attachment]\n"),
                }
            }
            "img" | "emoji" => out.push_str(&attribute(tag, "alt").unwrap_or_default()),
            "a" if !closing => link = attribute(tag, "href").map(|address| (address, out.len())),
            "a" => {
                if let Some((address, from)) = link.take() {
                    let words = out.get(from..).unwrap_or_default().trim();
                    if !address.trim().is_empty() && words != address.trim() {
                        out.push_str(&format!(" ({})", address.trim()));
                    }
                }
            }
            "td" | "th" if !closing => {
                if !first_cell {
                    out.push_str(" | ");
                }
                first_cell = false;
            }
            name if BLOCKS.contains(&name) => {
                first_cell = first_cell || name == "tr";
                out.push('\n');
            }
            _ => {}
        }
    }
    out.push_str(&unescaped(rest));
    // One space between words, no empty lines.
    out.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// What follows a `<`.
enum TagEnd {
    /// A tag, which ends at this `>`.
    At(usize),
    /// A comment, which ends before this place, or with the text.
    Comment(usize),
    /// Not a tag. This much was read to find that out.
    NotATag(usize),
}

/// Finds where the tag that begins `after` ends, as a browser would: a `>`
/// inside a quoted attribute value does not end it.
///
/// Reading stops at a `<` outside a quoted value, or at the end of the text,
/// and says how far it read. The caller goes on from there, which is what
/// keeps the whole reading to one pass.
fn tag_end(after: &str) -> TagEnd {
    let bytes = after.as_bytes();
    if let Some(comment) = after.strip_prefix("!--") {
        return TagEnd::Comment(comment.find("-->").map_or(bytes.len(), |end| end + 6));
    }
    let begins_a_name = |at: usize| bytes.get(at).is_some_and(u8::is_ascii_alphabetic);
    if !(begins_a_name(0) || bytes.first() == Some(&b'!') || (bytes.first() == Some(&b'/') && begins_a_name(1))) {
        return TagEnd::NotATag(0);
    }
    let mut at = 0;
    // A quote opens a value only where a value can be: after `=`.
    let mut value_next = false;
    while at < bytes.len() {
        match bytes[at] {
            b'>' => return TagEnd::At(at),
            b'<' => return TagEnd::NotATag(at),
            b'=' => value_next = true,
            quote @ (b'"' | b'\'') if value_next => {
                let Some(length) = bytes[at + 1..].iter().position(|&byte| byte == quote) else {
                    return TagEnd::NotATag(bytes.len());
                };
                at += 1 + length;
                value_next = false;
            }
            byte if byte.is_ascii_whitespace() => {}
            _ => value_next = false,
        }
        at += 1;
    }
    TagEnd::NotATag(bytes.len())
}

/// What follows the tag that closes `name`, or nothing when it is never closed.
fn after_closing<'a>(rest: &'a str, name: &str) -> &'a str {
    let closes = |at: usize| {
        rest[at + 2..]
            .get(..name.len())
            .is_some_and(|found| found.eq_ignore_ascii_case(name))
    };
    rest.match_indices("</")
        .find(|&(at, _)| closes(at))
        .and_then(|(at, _)| rest[at..].find('>').map(|end| &rest[at + end + 1..]))
        .unwrap_or("")
}

/// The value of the attribute called `wanted`, with its escapes read.
///
/// The tag's attributes are read one after another, so a name is found only
/// where a name is, and never inside another attribute's value.
fn attribute(tag: &str, wanted: &str) -> Option<String> {
    let bytes = tag.as_bytes();
    let space = |at: usize| bytes[at].is_ascii_whitespace();
    // Past the tag's own name.
    let mut at = (0..bytes.len()).find(|&at| space(at)).unwrap_or(bytes.len());
    while at < bytes.len() {
        while at < bytes.len() && (space(at) || bytes[at] == b'/') {
            at += 1;
        }
        let name_from = at;
        while at < bytes.len() && !space(at) && bytes[at] != b'=' {
            at += 1;
        }
        let name = &tag[name_from..at];
        while at < bytes.len() && space(at) {
            at += 1;
        }
        let mut value = "";
        if at < bytes.len() && bytes[at] == b'=' {
            at += 1;
            while at < bytes.len() && space(at) {
                at += 1;
            }
            let quote = bytes.get(at).copied().filter(|byte| matches!(byte, b'"' | b'\''));
            let from = at + usize::from(quote.is_some());
            let ends = |at: usize| match quote {
                Some(quote) => bytes[at] == quote,
                None => space(at),
            };
            at = (from..bytes.len()).find(|&at| ends(at)).unwrap_or(bytes.len());
            value = &tag[from.min(bytes.len())..at];
            at += usize::from(quote.is_some());
        }
        if name.eq_ignore_ascii_case(wanted) {
            return Some(unescaped(value));
        }
    }
    None
}

/// Text with HTML's escapes read. What looks like one and is not is kept as it was written.
fn unescaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp + 1..];
        // An escape is short. Its end is looked for nearby only: looking
        // further would read the rest of the text once for every `&` in it.
        let end = after.as_bytes().iter().take(11).position(|&byte| byte == b';');
        let read = end.and_then(|end| {
            let name = &after[..end];
            let character = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => {
                    let code = match name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => name.strip_prefix('#').and_then(|decimal| decimal.parse().ok()),
                    };
                    code.and_then(char::from_u32)
                }
            };
            character.map(|character| (character, end))
        });
        match read {
            Some((character, end)) => {
                out.push(character);
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}
