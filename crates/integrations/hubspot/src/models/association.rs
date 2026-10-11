//! Associations: the links between records.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::RecordId;
use super::nullable::{id, nullable};

/// A record that another record is associated with, and how.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Association {
    /// The id of the record at the other end.
    #[serde(deserialize_with = "id")]
    pub to_object_id: String,
    /// Every kind of association between the two records.
    #[serde(deserialize_with = "nullable")]
    pub association_types: Vec<AssociationLabel>,
}

/// One kind of association between two records, as HubSpot returns it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct AssociationLabel {
    /// `HUBSPOT_DEFINED`, `USER_DEFINED` or `INTEGRATOR_DEFINED`.
    pub category: Option<String>,
    pub type_id: Option<i64>,
    /// The label a person sees, such as `Billing contact`. An unlabelled association has none.
    pub label: Option<String>,
}

/// One kind of association to make between two records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AssociationType {
    /// `HUBSPOT_DEFINED` for HubSpot's own kinds, `USER_DEFINED` for a label
    /// made in the account.
    pub association_category: String,
    pub association_type_id: i64,
}

impl AssociationType {
    /// One of HubSpot's own kinds of association, by its number.
    pub fn hubspot_defined(association_type_id: i64) -> Self {
        Self {
            association_category: "HUBSPOT_DEFINED".into(),
            association_type_id,
        }
    }
}

/// How to associate two records.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CreateAssociation {
    /// The kinds of association the two records are to have, for a labelled
    /// association. They replace the labels that were there, so name the
    /// ones to keep as well. HubSpot makes its default, unlabelled
    /// association between the two object types when this is not given.
    pub types: Option<Vec<AssociationType>>,
}

/// An association that was made.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Associated {
    #[serde(deserialize_with = "id")]
    pub from_object_id: String,
    #[serde(deserialize_with = "id")]
    pub to_object_id: String,
    /// The labels now between the two records. HubSpot returns them for a
    /// labelled association.
    #[serde(deserialize_with = "nullable")]
    pub labels: Vec<String>,
    /// The kinds of association that were made. HubSpot returns them for a
    /// default association.
    pub types: Vec<AssociationType>,
}

/// One end of a default association, as HubSpot answers the request that
/// makes one. It lists the association once from each end.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DefaultAssociation {
    pub(crate) from: RecordId,
    pub(crate) to: RecordId,
    pub(crate) association_spec: Option<AssociationType>,
}
