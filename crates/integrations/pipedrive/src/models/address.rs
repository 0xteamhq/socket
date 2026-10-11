//! An address, as Pipedrive breaks one down.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// An organisation's address, a person's postal address, or where an
/// activity takes place. `value` is the whole address on one line; the rest
/// are its parts, which Pipedrive fills in when it recognises the address.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Address {
    /// The whole address, on one line.
    pub value: Option<String>,
    pub country: Option<String>,
    /// The state or region.
    pub admin_area_level_1: Option<String>,
    /// The county or district.
    pub admin_area_level_2: Option<String>,
    /// The city or town.
    pub locality: Option<String>,
    pub sublocality: Option<String>,
    /// The street.
    pub route: Option<String>,
    pub street_number: Option<String>,
    /// The apartment or suite.
    pub subpremise: Option<String>,
    pub postal_code: Option<String>,
}
