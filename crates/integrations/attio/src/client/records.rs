//! Records: reading, querying and writing them.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{A_PAGE, Api};
use crate::models::{Paging, QueryRecords, Record, RecordEntry, RecordRow, WriteRecord};

/// The records of an object: reading, querying and writing them.
#[derive(Debug, Clone, Copy)]
pub struct Records<'a>(pub(crate) Api<'a>);

impl Records<'_> {
    /// Finds records of an object by a filter, in a chosen order. Each row
    /// carries the record's current values by themselves; `get` returns the
    /// values in full. Attio takes this as a POST, and it changes nothing.
    pub async fn query(&self, object: &str, query: QueryRecords) -> Result<Page<RecordRow>> {
        let path = format!("{}/query", self.records(object)?);
        let search = self.0.search(query.filter, query.sorts.as_deref())?;
        let paging = Paging {
            cursor: query.cursor,
            limit: query.limit,
        };
        let page: Page<Record> = self.0.query(path, search, &paging, "records").await?;
        let only = query.attributes.as_deref();
        Ok(Page {
            items: page
                .items
                .into_iter()
                .map(|record| RecordRow::of(record, only))
                .collect(),
            next_cursor: page.next_cursor,
        })
    }

    /// Gets one record, with every value it holds and each attribute's current value.
    pub async fn get(&self, object: &str, record: &str) -> Result<Record> {
        self.record(RawRequest::get(self.path(object, record)?)).await
    }

    /// Lists the lists a record is on, with its entry on each. At most 1000 a page.
    pub async fn entries(&self, object: &str, record: &str, paging: Paging) -> Result<Page<RecordEntry>> {
        let request = RawRequest::get(format!("{}/entries", self.path(object, record)?));
        self.0
            .page(request, &paging, (A_PAGE, 1000), "a record's entries")
            .await
    }

    /// Creates a record. Attio refuses one whose unique attribute, such as
    /// an email address, another record already holds; `assert` is for that.
    pub async fn create(&self, object: &str, record: WriteRecord) -> Result<Record> {
        let path = self.records(object)?;
        self.record(RawRequest::post(path, body(record.values))).await
    }

    /// Changes a record's values. An attribute that holds several values has
    /// the given ones added to it, and none taken away; any other attribute
    /// takes the given value in place of the one it had, which Attio keeps
    /// in the attribute's history.
    pub async fn update(&self, object: &str, record: &str, changes: WriteRecord) -> Result<Record> {
        let path = self.path(object, record)?;
        self.changing(&changes)?;
        self.record(RawRequest::new("PATCH", path).with_body(body(changes.values)))
            .await
    }

    /// Creates a record, or changes the one that already has the same value
    /// for `matching_attribute`, which has to be a unique attribute and be
    /// among the values given.
    ///
    /// Where a record is found, an attribute that holds several values ends
    /// up with exactly the ones given: any it had beside them are removed.
    /// Only the matching attribute itself is added to and never taken from.
    pub async fn assert(&self, object: &str, matching_attribute: &str, record: WriteRecord) -> Result<Record> {
        let path = self.records(object)?;
        let matching = self.0.id("a matching attribute", matching_attribute)?;
        self.changing(&record)?;
        let request = RawRequest::new("PUT", path)
            .with_query("matching_attribute", matching)
            .with_body(body(record.values));
        self.record(request).await
    }

    /// Deletes a record.
    pub async fn delete(&self, object: &str, record: &str) -> Result<()> {
        let path = self.path(object, record)?;
        self.0.send(RawRequest::new("DELETE", path)).await?;
        Ok(())
    }

    fn records(&self, object: &str) -> Result<String> {
        Ok(format!("objects/{}/records", self.0.id("an object", object)?))
    }

    fn path(&self, object: &str, record: &str) -> Result<String> {
        let record = self.0.id("a record id", record)?;
        Ok(format!("{}/{record}", self.records(object)?))
    }

    /// A write that names no attribute would be sent, answered, and change nothing.
    fn changing(&self, record: &WriteRecord) -> Result<()> {
        if record.values.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`values` names no attribute to write"));
        }
        Ok(())
    }

    /// Sends `request`, which reads or writes one record, and returns the
    /// record with each attribute's current value read out.
    async fn record(&self, request: RawRequest) -> Result<Record> {
        let record: Record = self.0.one(request, "a record").await?;
        if record.id.record_id.is_empty() {
            return Err(self.0.missing("a record"));
        }
        Ok(record.with_current())
    }
}

/// The body of a write. The values go as they were given: a `null` among
/// them is a value to write, not a field left unset.
fn body(values: Map<String, Value>) -> Value {
    json!({ "data": { "values": values } })
}
