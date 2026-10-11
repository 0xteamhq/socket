//! The data Salesforce returns, and the content and options its methods take.
//!
//! One file per area of the API. Everything is re-exported here, so a caller
//! writes `socketkit::salesforce::models::Record` whichever file it lives in.
//!
//! An organisation defines its own objects and fields, so a record is not a
//! struct with named fields: it is the object type, the id and a map of the
//! fields that were asked for. What describes an object keeps Salesforce's
//! own names (`keyPrefix`, `picklistValues`), so what Salesforce's
//! documentation says about a property holds here too. Every property
//! Salesforce may omit or leave null has a default, so a response that
//! carries less than these types describe still reads.

mod describe;
mod limits;
mod nullable;
mod query;
mod record;
mod search;

pub use describe::{ChildRelationship, Describe, Field, ListSObjects, PicklistValue, RecordType, SObjectSummary};
pub use limits::{Limit, Limits};
pub use query::{QueryOptions, QueryResult};
pub use record::{GetRecord, Record, RecordFields, Saved};
pub use search::{Find, FindIn, SearchResult, SearchScope};
