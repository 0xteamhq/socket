//! Pages.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::nullable::given;
use super::rich_text::{plain_text, runs};
use super::{Parent, PropertyValue, User};

/// A page: its properties and where it lives. Its content is a tree of
/// blocks, read with `pages.read` or `blocks.children`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Page {
    pub id: String,
    pub created_time: Option<String>,
    pub last_edited_time: Option<String>,
    pub created_by: Option<User>,
    pub last_edited_by: Option<User>,
    /// Whether the page is in the trash.
    pub in_trash: bool,
    pub is_locked: Option<bool>,
    /// An emoji, a file or an icon, as Notion writes it.
    pub icon: Option<Value>,
    pub cover: Option<Value>,
    pub parent: Option<Parent>,
    /// The page's properties by name. A page that is not a row of a data
    /// source has only its title. A relation, a list of people, a title or a
    /// text is cut short here at 25 references; `pages.property` reads it in full.
    pub properties: BTreeMap<String, PropertyValue>,
    /// The address that opens the page in Notion.
    pub url: Option<String>,
    pub public_url: Option<String>,
}

impl Page {
    /// The page's title, read from the one property of the `title` kind.
    pub fn title(&self) -> Option<String> {
        let title = self
            .properties
            .values()
            .find(|property| property.kind.as_deref() == Some("title"))?;
        let said = plain_text(&runs(title.value.get("title")?));
        Some(said).filter(|said| !said.is_empty())
    }
}

/// A page to create.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CreatePage {
    /// Where the page goes: under a page (`page_id`), or as a row of a data
    /// source (`data_source_id`).
    pub parent: Parent,
    /// Property values by name or id, as Notion writes them. Under a page
    /// only `title` is taken; under a data source they follow its schema.
    pub properties: Option<BTreeMap<String, Value>>,
    /// The page's content: at most 100 blocks, as Notion writes them.
    pub children: Option<Vec<Value>>,
    pub icon: Option<Value>,
    pub cover: Option<Value>,
}

/// What to change on a page. Fields left out stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdatePage {
    /// Property values by name or id, as Notion writes them. A property left
    /// out is not changed; a list that is given replaces the whole list; a
    /// value given as `null` is cleared.
    pub properties: Option<BTreeMap<String, Value>>,
    /// A new icon, as Notion writes it. `null` asks Notion to remove it.
    #[serde(default, deserialize_with = "given")]
    pub icon: Option<Value>,
    /// A new cover, as Notion writes it. `null` asks Notion to remove it.
    #[serde(default, deserialize_with = "given")]
    pub cover: Option<Value>,
}
