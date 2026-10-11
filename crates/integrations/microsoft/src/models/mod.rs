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
mod attendance;
mod availability;
mod calendar;
mod channel;
mod chat;
mod chat_message;
mod date_time;
mod email_address;
mod event;
mod folder;
mod html;
mod html_symbols;
mod identity;
mod item_body;
mod member;
mod message;
mod nullable;
mod online_meeting;
mod paging;
mod recording;
mod team;
mod transcript;
mod webvtt;

pub use attachment::Attachment;
pub use attendance::{AttendanceInterval, AttendanceRecord, AttendanceReport};
pub use availability::{
    AttendeeAvailability, FindMeetingTimes, GetSchedule, MeetingTimeSuggestion, MeetingTimeSuggestions, ScheduleError,
    ScheduleInformation, ScheduleItem, TimeConstraint,
};
pub use calendar::Calendar;
pub use channel::Channel;
pub use chat::{Chat, ChatType, CreateChat};
pub use chat_message::{
    ChannelIdentity, ChatAttachment, ChatMessage, Mention, Mentioned, MentionedConversation, MessageSender, Reaction,
    SendChatMessage, SenderIdentity,
};
pub use date_time::{DateTimeTimeZone, TimeSlot};
pub use email_address::{EmailAddress, Recipient};
pub use event::{
    Attendee, CancelEvent, CreateEvent, Event, EventResponse, Location, OnlineMeetingInfo, RespondToEvent,
    ResponseStatus, UpdateEvent,
};
pub use folder::{ListFolders, MailFolder};
pub use identity::{Identity, IdentitySet};
pub use item_body::ItemBody;
pub use member::ConversationMember;
pub use message::{
    BodyType, DraftMessage, FollowupFlag, GetMessage, ListMessages, Message, ReplyContent, SendMail, UpdateMessage,
};
pub use online_meeting::{ChatInfo, MeetingParticipant, MeetingParticipants, OnlineMeeting};
pub use paging::{Cursor, Paging};
pub use recording::Recording;
pub use team::Team;
pub use transcript::{Transcript, TranscriptContent, TranscriptEntry};
