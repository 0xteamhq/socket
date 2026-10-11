//! Pages: their properties, and creating, changing and trashing them.
//!
//! Reading a page's content whole is in `pages_read.rs`.

use serde_json::{Map, Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, patch};
use crate::models::{CreatePage, Page, Paging, PropertyItems, UpdatePage};

/// The most blocks Notion takes in one request.
const MOST_BLOCKS: usize = 100;

/// Pages: their properties and content, and creating, changing and trashing them.
#[derive(Debug, Clone, Copy)]
pub struct Pages<'a>(pub(crate) Api<'a>);

impl Pages<'_> {
    /// Gets a page: its properties and where it lives, without its content.
    pub async fn get(&self, page: &str) -> Result<Page> {
        let body = self.0.send(RawRequest::get(self.path(page)?)).await?;
        self.0.object(body, "page")
    }

    /// Reads one property of a page in full, a page of items at a time.
    ///
    /// `get` cuts a relation, a list of people, a title or a text short at
    /// 25 references. `property` is the property's id as the page gives it,
    /// or its name.
    pub async fn property(&self, page: &str, property: &str, paging: Paging) -> Result<PropertyItems> {
        let path = format!("{}/properties/{}", self.path(page)?, self.property_segment(property)?);
        let body = self.0.send(self.0.listing(path, &paging)?).await?;
        // A property with one value comes back as that value; one with many
        // comes back as a list, with what describes the property beside it.
        if body["object"] == "property_item" {
            let named = |field: &str| body[field].as_str().map(str::to_owned);
            return Ok(PropertyItems {
                id: named("id"),
                kind: named("type"),
                items: vec![self.0.decode(body, "a property")?],
                next_cursor: None,
            });
        }
        let named = |field: &str| body["property_item"][field].as_str().map(str::to_owned);
        let (id, kind) = (named("id"), named("type"));
        let items = self.0.list(body, "a property")?;
        Ok(PropertyItems {
            id,
            kind,
            items: items.items,
            next_cursor: items.next_cursor,
        })
    }

    /// Creates a page under a page, or as a row of a data source.
    pub async fn create(&self, page: CreatePage) -> Result<Page> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        let parent = &page.parent;
        let named = [&parent.page_id, &parent.data_source_id, &parent.database_id];
        if named.iter().all(|id| id.is_none()) && parent.workspace != Some(true) {
            return Err(invalid("a page needs a `parent`: a `page_id` or a `data_source_id`"));
        }
        if page.children.as_ref().is_some_and(|blocks| blocks.len() > MOST_BLOCKS) {
            return Err(invalid(
                "a page is created with at most 100 blocks; add the rest with `blocks.append`",
            ));
        }
        let mut body = Map::new();
        body.insert("parent".to_owned(), json!(parent));
        // What the caller wrote is sent as it stands, a `null` included:
        // that is how Notion is told to leave a property empty.
        body.extend(
            page.properties
                .map(|properties| ("properties".to_owned(), json!(properties))),
        );
        body.extend(page.children.map(|children| ("children".to_owned(), json!(children))));
        body.extend(page.icon.map(|icon| ("icon".to_owned(), icon)));
        body.extend(page.cover.map(|cover| ("cover".to_owned(), cover)));
        let created = self.0.send(RawRequest::post("pages", body.into())).await?;
        self.0.object(created, "page")
    }

    /// Changes a page's properties, icon or cover. Only what is set in
    /// `changes` is sent; the rest stays as it is.
    pub async fn update(&self, page: &str, changes: UpdatePage) -> Result<Page> {
        let mut body = Map::new();
        body.extend(
            changes
                .properties
                .map(|properties| ("properties".to_owned(), json!(properties))),
        );
        body.extend(changes.icon.map(|icon| ("icon".to_owned(), icon)));
        body.extend(changes.cover.map(|cover| ("cover".to_owned(), cover)));
        if body.is_empty() {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "nothing to change: give `properties`, `icon` or `cover`",
            ));
        }
        self.change(page, body.into()).await
    }

    /// Moves a page to the trash, with everything inside it. It can be
    /// restored from the trash in Notion for a time; the API cannot delete
    /// it for good.
    pub async fn archive(&self, page: &str) -> Result<Page> {
        self.change(page, json!({ "in_trash": true })).await
    }

    async fn change(&self, page: &str, body: Value) -> Result<Page> {
        let changed = self.0.send(patch(self.path(page)?, body)).await?;
        self.0.object(changed, "page")
    }

    fn path(&self, page: &str) -> Result<String> {
        Ok(format!("pages/{}", self.0.id("a page id", page)?))
    }

    /// Writes a property's id or name as one segment of a path.
    ///
    /// Notion gives a property's id already percent-encoded (`%3EfC`) and
    /// asks for it back as given, so an escape that is already there is
    /// kept. Everything else outside the characters a URL leaves alone is
    /// encoded, so a name can never add a segment, a query or a fragment.
    fn property_segment(&self, property: &str) -> Result<String> {
        if property.trim().is_empty() || property == "." || property == ".." {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "a property id or name is required"));
        }
        let bytes = property.as_bytes();
        let escape = |at: usize| {
            bytes
                .get(at + 1..at + 3)
                .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
        };
        let mut segment = String::with_capacity(property.len());
        for (at, &byte) in bytes.iter().enumerate() {
            let kept = byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~');
            if kept || (byte == b'%' && escape(at)) {
                segment.push(char::from(byte));
            } else {
                segment.push_str(&format!("%{byte:02X}"));
            }
        }
        Ok(segment)
    }
}
