//! What an organisation's objects are, and which fields each has.

use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::models::{Describe, ListSObjects, SObjectSummary};

/// The object types of an organisation, and what each is made of.
///
/// Every organisation defines objects and fields of its own, so this is how
/// a caller learns what there is to query and what a record may hold.
#[derive(Debug, Clone, Copy)]
pub struct SObjects<'a>(pub(crate) Api<'a>);

impl SObjects<'_> {
    /// Lists the object types the account can see, each in a few words.
    ///
    /// Salesforce returns all of them at once, hundreds in most
    /// organisations. `options` narrows what is passed on.
    pub async fn list(&self, options: ListSObjects) -> Result<Vec<SObjectSummary>> {
        let body = self.0.send(RawRequest::get("sobjects")).await?;
        let listed = body.get("sobjects").ok_or_else(|| {
            self.0
                .error(ErrorKind::Decode, "salesforce answered without object types")
        })?;
        let all: Vec<SObjectSummary> = self.0.decode(listed.clone(), "object types")?;
        let wanted = options.contains.as_deref().map(|text| text.trim().to_lowercase());
        let wanted = wanted.as_deref().filter(|text| !text.is_empty());
        Ok(all
            .into_iter()
            .filter(|object| options.custom.is_none_or(|custom| object.custom == custom))
            .filter(|object| {
                wanted.is_none_or(|text| {
                    object.name.to_lowercase().contains(text) || object.label.to_lowercase().contains(text)
                })
            })
            .collect())
    }

    /// Describes one object type: its fields with their types and picklist
    /// values, the object types that refer to it, and its record types.
    pub async fn describe(&self, object: &str) -> Result<Describe> {
        let object = self.0.object(object)?;
        let body = self
            .0
            .send(RawRequest::get(format!("sobjects/{object}/describe")))
            .await?;
        let described: Describe = self.0.decode(body, "an object type")?;
        if described.name.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "salesforce answered without an object type"));
        }
        Ok(described)
    }
}
