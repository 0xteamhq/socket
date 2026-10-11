//! Lists: a workspace's own groupings of records.

use socketkit_core::{RawRequest, Result};

use super::Api;
use crate::models::List;

/// Lists: a workspace's own groupings of records.
#[derive(Debug, Clone, Copy)]
pub struct Lists<'a>(pub(crate) Api<'a>);

impl Lists<'_> {
    /// Lists every list the token can see, in the order of Attio's sidebar.
    /// Attio returns them all at once.
    pub async fn list(&self) -> Result<Vec<List>> {
        self.0.one(RawRequest::get("lists"), "lists").await
    }

    /// Gets one list, by its slug or id.
    pub async fn get(&self, list: &str) -> Result<List> {
        let list = self.0.id("a list", list)?;
        let list: List = self.0.one(RawRequest::get(format!("lists/{list}")), "a list").await?;
        if list.id.list_id.is_empty() {
            return Err(self.0.missing("a list"));
        }
        Ok(list)
    }
}
