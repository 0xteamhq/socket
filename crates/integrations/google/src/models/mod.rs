//! The data Google returns, and the content and options its methods take.
//!
//! One file per area of an API. Everything is re-exported here, so a caller
//! writes `socketkit::google::models::GmailMessage` whichever file it lives in.
//!
//! The types follow Google's own shapes and are written in JSON with Google's
//! own names (`threadId`, `hangoutLink`), so what Google's documentation says
//! about a field holds here too. Every field Google may omit has a default,
//! so a response that carries less than these types describe still reads.

mod paging;

pub use paging::Paging;

// ── gmail: modules ──
mod gmail_address;
mod gmail_draft;
mod gmail_label;
mod gmail_message;
mod gmail_mime;
mod gmail_profile;
mod gmail_reply;
mod gmail_rfc2822;
mod gmail_thread;
mod gmail_words;

// ── calendar: modules ──
mod calendar;
mod calendar_attendee;
mod calendar_conference;
mod calendar_event;
mod calendar_event_filter;
mod calendar_event_time;
mod calendar_freebusy;

// ── meet: modules ──
mod meet_conference;
mod meet_participant;
mod meet_recording;
mod meet_space;
mod meet_time;
mod meet_transcript;
mod meet_transcript_content;

// ── drive: modules ──
mod drive_export;
mod drive_file;
mod drive_permission;
mod shared_drive;

// ── docs and sheets: modules ──
mod document;
mod document_structure;
mod document_text;
mod spreadsheet;
mod value_range;

// ── gmail: types ──
pub use gmail_address::GmailAddress;
pub(crate) use gmail_draft::GmailWireDraft;
pub use gmail_draft::{GmailDraft, GmailDraftRef, GmailListDrafts};
pub use gmail_label::{GmailLabel, GmailLabelColor};
pub use gmail_message::{
    GmailAttachment, GmailAttachmentBody, GmailFormat, GmailGetMessage, GmailListMessages, GmailMessage,
    GmailMessageRef, GmailModifyMessage, GmailSendMessage,
};
pub(crate) use gmail_mime::GmailWireMessage;
pub use gmail_profile::GmailProfile;
pub use gmail_reply::GmailReply;
pub(crate) use gmail_rfc2822::{GmailThreading, raw as gmail_raw, sendable as gmail_sendable};
pub(crate) use gmail_thread::GmailWireThread;
pub use gmail_thread::{GmailGetThread, GmailListThreads, GmailThread};

// ── calendar: types ──
pub use calendar::{CalendarListEntry, CalendarListFilter};
pub use calendar_attendee::{EventAttendee, EventInvitee, EventResponse};
pub use calendar_conference::{
    EventConference, EventConferenceRequest, EventConferenceRequestStatus, EventConferenceSolution,
    EventConferenceSolutionKey, EventEntryPoint,
};
pub use calendar_event::{CalendarEvent, EventAttachment, EventDelete, EventInsert, EventPatch, EventPerson};
pub use calendar_event_filter::{EventFilter, EventInstancesFilter};
pub use calendar_event_time::EventTime;
pub use calendar_freebusy::{FreeBusy, FreeBusyCalendar, FreeBusyError, FreeBusyPeriod, FreeBusyQuery};

// ── meet: types ──
pub use meet_conference::{ConferenceRecord, MeetListConferenceRecords};
pub use meet_participant::{
    MeetAnonymousUser, MeetParticipant, MeetParticipantSession, MeetPhoneUser, MeetSignedinUser,
};
pub use meet_recording::{MeetDriveDestination, MeetRecording};
pub use meet_space::{
    MeetActiveConference, MeetArtifactConfig, MeetRecordingConfig, MeetSmartNotesConfig, MeetSpace, MeetSpaceConfig,
    MeetTranscriptionConfig,
};
pub(crate) use meet_time::millis as meet_millis;
pub use meet_transcript::{MeetDocsDestination, MeetReadTranscript, MeetTranscript, MeetTranscriptEntry};
pub use meet_transcript_content::{MeetTranscriptContent, MeetTranscriptContentEntry};

// ── drive: types ──
pub use drive_export::{DriveExport, DriveExportFormat};
pub use drive_file::{DriveCopyFile, DriveCreateFolder, DriveFile, DriveListFiles, DriveShortcutDetails, DriveUser};
pub use drive_permission::{DrivePermission, DrivePermissionDetail};
pub use shared_drive::SharedDrive;

// ── docs and sheets: types ──
pub use document::{
    DocsAppendText, DocsCreateDocument, Document, DocumentTab, DocumentTabText, DocumentText, DocumentUpdate,
    DocumentWriteControl,
};
pub(crate) use document_structure::DocumentResource;
pub use spreadsheet::{Sheet, SheetGridProperties, SheetProperties, Spreadsheet, SpreadsheetProperties};
pub use value_range::{
    SheetsAppendValues, SheetsAppendedValues, SheetsDateTimeRenderOption, SheetsDimension, SheetsGetValues,
    SheetsUpdateValues, SheetsUpdatedValues, SheetsValueInputOption, SheetsValueRanges, SheetsValueRenderOption,
    ValueRange,
};
