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
    pub fn from_vtt(text: &str) -> Result<Self> {
        let unreadable = |what: &str| Error::new(ErrorKind::Decode, format!("the transcript has {what}"));
        let normalized = text.trim_start_matches('\u{feff}').replace("\r\n", "\n");
        let lines: Vec<&str> = normalized.split(['\n', '\r']).collect();
        let mut entries = Vec::new();
        // A block is the lines between two empty ones.
        for block in lines.split(|line| line.is_empty()) {
            let Some(first) = block.iter().find(|line| !line.trim().is_empty()) else {
                continue;
            };
            if ["WEBVTT", "NOTE", "STYLE", "REGION"]
                .iter()
                .any(|word| first.starts_with(word))
            {
                continue;
            }
            // A cue may begin with an identifier; the timing follows it. A
            // second timing line in the same block begins the next cue.
            let timings: Vec<usize> = (0..block.len()).filter(|&i| block[i].contains("-->")).collect();
            if timings.is_empty() {
                return Err(unreadable("text that is not a cue"));
            }
            for (n, &timing) in timings.iter().enumerate() {
                let (start_ms, end_ms) =
                    cue_timing(block[timing]).ok_or_else(|| unreadable("a cue whose timing could not be read"))?;
                let payload_end = timings.get(n + 1).copied().unwrap_or(block.len());
                let (speaker, text) = cue_payload(&block[timing + 1..payload_end].join("\n"));
                entries.push(TranscriptEntry {
                    speaker,
                    start_ms,
                    end_ms,
                    text,
                });
            }
        }
        Ok(Self {
            text: text.to_owned(),
            entries,
        })
    }
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
/// The speaker is the first voice tag's name. Every other tag is dropped and
/// its text kept, so `We <i>really</i> ship` reads `We really ship`. A `<`
/// that opens no tag is kept as it was said.
fn cue_payload(payload: &str) -> (Option<String>, String) {
    let mut speaker = None;
    let mut words = String::new();
    let mut rest = payload;
    while let Some(open) = rest.find('<') {
        words.push_str(&rest[..open]);
        let Some(close) = rest[open..].find('>') else {
            // A `<` that opens no tag is something that was said.
            words.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let tag = &rest[open + 1..open + close];
        if !is_tag(tag) {
            // Someone said "x < 5": the sign is kept and reading goes on after it.
            words.push('<');
            rest = &rest[open + 1..];
            continue;
        }
        // `<v Ada Lovelace>`, or `<v.loud Ada Lovelace>` with a class.
        let is_voice = tag.starts_with("v ") || tag.starts_with("v.") || tag.starts_with("v\t");
        if speaker.is_none() && is_voice {
            speaker = tag
                .split_once(char::is_whitespace)
                .map(|(_, name)| plain(name))
                .filter(|name| !name.is_empty());
        }
        rest = &rest[open + close + 1..];
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
