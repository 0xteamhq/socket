//! Properties: the fields an object type has, in this account.
//!
//! A record returns only the properties it is asked for, and every account
//! adds its own. This is where their names are found.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, with_query};
use crate::API_VERSION;
use crate::models::{ListProperties, Property, PropertySummary};

/// The properties of an object type.
#[derive(Debug, Clone, Copy)]
pub struct Properties<'a>(pub(crate) Api<'a>);

impl Properties<'_> {
    /// Lists every property of an object type: its name, its label and the
    /// kind of value it holds. HubSpot returns them all at once, so each row
    /// is kept short; `get` returns a property's description and options.
    pub async fn list(&self, object_type: &str, options: ListProperties) -> Result<Vec<PropertySummary>> {
        let request = RawRequest::get(self.path(object_type)?);
        let request = with_query(
            request,
            "archived",
            options.archived.map(|archived| archived.to_string()),
        );
        let body = self.0.send(request).await?;
        self.0.results(&body, "properties")
    }

    /// Gets one property by its internal name, with the values it can take.
    pub async fn get(&self, object_type: &str, name: &str) -> Result<Property> {
        let name = self.0.segment("`name`", name)?;
        let request = RawRequest::get(format!("{}/{name}", self.path(object_type)?));
        let property: Property = self.0.decode(self.0.send(request).await?, "a property")?;
        if property.name.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without a property"));
        }
        Ok(property)
    }

    fn path(&self, object_type: &str) -> Result<String> {
        let object_type = self.0.object_type("`object_type`", object_type)?;
        Ok(format!("crm/properties/{API_VERSION}/{object_type}"))
    }
}
