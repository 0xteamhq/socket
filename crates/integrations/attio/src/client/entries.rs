//! List entries: reading, querying and writing them.

use serde_json::json;
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{Entry, EntryRow, Paging, QueryEntries, WriteEntry};

/// The entries of a list: reading, querying and writing them.
#[derive(Debug, Clone, Copy)]
pub struct Entries<'a>(pub(crate) Api<'a>);

impl Entries<'_> {
    /// Finds entries of a list by a filter, in a chosen order. Each row
    /// carries the entry's current values by themselves; `get` returns the
    /// values in full. Attio takes this as a POST, and it changes nothing.
    pub async fn query(&self, list: &str, query: QueryEntries) -> Result<Page<EntryRow>> {
        let path = format!("{}/query", self.entries(list)?);
        let search = self.0.search(query.filter, query.sorts.as_deref())?;
        let paging = Paging {
            cursor: query.cursor,
            limit: query.limit,
        };
        let page: Page<Entry> = self.0.query(path, search, &paging, "list entries").await?;
        let only = query.attributes.as_deref();
        Ok(Page {
            items: page.items.into_iter().map(|entry| EntryRow::of(entry, only)).collect(),
            next_cursor: page.next_cursor,
        })
    }

    /// Gets one entry, with every value of the list's attributes and each one's current value.
    pub async fn get(&self, list: &str, entry: &str) -> Result<Entry> {
        self.entry(RawRequest::get(self.path(list, entry)?)).await
    }

    /// Puts a record on a list, with values for the list's own attributes.
    /// A record may be on a list more than once, where the list allows it.
    pub async fn create(
        &self,
        list: &str,
        parent_object: &str,
        parent_record: &str,
        entry: WriteEntry,
    ) -> Result<Entry> {
        let path = self.entries(list)?;
        let body = json!({ "data": {
            "parent_object": self.0.id("a parent object", parent_object)?,
            "parent_record_id": self.0.id("a parent record id", parent_record)?,
            "entry_values": entry.entry_values,
        } });
        self.entry(RawRequest::post(path, body)).await
    }

    /// Changes an entry's values. An attribute that holds several values has
    /// the given ones added to it, and none taken away; any other attribute
    /// takes the given value in place of the one it had.
    pub async fn update(&self, list: &str, entry: &str, changes: WriteEntry) -> Result<Entry> {
        let path = self.path(list, entry)?;
        // A write that names no attribute would be sent, answered, and change nothing.
        if changes.entry_values.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`entry_values` names no attribute to write"));
        }
        let body = json!({ "data": { "entry_values": changes.entry_values } });
        self.entry(RawRequest::new("PATCH", path).with_body(body)).await
    }

    /// Takes an entry off its list. The record it was for stays.
    pub async fn delete(&self, list: &str, entry: &str) -> Result<()> {
        let path = self.path(list, entry)?;
        self.0.send(RawRequest::new("DELETE", path)).await?;
        Ok(())
    }

    fn entries(&self, list: &str) -> Result<String> {
        Ok(format!("lists/{}/entries", self.0.id("a list", list)?))
    }

    fn path(&self, list: &str, entry: &str) -> Result<String> {
        let entry = self.0.id("an entry id", entry)?;
        Ok(format!("{}/{entry}", self.entries(list)?))
    }

    /// Sends `request`, which reads or writes one entry, and returns the
    /// entry with each attribute's current value read out.
    async fn entry(&self, request: RawRequest) -> Result<Entry> {
        let entry: Entry = self.0.one(request, "a list entry").await?;
        if entry.id.entry_id.is_empty() {
            return Err(self.0.missing("a list entry"));
        }
        Ok(entry.with_current())
    }
}
