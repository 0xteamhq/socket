//! Records of any object type: reading, creating, changing and archiving them one at a time.

use std::collections::BTreeMap;

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, with};
use crate::models::{CreateObject, GetObject, ListObjects, NewAssociation, Paging, Record, UpdateObject};

/// Records of any object type. The type is the first argument of every
/// method: `contacts`, `companies`, `deals`, `tickets`, `notes`, `calls`,
/// `meetings`, `emails`, `tasks`, or a custom object's type id such as `2-12345`.
#[derive(Debug, Clone, Copy)]
pub struct Objects<'a>(pub(crate) Api<'a>);

impl Objects<'_> {
    /// Lists the records of a type, with the properties that were asked for.
    pub async fn list(&self, object_type: &str, options: ListObjects) -> Result<Page<Record>> {
        let mut request = RawRequest::get(self.0.records(object_type)?);
        if let Some(properties) = self.0.names("properties", options.properties.as_deref())? {
            request = request.with_query("properties", properties);
        }
        if let Some(associations) = self.0.names("associations", options.associations.as_deref())? {
            request = request.with_query("associations", associations);
        }
        if let Some(archived) = options.archived {
            request = request.with_query("archived", archived.to_string());
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        // HubSpot's guide speaks of a number of records "under 100".
        self.0.page(100, request, &paging, "records").await
    }

    /// Gets one record, with the properties and the associations that were asked for.
    pub async fn get(&self, object_type: &str, record: &str, options: GetObject) -> Result<Record> {
        let mut request = RawRequest::get(self.one(object_type, record)?);
        if let Some(properties) = self.0.names("properties", options.properties.as_deref())? {
            request = request.with_query("properties", properties);
        }
        if let Some(associations) = self.0.names("associations", options.associations.as_deref())? {
            request = request.with_query("associations", associations);
        }
        if let Some(archived) = options.archived {
            request = request.with_query("archived", archived.to_string());
        }
        if let Some(property) = &options.id_property {
            self.0.required("`idProperty`", property)?;
            request = request.with_query("idProperty", property.as_str());
        }
        let body = self.0.send(request).await?;
        self.record(body)
    }

    /// Creates a record, and associates it with the records that are named.
    pub async fn create(&self, object_type: &str, object: CreateObject) -> Result<Record> {
        let records = self.0.records(object_type)?;
        self.properties(&object.properties)?;
        self.associations(object.associations.as_deref())?;
        let body = self.0.send(RawRequest::post(records, with(json!({}), &object))).await?;
        self.record(body)
    }

    /// Changes a record. Only the properties that are named are touched.
    pub async fn update(&self, object_type: &str, record: &str, changes: UpdateObject) -> Result<Record> {
        let mut request = RawRequest::new("PATCH", self.one(object_type, record)?);
        self.properties(&changes.properties)?;
        if changes.properties.is_empty() {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "an update needs at least one property to change",
            ));
        }
        if let Some(property) = &changes.id_property {
            self.0.required("`idProperty`", property)?;
            request = request.with_query("idProperty", property.as_str());
        }
        let content = json!({ "properties": changes.properties });
        let body = self.0.send(request.with_body(content)).await?;
        self.record(body)
    }

    /// Moves a record to the recycling bin, where HubSpot keeps it for 90 days.
    pub async fn archive(&self, object_type: &str, record: &str) -> Result<()> {
        let request = RawRequest::new("DELETE", self.one(object_type, record)?);
        self.0.send(request).await.map(drop)
    }

    /// The address of one record.
    fn one(&self, object_type: &str, record: &str) -> Result<String> {
        Ok(format!(
            "{}/{}",
            self.0.records(object_type)?,
            self.0.segment("a record id", record)?
        ))
    }

    /// Every property that is written has to have a name.
    pub(super) fn properties(&self, properties: &BTreeMap<String, String>) -> Result<()> {
        if properties.keys().any(|name| name.trim().is_empty()) {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`properties` holds a property with no name"));
        }
        Ok(())
    }

    /// Every association of a new record has to say which record and which kind.
    pub(super) fn associations(&self, associations: Option<&[NewAssociation]>) -> Result<()> {
        let unfinished =
            |association: &NewAssociation| association.to.id.trim().is_empty() || association.types.is_empty();
        if associations.unwrap_or_default().iter().any(unfinished) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "every association needs `to.id` and at least one of `types`",
            ));
        }
        Ok(())
    }

    pub(super) fn record(&self, body: Value) -> Result<Record> {
        let record: Record = self.0.decode(body, "a record")?;
        if record.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without a record"));
        }
        Ok(record)
    }
}
