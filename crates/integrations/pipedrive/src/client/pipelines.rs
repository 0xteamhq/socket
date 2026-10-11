//! Pipelines and their stages. Version 2 of Pipedrive's API throughout.

use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{Paging, Pipeline, Stage};

/// Pipelines, and the stages a deal moves through in each.
#[derive(Debug, Clone, Copy)]
pub struct Pipelines<'a>(pub(crate) Api<'a>);

impl Pipelines<'_> {
    /// Lists the company's pipelines.
    pub async fn list(&self, paging: Paging) -> Result<Page<Pipeline>> {
        self.0.page(RawRequest::get("v2/pipelines"), &paging, "pipelines").await
    }

    /// Lists the stages of one pipeline, or of every pipeline when none is
    /// named. A deal's `stage_id` is one of these.
    pub async fn stages(&self, pipeline: Option<u64>, paging: Paging) -> Result<Page<Stage>> {
        let mut request = RawRequest::get("v2/stages");
        if let Some(pipeline) = pipeline {
            request = request.with_query("pipeline_id", pipeline.to_string());
        }
        self.0.page(request, &paging, "stages").await
    }
}
