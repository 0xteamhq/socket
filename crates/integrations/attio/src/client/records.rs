//! The records of an object.

use serde_json::json;
use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{Paging, Query, Record, RecordEntry, WriteRecord};

/// The records of an object: one person, one company, one deal. `object` is
/// always the object's slug, such as `people`, or its id.
#[derive(Debug, Clone, Copy)]
pub struct Records<'a>(pub(crate) Api<'a>);

impl Records<'_> {
    /// Lists the records of an object that match a filter, in the order asked for.
    ///
    /// Attio takes this as a POST. It reads records and changes nothing.
    pub async fn query(&self, object: &str, query: Query) -> Result<Page<Record>> {
        let object = self.0.segment("an object", object)?;
        let (body, window) = self.0.query(&query)?;
        let request = RawRequest::post(format!("objects/{object}/records/query"), body);
        let records: Vec<Record> = self.0.all(request, "records").await?;
        Ok(window.page(records.into_iter().map(Record::summarised).collect()))
    }

    /// Gets one record, with every value it holds.
    pub async fn get(&self, object: &str, record: &str) -> Result<Record> {
        self.record(RawRequest::get(self.one(object, record)?)).await
    }

    /// Lists the lists a record is on, and its entry in each. The entries'
    /// own values are read with `entries.get`.
    pub async fn entries(&self, object: &str, record: &str, paging: Paging) -> Result<Page<RecordEntry>> {
        let record = self.one(object, record)?;
        // Attio returns 100 unless told otherwise, and at most 1000.
        let window = self.0.window(paging.cursor.as_deref(), paging.limit, 100, 1000)?;
        let request = RawRequest::get(format!("{record}/entries"))
            .with_query("limit", window.limit.to_string())
            .with_query("offset", window.offset.to_string());
        Ok(window.page(self.0.all(request, "entries").await?))
    }

    /// Creates a record. Attio refuses one whose unique attribute, such as a
    /// person's email address, is already held by another record.
    pub async fn create(&self, object: &str, record: WriteRecord) -> Result<Record> {
        let object = self.0.segment("an object", object)?;
        self.0.values(&record.values)?;
        let body = json!({ "data": { "values": record.values } });
        self.record(RawRequest::post(format!("objects/{object}/records"), body))
            .await
    }

    /// Changes a record. Only the attributes named are touched. For an
    /// attribute that holds several values, those given are added to what is
    /// there, and nothing is taken away.
    pub async fn update(&self, object: &str, record: &str, changes: WriteRecord) -> Result<Record> {
        let record = self.one(object, record)?;
        self.0.values(&changes.values)?;
        let body = json!({ "data": { "values": changes.values } });
        self.record(RawRequest::new("PATCH", record).with_body(body)).await
    }

    /// Creates a record, or changes the one that already holds the same
    /// value of `matching_attribute`.
    ///
    /// `matching_attribute` is the slug or id of a unique attribute, such as
    /// `email_addresses` for people or `domains` for companies, and `values`
    /// has to give it a value. For any other attribute that holds several
    /// values, the record ends with exactly those given: values that were
    /// there and are not given are removed.
    pub async fn assert(&self, object: &str, matching_attribute: &str, record: WriteRecord) -> Result<Record> {
        let object = self.0.segment("an object", object)?;
        self.0.required("a matching attribute", matching_attribute)?;
        self.0.values(&record.values)?;
        let body = json!({ "data": { "values": record.values } });
        let request = RawRequest::new("PUT", format!("objects/{object}/records"))
            .with_query("matching_attribute", matching_attribute.trim())
            .with_body(body);
        self.record(request).await
    }

    /// Deletes a record, with the entries it has on lists.
    pub async fn delete(&self, object: &str, record: &str) -> Result<()> {
        self.0.done(RawRequest::new("DELETE", self.one(object, record)?)).await
    }

    /// The address of one record.
    fn one(&self, object: &str, record: &str) -> Result<String> {
        let object = self.0.segment("an object", object)?;
        let record = self.0.segment("a record id", record)?;
        Ok(format!("objects/{object}/records/{record}"))
    }

    async fn record(&self, request: RawRequest) -> Result<Record> {
        let record: Record = self.0.one(request, "a record").await?;
        Ok(record.summarised())
    }
}
