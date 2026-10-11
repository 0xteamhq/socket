//! The scopes Google's operations ask for, so that nobody retypes the URLs.
//!
//! Only the two Drive and Docs read scopes are among the provider's defaults.
//! An application names the others it wants in [`crate::GoogleOAuth::scopes`];
//! each operation lists what it needs in `required_scopes`.

/// Read mail, threads, labels, drafts and the profile.
pub const GMAIL_READONLY: &str = "https://www.googleapis.com/auth/gmail.readonly";
/// Create, change, send and delete drafts.
pub const GMAIL_COMPOSE: &str = "https://www.googleapis.com/auth/gmail.compose";
/// Send mail, and nothing else.
pub const GMAIL_SEND: &str = "https://www.googleapis.com/auth/gmail.send";
/// Change a message's labels, and move it to and from the bin.
pub const GMAIL_MODIFY: &str = "https://www.googleapis.com/auth/gmail.modify";

/// Read calendars, events and when people are busy.
pub const CALENDAR_READONLY: &str = "https://www.googleapis.com/auth/calendar.readonly";
/// Create, change, answer and delete events.
pub const CALENDAR_EVENTS: &str = "https://www.googleapis.com/auth/calendar.events";

/// Read conference records, participants, transcripts and recordings of Meet.
pub const MEETINGS_SPACE_READONLY: &str = "https://www.googleapis.com/auth/meetings.space.readonly";

/// Read every file in Drive. A default scope of the provider.
pub const DRIVE_READONLY: &str = "https://www.googleapis.com/auth/drive.readonly";
/// Create files, and change the files this application created or was given.
pub const DRIVE_FILE: &str = "https://www.googleapis.com/auth/drive.file";
/// Read Google Docs. A default scope of the provider.
pub const DOCUMENTS_READONLY: &str = "https://www.googleapis.com/auth/documents.readonly";
/// Create and change Google Docs.
pub const DOCUMENTS: &str = "https://www.googleapis.com/auth/documents";
/// Read Google Sheets.
pub const SPREADSHEETS_READONLY: &str = "https://www.googleapis.com/auth/spreadsheets.readonly";
/// Change Google Sheets.
pub const SPREADSHEETS: &str = "https://www.googleapis.com/auth/spreadsheets";
