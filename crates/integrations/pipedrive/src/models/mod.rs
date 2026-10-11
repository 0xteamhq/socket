//! The data Pipedrive returns, and the content and options its methods take.
//!
//! One file per area of Pipedrive. Everything is re-exported here, so a
//! caller writes `socketkit::pipedrive::models::Deal` whichever file it
//! lives in.
//!
//! The types follow Pipedrive's own shapes and are written in JSON with
//! Pipedrive's own names (`org_id`, `expected_close_date`), so what
//! Pipedrive's documentation says about a field holds here too. Where the two
//! versions of the API differ, the names are version 2's. Every field
//! Pipedrive may omit or leave null is optional, so a response that carries
//! less than these types describe still reads; a record without its id does
//! not.

mod activity;
mod address;
mod custom_fields;
mod deal;
mod field;
mod lead;
mod note;
mod nullable;
mod organization;
mod paging;
mod person;
mod pipeline;
mod search;
mod sort;
mod user;

pub use activity::{Activity, Attendee, CreateActivity, ListActivities, Participant, UpdateActivity};
pub use address::Address;
pub use custom_fields::{CustomFields, NamedValue, named_custom_fields};
pub use deal::{CreateDeal, Deal, DealStatus, DealStatusFilter, ListDeals, SearchDeals, UpdateDeal};
pub use field::{Field, FieldOption};
pub use lead::{CreateLead, Lead, LeadValue, ListLeads, SearchLeads, UpdateLead};
pub use note::{CreateNote, ListNotes, Note, UpdateNote};
pub use organization::{CreateOrganization, ListOrganizations, Organization, SearchOrganizations, UpdateOrganization};
pub use paging::Paging;
pub use person::{ContactDetail, CreatePerson, ListPersons, Person, SearchPersons, UpdatePerson};
pub use pipeline::{Pipeline, Stage};
pub use search::{ItemId, ItemType, Linked, SearchItem, SearchItems, SearchResult};
pub use sort::SortDirection;
pub use user::User;

pub(crate) use custom_fields::{gather_custom_fields, is_field_key};
pub(crate) use note::preview;
