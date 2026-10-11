//! Recordings of Teams online meetings.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::IdentitySet;

/// One recording of a meeting. A meeting recorded twice has two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Recording {
    pub id: String,
    pub meeting_id: Option<String>,
    pub call_id: Option<String>,
    /// The same value on the transcript made from this recording.
    pub content_correlation_id: Option<String>,
    pub created_date_time: Option<String>,
    pub end_date_time: Option<String>,
    /// Where the video is. Needs the token to download.
    pub recording_content_url: Option<String>,
    pub meeting_organizer: Option<IdentitySet>,
}
