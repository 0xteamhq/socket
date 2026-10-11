//! What Google answers for Meet, as the tests need it: fixtures and constants.

use serde_json::{Value, json};

/// A conference record's name, as Google returns it, and its id alone.
pub const RECORD: &str = "conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77";
pub const RECORD_ID: &str = "0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77";
/// Where that record is asked for.
pub const RECORD_PATH: &str = "/v2/conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77";

pub const SPACE: &str = "spaces/jQCFfuBOdN5z";
pub const MEETING_CODE: &str = "abc-mnop-xyz";
pub const MEETING_LINK: &str = "https://meet.google.com/abc-mnop-xyz";

/// Three participants, one of each kind.
pub const ADA: &str = "conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77/participants/118203456789";
pub const ADA_ID: &str = "118203456789";
pub const GRACE: &str = "conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77/participants/anon-4471";
pub const CALLER: &str = "conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77/participants/phone-0093";

pub const TRANSCRIPT: &str = "conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77/transcripts/tr-01";
pub const TRANSCRIPT_ID: &str = "tr-01";
pub const TRANSCRIPT_PATH: &str = "/v2/conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77/transcripts/tr-01";
pub const RECORDING: &str = "conferenceRecords/0c1b6f2e-7a4d-4f5a-9d2e-3b8c5a1f9e77/recordings/rec-01";

/// The Google Doc the transcript was saved to, and the Drive file of the recording.
pub const DOCUMENT: &str = "1kuceFZohVoCh6FulBHxwy6I15Ogpc4hP";
pub const FILE: &str = "1mZq9Xc0wT3vUu7rLhYp2sNdEaKbJ4gQf";

pub fn conference_record() -> Value {
    json!({
        "name": RECORD,
        "startTime": "2026-10-12T16:00:00.123456Z",
        "endTime": "2026-10-12T16:45:10.500Z",
        "expireTime": "2026-11-11T16:45:10.500Z",
        "space": SPACE
    })
}

/// Someone signed in, who left once and came back.
pub fn ada() -> Value {
    json!({
        "name": ADA,
        "earliestStartTime": "2026-10-12T16:00:02Z",
        "latestEndTime": "2026-10-12T16:45:10Z",
        "signedinUser": { "user": "users/118203456789", "displayName": "Ada Lovelace" }
    })
}

/// Someone who joined without signing in.
pub fn grace() -> Value {
    json!({
        "name": GRACE,
        "earliestStartTime": "2026-10-12T16:01:30Z",
        "latestEndTime": "2026-10-12T16:44:00Z",
        "anonymousUser": { "displayName": "Grace (guest)" }
    })
}

/// Someone who dialled in.
pub fn caller() -> Value {
    json!({
        "name": CALLER,
        "earliestStartTime": "2026-10-12T16:03:00Z",
        "latestEndTime": "2026-10-12T16:40:00Z",
        "phoneUser": { "displayName": "+1 ***-***-0093" }
    })
}

pub fn session(id: &str, start: &str, end: &str) -> Value {
    json!({ "name": format!("{ADA}/participantSessions/{id}"), "startTime": start, "endTime": end })
}

pub fn transcript() -> Value {
    json!({
        "name": TRANSCRIPT,
        "state": "FILE_GENERATED",
        "startTime": "2026-10-12T16:00:30Z",
        "endTime": "2026-10-12T16:45:00Z",
        "docsDestination": {
            "document": DOCUMENT,
            "exportUri": "https://docs.google.com/document/d/1kuceFZohVoCh6FulBHxwy6I15Ogpc4hP/view"
        }
    })
}

/// One thing `participant` said, from `start` to `end`.
pub fn entry(id: &str, participant: &str, start: &str, end: &str, text: &str) -> Value {
    json!({
        "name": format!("{TRANSCRIPT}/entries/{id}"),
        "participant": participant,
        "text": text,
        "languageCode": "en-US",
        "startTime": start,
        "endTime": end
    })
}

pub fn recording() -> Value {
    json!({
        "name": RECORDING,
        "state": "FILE_GENERATED",
        "startTime": "2026-10-12T16:00:20Z",
        "endTime": "2026-10-12T16:45:05Z",
        "driveDestination": {
            "file": FILE,
            "exportUri": "https://drive.google.com/file/d/1mZq9Xc0wT3vUu7rLhYp2sNdEaKbJ4gQf/view"
        }
    })
}

pub fn space() -> Value {
    json!({
        "name": SPACE,
        "meetingUri": MEETING_LINK,
        "meetingCode": MEETING_CODE,
        "config": {
            "accessType": "TRUSTED",
            "entryPointAccess": "ALL",
            "artifactConfig": { "transcriptionConfig": { "autoTranscriptionGeneration": "ON" } }
        },
        "activeConference": { "conferenceRecord": RECORD }
    })
}
