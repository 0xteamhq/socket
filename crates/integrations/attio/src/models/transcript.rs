//! What was said in a recorded call.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::nullable::nullable;

/// What was said in a call.
///
/// This is the shape the other integrations give a transcript, so that a
/// caller reads one the same way wherever it came from. Attio's own names
/// for the same things are `raw_transcript`, and `segments` with `speech`,
/// `start_time` and `end_time` in seconds and `speaker.name`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Transcript {
    /// The transcript as Attio writes it out, a line for each turn:
    /// `[00:04] Tom Watson: I'm here.`
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
    /// When it began, in milliseconds from the start of the recording.
    pub start_ms: i64,
    /// When it ended, in milliseconds from the start of the recording.
    pub end_ms: i64,
    pub text: String,
}

/// A transcript as Attio sends it: `raw_transcript`, and `segments` with
/// times in seconds. The client reads this and turns it into a [`Transcript`].
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub(crate) struct SentTranscript {
    #[serde(deserialize_with = "nullable")]
    segments: Vec<Segment>,
    raw_transcript: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Segment {
    speech: Option<String>,
    #[serde(deserialize_with = "nullable")]
    start_time: f64,
    #[serde(deserialize_with = "nullable")]
    end_time: f64,
    speaker: Option<Speaker>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct Speaker {
    name: Option<String>,
}

/// Seconds, as Attio counts them, in whole milliseconds.
fn milliseconds(seconds: f64) -> i64 {
    // A cast from a float stops at the ends of the range, and a time that is
    // not a number becomes zero.
    (seconds * 1000.0).round() as i64
}

impl From<SentTranscript> for Transcript {
    fn from(sent: SentTranscript) -> Self {
        Self {
            text: sent.raw_transcript.unwrap_or_default(),
            entries: sent
                .segments
                .into_iter()
                .map(|segment| TranscriptEntry {
                    speaker: segment
                        .speaker
                        .and_then(|speaker| speaker.name)
                        .filter(|name| !name.trim().is_empty()),
                    start_ms: milliseconds(segment.start_time),
                    end_ms: milliseconds(segment.end_time),
                    text: segment.speech.unwrap_or_default(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seconds_become_whole_milliseconds() {
        assert_eq!(milliseconds(0.51), 510);
        assert_eq!(milliseconds(4.91), 4910);
        assert_eq!(milliseconds(0.0), 0);
        // 2.11 is not exact in binary; it must not come out as 2109.
        assert_eq!(milliseconds(2.11), 2110);
        assert_eq!(milliseconds(3600.0005), 3_600_001);
        assert_eq!(milliseconds(f64::NAN), 0);
        assert_eq!(milliseconds(f64::INFINITY), i64::MAX);
    }
}
