//! Records of any object type, several in one call.
//!
//! HubSpot takes at most 100 records in a batch. The rest of the group is
//! in `objects.rs`.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Objects, with_query};
use crate::models::{BatchCreate, BatchRead, BatchResult, BatchUpdate};

/// The most records HubSpot takes in one batch.
const LARGEST_BATCH: usize = 100;

impl Objects<'_> {
    /// Reads up to 100 records by their ids, or by the values of a unique
    /// property. Changes nothing; HubSpot takes it as a POST.
    ///
    /// The records that exist are returned. Those that do not are named in
    /// the result's `errors`, and the call still succeeds.
    pub async fn batch_read(&self, object_type: &str, batch: BatchRead) -> Result<BatchResult> {
        let path = format!("{}/batch/read", self.path(object_type)?);
        self.sized("`ids`", batch.ids.len())?;
        if batch.ids.iter().any(|id| id.trim().is_empty()) {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "every one of `ids` needs a value"));
        }
        let inputs: Vec<Value> = batch.ids.iter().map(|id| json!({ "id": id })).collect();
        // HubSpot's reference requires both lists of properties, so they are
        // sent when they are empty too.
        let mut body = json!({
            "inputs": inputs,
            "properties": batch.properties.unwrap_or_default(),
            "propertiesWithHistory": [],
        });
        if let Some(property) = self.named(batch.id_property.as_deref()) {
            body["idProperty"] = json!(property);
        }
        let request = RawRequest::post(path, body);
        let request = with_query(request, "archived", batch.archived.map(|archived| archived.to_string()));
        self.done(self.0.send(request).await?)
    }

    /// Creates up to 100 records in one call. HubSpot creates all of them or none.
    pub async fn batch_create(&self, object_type: &str, batch: BatchCreate) -> Result<BatchResult> {
        let path = format!("{}/batch/create", self.path(object_type)?);
        self.sized("`inputs`", batch.inputs.len())?;
        let inputs = batch
            .inputs
            .iter()
            .map(|record| self.new_record(record))
            .collect::<Result<Vec<Value>>>()?;
        self.done(self.0.send(RawRequest::post(path, json!({ "inputs": inputs }))).await?)
    }

    /// Changes up to 100 records in one call, each by the properties given for it.
    pub async fn batch_update(&self, object_type: &str, batch: BatchUpdate) -> Result<BatchResult> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let path = format!("{}/batch/update", self.path(object_type)?);
        self.sized("`inputs`", batch.inputs.len())?;
        if batch.inputs.iter().any(|change| change.id.trim().is_empty()) {
            return Err(invalid("every one of `inputs` needs an `id`"));
        }
        if batch.inputs.iter().any(|change| change.properties.is_empty()) {
            return Err(invalid("every one of `inputs` needs `properties` to change"));
        }
        let inputs: Vec<Value> = batch
            .inputs
            .iter()
            .map(|change| {
                let mut input = json!({ "id": change.id, "properties": change.properties });
                if let Some(property) = self.named(change.id_property.as_deref()) {
                    input["idProperty"] = json!(property);
                }
                input
            })
            .collect();
        self.done(self.0.send(RawRequest::post(path, json!({ "inputs": inputs }))).await?)
    }

    fn sized(&self, what: &str, count: usize) -> Result<()> {
        if (1..=LARGEST_BATCH).contains(&count) {
            return Ok(());
        }
        Err(self.0.error(
            ErrorKind::InvalidInput,
            format!("{what} takes from 1 to {LARGEST_BATCH} records"),
        ))
    }

    /// Reads what a batch did. An answer that does not say which records it
    /// covers is not a batch that was done.
    fn done(&self, body: Value) -> Result<BatchResult> {
        if !body["results"].is_array() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "hubspot answered without the records of the batch"));
        }
        self.0.decode(body, "a batch of records")
    }
}
