//! Attributes: the fields of an object or of a list.

use socketkit_core::{Page, RawRequest, Result};

use super::{A_PAGE, Api, MOST};
use crate::models::{Attribute, ListAttributes, Paging, SelectOption, ShowArchived, Status, Target};

/// Attributes: the fields of an object or of a list. This is how a caller
/// learns what a workspace's records hold.
#[derive(Debug, Clone, Copy)]
pub struct Attributes<'a>(pub(crate) Api<'a>);

impl Attributes<'_> {
    /// Lists the attributes of an object or of a list, in the order Attio shows them.
    pub async fn list(&self, target: Target, identifier: &str, options: ListAttributes) -> Result<Page<Attribute>> {
        let request = archived(RawRequest::get(self.path(target, identifier)?), options.show_archived);
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.page(request, &paging, (A_PAGE, MOST), "attributes").await
    }

    /// Gets one attribute, by its slug or id.
    pub async fn get(&self, target: Target, identifier: &str, attribute: &str) -> Result<Attribute> {
        let path = self.one(target, identifier, attribute)?;
        let attribute: Attribute = self.0.one(RawRequest::get(path), "an attribute").await?;
        if attribute.id.attribute_id.is_empty() {
            return Err(self.0.missing("an attribute"));
        }
        Ok(attribute)
    }

    /// Lists what a select attribute can be set to.
    pub async fn options(
        &self,
        target: Target,
        identifier: &str,
        attribute: &str,
        options: ShowArchived,
    ) -> Result<Vec<SelectOption>> {
        let path = format!("{}/options", self.one(target, identifier, attribute)?);
        let request = archived(RawRequest::get(path), options.show_archived);
        self.0.one(request, "select options").await
    }

    /// Lists the statuses a status attribute can be at.
    pub async fn statuses(
        &self,
        target: Target,
        identifier: &str,
        attribute: &str,
        options: ShowArchived,
    ) -> Result<Vec<Status>> {
        let path = format!("{}/statuses", self.one(target, identifier, attribute)?);
        let request = archived(RawRequest::get(path), options.show_archived);
        self.0.one(request, "statuses").await
    }

    fn path(&self, target: Target, identifier: &str) -> Result<String> {
        let what = match target {
            Target::Objects => "an object",
            Target::Lists => "a list",
        };
        Ok(format!(
            "{}/{}/attributes",
            target.as_str(),
            self.0.id(what, identifier)?
        ))
    }

    fn one(&self, target: Target, identifier: &str, attribute: &str) -> Result<String> {
        let attribute = self.0.id("an attribute", attribute)?;
        Ok(format!("{}/{attribute}", self.path(target, identifier)?))
    }
}

/// `request`, asking for what was archived too when that was asked for.
fn archived(request: RawRequest, show_archived: Option<bool>) -> RawRequest {
    match show_archived {
        Some(show) => request.with_query("show_archived", show.to_string()),
        None => request,
    }
}
