//! The data HubSpot returns, and the content and options its methods take.
//!
//! One file per area of the CRM API. Everything is re-exported here, so a
//! caller writes `socketkit::hubspot::models::Object` whichever file it lives in.
//!
//! The types follow HubSpot's own shapes and are written in JSON with
//! HubSpot's own names (`createdAt`, `filterGroups`, `associationTypeId`), so
//! what HubSpot's documentation says about a field holds here too. Every
//! field HubSpot may omit or leave null has a default, so a response that
//! carries less than these types describe still reads.

mod association;
mod batch;
mod object;
mod owner;
mod paging;
mod pipeline;
mod property;
mod search;
mod text;

pub use association::{
    Association, AssociationCategory, AssociationCreated, AssociationSpec, AssociationType, CreateAssociation,
};
pub use batch::{BatchCreate, BatchError, BatchRead, BatchResult, BatchUpdate, BatchUpdateInput};
pub use object::{
    AssociatedId, AssociatedIds, CreateObject, GetObject, ListObjects, NewAssociation, Object, ObjectId, PropertyValue,
    UpdateObject,
};
pub use owner::{GetOwner, ListOwners, Owner, OwnerIdProperty, OwnerTeam};
pub use paging::Paging;
pub use pipeline::{Pipeline, PipelineStage};
pub use property::{ListProperties, Property, PropertyModification, PropertyOption, PropertySummary};
pub use search::{Filter, FilterGroup, FilterOperator, Search, SearchResults, Sort, SortDirection};
