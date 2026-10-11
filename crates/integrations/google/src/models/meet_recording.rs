//! Recordings of a meeting.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// One recording of a meeting. A meeting recorded twice has two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetRecording {
    /// `conferenceRecords/{id}/recordings/{id}`.
    pub name: String,
    /// `STARTED` while recording, `ENDED` once it stopped, and
    /// `FILE_GENERATED` when the file is in Drive and can be read.
    pub state: Option<String>,
    /// When recording began.
    pub start_time: Option<String>,
    /// When recording stopped.
    pub end_time: Option<String>,
    /// The file in Drive. Absent until the file has been written.
    pub drive_destination: Option<MeetDriveDestination>,
}

/// Where a recording was saved in Drive, as an MP4 file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct MeetDriveDestination {
    /// The Drive file's id, as Drive's own methods take it.
    pub file: Option<String>,
    /// The address that plays the recording in a browser.
    pub export_uri: Option<String>,
}
