//! Pipelines and their stages.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, area};
use crate::models::Pipeline;

/// The pipelines of an object type, each with its stages. Deals and tickets
/// have them.
#[derive(Debug, Clone, Copy)]
pub struct Pipelines<'a>(pub(crate) Api<'a>);

impl Pipelines<'_> {
    /// Lists the pipelines of an object type, with their stages.
    pub async fn list(&self, object_type: &str) -> Result<Vec<Pipeline>> {
        let body = self.0.send(RawRequest::get(self.of(object_type)?)).await?;
        self.0.results(&body, "pipelines")
    }

    /// Gets one pipeline, with its stages.
    pub async fn get(&self, object_type: &str, pipeline: &str) -> Result<Pipeline> {
        let id = self.0.segment("a pipeline id", pipeline)?;
        let request = RawRequest::get(format!("{}/{id}", self.of(object_type)?));
        let pipeline: Pipeline = self.0.decode(self.0.send(request).await?, "a pipeline")?;
        if pipeline.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without a pipeline"));
        }
        Ok(pipeline)
    }

    /// The address of an object type's pipelines.
    fn of(&self, object_type: &str) -> Result<String> {
        Ok(format!(
            "{}/{}",
            area("pipelines"),
            self.0.segment("an object type", object_type)?
        ))
    }
}
