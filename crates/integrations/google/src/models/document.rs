//! Google Docs: a document and its tabs, the document as plain text, and what creating one and adding to one take.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::document_structure::DocumentResource;

/// A document and its tabs, without what is written in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Document {
    /// The id in the document's address: `docs.google.com/document/d/{id}/edit`.
    pub document_id: String,
    pub title: String,
    /// Names the document as it stood when it was read: the same value on
    /// two reads means nothing changed between them. Google sends it only to
    /// an account that may edit the document, and it is good for a day, for
    /// that account alone.
    pub revision_id: Option<String>,
    /// Every tab in the order a person sees them, a child tab after the tab
    /// it is inside.
    pub tabs: Vec<DocumentTab>,
}

/// One tab of a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DocumentTab {
    /// What `tabId` takes where a tab is named, as in `append_text`.
    pub tab_id: String,
    pub title: String,
    /// The tab this one is inside. Absent for a tab at the top.
    pub parent_tab_id: Option<String>,
    /// Where the tab stands among those beside it, from 0.
    pub index: i32,
    /// How deep the tab is: 0 at the top, 1 inside a tab, and so on.
    pub nesting_level: i32,
    pub icon_emoji: Option<String>,
}

/// A document as plain text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DocumentText {
    pub document_id: String,
    pub title: String,
    pub revision_id: Option<String>,
    /// The whole document as one text, for a caller in Rust. With one tab
    /// it is that tab's text. With more, each tab's text follows a line that
    /// names it, `[tab: Plan > Budget]` for a tab called Budget inside one
    /// called Plan.
    ///
    /// It is not written in JSON, where it would only repeat what `tabs`
    /// holds: a named operation returns each tab's text once, and nothing
    /// written in a tab can pass for the line that names another.
    #[serde(skip_serializing)]
    #[schemars(skip)]
    pub text: String,
    /// Every tab with its own text, in the order a person sees them, a child
    /// tab after the tab it is inside.
    pub tabs: Vec<DocumentTabText>,
}

/// One tab of a document, and what is written in it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DocumentTabText {
    /// Which tab it is. A document read without its tabs has one tab here,
    /// with no id and no title.
    #[serde(flatten)]
    pub tab: DocumentTab,
    /// The tab as plain text: a heading is a `#` line, a list item keeps
    /// its marker and its depth, and a table has one line for each row.
    pub text: String,
}

/// A document to create. Google makes it blank: a title is all it takes.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocsCreateDocument {
    /// The document's title.
    pub title: String,
}

/// Text to add at the end of a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DocsAppendText {
    /// The text to add. It carries on the document's last paragraph: start
    /// it with a newline to begin a new one, and put a newline wherever
    /// another paragraph begins. A new paragraph looks like the one before
    /// it, so text added after a heading or a list item is one too.
    pub text: String,
    /// The tab to add to, as a tab's `tabId`. The first tab when not given.
    pub tab_id: Option<String>,
}

/// What Google reports after changing a document.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DocumentUpdate {
    pub document_id: String,
    pub write_control: Option<DocumentWriteControl>,
}

/// The state a change left a document in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DocumentWriteControl {
    /// The document's revision after the change.
    pub required_revision_id: Option<String>,
}

impl From<DocumentResource> for Document {
    fn from(document: DocumentResource) -> Self {
        let tabs = document
            .every_tab()
            .into_iter()
            .map(|(_, tab)| tab.tab_properties.clone())
            .collect();
        Self {
            document_id: document.document_id,
            title: document.title,
            revision_id: document.revision_id,
            tabs,
        }
    }
}
