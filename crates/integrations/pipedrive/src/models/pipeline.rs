//! Pipelines and the stages a deal moves through.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A pipeline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Pipeline {
    pub id: u64,
    pub name: Option<String>,
    /// The pipeline's place among the company's pipelines.
    pub order_nr: Option<u64>,
    pub is_deleted: Option<bool>,
    /// Whether deals in this pipeline carry a probability.
    pub is_deal_probability_enabled: Option<bool>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
}

/// One stage of a pipeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stage {
    pub id: u64,
    pub name: Option<String>,
    /// The stage's place in its pipeline, first to last.
    pub order_nr: Option<u64>,
    pub pipeline_id: Option<u64>,
    /// How likely a deal in this stage is to be won, as a percentage.
    pub deal_probability: Option<f64>,
    /// Whether a deal left in this stage is marked as rotting.
    pub is_deal_rot_enabled: Option<bool>,
    /// After how many days without activity a deal in this stage rots.
    pub days_to_rotten: Option<u64>,
    pub is_deleted: Option<bool>,
    pub add_time: Option<String>,
    pub update_time: Option<String>,
}
