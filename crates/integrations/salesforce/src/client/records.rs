//! Records of any object type: reading, creating, changing and deleting them.

use serde_json::Value;
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::Api;
use crate::models::{GetRecord, Record, RecordFields, Saved};

/// Records of any object type.
#[derive(Debug, Clone, Copy)]
pub struct Records<'a>(pub(crate) Api<'a>);

impl Records<'_> {
    /// Gets one record by its id, with the fields that are named.
    pub async fn get(&self, object: &str, id: &str, options: GetRecord) -> Result<Record> {
        let path = format!("sobjects/{}/{}", self.0.object(object)?, self.0.id(id)?);
        self.read(path, &options).await
    }

    /// Gets one record by the id another system knows it by: the value of a
    /// field the organisation marked as an external id.
    pub async fn get_by_external_id(
        &self,
        object: &str,
        field: &str,
        value: &str,
        options: GetRecord,
    ) -> Result<Record> {
        self.read(self.by_external_id(object, field, value)?, &options).await
    }

    /// Creates a record and returns its id.
    pub async fn create(&self, object: &str, record: RecordFields) -> Result<Saved> {
        let path = format!("sobjects/{}", self.0.object(object)?);
        let body = self
            .0
            .send(RawRequest::post(path, Value::Object(record.fields)))
            .await?;
        let Some(id) = saved_id(&body) else {
            return Err(self.0.error(
                ErrorKind::Decode,
                "salesforce did not confirm the new record with its id",
            ));
        };
        Ok(Saved { id, created: true })
    }

    /// Changes the fields that are named on one record, and leaves the rest
    /// as they are.
    pub async fn update(&self, object: &str, id: &str, changes: RecordFields) -> Result<()> {
        let path = format!("sobjects/{}/{}", self.0.object(object)?, self.0.id(id)?);
        if changes.fields.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`fields` names nothing to change"));
        }
        self.0
            .send(RawRequest::new("PATCH", path).with_body(Value::Object(changes.fields)))
            .await
            .map(drop)
    }

    /// Creates the record another system knows by `value`, or changes it
    /// when it exists: the record whose external id `field` holds `value`.
    ///
    /// The external id is given here and not among the fields. When more
    /// than one record holds the value, Salesforce refuses and writes nothing.
    pub async fn upsert(&self, object: &str, field: &str, value: &str, record: RecordFields) -> Result<Saved> {
        let path = self.by_external_id(object, field, value)?;
        let body = self
            .0
            .send(RawRequest::new("PATCH", path).with_body(Value::Object(record.fields)))
            .await?;
        match (saved_id(&body), body["created"].as_bool()) {
            (Some(id), Some(created)) => Ok(Saved { id, created }),
            // The record was written. Only the answer could not be read, so
            // the error says that and not that the write failed.
            _ => Err(self.0.error(
                ErrorKind::Decode,
                "salesforce accepted the record without saying which one it wrote; \
                 its answer is read as API version 46.0 and later write it",
            )),
        }
    }

    /// Deletes one record. It goes to the organisation's recycle bin, where
    /// Salesforce keeps it for a time, and records that depend on it may go with it.
    pub async fn delete(&self, object: &str, id: &str) -> Result<()> {
        let path = format!("sobjects/{}/{}", self.0.object(object)?, self.0.id(id)?);
        self.0.send(RawRequest::new("DELETE", path)).await.map(drop)
    }

    /// The path of the record whose external id `field` holds `value`.
    fn by_external_id(&self, object: &str, field: &str, value: &str) -> Result<String> {
        Ok(format!(
            "sobjects/{}/{}/{}",
            self.0.object(object)?,
            self.0.field(field)?,
            self.0.segment("an external id", value)?
        ))
    }

    async fn read(&self, path: String, options: &GetRecord) -> Result<Record> {
        let request = match self.0.field_list(options.fields.as_deref())? {
            Some(fields) => RawRequest::get(path).with_query("fields", fields),
            None => RawRequest::get(path),
        };
        self.0.record(self.0.send(request).await?)
    }
}

/// The id of the record a write names, unless Salesforce says it failed.
fn saved_id(body: &Value) -> Option<String> {
    let id = body["id"].as_str().filter(|id| !id.is_empty())?;
    (body["success"] != false).then(|| id.to_owned())
}
