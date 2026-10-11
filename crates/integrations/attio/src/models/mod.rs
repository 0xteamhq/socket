//! The data Attio returns, and the content and options its methods take.
//!
//! One file per area of Attio. Everything is re-exported here, so a caller
//! writes `socketkit::attio::models::Record` whichever file it lives in.
//!
//! The types follow Attio's own shapes and are written in JSON with Attio's
//! own names (`api_slug`, `parent_record_id`), so what Attio's documentation
//! says about a field holds here too. Every field Attio may omit or leave
//! null has a default, so a response that carries less than these types
//! describe still reads.
//!
//! Three things are added to what Attio sends, each named where it is
//! defined: the current value of every attribute beside the values Attio
//! lists for it ([`Values`]), a token's scopes as a list ([`TokenInfo`]), and
//! a transcript in the shape the other integrations use ([`Transcript`]).

mod actor;
mod attribute;
mod call_recording;
mod entry;
mod list;
mod meeting;
mod member;
mod note;
mod nullable;
mod object;
mod paging;
mod query;
mod record;
mod task;
mod thread;
mod token;
mod transcript;
mod value;

pub use actor::Actor;
pub use attribute::{
    Attribute, AttributeConfig, AttributeId, CurrencyConfig, ListAttributes, RecordReferenceConfig, Relationship,
    SelectOption, SelectOptionId, ShowArchived, Status, StatusId, Target,
};
pub(crate) use call_recording::SentCallRecording;
pub use call_recording::{CallRecording, CallRecordingId, CallRecordingRow};
pub use entry::{Entry, EntryId, EntryRow, QueryEntries, WriteEntry};
pub use list::{List, ListId, MemberAccess};
pub use meeting::{ListMeetings, Meeting, MeetingId, MeetingRecord, MeetingSort, MeetingTime, Participant};
pub use member::{WorkspaceMember, WorkspaceMemberId};
pub use note::{CreateNote, ListNotes, Note, NoteFormat, NoteId, NoteRow, NoteTag};
pub use object::{Object, ObjectId};
pub use paging::Paging;
pub use query::{Direction, Sort};
pub use record::{QueryRecords, Record, RecordEntry, RecordId, RecordRow, WriteRecord};
pub use task::{
    AssignTo, CreateTask, LinkRecord, ListTasks, Task, TaskAssignee, TaskId, TaskRecord, TaskSort, UpdateTask,
};
pub use thread::{
    Comment, CommentEntry, CommentId, CommentRecord, CreateComment, ListThreads, OnEntry, OnRecord, Thread, ThreadId,
    ThreadRow,
};
pub use token::TokenInfo;
pub use transcript::{Transcript, TranscriptEntry};
pub use value::{AttributeValue, Values};
