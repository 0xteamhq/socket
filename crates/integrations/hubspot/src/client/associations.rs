//! Associations: the links between records.

use serde_json::{Value, json};
use socketkit_core::{Error, ErrorKind, Page, RawRequest, Result, provider_message};

use super::{Api, area};
use crate::models::{Associated, Association, CreateAssociation, DefaultAssociation, Paging};

/// The links between records: a contact and its company, a note and the deal
/// it is about.
#[derive(Debug, Clone, Copy)]
pub struct Associations<'a>(pub(crate) Api<'a>);

impl Associations<'_> {
    /// Lists the records of one type that a record is associated with, and
    /// the kinds of association between them.
    pub async fn list(
        &self,
        object_type: &str,
        record: &str,
        to_object_type: &str,
        paging: Paging,
    ) -> Result<Page<Association>> {
        let path = self.to(object_type, record, to_object_type)?;
        // HubSpot returns 500 a page when none is asked for, and names no larger page.
        self.0.page(500, RawRequest::get(path), &paging, "associations").await
    }

    /// Associates two records. Without `types`, HubSpot makes its default,
    /// unlabelled association between the two object types.
    ///
    /// Making a default association that is already there changes nothing.
    /// The kinds that are named become all the labels between the two
    /// records: HubSpot sets them, and a label that was there and is not
    /// named is removed. To add a label, name the ones to keep as well.
    /// They go to the address of the pair as a PUT. A
    /// default association goes to HubSpot's batch address as a POST with
    /// one pair in it: the address of the pair takes no body for a default,
    /// and the transport sends no length with a request that has none,
    /// which a server may refuse.
    pub async fn create(
        &self,
        object_type: &str,
        record: &str,
        to_object_type: &str,
        to_record: &str,
        association: CreateAssociation,
    ) -> Result<Associated> {
        let Some(types) = association.types.filter(|types| !types.is_empty()) else {
            let from = self.0.segment("an object type", object_type)?;
            let to = self.0.segment("the object type to associate", to_object_type)?;
            self.0.required("a record id", record)?;
            self.0.required("the id of the record to associate", to_record)?;
            // The space around an id is not part of it, here as in a path.
            let (record, to_record) = (record.trim(), to_record.trim());
            let path = format!("{}/{from}/{to}/batch/associate/default", area("associations"));
            let pair = json!({ "from": { "id": record }, "to": { "id": to_record } });
            let body = self.0.send(RawRequest::post(path, json!({ "inputs": [pair] }))).await?;
            return self.by_default(&body, record);
        };
        let to_record = self.0.segment("the id of the record to associate", to_record)?;
        if types.iter().any(|kind| kind.association_category.trim().is_empty()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "every one of `types` needs `associationCategory`",
            ));
        }
        let path = format!("{}/{to_record}", self.to(object_type, record, to_object_type)?);
        let body = self
            .0
            .send(RawRequest::new("PUT", path).with_body(json!(types)))
            .await?;
        let made: Associated = self.0.decode(body, "an association")?;
        if made.from_object_id.is_empty() || made.to_object_id.is_empty() {
            return Err(self.missing());
        }
        Ok(made)
    }

    /// Removes every association between two records. The records themselves stay.
    pub async fn remove(&self, object_type: &str, record: &str, to_object_type: &str, to_record: &str) -> Result<()> {
        let to_record = self.0.segment("the id of the associated record", to_record)?;
        let path = format!("{}/{to_record}", self.to(object_type, record, to_object_type)?);
        self.0.send(RawRequest::new("DELETE", path)).await.map(drop)
    }

    /// The address of a record's associations with one object type.
    fn to(&self, object_type: &str, record: &str, to_object_type: &str) -> Result<String> {
        Ok(format!(
            "{}/{}/associations/{}",
            self.0.records(object_type)?,
            self.0.segment("a record id", record)?,
            self.0.segment("the object type to associate", to_object_type)?
        ))
    }

    /// Reads HubSpot's answer to a default association. It lists the
    /// association once from each end; the one from `record` is the one that
    /// was asked for.
    ///
    /// A pair HubSpot could not associate is answered with success and an
    /// error beside it, which is reported as the refusal it is.
    fn by_default(&self, body: &Value, record: &str) -> Result<Associated> {
        if let Some(refusal) = body["errors"].as_array().and_then(|errors| errors.first()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                format!("hubspot did not make the association: {}", provider_message(refusal)),
            ));
        }
        let made: Vec<DefaultAssociation> = self.0.results(body, "an association")?;
        let mut from_here = made.into_iter().filter(|made| made.from.id == record).peekable();
        let Some(to_object_id) = from_here.peek().map(|made| made.to.id.clone()) else {
            return Err(self.missing());
        };
        Ok(Associated {
            from_object_id: record.to_owned(),
            to_object_id,
            labels: Vec::new(),
            types: from_here.filter_map(|made| made.association_spec).collect(),
        })
    }

    fn missing(&self) -> Error {
        self.0
            .error(ErrorKind::Decode, "hubspot answered without the association")
    }
}
