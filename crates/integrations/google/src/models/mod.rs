//! The data Google returns, and the content and options its methods take.
//!
//! One file per area of Google. Everything is re-exported here, so a caller
//! writes `socketkit::google::models::Event` whichever file it lives in.
//!
//! Fields carry Google's own names in JSON (`timeMin`, `hangoutLink`), so an
//! operation's input and output read like Google's documentation. Every field
//! Google may omit has a default, so a response that carries less than these
//! types describe still reads.

mod calendar;
mod calendar_attendee;
mod calendar_conference;
mod calendar_event;
mod calendar_freebusy;

pub use calendar::{CalendarListEntry, ListCalendars};
pub use calendar_attendee::{Attendee, NewAttendee, Respond};
pub use calendar_conference::{
    ConferenceData, ConferenceRequest, ConferenceRequestStatus, ConferenceSolution, ConferenceSolutionKey, EntryPoint,
};
pub use calendar_event::{
    Attachment, DeleteEvent, Event, EventTime, InsertEvent, Instances, ListEvents, PatchEvent, Person,
};
pub use calendar_freebusy::{BusyPeriod, CalendarBusy, FreeBusy, FreeBusyError, FreeBusyQuery};
