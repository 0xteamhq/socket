//! The entries of a list.

use serde_json::json;
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{CreateEntry, Entry, Query, WriteEntry};

/// The entries of a list: a record's place on it, with the values of the
/// list's own attributes. `list` is always the list's slug or its id.
#[derive(Debug, Clone, Copy)]
pub struct Entries<'a>(pub(crate) Api<'a>);

impl Entries<'_> {
    /// Lists the entries of a list that match a filter, in the order asked for.
    ///
    /// Attio takes this as a POST. It reads entries and changes nothing.
    pub async fn query(&self, list: &str, query: Query) -> Result<Page<Entry>> {
        let list = self.0.segment("a list", list)?;
        let (body, window) = self.0.query(&query)?;
        let request = RawRequest::post(format!("lists/{list}/entries/query"), body);
        let entries: Vec<Entry> = self.0.all(request, "entries").await?;
        Ok(window.page(entries.into_iter().map(Entry::summarised).collect()))
    }

    /// Gets one entry, with every value it holds.
    pub async fn get(&self, list: &str, entry: &str) -> Result<Entry> {
        self.entry(RawRequest::get(self.one(list, entry)?)).await
    }

    /// Puts a record on a list. A record may be on a list more than once,
    /// so doing this twice makes two entries.
    pub async fn create(&self, list: &str, entry: CreateEntry) -> Result<Entry> {
        let list = self.0.segment("a list", list)?;
        self.0.required("`parent_object`", &entry.parent_object)?;
        self.0.required("`parent_record_id`", &entry.parent_record_id)?;
        let body = json!({ "data": {
            "parent_object": entry.parent_object.trim(),
            "parent_record_id": entry.parent_record_id.trim(),
            // Attio asks for the values even when there are none.
            "entry_values": entry.entry_values.unwrap_or_default(),
        } });
        self.entry(RawRequest::post(format!("lists/{list}/entries"), body))
            .await
    }

    /// Changes an entry. Only the attributes named are touched. For an
    /// attribute that holds several values, those given are added to what is
    /// there, and nothing is taken away.
    pub async fn update(&self, list: &str, entry: &str, changes: WriteEntry) -> Result<Entry> {
        let entry = self.one(list, entry)?;
        if changes.entry_values.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`entry_values` needs at least one attribute"));
        }
        let body = json!({ "data": { "entry_values": changes.entry_values } });
        self.entry(RawRequest::new("PATCH", entry).with_body(body)).await
    }

    /// Takes a record off a list by deleting its entry. The record itself stays.
    pub async fn delete(&self, list: &str, entry: &str) -> Result<()> {
        self.0.done(RawRequest::new("DELETE", self.one(list, entry)?)).await
    }

    /// The address of one entry.
    fn one(&self, list: &str, entry: &str) -> Result<String> {
        let list = self.0.segment("a list", list)?;
        let entry = self.0.segment("an entry id", entry)?;
        Ok(format!("lists/{list}/entries/{entry}"))
    }

    async fn entry(&self, request: RawRequest) -> Result<Entry> {
        let entry: Entry = self.0.one(request, "an entry").await?;
        Ok(entry.summarised())
    }
}
