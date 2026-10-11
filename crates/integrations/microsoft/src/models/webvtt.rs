//! Reading WebVTT as Teams writes a transcript: one cue for each thing said,
//! with the speaker in a voice tag at its start.
//!
//! The text is read once, line by line, and nothing of it is held but the cue
//! being read: a transcript may be ten megabytes, and what was said in it is
//! not ours.

use socketkit_core::{Error, ErrorKind, Result};

use super::TranscriptEntry;

/// The entries of a transcript, or an error when it cannot be read whole.
pub(super) fn entries(text: &str) -> Result<Vec<TranscriptEntry>> {
    let mut reader = Reader::default();
    let body = text.trim_start_matches('\u{feff}');
    // A line ends at `\n`, `\r\n` or a lone `\r`.
    let mut after_return = false;
    for line in body.split_inclusive(['\n', '\r']) {
        let ended_by_return = line.ends_with('\r');
        let line = line.trim_end_matches(['\n', '\r']);
        // The `\n` of a `\r\n` ends the line the `\r` already ended.
        if !(after_return && line.is_empty() && !ended_by_return) {
            reader.line(line)?;
        }
        after_return = ended_by_return;
    }
    reader.end_of_block()?;
    Ok(reader.entries)
}

/// Reads a transcript a line at a time.
#[derive(Default)]
struct Reader {
    entries: Vec<TranscriptEntry>,
    /// The cue being read: when it begins and ends, and its words so far.
    cue: Option<(i64, i64, String)>,
    /// How many lines with something on them this block has had before any timing.
    before_timing: usize,
    /// Whether this block opened as a header, a note or a style.
    not_speech: bool,
    /// Whether this block has had a timing line.
    timed: bool,
    /// Whether a block has been read already. The header is only the first.
    begun: bool,
}

impl Reader {
    fn line(&mut self, line: &str) -> Result<()> {
        // Only an empty line ends a block; a line of spaces does not.
        if line.is_empty() {
            return self.end_of_block();
        }
        // Inside a cue, an arrow is a timing only on a line that sets out to
        // be one. "A --> B" is something that was said.
        if line.contains("-->") && (self.cue.is_none() || sets_out_to_be_a_timing(line)) {
            // A second timing in the same block begins the next cue.
            self.end_of_cue();
            // One line before a timing is the cue's identifier, and a header
            // may run on into its first cue. Anything more is text that
            // belongs to no cue.
            if !self.timed && !self.not_speech && self.before_timing > 1 {
                return Err(unreadable("text that is not a cue"));
            }
            let (start_ms, end_ms) =
                cue_timing(line).ok_or_else(|| unreadable("a cue whose timing could not be read"))?;
            self.cue = Some((start_ms, end_ms, String::new()));
            self.timed = true;
        } else if let Some((_, _, words)) = &mut self.cue {
            words.push_str(line);
            words.push('\n');
        } else if !line.trim().is_empty() {
            if self.before_timing == 0 {
                self.not_speech = self.opens_what_is_not_speech(line);
            }
            self.before_timing += 1;
        }
        Ok(())
    }

    /// Whether a block that begins with `line` is a header, a note or a
    /// style. A note is `NOTE` and then a space or the end of the line; a
    /// style or a region is that word alone; the header is `WEBVTT` at the
    /// start of the text. Words that only begin the same way are words.
    fn opens_what_is_not_speech(&self, line: &str) -> bool {
        let word_then_more = |word: &str| {
            line.strip_prefix(word)
                .is_some_and(|more| more.is_empty() || more.starts_with([' ', '\t']))
        };
        let alone = |word: &str| line.trim_end() == word;
        (!self.begun && word_then_more("WEBVTT")) || word_then_more("NOTE") || alone("STYLE") || alone("REGION")
    }

    fn end_of_cue(&mut self) {
        let Some((start_ms, end_ms, words)) = self.cue.take() else {
            return;
        };
        let (speaker, text) = cue_payload(&words);
        self.entries.push(TranscriptEntry {
            speaker,
            start_ms,
            end_ms,
            text,
        });
    }

    fn end_of_block(&mut self) -> Result<()> {
        self.end_of_cue();
        // A block with words and no timing is a header, a note or a style,
        // or else it is not a transcript.
        let stray = self.before_timing > 0 && !self.timed && !self.not_speech;
        self.begun = self.begun || self.before_timing > 0 || self.timed;
        self.before_timing = 0;
        self.not_speech = false;
        self.timed = false;
        if stray {
            return Err(unreadable("text that is not a cue"));
        }
        Ok(())
    }
}

/// Whether a line begins as a timing does: with a time, before its arrow.
/// Such a line is a timing or an error, and never words.
fn sets_out_to_be_a_timing(line: &str) -> bool {
    line.split_once("-->").is_some_and(|(start, _)| {
        let start = start.trim();
        !start.is_empty()
            && start
                .chars()
                .all(|c| c.is_ascii_digit() || matches!(c, ':' | '.' | '-'))
    })
}

fn unreadable(what: &str) -> Error {
    Error::new(ErrorKind::Decode, format!("the transcript has {what}"))
}

/// Reads `00:00:16.246 --> 00:00:17.726`, with or without cue settings after it.
fn cue_timing(line: &str) -> Option<(i64, i64)> {
    let (start, rest) = line.split_once("-->")?;
    let end = rest.split_whitespace().next()?;
    Some((milliseconds(start.trim())?, milliseconds(end)?))
}

/// Reads `hh:mm:ss.ttt` or `mm:ss.ttt` as milliseconds. Teams writes a
/// leading `-` for a time before the transcript began.
fn milliseconds(stamp: &str) -> Option<i64> {
    let (sign, stamp) = match stamp.strip_prefix('-') {
        Some(rest) => (-1, rest),
        None => (1, stamp),
    };
    let (clock, fraction) = stamp.split_once('.')?;
    let fraction = digits(fraction).filter(|_| fraction.len() == 3)?;
    let (hours, minutes, seconds) = match clock.split(':').collect::<Vec<_>>().as_slice() {
        [minutes, seconds] => (0, below_sixty(minutes)?, below_sixty(seconds)?),
        [hours, minutes, seconds] => (digits(hours)?, below_sixty(minutes)?, below_sixty(seconds)?),
        _ => return None,
    };
    let total = hours
        .checked_mul(3_600_000)?
        .checked_add(minutes * 60_000 + seconds * 1_000 + fraction)?;
    Some(sign * total)
}

/// A run of digits as a number.
fn digits(part: &str) -> Option<i64> {
    if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    part.parse().ok()
}

/// Minutes or seconds: two digits, below sixty.
fn below_sixty(part: &str) -> Option<i64> {
    digits(part).filter(|n| part.len() == 2 && *n < 60)
}

/// A cue's speaker and its words, without markup.
///
/// The speaker is named by a voice tag that opens the cue, which is where
/// Teams writes it. A voice tag anywhere later is kept as text and not
/// believed: it is something that was said, or something made to look like a
/// tag so that the words after it would be read as another person's. Every
/// other tag is dropped and its text kept, so `We <i>really</i> ship` reads
/// `We really ship`. A `<` that opens no tag is kept as it was said.
fn cue_payload(payload: &str) -> (Option<String>, String) {
    let mut speaker = None;
    let mut words = String::new();
    // Whether nothing has come before: no words, and no other tag.
    let mut opening = true;
    let mut rest = payload;
    while let Some(open) = rest.find('<') {
        words.push_str(&rest[..open]);
        opening = opening && words.trim().is_empty();
        let after = &rest[open + 1..];
        // A tag holds no `<`, so the search for its end stops at the next
        // one. Looking further would read the rest of the cue once for every
        // `<` in it.
        let close = after
            .find(['<', '>'])
            .filter(|&at| after[at..].starts_with('>') && is_tag(&after[..at]));
        let Some(close) = close else {
            // Someone said "x < 5": the sign is kept and reading goes on after it.
            words.push('<');
            rest = after;
            continue;
        };
        let tag = &after[..close];
        rest = &after[close + 1..];
        // `<v Ada Lovelace>`, or `<v.loud Ada Lovelace>` with a class.
        let is_voice = tag.starts_with("v ") || tag.starts_with("v.") || tag.starts_with("v\t");
        if is_voice && opening {
            speaker = tag
                .split_once(char::is_whitespace)
                .map(|(_, name)| plain(name))
                .filter(|name| !name.is_empty());
        } else if is_voice {
            words.push('<');
            words.push_str(tag);
            words.push('>');
        }
        opening = false;
    }
    words.push_str(rest);
    (speaker, plain(&words))
}

/// True for what WebVTT puts between `<` and `>`: a tag's name, with or
/// without a leading `/`, or a timestamp. Anything else was said, not marked up.
fn is_tag(inner: &str) -> bool {
    let name = inner.strip_prefix('/').unwrap_or(inner);
    let is_timestamp = !inner.is_empty() && inner.chars().all(|c| c.is_ascii_digit() || matches!(c, ':' | '.'));
    name.starts_with(|c: char| c.is_ascii_alphabetic()) || is_timestamp
}

/// Text with WebVTT's escapes read and runs of whitespace made one space.
fn plain(text: &str) -> String {
    let unescaped = text
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&lrm;", "")
        .replace("&rlm;", "")
        .replace("&amp;", "&");
    unescaped.split_whitespace().collect::<Vec<_>>().join(" ")
}
