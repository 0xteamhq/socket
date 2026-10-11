//! Associations: which records are linked to which, and how.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::text::{id, nullable};

/// A record that another record is associated with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Association {
    /// The id of the associated record.
    #[serde(deserialize_with = "id")]
    pub to_object_id: String,
    /// Every way the two are associated: the plain association, and each label.
    #[serde(deserialize_with = "nullable")]
    pub association_types: Vec<AssociationType>,
}

/// One way two records are associated.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AssociationType {
    /// `HUBSPOT_DEFINED`, `USER_DEFINED` or `INTEGRATOR_DEFINED`.
    pub category: Option<String>,
    pub type_id: Option<i64>,
    /// The label, such as `Primary` or `Decision maker`. The plain association has none.
    pub label: Option<String>,
}

/// Who defined a kind of association.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssociationCategory {
    /// One of HubSpot's own, such as a contact's primary company.
    HubspotDefined,
    /// A label the account made.
    UserDefined,
    /// A label an application made.
    IntegratorDefined,
}

/// A kind of association, as HubSpot numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssociationSpec {
    pub association_category: AssociationCategory,
    /// The type id. It depends on the direction: contact to company is `279`,
    /// company to contact `280`.
    pub association_type_id: u32,
}

/// How to associate two records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateAssociation {
    /// The labels to give the association. They replace the labels it has.
    /// The plain association, without a label, when not given.
    pub types: Option<Vec<AssociationSpec>>,
}

/// An association that HubSpot confirmed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AssociationCreated {
    #[serde(deserialize_with = "id")]
    pub from_object_id: String,
    #[serde(deserialize_with = "id")]
    pub to_object_id: String,
    /// The type ids of the two object types, such as `0-1` for contacts.
    /// HubSpot names them when labels were set.
    pub from_object_type_id: Option<String>,
    pub to_object_type_id: Option<String>,
    /// The labels the association now has.
    #[serde(deserialize_with = "nullable")]
    pub labels: Vec<String>,
}
