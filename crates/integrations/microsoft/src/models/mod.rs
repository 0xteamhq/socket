//! The data Microsoft Graph returns, and the content and options its methods take.
//!
//! One file per area of Graph. Everything is re-exported here, so a caller
//! writes `socketkit::microsoft::models::Event` whichever file it lives in.
//!
//! The types follow Graph's own shapes and are written in JSON with Graph's
//! own names (`isOnlineMeeting`, `onlineMeeting.joinUrl`), so what Graph's
//! documentation says about a field holds here too. Every field Graph may
//! omit or leave null has a default, so a response that carries less than
//! these types describe still reads.

mod attachment;
mod availability;
mod calendar;
mod date_time;
mod email_address;
mod event;
mod folder;
mod item_body;
mod message;
mod nullable;
mod paging;

pub use attachment::Attachment;
pub use availability::{
    AttendeeAvailability, FindMeetingTimes, GetSchedule, MeetingTimeSuggestion, MeetingTimeSuggestions, ScheduleError,
    ScheduleInformation, ScheduleItem, TimeConstraint,
};
pub use calendar::Calendar;
pub use date_time::{DateTimeTimeZone, TimeSlot};
pub use email_address::{EmailAddress, Recipient};
pub use event::{
    Attendee, CancelEvent, CreateEvent, Event, EventResponse, Location, OnlineMeeting, RespondToEvent, ResponseStatus,
    UpdateEvent,
};
pub use folder::{ListFolders, MailFolder};
pub use item_body::ItemBody;
pub use message::{
    BodyType, DraftMessage, FollowupFlag, GetMessage, ListMessages, Message, ReplyContent, SendMail, UpdateMessage,
};
pub use paging::Paging;
