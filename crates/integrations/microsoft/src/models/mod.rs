//! The data Microsoft Graph returns, and the options its methods take.
//!
//! One file per area of Graph. Everything is re-exported here, so a caller
//! writes `socketkit::microsoft::models::Transcript` whichever file it lives in.
//!
//! Field names are Graph's own (`joinWebUrl`, `createdDateTime`) when read or
//! written as JSON. Apart from an `id`, every field Graph may omit or send
//! as `null` has a default, so a response that carries less than these types
//! describe still reads. Dates are the ISO 8601 text Graph sends, in UTC.

mod attendance;
mod identity;
mod nullable;
mod online_meeting;
mod paging;
mod recording;
mod transcript;

pub use attendance::{AttendanceInterval, AttendanceRecord, AttendanceReport};
pub use identity::{Identity, IdentitySet};
pub use online_meeting::{ChatInfo, MeetingParticipant, MeetingParticipants, OnlineMeeting};
pub use paging::Paging;
pub use recording::Recording;
pub use transcript::{Transcript, TranscriptContent, TranscriptEntry};
