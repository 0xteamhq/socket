//! The data Slack returns, and the content and options its methods take.
//!
//! One file per area of Slack. Everything is re-exported here, so a caller
//! writes `socketkit::slack::models::Message` whichever file it lives in.
//!
//! Every field Slack may omit has a default, so a response that carries less
//! than these types describe still reads.

mod conversation;
mod file;
mod message;
mod paging;
mod search;
mod user;
mod workspace;

pub use conversation::{Bookmark, Channel, CreateConversation, Described, ListConversations};
pub use file::{File, ListFiles};
pub use message::{
    History, ListScheduled, Message, Pin, PostMessage, PostedMessage, Reaction, ScheduledMessage, UpdateMessage,
};
pub use paging::Paging;
pub use search::{Search, SearchChannel, SearchMatch, SearchResults};
pub use user::{DndStatus, ListUserGroups, Presence, Profile, User, UserGroup};
pub use workspace::{Emoji, Reminder, Team};
