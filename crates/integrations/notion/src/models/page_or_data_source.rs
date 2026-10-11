//! What a search or a query finds.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{DataSource, Page};

/// One thing found: a page or a data source. `object` says which.
///
/// A search returns both. A query returns the rows of a data source, which
/// are pages; a wiki also holds data sources among its rows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "object", rename_all = "snake_case")]
pub enum PageOrDataSource {
    Page(Box<Page>),
    DataSource(Box<DataSource>),
    /// A kind of object this version of Socket does not know.
    #[serde(other)]
    Other,
}
