//! The lists of a workspace.

use socketkit_core::{RawRequest, Result};

use super::Api;
use crate::models::List;

/// The lists of a workspace, each a process its records move through.
#[derive(Debug, Clone, Copy)]
pub struct Lists<'a>(pub(crate) Api<'a>);

impl Lists<'_> {
    /// Lists every list the connection can see, in the order Attio's sidebar shows them.
    pub async fn list(&self) -> Result<Vec<List>> {
        self.0.all(RawRequest::get("lists"), "lists").await
    }

    /// Gets one list, by its slug or its id.
    pub async fn get(&self, list: &str) -> Result<List> {
        let list = self.0.segment("a list", list)?;
        self.0.one(RawRequest::get(format!("lists/{list}")), "a list").await
    }
}
