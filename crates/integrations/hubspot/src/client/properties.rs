//! Properties: the fields of an object type.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, area};
use crate::models::{ListProperties, Property};

/// The fields of an object type, their types and their options.
///
/// A record comes back with only the properties that were asked for, so this
/// is how a program learns which there are to ask for.
#[derive(Debug, Clone, Copy)]
pub struct Properties<'a>(pub(crate) Api<'a>);

impl Properties<'_> {
    /// Lists every property of an object type. HubSpot returns them all at once.
    pub async fn list(&self, object_type: &str, options: ListProperties) -> Result<Vec<Property>> {
        let mut request = RawRequest::get(self.of(object_type)?);
        if let Some(archived) = options.archived {
            request = request.with_query("archived", archived.to_string());
        }
        let body = self.0.send(request).await?;
        self.0.results(&body, "properties")
    }

    /// Gets one property of an object type, by its internal name.
    pub async fn get(&self, object_type: &str, property: &str) -> Result<Property> {
        let name = self.0.segment("a property name", property)?;
        let request = RawRequest::get(format!("{}/{name}", self.of(object_type)?));
        let property: Property = self.0.decode(self.0.send(request).await?, "a property")?;
        if property.name.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without a property"));
        }
        Ok(property)
    }

    /// The address of an object type's properties.
    fn of(&self, object_type: &str) -> Result<String> {
        Ok(format!(
            "{}/{}",
            area("properties"),
            self.0.segment("an object type", object_type)?
        ))
    }
}
