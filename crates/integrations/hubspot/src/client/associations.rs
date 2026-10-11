//! Associations: which records are linked to which, and how.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{Association, AssociationCreated, CreateAssociation, Paging};

/// The associations between records.
#[derive(Debug, Clone, Copy)]
pub struct Associations<'a>(pub(crate) Api<'a>);

impl Associations<'_> {
    /// Lists the records of one object type that a record is associated
    /// with, at most 500 a page, each with the labels of the association.
    pub async fn list(
        &self,
        from_object_type: &str,
        from_id: &str,
        to_object_type: &str,
        paging: Paging,
    ) -> Result<Page<Association>> {
        let path = self.from(from_object_type, from_id, None, to_object_type)?;
        self.0
            .page(Some(500), RawRequest::get(path), &paging, "associations")
            .await
    }

    /// Associates two records. Without `types` it is the plain association
    /// HubSpot makes between the two object types; with them, the labels
    /// given replace those the association has.
    pub async fn create(
        &self,
        from_object_type: &str,
        from_id: &str,
        to_object_type: &str,
        to_id: &str,
        options: CreateAssociation,
    ) -> Result<AssociationCreated> {
        let to = self.0.segment("`to_id`", to_id)?;
        match options.types {
            None => {
                let path = self.from(from_object_type, from_id, Some("default"), to_object_type)?;
                let body = self.0.send(RawRequest::new("PUT", format!("{path}/{to}"))).await?;
                self.plain(&body, from_id, to_id)
            }
            Some(types) if types.is_empty() => Err(self.0.error(
                ErrorKind::InvalidInput,
                "`types` needs a label; leave it out for the plain association",
            )),
            Some(types) => {
                let path = self.from(from_object_type, from_id, None, to_object_type)?;
                let request = RawRequest::new("PUT", format!("{path}/{to}")).with_body(json!(types));
                let created: AssociationCreated = self.0.decode(self.0.send(request).await?, "an association")?;
                if created.from_object_id.is_empty() || created.to_object_id.is_empty() {
                    return Err(self.unconfirmed());
                }
                Ok(created)
            }
        }
    }

    /// Removes every association between two records, labelled or not. The
    /// records themselves stay.
    pub async fn remove(&self, from_object_type: &str, from_id: &str, to_object_type: &str, to_id: &str) -> Result<()> {
        let to = self.0.segment("`to_id`", to_id)?;
        let path = self.from(from_object_type, from_id, None, to_object_type)?;
        self.0.send(RawRequest::new("DELETE", format!("{path}/{to}"))).await?;
        Ok(())
    }

    /// The address of a record's associations with one object type:
    /// `crm/objects/2026-09/contacts/1/associations/companies`. `kind` is
    /// the word HubSpot puts before the object type for the plain association.
    fn from(&self, from_object_type: &str, from_id: &str, kind: Option<&str>, to_object_type: &str) -> Result<String> {
        let from_type = self.0.object_type("`from_object_type`", from_object_type)?;
        let from = self.0.segment("`from_id`", from_id)?;
        let to_type = self.0.object_type("`to_object_type`", to_object_type)?;
        let kind = kind.map(|kind| format!("{kind}/")).unwrap_or_default();
        Ok(format!(
            "crm/objects/{}/{from_type}/{from}/associations/{kind}{to_type}",
            crate::API_VERSION
        ))
    }

    /// Reads the answer to a plain association. HubSpot answers as it does
    /// for a batch, with one entry for each direction, and may report a
    /// failure inside an answer that succeeded. So the pair asked for has to
    /// be among the results.
    fn plain(&self, body: &Value, from_id: &str, to_id: &str) -> Result<AssociationCreated> {
        // Space around an id is never part of it: the request was made
        // without it, so the answer is compared without it too.
        let (from_id, to_id) = (from_id.trim(), to_id.trim());
        let id = |value: &Value| match value {
            Value::String(id) => id.trim().to_owned(),
            Value::Number(id) => id.to_string(),
            _ => String::new(),
        };
        let confirmed = body["results"].as_array().into_iter().flatten().any(|result| {
            let (from, to) = (id(&result["from"]["id"]), id(&result["to"]["id"]));
            (from == from_id && to == to_id) || (from == to_id && to == from_id)
        });
        if !confirmed {
            return Err(self.unconfirmed());
        }
        Ok(AssociationCreated {
            from_object_id: from_id.to_owned(),
            to_object_id: to_id.to_owned(),
            ..AssociationCreated::default()
        })
    }

    fn unconfirmed(&self) -> socketkit_core::Error {
        self.0
            .error(ErrorKind::Decode, "hubspot answered without the association")
    }
}
