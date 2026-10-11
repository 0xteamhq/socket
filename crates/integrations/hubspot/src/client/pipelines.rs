//! Pipelines: the stages a deal or a ticket moves through.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::API_VERSION;
use crate::models::Pipeline;

/// The pipelines of an object type.
#[derive(Debug, Clone, Copy)]
pub struct Pipelines<'a>(pub(crate) Api<'a>);

impl Pipelines<'_> {
    /// Lists the pipelines of an object type, `deals` or `tickets` as a
    /// rule, each with its stages.
    pub async fn list(&self, object_type: &str) -> Result<Vec<Pipeline>> {
        let body = self.0.send(RawRequest::get(self.path(object_type)?)).await?;
        self.0.results(&body, "pipelines")
    }

    /// Gets one pipeline, with its stages.
    pub async fn get(&self, object_type: &str, pipeline: &str) -> Result<Pipeline> {
        let pipeline = self.0.segment("`pipeline`", pipeline)?;
        let request = RawRequest::get(format!("{}/{pipeline}", self.path(object_type)?));
        let pipeline: Pipeline = self.0.decode(self.0.send(request).await?, "a pipeline")?;
        if pipeline.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without a pipeline"));
        }
        Ok(pipeline)
    }

    fn path(&self, object_type: &str) -> Result<String> {
        let object_type = self.0.object_type("`object_type`", object_type)?;
        Ok(format!("crm/pipelines/{API_VERSION}/{object_type}"))
    }
}
