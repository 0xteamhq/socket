//! The data Attio returns, and the content and options its methods take.
//!
//! One file per area of Attio's API. Everything is re-exported here, so a
//! caller writes `socketkit::attio::models::Record` whichever file it lives in.
//!
//! The types follow Attio's own shapes and are written in JSON with Attio's
//! own names (`api_slug`, `parent_record_id`), so what Attio's documentation
//! says about a field holds here too. Every field Attio may omit or leave
//! null has a default, so a response that carries less than these types
//! describe still reads.

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
mod value;

pub use actor::Actor;
pub use attribute::{
    Attribute, AttributeId, AttributeTarget, ListAttributes, Relationship, SelectOption, SelectOptionId, ShowArchived,
    Status, StatusId,
};
pub use call_recording::{CallRecording, CallRecordingId, Speaker, Transcript, TranscriptSegment};
pub use entry::{CreateEntry, Entry, EntryId, WriteEntry};
pub use list::{List, ListId, MemberAccess};
pub use meeting::{ListMeetings, Meeting, MeetingId, MeetingRecord, MeetingSort, MeetingTime, Participant};
pub use member::{WorkspaceMember, WorkspaceMemberId};
pub use note::{CreateNote, ListNotes, Note, NoteFormat, NoteId};
pub use object::{Object, ObjectId};
pub use paging::Paging;
pub use query::{Query, Sort, SortDirection};
pub use record::{Record, RecordEntry, RecordId, WriteRecord};
pub use task::{Assignee, CreateTask, LinkedRecord, ListTasks, Task, TaskId, TaskSort, UpdateTask};
pub use thread::{
    Comment, CommentId, CommentOnEntry, CommentOnRecord, CommentedEntry, CommentedRecord, CreateComment, GetThread,
    ListThreads, Thread, ThreadId,
};
pub use value::{AttributeValue, Values};
