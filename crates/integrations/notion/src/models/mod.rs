//! The data Notion returns, and the content and options its methods take.
//!
//! One file per area of Notion's API. Everything is re-exported here, so a
//! caller writes `socketkit::notion::models::Page` whichever file it lives in.
//!
//! The types follow Notion's own shapes and are written in JSON with
//! Notion's own names (`in_trash`, `has_children`, `rich_text`), so what
//! Notion's documentation says about a field holds here too. What Notion
//! writes in many forms, such as a property's value, a block's content or a
//! filter, is kept as the JSON Notion documents. Every field Notion may omit
//! has a default, so a response that carries less than these types describe
//! still reads.

mod block;
mod comment;
mod database;
mod markdown;
mod nullable;
mod page;
mod page_content;
mod page_or_data_source;
mod paging;
mod parent;
mod property;
mod rich_text;
mod search;
mod sort;
mod tree;
mod user;

pub use block::{AppendBlocks, Block, BlockRef, Position, PositionKind, UpdateBlock};
pub use comment::{Comment, CreateComment};
pub use database::{DataSource, DataSourceRef, Database, QueryDataSource};
pub use page::{CreatePage, Page, UpdatePage};
pub use page_content::{PageContent, ReadPage};
pub use page_or_data_source::PageOrDataSource;
pub use paging::Paging;
pub use parent::Parent;
pub use property::{PropertyItems, PropertyValue};
pub use rich_text::{Annotations, Equation, Link, RichText, Text};
pub use search::{SearchFilter, SearchObject, SearchQuery, SearchSort};
pub use sort::{Sort, SortDirection};
pub(crate) use tree::{Cut, Tree};
pub use user::{Bot, Person, User};
