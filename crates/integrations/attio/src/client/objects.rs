//! Objects: the kinds of record a workspace keeps.

use socketkit_core::{RawRequest, Result};

use super::Api;
use crate::models::Object;

/// Objects: the kinds of record a workspace keeps.
#[derive(Debug, Clone, Copy)]
pub struct Objects<'a>(pub(crate) Api<'a>);

impl Objects<'_> {
    /// Lists every object of the workspace, Attio's own and those the
    /// workspace made. Attio returns them all at once.
    pub async fn list(&self) -> Result<Vec<Object>> {
        self.0.one(RawRequest::get("objects"), "objects").await
    }

    /// Gets one object, by its slug or id.
    pub async fn get(&self, object: &str) -> Result<Object> {
        let object = self.0.id("an object", object)?;
        let object: Object = self
            .0
            .one(RawRequest::get(format!("objects/{object}")), "an object")
            .await?;
        if object.id.object_id.is_empty() {
            return Err(self.0.missing("an object"));
        }
        Ok(object)
    }
}
