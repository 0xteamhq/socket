//! The attributes of an object or a list.

use socketkit_core::{Page, RawRequest, Result};

use super::{Api, asking};
use crate::models::{Attribute, AttributeTarget, ListAttributes, SelectOption, ShowArchived, Status};

/// The attributes of an object or a list, with their options and statuses.
/// This is how the shape of a workspace's data is learnt.
#[derive(Debug, Clone, Copy)]
pub struct Attributes<'a>(pub(crate) Api<'a>);

impl Attributes<'_> {
    /// Lists the attributes of an object or a list, in the order Attio shows them.
    ///
    /// `identifier` is the slug or id of the object or the list. Every
    /// attribute comes back unless a limit is given.
    pub async fn list(
        &self,
        target: AttributeTarget,
        identifier: &str,
        options: ListAttributes,
    ) -> Result<Page<Attribute>> {
        let attributes = self.place(target, identifier)?;
        // Attio documents no largest page for attributes.
        let window = self.0.window(options.cursor.as_deref(), options.limit, 0, u32::MAX)?;
        let request = asking(RawRequest::get(attributes), "limit", options.limit);
        let request = asking(request, "offset", (window.offset > 0).then_some(window.offset));
        let request = asking(request, "show_archived", options.show_archived);
        let items = self.0.all(request, "attributes").await?;
        Ok(match options.limit {
            Some(_) => window.page(items),
            None => Page {
                items,
                next_cursor: None,
            },
        })
    }

    /// Gets one attribute, by its slug or its id.
    pub async fn get(&self, target: AttributeTarget, identifier: &str, attribute: &str) -> Result<Attribute> {
        let attribute = self.one(target, identifier, attribute)?;
        self.0.one(RawRequest::get(attribute), "an attribute").await
    }

    /// Lists the options of a select attribute: the values it may hold.
    pub async fn options(
        &self,
        target: AttributeTarget,
        identifier: &str,
        attribute: &str,
        options: ShowArchived,
    ) -> Result<Vec<SelectOption>> {
        let attribute = self.one(target, identifier, attribute)?;
        let request = RawRequest::get(format!("{attribute}/options"));
        let request = asking(request, "show_archived", options.show_archived);
        self.0.all(request, "options").await
    }

    /// Lists the statuses of a status attribute: the stages it may be in.
    pub async fn statuses(
        &self,
        target: AttributeTarget,
        identifier: &str,
        attribute: &str,
        options: ShowArchived,
    ) -> Result<Vec<Status>> {
        let attribute = self.one(target, identifier, attribute)?;
        let request = RawRequest::get(format!("{attribute}/statuses"));
        let request = asking(request, "show_archived", options.show_archived);
        self.0.all(request, "statuses").await
    }

    /// The address of the attributes of an object or a list.
    fn place(&self, target: AttributeTarget, identifier: &str) -> Result<String> {
        let what = match target {
            AttributeTarget::Objects => "an object",
            AttributeTarget::Lists => "a list",
        };
        let identifier = self.0.segment(what, identifier)?;
        Ok(format!("{}/{identifier}/attributes", target.as_str()))
    }

    /// The address of one attribute.
    fn one(&self, target: AttributeTarget, identifier: &str, attribute: &str) -> Result<String> {
        let attributes = self.place(target, identifier)?;
        let attribute = self.0.segment("an attribute", attribute)?;
        Ok(format!("{attributes}/{attribute}"))
    }
}
