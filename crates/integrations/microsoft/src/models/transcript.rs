//! Transcripts of Teams online meetings.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use socketkit_core::{Error, ErrorKind, Result};

use super::IdentitySet;

/// One transcript of a meeting. A meeting transcribed twice has two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Transcript {
    pub id: String,
    pub meeting_id: Option<String>,
    pub call_id: Option<String>,
    /// The same value on the recording this transcript was made from.
    pub content_correlation_id: Option<String>,
    pub created_date_time: Option<String>,
    pub end_date_time: Option<String>,
    pub transcript_content_url: Option<String>,
    pub meeting_organizer: Option<IdentitySet>,
}

/// What was said in a meeting.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct TranscriptContent {
    /// The transcript exactly as Microsoft sent it, in WebVTT.
    pub text: String,
    /// The same transcript, one entry for each thing someone said.
    pub entries: Vec<TranscriptEntry>,
}

/// One thing someone said.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct TranscriptEntry {
    /// Who spoke. Absent when the transcript does not say.
    pub speaker: Option<String>,
    /// When it began, in milliseconds from the start of the transcript.
    /// Negative when transcription was switched on mid-conversation.
    pub start_ms: i64,
    /// When it ended, in milliseconds from the start of the transcript.
    pub end_ms: i64,
    pub text: String,
}

impl TranscriptContent {
    /// Reads a WebVTT transcript as Teams writes it: one cue for each thing
    /// said, with the speaker in a voice tag (`<v Ada Lovelace>…</v>`).
    ///
    /// Notes, styles and cue identifiers are skipped, and markup inside a cue
    /// is removed. A cue whose timing cannot be read, or text that is not a
    /// cue at all, is an error: leaving it out would return a transcript that
    /// looks whole and is not. The error never repeats what was said.
    ///
    /// The text is read once, line by line, and nothing of it is held but
    /// the cue being read.
    pub fn from_vtt(text: &str) -> Result<Self> {
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
        Ok(Self {
            text: text.to_owned(),
            entries: reader.entries,
        })
    }
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
}

impl Reader {
    fn line(&mut self, line: &str) -> Result<()> {
        // Only an empty line ends a block; a line of spaces does not.
        if line.is_empty() {
            return self.end_of_block();
        }
        if line.contains("-->") {
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
                self.not_speech = ["WEBVTT", "NOTE", "STYLE", "REGION"]
                    .iter()
                    .any(|word| line.starts_with(word));
            }
            self.before_timing += 1;
        }
        Ok(())
    }

    fn end_of_cue(&mut self) {
        let Some((start_ms, end_ms, words)) = self.cue.take() else {
            return;
        };
        for (speaker, text) in cue_payload(&words) {
            self.entries.push(TranscriptEntry {
                speaker,
                start_ms,
                end_ms,
                text,
            });
        }
    }

    fn end_of_block(&mut self) -> Result<()> {
        self.end_of_cue();
        // A block with words and no timing is a header, a note or a style,
        // or else it is not a transcript.
        let stray = self.before_timing > 0 && !self.timed && !self.not_speech;
        self.before_timing = 0;
        self.not_speech = false;
        self.timed = false;
        if stray {
            return Err(unreadable("text that is not a cue"));
        }
        Ok(())
    }
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

/// A cue's words without markup, in one part for each speaker.
///
/// A voice tag names who speaks from there on, so a cue with two voices
/// gives two parts, and words before the first voice have no speaker. Every
/// other tag is dropped and its text kept, so `We <i>really</i> ship` reads
/// `We really ship`. A `<` that opens no tag is kept as it was said. A cue
/// always gives at least one part, empty when nothing was said in it.
fn cue_payload(payload: &str) -> Vec<(Option<String>, String)> {
    let mut parts = Vec::new();
    let mut speaker = None;
    let mut words = String::new();
    let mut rest = payload;
    while let Some(open) = rest.find('<') {
        words.push_str(&rest[..open]);
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
        let named = tag
            .split_once(char::is_whitespace)
            .map(|(_, name)| plain(name))
            .filter(|name| is_voice && !name.is_empty());
        if named.is_some() && named != speaker {
            let said = plain(&words);
            if !said.is_empty() {
                parts.push((speaker.take(), said));
            }
            words.clear();
            speaker = named;
        }
    }
    words.push_str(rest);
    let said = plain(&words);
    if !said.is_empty() || parts.is_empty() {
        parts.push((speaker, said));
    }
    parts
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
