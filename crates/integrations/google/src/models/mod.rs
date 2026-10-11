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

// ── calendar: modules ──
mod calendar;
mod calendar_attendee;
mod calendar_conference;
mod calendar_event;
mod calendar_event_filter;
mod calendar_event_time;
mod calendar_freebusy;

// ── meet: modules ──

// ── drive: modules ──

// ── docs and sheets: modules ──

// ── gmail: types ──

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

// ── drive: types ──

// ── docs and sheets: types ──
