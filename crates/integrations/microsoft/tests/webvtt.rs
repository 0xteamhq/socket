//! Reading a Teams transcript: WebVTT with the speaker in a voice tag.

use socketkit_core::ErrorKind;
use socketkit_microsoft::models::{TranscriptContent, TranscriptEntry};

fn entry(speaker: Option<&str>, start_ms: i64, end_ms: i64, text: &str) -> TranscriptEntry {
    TranscriptEntry {
        speaker: speaker.map(str::to_owned),
        start_ms,
        end_ms,
        text: text.to_owned(),
    }
}

fn entries(vtt: &str) -> Vec<TranscriptEntry> {
    TranscriptContent::from_vtt(vtt).unwrap().entries
}

#[test]
fn a_teams_transcript_is_read_into_who_said_what_and_when() {
    let vtt = "WEBVTT\n\n00:00:16.246 --> 00:00:17.726\n<v Ada Lovelace>This is a transcript test.</v>\n\n\
               00:01:02.000 --> 01:00:03.500\n<v Grace Hopper>Agreed, we ship on Friday.</v>\n";
    let content = TranscriptContent::from_vtt(vtt).unwrap();
    assert_eq!(content.text, vtt, "the text is kept exactly as Microsoft sent it");
    assert_eq!(
        content.entries,
        [
            entry(Some("Ada Lovelace"), 16_246, 17_726, "This is a transcript test."),
            entry(Some("Grace Hopper"), 62_000, 3_603_500, "Agreed, we ship on Friday."),
        ]
    );
}

#[test]
fn what_is_not_speech_is_skipped_and_what_surrounds_a_cue_does_not_change_it() {
    // A byte-order mark, Windows line endings, a header with text after it, a
    // note, a style block, a cue identifier, cue settings, and timestamps
    // written without hours.
    let vtt = "\u{feff}WEBVTT - meeting 42\r\nKind: captions\r\n\r\nNOTE recorded by Teams\r\nsecond line of the note\r\n\r\n\
               STYLE\r\n::cue { color: red }\r\n\r\n\
               0a1b-2c3d/14-0\r\n00:05.000 --> 00:07.250 align:start position:10%\r\n<v Ada>Hello.</v>\r\n\r\n\r\n\
               1\r\n01:00.000 --> 01:01.000\r\n<v Grace>Hi.</v>";
    assert_eq!(
        entries(vtt),
        [
            entry(Some("Ada"), 5_000, 7_250, "Hello."),
            entry(Some("Grace"), 60_000, 61_000, "Hi."),
        ]
    );
}

#[test]
fn a_cue_over_several_lines_with_markup_and_entities_becomes_plain_text() {
    let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:04.000\n\
               <v.loud Ada &amp; Co>We <i>really</i> need R&amp;D\nto ship &lt;v2&gt;&nbsp;now.</v>\n";
    assert_eq!(
        entries(vtt),
        [entry(
            Some("Ada & Co"),
            1_000,
            4_000,
            "We really need R&D to ship <v2> now."
        )]
    );
}

#[test]
fn a_cue_without_a_voice_tag_has_no_speaker() {
    // What Microsoft sends when a tenant switches speaker attribution off:
    // no header and no voice tags.
    let vtt = "00:00:01.500 --> 00:00:04.000 \nHello, thanks for joining. \n\n00:00:04.000 --> 00:00:07.200 \nGlad to be here. \n";
    assert_eq!(
        entries(vtt),
        [
            entry(None, 1_500, 4_000, "Hello, thanks for joining."),
            entry(None, 4_000, 7_200, "Glad to be here."),
        ]
    );
}

#[test]
fn a_time_before_the_start_is_negative() {
    // Microsoft: "Negative offsets indicate that the transcription began
    // while the conversation was ongoing."
    let vtt = "WEBVTT\n\n-00:00:01.500 --> 00:00:02.000\n<v Ada>Already talking.</v>\n";
    assert_eq!(entries(vtt), [entry(Some("Ada"), -1_500, 2_000, "Already talking.")]);
}

#[test]
fn a_transcript_with_nothing_said_has_no_entries() {
    for vtt in ["", "WEBVTT", "WEBVTT\n\nNOTE nothing was said\n"] {
        assert!(entries(vtt).is_empty(), "{vtt:?}");
    }
}

#[test]
fn a_cue_whose_timing_cannot_be_read_is_an_error_not_a_shorter_transcript() {
    for timing in [
        "00:00:01.000 --> later",
        "soon --> 00:00:02.000",
        "00:00:01 --> 00:00:02",
        "00:61:01.000 --> 00:00:02.000",
        "1:2:3:4.000 --> 00:00:02.000",
        "--> 00:00:02.000",
    ] {
        let vtt =
            format!("WEBVTT\n\n00:00:00.000 --> 00:00:01.000\n<v Ada>Fine.</v>\n\n{timing}\n<v Grace>Lost?</v>\n");
        let err = TranscriptContent::from_vtt(&vtt).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{timing}");
        // What was said is the caller's data and never goes into an error.
        assert!(!err.message().contains("Lost"), "{}", err.message());
    }
}

#[test]
fn a_less_than_sign_that_was_said_is_kept_and_only_markup_is_removed() {
    // WebVTT asks for `&lt;`, but nothing makes a transcriber write it.
    let said = |payload: &str| {
        let vtt = format!("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n{payload}\n");
        entries(&vtt).remove(0).text
    };
    assert_eq!(said("<v Ada>if x < 5 then stop</v>"), "if x < 5 then stop");
    assert_eq!(said("<v Ada>I <3 this, 2 > 1</v>"), "I <3 this, 2 > 1");
    assert_eq!(said("<v Ada>a <= b</v> and more"), "a <= b and more");
    assert_eq!(said("<v Ada>ends with <"), "ends with <");
    // Markup is still removed: a karaoke timestamp, a class, a closing tag.
    assert_eq!(
        said("<v Ada>one <00:00:01.500><c.loud>two</c> <b>three</b></v>"),
        "one two three"
    );
}

#[test]
fn a_line_of_spaces_inside_a_cue_does_not_end_it() {
    // Only an empty line ends a cue.
    let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<v Ada>first\n \t\nsecond</v>\n\n00:00:03.000 --> 00:00:04.000\n<v Grace>third</v>\n";
    assert_eq!(
        entries(vtt),
        [
            entry(Some("Ada"), 1_000, 2_000, "first second"),
            entry(Some("Grace"), 3_000, 4_000, "third"),
        ]
    );
}

#[test]
fn two_cues_with_no_empty_line_between_them_are_still_two_cues() {
    // The line between them holds a space, so it does not end the first cue;
    // the second cue's timing does.
    let vtt = "WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<v Ada>first</v>\n \n00:00:03.000 --> 00:00:04.000\n<v Grace>second</v>\n";
    assert_eq!(
        entries(vtt),
        [
            entry(Some("Ada"), 1_000, 2_000, "first"),
            entry(Some("Grace"), 3_000, 4_000, "second"),
        ]
    );
}

#[test]
fn text_that_is_not_a_transcript_is_an_error_not_a_meeting_where_nothing_was_said() {
    for body in [
        "<html><body>Sign in to your account</body></html>",
        "Service temporarily unavailable",
        // Words with no timing, after a cue that is fine.
        "WEBVTT\n\n00:00:00.000 --> 00:00:01.000\n<v Ada>Fine.</v>\n\nstray secret words\n",
    ] {
        let err = TranscriptContent::from_vtt(body).unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{body}");
        for part in ["Sign in", "unavailable", "secret"] {
            assert!(!err.message().contains(part), "{}", err.message());
        }
    }
}
