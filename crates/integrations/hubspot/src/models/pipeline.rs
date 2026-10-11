//! Pipelines: the stages a deal or a ticket moves through.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::text::{id, nullable};

/// A pipeline, with its stages.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Pipeline {
    /// What a record's `pipeline` property (`hs_pipeline` for a ticket) is set to.
    #[serde(deserialize_with = "id")]
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    pub display_order: Option<i64>,
    #[serde(deserialize_with = "nullable")]
    pub stages: Vec<PipelineStage>,
    pub archived: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// One stage of a pipeline.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct PipelineStage {
    /// What a record's `dealstage` property (`hs_pipeline_stage` for a
    /// ticket) is set to, to move it to this stage.
    #[serde(deserialize_with = "id")]
    pub id: String,
    #[serde(deserialize_with = "nullable")]
    pub label: String,
    pub display_order: Option<i64>,
    /// What the stage means, each value as text: `probability` and
    /// `isClosed` for a deal, `ticketState` for a ticket.
    #[serde(deserialize_with = "nullable")]
    pub metadata: BTreeMap<String, String>,
    /// `CRM_PERMISSIONS_ENFORCEMENT`, `READ_ONLY` or `INTERNAL_ONLY`.
    pub write_permissions: Option<String>,
    pub archived: Option<bool>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}
