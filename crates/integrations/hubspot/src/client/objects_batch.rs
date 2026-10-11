//! Records of any object type, many at a time.
//!
//! These are methods of [`Objects`], whose struct and single-record methods
//! are in `objects.rs`.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::objects::Objects;
use super::with;
use crate::models::{BatchCreate, BatchRead, BatchResult, BatchUpdate};

/// The most inputs HubSpot takes in one batch.
const MOST: usize = 100;

impl Objects<'_> {
    /// Reads up to 100 records at once, by id or by a property with unique
    /// values. Changes nothing.
    ///
    /// A record that does not exist is left out of `results` and reported
    /// under `errors`; the call itself succeeds.
    pub async fn batch_read(&self, object_type: &str, batch: BatchRead) -> Result<BatchResult> {
        let path = format!("{}/batch/read", self.0.records(object_type)?);
        self.sized("`ids`", batch.ids.len())?;
        if batch.ids.iter().any(|id| id.trim().is_empty()) {
            return Err(self.0.error(ErrorKind::InvalidInput, "`ids` holds a blank id"));
        }
        let inputs: Vec<Value> = batch.ids.iter().map(|id| json!({ "id": id })).collect();
        let mut content = json!({ "inputs": inputs });
        // An empty list of properties is no list, as it is for a single record.
        if let Some(properties) = batch.properties.filter(|properties| !properties.is_empty()) {
            self.0.names("properties", Some(&properties))?;
            content["properties"] = json!(properties);
        }
        if let Some(property) = &batch.id_property {
            self.0.required("`idProperty`", property)?;
            content["idProperty"] = json!(property);
        }
        let mut request = RawRequest::post(path, content);
        if let Some(archived) = batch.archived {
            request = request.with_query("archived", archived.to_string());
        }
        let body = self.0.send(request).await?;
        self.batch(body)
    }

    /// Creates up to 100 records at once.
    pub async fn batch_create(&self, object_type: &str, batch: BatchCreate) -> Result<BatchResult> {
        let path = format!("{}/batch/create", self.0.records(object_type)?);
        self.sized("`inputs`", batch.inputs.len())?;
        for input in &batch.inputs {
            self.properties(&input.properties)?;
            self.associations(input.associations.as_deref())?;
        }
        let body = self.0.send(RawRequest::post(path, with(json!({}), &batch))).await?;
        self.batch(body)
    }

    /// Changes up to 100 records at once. Only the properties that are named
    /// are touched.
    pub async fn batch_update(&self, object_type: &str, batch: BatchUpdate) -> Result<BatchResult> {
        let path = format!("{}/batch/update", self.0.records(object_type)?);
        self.sized("`inputs`", batch.inputs.len())?;
        for input in &batch.inputs {
            self.0.required("the `id` of every input", &input.id)?;
            self.properties(&input.properties)?;
            if input.properties.is_empty() {
                return Err(self.0.error(
                    ErrorKind::InvalidInput,
                    "every input needs at least one property to change",
                ));
            }
            if let Some(property) = &input.id_property {
                self.0.required("`idProperty`", property)?;
            }
        }
        let body = self.0.send(RawRequest::post(path, with(json!({}), &batch))).await?;
        self.batch(body)
    }

    /// A batch has something in it, and no more than HubSpot takes at once.
    fn sized(&self, what: &str, size: usize) -> Result<()> {
        let problem = match size {
            0 => format!("{what} needs at least one entry"),
            size if size > MOST => format!("{what} takes at most {MOST} entries: HubSpot reads no more at once"),
            _ => return Ok(()),
        };
        Err(self.0.error(ErrorKind::InvalidInput, problem))
    }

    fn batch(&self, body: Value) -> Result<BatchResult> {
        if !body["results"].is_array() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "hubspot answered without the batch's results"));
        }
        self.0.decode(body, "a batch")
    }
}
