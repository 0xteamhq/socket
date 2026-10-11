//! A document as Google's API writes it: tabs, and in each tab a body of
//! structural elements, with the lists, objects and footnotes they refer to.
//!
//! Only what a document's text is read from is named here. Everything else
//! Google sends (styles, positions, sizes) is passed over, and so is an
//! element of a kind that is not named: it reads as an element with nothing
//! in it. Nothing here is returned to a caller; see `document_text.rs`.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde::de::IgnoredAny;

use super::DocumentTab;

/// Google's `Document`.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct DocumentResource {
    pub(super) document_id: String,
    pub(super) title: String,
    pub(super) revision_id: Option<String>,
    pub(super) tabs: Vec<Tab>,
    /// The first tab's content, where the tabs were not asked for. Google
    /// keeps these four from before a document had tabs.
    pub(super) body: Option<Body>,
    pub(super) lists: BTreeMap<String, List>,
    pub(super) inline_objects: BTreeMap<String, InlineObject>,
    pub(super) footnotes: BTreeMap<String, Body>,
}

impl DocumentResource {
    /// Every tab in the order a person sees them, a child after the tab it
    /// is inside, each with the titles that lead to it and its own.
    pub(super) fn every_tab(&self) -> Vec<(Vec<&str>, &Tab)> {
        fn walk<'a>(tabs: &'a [Tab], above: &mut Vec<&'a str>, found: &mut Vec<(Vec<&'a str>, &'a Tab)>) {
            for tab in tabs {
                above.push(&tab.tab_properties.title);
                found.push((above.clone(), tab));
                walk(&tab.child_tabs, above, found);
                above.pop();
            }
        }
        let mut found = Vec::new();
        walk(&self.tabs, &mut Vec::new(), &mut found);
        found
    }

    /// The content at the top of the document, when it came without tabs.
    pub(super) fn without_tabs(self) -> Option<TabContent> {
        Some(TabContent {
            body: self.body?,
            lists: self.lists,
            inline_objects: self.inline_objects,
            footnotes: self.footnotes,
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Tab {
    pub(super) tab_properties: DocumentTab,
    pub(super) child_tabs: Vec<Tab>,
    pub(super) document_tab: Option<TabContent>,
}

/// Google's `DocumentTab`: what is written in one tab.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct TabContent {
    pub(super) body: Body,
    pub(super) lists: BTreeMap<String, List>,
    pub(super) inline_objects: BTreeMap<String, InlineObject>,
    pub(super) footnotes: BTreeMap<String, Body>,
}

/// Blocks one after another: a tab's body, a table's cell, a table of
/// contents, a footnote.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct Body {
    pub(super) content: Vec<StructuralElement>,
}

/// One block of a body. Google sets one of these; a kind added later sets none.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct StructuralElement {
    pub(super) paragraph: Option<Paragraph>,
    pub(super) section_break: Option<IgnoredAny>,
    pub(super) table: Option<Table>,
    pub(super) table_of_contents: Option<Body>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Paragraph {
    pub(super) elements: Vec<ParagraphElement>,
    pub(super) paragraph_style: ParagraphStyle,
    pub(super) bullet: Option<Bullet>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct ParagraphStyle {
    /// `NORMAL_TEXT`, `TITLE`, `SUBTITLE`, or `HEADING_1` to `HEADING_6`.
    pub(super) named_style_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Bullet {
    pub(super) list_id: String,
    pub(super) nesting_level: i32,
}

/// One piece of a paragraph. Google sets one of these; a page break, a
/// column break, a page number and a kind added later set none.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct ParagraphElement {
    pub(super) text_run: Option<TextRun>,
    pub(super) footnote_reference: Option<FootnoteReference>,
    pub(super) horizontal_rule: Option<IgnoredAny>,
    pub(super) equation: Option<IgnoredAny>,
    pub(super) inline_object_element: Option<InlineObjectElement>,
    pub(super) person: Option<Person>,
    pub(super) rich_link: Option<RichLink>,
    pub(super) date_element: Option<DateElement>,
    pub(super) dropdown: Option<Dropdown>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct TextRun {
    pub(super) content: String,
    pub(super) text_style: TextStyle,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct TextStyle {
    pub(super) link: Option<Link>,
}

/// Where linked words lead. Only an address outside the document is kept;
/// a link to a heading, a bookmark or a tab has none.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct Link {
    pub(super) url: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct FootnoteReference {
    pub(super) footnote_id: String,
    /// The number a person sees.
    pub(super) footnote_number: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct InlineObjectElement {
    pub(super) inline_object_id: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Person {
    pub(super) person_properties: PersonProperties,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct PersonProperties {
    /// Present when the chip shows a name and not the address.
    pub(super) name: Option<String>,
    pub(super) email: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct RichLink {
    pub(super) rich_link_properties: RichLinkProperties,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub(super) struct RichLinkProperties {
    pub(super) title: String,
    pub(super) uri: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct DateElement {
    pub(super) date_element_properties: DateElementProperties,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct DateElementProperties {
    /// The date as the document shows it.
    pub(super) display_text: Option<String>,
    pub(super) timestamp: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Dropdown {
    pub(super) dropdown_properties: DropdownProperties,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct DropdownProperties {
    /// The words of the option that is chosen.
    pub(super) display_value: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct Table {
    pub(super) table_rows: Vec<TableRow>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct TableRow {
    /// Each cell is a body of its own, and may hold a table.
    pub(super) table_cells: Vec<Body>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct List {
    pub(super) list_properties: ListProperties,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct ListProperties {
    /// How the items look at each depth, the shallowest first.
    pub(super) nesting_levels: Vec<NestingLevel>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct NestingLevel {
    /// Set on a numbered list: `DECIMAL`, `ALPHA`, `ROMAN` and the like.
    pub(super) glyph_type: Option<String>,
    /// The number of the first item at this depth.
    pub(super) start_number: Option<i32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct InlineObject {
    pub(super) inline_object_properties: InlineObjectProperties,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct InlineObjectProperties {
    pub(super) embedded_object: EmbeddedObject,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(super) struct EmbeddedObject {
    pub(super) title: Option<String>,
    pub(super) description: Option<String>,
    /// Present when the object is a picture.
    pub(super) image_properties: Option<IgnoredAny>,
}
