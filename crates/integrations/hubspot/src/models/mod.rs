//! The data HubSpot returns, and the content and options its methods take.
//!
//! One file per area of the CRM. Everything is re-exported here, so a caller
//! writes `socketkit::hubspot::models::Record` whichever file it lives in.
//!
//! The types follow HubSpot's own shapes and are written in JSON with
//! HubSpot's own names (`createdAt`, `filterGroups`, `associationTypeId`), so
//! what HubSpot's documentation says about a field holds here too. Every
//! field HubSpot may omit or leave null has a default, so a response that
//! carries less than these types describe still reads.

mod association;
mod batch;
mod nullable;
mod object;
mod owner;
mod paging;
mod pipeline;
mod property;
mod search;

pub(crate) use association::DefaultAssociation;
pub use association::{Associated, Association, AssociationLabel, AssociationType, CreateAssociation};
pub use batch::{BatchCreate, BatchError, BatchRead, BatchResult, BatchUpdate, RecordUpdate};
pub use object::{
    AssociatedRecord, AssociatedRecords, CreateObject, GetObject, ListObjects, MoreAssociated, NewAssociation,
    NextAssociated, Record, RecordId, UpdateObject,
};
pub use owner::{GetOwner, ListOwners, Owner, OwnerIdProperty, Team};
pub use paging::Paging;
pub use pipeline::{Pipeline, PipelineStage};
pub use property::{ListProperties, ModificationMetadata, Property, PropertyOption};
pub use search::{Direction, Filter, FilterGroup, Operator, Search, Sort};
