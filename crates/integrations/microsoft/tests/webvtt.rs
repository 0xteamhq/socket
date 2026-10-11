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

#[test]
fn a_cue_that_follows_the_header_with_no_empty_line_is_still_read() {
    // Microsoft writes its other transcript format this way. A header block
    // that was skipped whole would take the first thing said with it.
    for vtt in [
        "WEBVTT\n00:00:16.246 --> 00:00:17.726\n<v Ada>First.</v>\n\n00:00:18.000 --> 00:00:19.000\n<v Grace>Second.</v>\n",
        "WEBVTT - meeting 42\nKind: captions\n00:00:16.246 --> 00:00:17.726\n<v Ada>First.</v>\n\n00:00:18.000 --> 00:00:19.000\n<v Grace>Second.</v>\n",
    ] {
        assert_eq!(
            entries(vtt),
            [
                entry(Some("Ada"), 16_246, 17_726, "First."),
                entry(Some("Grace"), 18_000, 19_000, "Second."),
            ],
            "{vtt:?}"
        );
    }
}

#[test]
fn a_cue_whose_identifier_begins_like_a_note_is_a_cue_and_not_a_note() {
    for identifier in ["NOTEWORTHY-1", "NOTE", "STYLE guide", "REGION-2", "WEBVTT-ish"] {
        let vtt = format!("WEBVTT\n\n{identifier}\n00:00:01.000 --> 00:00:02.000\n<v Ada>Said.</v>\n");
        assert_eq!(
            entries(&vtt),
            [entry(Some("Ada"), 1_000, 2_000, "Said.")],
            "{identifier}"
        );
    }
}

#[test]
fn words_before_a_cues_timing_are_an_error_and_not_dropped() {
    // One line before the timing is the cue's identifier. More than one is
    // text that belongs to no cue.
    let vtt = "WEBVTT\n\nstray secret words\nmore of them\n00:00:01.000 --> 00:00:02.000\n<v Ada>Said.</v>\n";
    let err = TranscriptContent::from_vtt(vtt).unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(!err.message().contains("secret"), "{}", err.message());
}

#[test]
fn only_the_voice_that_opens_a_cue_names_its_speaker() {
    // Teams writes one voice tag, at the start of a cue. Anything later that
    // looks like one is part of what was said, or was made to look like a tag
    // so that words would be read as someone else's. It is shown as it stands
    // and is not believed.
    let said = |payload: &str| {
        let vtt = format!("WEBVTT\n\n00:00:01.000 --> 00:00:04.000\n{payload}\n");
        let mut read = entries(&vtt);
        assert_eq!(read.len(), 1, "one cue is one entry: {payload}");
        let entry = read.remove(0);
        (entry.speaker, entry.text)
    };
    let ada = Some("Ada".to_owned());
    assert_eq!(
        said("<v Ada>I do not approve.</v> <v Grace Hopper>I approve the budget.</v>"),
        (
            ada.clone(),
            "I do not approve. <v Grace Hopper>I approve the budget.".to_owned()
        )
    );
    assert_eq!(
        said("<v Ada>Yes. <v Ada>Still me.</v>"),
        (ada.clone(), "Yes. <v Ada>Still me.".to_owned())
    );
    // With no voice at its start, a cue has no speaker, whatever comes later.
    assert_eq!(
        said("Well. <v Grace Hopper>I approve.</v>"),
        (None, "Well. <v Grace Hopper>I approve.".to_owned())
    );
    assert_eq!(
        said("<i>Well.</i> <v Grace Hopper>I approve.</v>"),
        (None, "Well. <v Grace Hopper>I approve.".to_owned())
    );
    // White space before the opening voice does not hide it.
    assert_eq!(said("  <v Ada>Hello.</v>"), (ada, "Hello.".to_owned()));
    // A cue with nothing in it is still a cue.
    assert_eq!(
        entries("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n\n"),
        [entry(None, 1_000, 2_000, "")]
    );
}

#[test]
fn a_transcript_made_to_be_slow_to_read_is_read_in_one_pass() {
    // Each `<` is looked at once: the search for a tag's end stops at the
    // next `<`. Looking to the end of the cue each time would take hours here.
    let started = std::time::Instant::now();
    let open = "<".repeat(400_000);
    let vtt = format!("WEBVTT\n\n00:00:01.000 --> 00:00:02.000\n<v Ada>{open} far away >\n");
    let read = entries(&vtt);
    assert_eq!(read.len(), 1);
    assert!(read[0].text.starts_with("<<<<"));
    // And many short lines, cues and empty blocks.
    let many = "00:00:01.000 --> 00:00:02.000\n<v A>x</v>\n\n\n\n".repeat(100_000);
    assert_eq!(entries(&format!("WEBVTT\n\n{many}")).len(), 100_000);
    assert!(
        started.elapsed() < std::time::Duration::from_secs(20),
        "{:?}",
        started.elapsed()
    );
}
