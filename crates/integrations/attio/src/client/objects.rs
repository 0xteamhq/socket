//! The objects a workspace keeps.

use socketkit_core::{RawRequest, Result};

use super::Api;
use crate::models::Object;

/// The objects a workspace keeps: people, companies, deals and its own.
#[derive(Debug, Clone, Copy)]
pub struct Objects<'a>(pub(crate) Api<'a>);

impl Objects<'_> {
    /// Lists every object of the workspace, Attio's own and those the workspace defined.
    pub async fn list(&self) -> Result<Vec<Object>> {
        self.0.all(RawRequest::get("objects"), "objects").await
    }

    /// Gets one object, by its slug or its id.
    pub async fn get(&self, object: &str) -> Result<Object> {
        let object = self.0.segment("an object", object)?;
        self.0
            .one(RawRequest::get(format!("objects/{object}")), "an object")
            .await
    }
}
