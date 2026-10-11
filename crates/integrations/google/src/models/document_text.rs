//! A document as plain text: what a person reads, in the order they read it.
//!
//! Google sends a document as a tree: tabs, in each a body of paragraphs,
//! tables and breaks, and in each paragraph runs of text and the things that
//! sit among them. This walks that tree once. It never fails: what it does
//! not know it leaves out, because a kind of element Google adds later must
//! not stop a document from being read.
//!
//! What it writes:
//!
//! - A paragraph is a line. A line break inside one stays a line break.
//! - A heading is a Markdown heading: `#` for a title or a first heading,
//!   `##` to `######` for the five below. A subtitle is an ordinary line.
//! - A list item keeps its marker, `-` or its number, behind two spaces for
//!   each list it is inside.
//! - A table has one line for each row, its cells between `|`.
//! - Linked words are `[words](address)`, and so is a smart chip for a file
//!   or a page. A person's chip is the name, a date's is the date as shown.
//! - A picture is `[image: its description]`, a footnote is `[^1]` where it
//!   is referred to and `[^1]: …` after the text, a rule across the page is
//!   `---`.
//! - A page break, a column break and a section break hold no words. The
//!   paragraph around one stays a line, and a section break leaves an empty
//!   line.
//! - Headers, footers and page numbers are not part of the text.

use std::collections::HashMap;

use super::document_structure::{
    Bullet, DocumentResource, Paragraph, ParagraphElement, StructuralElement, TabContent, TableRow,
};
use super::{DocumentTab, DocumentTabText, DocumentText};

/// The deepest a list item is written. Google's lists go nine deep.
const DEEPEST: usize = 8;

impl From<DocumentResource> for DocumentText {
    fn from(document: DocumentResource) -> Self {
        let mut tabs: Vec<(String, DocumentTabText)> = document
            .every_tab()
            .into_iter()
            .map(|(titles, tab)| {
                let read = DocumentTabText {
                    tab: tab.tab_properties.clone(),
                    text: tab.document_tab.as_ref().map(text).unwrap_or_default(),
                };
                (titles.join(" > "), read)
            })
            .collect();
        let (document_id, title, revision_id) = (
            document.document_id.clone(),
            document.title.clone(),
            document.revision_id.clone(),
        );
        // A document that came without its tabs has the first tab's content
        // at its top, and nothing that says which tab it was.
        if tabs.is_empty() {
            if let Some(content) = document.without_tabs() {
                let read = DocumentTabText {
                    tab: DocumentTab::default(),
                    text: text(&content),
                };
                tabs.push((String::new(), read));
            }
        }
        let whole = match tabs.as_slice() {
            [(_, only)] => only.text.clone(),
            several => several
                .iter()
                .map(|(titles, tab)| match tab.text.as_str() {
                    "" => format!("[tab: {titles}]"),
                    text => format!("[tab: {titles}]\n\n{text}"),
                })
                .collect::<Vec<_>>()
                .join("\n\n"),
        };
        Self {
            document_id,
            title,
            revision_id,
            text: whole,
            tabs: tabs.into_iter().map(|(_, tab)| tab).collect(),
        }
    }
}

/// One tab as plain text.
fn text(tab: &TabContent) -> String {
    let mut reader = Reader {
        tab,
        counted: HashMap::new(),
        notes: Vec::new(),
    };
    let mut lines = reader.blocks(&tab.body.content);
    // The footnotes that were referred to, after the text, in that order.
    let notes = std::mem::take(&mut reader.notes);
    lines.push(String::new());
    for (number, id) in notes {
        if let Some(note) = tab.footnotes.get(id) {
            lines.push(format!("[^{number}]: {}", on_one_line(&reader.blocks(&note.content))));
        }
    }
    let blank = |line: &String| line.trim().is_empty();
    let first = lines.iter().position(|line| !blank(line)).unwrap_or(lines.len());
    let last = lines.iter().rposition(|line| !blank(line)).map_or(first, |at| at + 1);
    lines[first..last].join("\n")
}

/// Reads one tab.
struct Reader<'a> {
    tab: &'a TabContent,
    /// How many items each list has had so far at each depth. A numbered
    /// list counts on where it left off, as Google shows it.
    counted: HashMap<&'a str, Vec<i64>>,
    /// The footnotes referred to so far: the number shown, and the id.
    notes: Vec<(String, &'a str)>,
}

impl<'a> Reader<'a> {
    /// The lines of some blocks. A paragraph with a line break in it is one
    /// entry that holds two lines.
    fn blocks(&mut self, elements: &'a [StructuralElement]) -> Vec<String> {
        let mut lines = Vec::new();
        for (at, element) in elements.iter().enumerate() {
            if let Some(paragraph) = &element.paragraph {
                lines.push(self.paragraph(paragraph));
            } else if let Some(table) = &element.table {
                for row in &table.table_rows {
                    lines.push(self.row(row));
                }
            } else if let Some(contents) = &element.table_of_contents {
                lines.extend(self.blocks(&contents.content));
            } else if element.section_break.is_some() && at > 0 {
                // Every body opens with a section break, which parts nothing.
                lines.push(String::new());
            }
            // Anything else is a kind of block this does not know.
        }
        lines
    }

    fn paragraph(&mut self, paragraph: &'a Paragraph) -> String {
        let words = self.words(&paragraph.elements);
        // The newline that ends the paragraph is the last thing in it. A
        // line break inside the paragraph is a vertical tab.
        let words = words.strip_suffix('\n').unwrap_or(&words).replace('\u{b}', "\n");
        if let Some(bullet) = &paragraph.bullet {
            let marker = self.marker(bullet);
            let under = format!("\n{}", " ".repeat(marker.len()));
            return format!("{marker}{}", words.replace('\n', &under));
        }
        let level = match paragraph.paragraph_style.named_style_type.as_deref() {
            Some("TITLE" | "HEADING_1") => 1,
            Some("HEADING_2") => 2,
            Some("HEADING_3") => 3,
            Some("HEADING_4") => 4,
            Some("HEADING_5") => 5,
            Some("HEADING_6") => 6,
            _ => 0,
        };
        if level == 0 || words.trim().is_empty() {
            return words;
        }
        // A heading is one line, whatever breaks it was written with.
        format!("{} {}", "#".repeat(level), words.replace('\n', " "))
    }

    /// What an item of a list begins with: its depth, then `-` or its number.
    fn marker(&mut self, bullet: &'a Bullet) -> String {
        let depth = usize::try_from(bullet.nesting_level).unwrap_or(0).min(DEEPEST);
        let level = self
            .tab
            .lists
            .get(&bullet.list_id)
            .and_then(|list| list.list_properties.nesting_levels.get(depth));
        let counted = self.counted.entry(bullet.list_id.as_str()).or_default();
        // An item ends the count of everything deeper than it.
        counted.resize(depth + 1, 0);
        counted[depth] += 1;
        let indent = "  ".repeat(depth);
        let numbered = level
            .and_then(|level| level.glyph_type.as_deref())
            .is_some_and(|glyph| {
                matches!(
                    glyph,
                    "DECIMAL" | "ZERO_DECIMAL" | "UPPER_ALPHA" | "ALPHA" | "UPPER_ROMAN" | "ROMAN"
                )
            });
        if !numbered {
            return format!("{indent}- ");
        }
        // Letters and Roman numerals are written as numbers too.
        let first = level.and_then(|level| level.start_number).map_or(1, i64::from);
        format!("{indent}{}. ", first + counted[depth] - 1)
    }

    /// One row of a table: its cells between `|`, each on one line.
    fn row(&mut self, row: &'a TableRow) -> String {
        let cells: Vec<String> = row
            .table_cells
            .iter()
            // A backslash is escaped before the bar is, so that one written
            // before a bar in the cell itself cannot undo the bar's escape.
            .map(|cell| {
                on_one_line(&self.blocks(&cell.content))
                    .replace('\\', "\\\\")
                    .replace('|', "\\|")
            })
            .collect();
        format!("| {} |", cells.join(" | "))
    }

    /// The words of a paragraph, with what sits among them as words too.
    fn words(&mut self, elements: &'a [ParagraphElement]) -> String {
        // Each piece of text with the address it leads to. Google splits
        // linked words wherever their look changes, so pieces that follow
        // one another to the same address are one link.
        let mut pieces: Vec<(String, Option<&'a str>)> = Vec::new();
        for element in elements {
            let (words, link) = self.piece(element);
            match pieces.last_mut() {
                Some((before, address)) if link.is_some() && *address == link => before.push_str(&words),
                _ => pieces.push((words, link)),
            }
        }
        pieces
            .into_iter()
            .map(|(words, link)| match link {
                Some(address) => linked(&words, address),
                None => words,
            })
            .collect()
    }

    /// What one piece of a paragraph reads as, and the address it leads to.
    fn piece(&mut self, element: &'a ParagraphElement) -> (String, Option<&'a str>) {
        let address = |address: &'a str| Some(address).filter(|address| !address.trim().is_empty());
        if let Some(run) = &element.text_run {
            let link = run.text_style.link.as_ref().and_then(|link| link.url.as_deref());
            return (run.content.clone(), link.and_then(address));
        }
        if let Some(chip) = &element.rich_link {
            let link = &chip.rich_link_properties;
            let title = if link.title.trim().is_empty() {
                &link.uri
            } else {
                &link.title
            };
            return (title.clone(), address(link.uri.as_str()));
        }
        let shown = |words: &Option<String>| words.clone().filter(|words| !words.trim().is_empty());
        let words = if let Some(person) = &element.person {
            let person = &person.person_properties;
            shown(&person.name).or_else(|| shown(&person.email))
        } else if let Some(date) = &element.date_element {
            let date = &date.date_element_properties;
            shown(&date.display_text).or_else(|| shown(&date.timestamp))
        } else if let Some(dropdown) = &element.dropdown {
            shown(&dropdown.dropdown_properties.display_value)
        } else if let Some(object) = &element.inline_object_element {
            Some(self.object(&object.inline_object_id))
        } else if let Some(note) = &element.footnote_reference {
            let number = match note.footnote_number.trim() {
                "" => (self.notes.len() + 1).to_string(),
                number => number.to_owned(),
            };
            self.notes.push((number.clone(), note.footnote_id.as_str()));
            Some(format!("[^{number}]"))
        } else if element.horizontal_rule.is_some() {
            Some("---".to_owned())
        } else if element.equation.is_some() {
            Some("[equation]".to_owned())
        } else {
            // A break, a page number, or a kind of piece this does not know.
            None
        };
        (words.unwrap_or_default(), None)
    }

    /// A picture, a drawing or a chart set among the words, by what it is
    /// described as. The picture itself is not text.
    fn object(&self, id: &str) -> String {
        let object = self
            .tab
            .inline_objects
            .get(id)
            .map(|object| &object.inline_object_properties.embedded_object);
        let kind = match object {
            Some(object) if object.image_properties.is_some() => "image",
            _ => "object",
        };
        // Its title, or its description when it has no title to speak of.
        fn said(words: &Option<String>) -> Option<&str> {
            words.as_deref().map(str::trim).filter(|words| !words.is_empty())
        }
        let described = object.and_then(|object| said(&object.title).or_else(|| said(&object.description)));
        match described {
            Some(described) => format!("[{kind}: {described}]"),
            None => format!("[{kind}]"),
        }
    }
}

/// Words that lead to `address`, as `[words](address)`. The space around
/// them, and the newline that ends a paragraph, stay outside. Words that are
/// the address itself are written once.
fn linked(words: &str, address: &str) -> String {
    let shown = words.trim();
    if shown.is_empty() || shown == address {
        return words.to_owned();
    }
    let start = words.len() - words.trim_start().len();
    let end = start + shown.len();
    // Words and addresses are the writer's own. Neither may end the link
    // early, or the words would seem to lead somewhere they do not.
    let label = shown.replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]");
    let target: String = address
        .chars()
        .map(|c| match c {
            '(' => "%28".to_owned(),
            ')' => "%29".to_owned(),
            c if c.is_whitespace() || c.is_control() => "%20".to_owned(),
            c => c.to_string(),
        })
        .collect();
    format!("{}[{label}]({target}){}", &words[..start], &words[end..])
}

/// Lines as one line, for a cell of a table or a footnote.
fn on_one_line(lines: &[String]) -> String {
    lines
        .iter()
        .flat_map(|line| line.lines())
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    /// A paragraph of plain text, as Google writes one.
    fn paragraph(words: &str) -> Value {
        styled(words, "NORMAL_TEXT")
    }

    fn styled(words: &str, style: &str) -> Value {
        json!({
            "startIndex": 1, "endIndex": 2,
            "paragraph": {
                "elements": [{ "startIndex": 1, "endIndex": 2, "textRun": { "content": format!("{words}\n"), "textStyle": {} } }],
                "paragraphStyle": { "namedStyleType": style, "direction": "LEFT_TO_RIGHT" }
            }
        })
    }

    /// An item of a list.
    fn item(words: &str, list: &str, depth: Option<i32>) -> Value {
        let mut item = paragraph(words);
        item["paragraph"]["bullet"] = json!({ "listId": list, "textStyle": {} });
        if let Some(depth) = depth {
            item["paragraph"]["bullet"]["nestingLevel"] = json!(depth);
        }
        item
    }

    /// A paragraph made of these pieces, closed by the newline Google ends one with.
    fn pieces(mut elements: Vec<Value>) -> Value {
        elements.push(json!({ "textRun": { "content": "\n", "textStyle": {} } }));
        json!({ "paragraph": { "elements": elements, "paragraphStyle": { "namedStyleType": "NORMAL_TEXT" } } })
    }

    fn run(words: &str) -> Value {
        json!({ "textRun": { "content": words, "textStyle": {} } })
    }

    fn link(words: &str, address: &str) -> Value {
        json!({ "textRun": { "content": words, "textStyle": { "link": { "url": address }, "underline": true } } })
    }

    fn table(rows: Vec<Vec<Vec<Value>>>) -> Value {
        let rows: Vec<Value> = rows
            .into_iter()
            .map(|cells| {
                let cells: Vec<Value> = cells
                    .into_iter()
                    .map(|content| json!({ "startIndex": 1, "endIndex": 2, "content": content, "tableCellStyle": { "rowSpan": 1, "columnSpan": 1 } }))
                    .collect();
                json!({ "startIndex": 1, "endIndex": 2, "tableCells": cells, "tableRowStyle": {} })
            })
            .collect();
        json!({ "startIndex": 1, "endIndex": 2, "table": { "rows": rows.len(), "columns": 2, "tableRows": rows, "tableStyle": {} } })
    }

    /// The body of a tab: Google opens every one with a section break.
    fn body(mut content: Vec<Value>) -> Value {
        content.insert(
            0,
            json!({ "endIndex": 1, "sectionBreak": { "sectionStyle": { "sectionType": "CONTINUOUS" } } }),
        );
        json!({ "content": content })
    }

    fn tab(id: &str, title: &str, content: Value, children: Vec<Value>) -> Value {
        let mut tab = json!({
            "tabProperties": { "tabId": id, "title": title, "index": 0 },
            "documentTab": content
        });
        if !children.is_empty() {
            tab["childTabs"] = json!(children);
        }
        tab
    }

    /// A document of one tab with this in it.
    fn document(content: Value) -> Value {
        json!({
            "documentId": "doc-1", "title": "Plan", "revisionId": "rev-7",
            "suggestionsViewMode": "PREVIEW_WITHOUT_SUGGESTIONS",
            "tabs": [tab("t.0", "Tab 1", content, vec![])]
        })
    }

    fn read(document: Value) -> DocumentText {
        DocumentText::from(serde_json::from_value::<DocumentResource>(document).unwrap())
    }

    /// The text of a document of one tab whose body holds these blocks.
    fn text_of(content: Vec<Value>) -> String {
        read(document(json!({ "body": body(content) }))).text
    }

    #[test]
    fn a_document_with_nothing_written_in_it_reads_as_no_text() {
        // A blank document: the section break, and one paragraph that is only its own end.
        let blank = read(document(json!({ "body": body(vec![paragraph("")]) })));
        assert_eq!(blank.text, "");
        assert_eq!((blank.document_id.as_str(), blank.title.as_str()), ("doc-1", "Plan"));
        assert_eq!(blank.revision_id.as_deref(), Some("rev-7"));
        assert_eq!(blank.tabs.len(), 1);
        assert_eq!(
            (blank.tabs[0].tab.tab_id.as_str(), blank.tabs[0].text.as_str()),
            ("t.0", "")
        );

        // Empty paragraphs around the words are not part of the text; one between them is.
        assert_eq!(
            text_of(vec![
                paragraph(""),
                paragraph("One"),
                paragraph(""),
                paragraph("Two"),
                paragraph(" ")
            ]),
            "One\n\nTwo"
        );

        // A tab with no body, a body with no content, and a document with nothing at all.
        for sparse in [
            document(json!({})),
            document(json!({ "body": {} })),
            json!({ "documentId": "doc-1", "tabs": [{ "tabProperties": { "tabId": "t.0" } }] }),
        ] {
            let read = read(sparse);
            assert_eq!((read.text.as_str(), read.tabs.len()), ("", 1));
        }
        let nothing = read(json!({ "documentId": "doc-1", "title": "Plan" }));
        assert_eq!((nothing.text.as_str(), nothing.tabs.len()), ("", 0));
    }

    #[test]
    fn a_document_that_came_without_its_tabs_is_read_from_its_body() {
        // What Google sends when `includeTabsContent` is not asked for: the
        // first tab's content at the top, with its lists beside it.
        let legacy = read(json!({
            "documentId": "doc-1", "title": "Plan", "revisionId": "rev-7",
            "body": body(vec![styled("Plan", "TITLE"), item("First", "kix.list1", None), paragraph("Done.")]),
            "lists": { "kix.list1": { "listProperties": { "nestingLevels": [{ "glyphType": "DECIMAL", "startNumber": 1 }] } } },
            "documentStyle": { "pageSize": { "height": { "magnitude": 792, "unit": "PT" } } }
        }));
        assert_eq!(legacy.text, "# Plan\n1. First\nDone.");
        assert_eq!(legacy.tabs.len(), 1, "the body is the one tab there is");
        assert_eq!(legacy.tabs[0].tab, DocumentTab::default(), "and nothing says which");
        assert_eq!(legacy.tabs[0].text, legacy.text);

        // With tabs, the body at the top is not read a second time.
        let mut both = document(json!({ "body": body(vec![paragraph("From the tab")]) }));
        both["body"] = body(vec![paragraph("From the top")]);
        let both = read(both);
        assert_eq!(both.text, "From the tab");
        assert_eq!(both.tabs.len(), 1);
    }

    #[test]
    fn every_tab_is_returned_with_its_text_and_child_tabs_follow_their_parent() {
        let of = |words: &str| json!({ "body": body(vec![paragraph(words)]) });
        let mut budget = tab(
            "t.2",
            "Budget",
            of("Costs"),
            vec![tab("t.3", "2026", of("Next year"), vec![])],
        );
        budget["tabProperties"]["parentTabId"] = json!("t.1");
        budget["tabProperties"]["nestingLevel"] = json!(1);
        budget["childTabs"][0]["tabProperties"]["parentTabId"] = json!("t.2");
        budget["childTabs"][0]["tabProperties"]["nestingLevel"] = json!(2);
        let mut notes = tab("t.4", "Notes", json!({ "body": body(vec![paragraph("")]) }), vec![]);
        notes["tabProperties"]["index"] = json!(1);
        let read = read(json!({
            "documentId": "doc-1", "title": "Plan",
            "tabs": [tab("t.1", "Plan", of("Overview"), vec![budget]), notes]
        }));

        let tabs: Vec<_> = read
            .tabs
            .iter()
            .map(|tab| {
                (
                    tab.tab.tab_id.as_str(),
                    tab.tab.title.as_str(),
                    tab.tab.nesting_level,
                    tab.tab.parent_tab_id.as_deref(),
                    tab.text.as_str(),
                )
            })
            .collect();
        assert_eq!(
            tabs,
            [
                ("t.1", "Plan", 0, None, "Overview"),
                ("t.2", "Budget", 1, Some("t.1"), "Costs"),
                ("t.3", "2026", 2, Some("t.2"), "Next year"),
                ("t.4", "Notes", 0, None, ""),
            ]
        );
        assert_eq!(read.tabs[3].tab.index, 1);
        assert_eq!(
            read.text,
            "[tab: Plan]\n\nOverview\n\n[tab: Plan > Budget]\n\nCosts\n\n[tab: Plan > Budget > 2026]\n\nNext year\n\n[tab: Notes]",
            "each tab under a line that names it, an empty one included"
        );

        // Written out, a tab's own fields sit beside its text.
        let written = serde_json::to_value(&read.tabs[1]).unwrap();
        assert_eq!(written["tabId"], "t.2");
        assert_eq!(written["parentTabId"], "t.1");
        assert_eq!(written["text"], "Costs");
    }

    #[test]
    fn a_heading_is_a_markdown_heading_and_a_subtitle_is_a_line() {
        assert_eq!(
            text_of(vec![
                styled("Plan for Q3", "TITLE"),
                styled("What we will build", "SUBTITLE"),
                styled("Goals", "HEADING_1"),
                styled("Reach", "HEADING_2"),
                styled("Europe", "HEADING_3"),
                styled("France", "HEADING_4"),
                styled("Paris", "HEADING_5"),
                styled("Left bank", "HEADING_6"),
                paragraph("Open a shop."),
                // A heading with nothing in it is an empty line, not a lone `#`.
                styled("", "HEADING_2"),
                // A style Google adds later is an ordinary line.
                styled("Aside", "HEADING_7"),
                // A heading broken over two lines is still one heading.
                styled("Risks\u{b}and costs", "HEADING_2"),
            ]),
            "# Plan for Q3\nWhat we will build\n# Goals\n## Reach\n### Europe\n#### France\n##### Paris\n###### Left bank\n\
             Open a shop.\n\nAside\n## Risks and costs"
        );
        // A paragraph with no style at all, and a line break inside a paragraph.
        assert_eq!(
            text_of(vec![
                json!({ "paragraph": { "elements": [run("12 Rue de Rivoli\u{b}75001 Paris\n")] } })
            ]),
            "12 Rue de Rivoli\n75001 Paris"
        );
    }

    #[test]
    fn a_nested_list_keeps_its_markers_its_numbers_and_its_depth() {
        let lists = json!({
            "kix.steps": { "listProperties": { "nestingLevels": [
                { "glyphType": "DECIMAL", "glyphFormat": "%0.", "startNumber": 1 },
                { "glyphType": "ALPHA", "glyphFormat": "%1.", "startNumber": 1 },
                { "glyphType": "ROMAN", "glyphFormat": "%2.", "startNumber": 3 }
            ] } },
            "kix.points": { "listProperties": { "nestingLevels": [
                { "glyphSymbol": "●", "glyphFormat": "%0" },
                { "glyphSymbol": "○", "glyphFormat": "%1" }
            ] } },
            "kix.boxes": { "listProperties": { "nestingLevels": [{ "glyphType": "GLYPH_TYPE_UNSPECIFIED" }] } }
        });
        let content = body(vec![
            item("Plan", "kix.steps", None),
            item("Budget", "kix.steps", Some(1)),
            item("Staff", "kix.steps", Some(1)),
            item("Hire", "kix.steps", Some(2)),
            item("Build", "kix.steps", None),
            // A deeper level starts again under each item above it.
            item("Test", "kix.steps", Some(1)),
            paragraph("Between the two halves."),
            // The same list goes on counting after the paragraph.
            item("Ship", "kix.steps", None),
            item("Fast", "kix.points", None),
            item("Cheap\u{b}within reason", "kix.points", Some(1)),
            item("Signed off", "kix.boxes", None),
            // A list the document does not describe, and depths no list has.
            item("Loose", "kix.unknown", Some(1)),
            item("Deep", "kix.points", Some(40)),
            item("Odd", "kix.points", Some(-2)),
        ]);
        let read = read(document(json!({ "body": content, "lists": lists })));
        assert_eq!(
            read.text,
            [
                "1. Plan",
                "  1. Budget",
                "  2. Staff",
                "    3. Hire",
                "2. Build",
                "  1. Test",
                "Between the two halves.",
                "3. Ship",
                "- Fast",
                "  - Cheap",
                "    within reason",
                "- Signed off",
                "  - Loose",
                "                - Deep",
                "- Odd",
            ]
            .join("\n")
        );
    }

    #[test]
    fn a_table_has_a_line_for_each_row_with_its_cells_kept_apart() {
        let inner = table(vec![vec![vec![paragraph("x")], vec![paragraph("y")]]]);
        let content = vec![
            paragraph("Owners:"),
            table(vec![
                vec![vec![paragraph("Area")], vec![paragraph("Owner")]],
                // Two paragraphs in a cell, and a `|` that is part of the words.
                vec![
                    vec![paragraph("Billing"), paragraph("and tax")],
                    vec![paragraph("Ada | Grace")],
                ],
                // An empty cell, and a table inside a cell.
                vec![vec![paragraph("")], vec![inner, paragraph("")]],
            ]),
            paragraph("End."),
        ];
        assert_eq!(
            text_of(content),
            "Owners:\n| Area | Owner |\n| Billing and tax | Ada \\| Grace |\n|  | \\| x \\| y \\| |\nEnd."
        );
        // A cell that already holds a backslash before a bar, and a table
        // inside a cell whose own cell holds a bar: neither makes a cell more.
        let nested = table(vec![vec![vec![paragraph("a|b")]]]);
        assert_eq!(
            text_of(vec![table(vec![vec![vec![paragraph("Ada \\| Grace")], vec![nested]]])]),
            "| Ada \\\\\\| Grace | \\| a\\\\\\|b \\| |"
        );
        // A table Google sent no rows for, and a row with no cells.
        assert_eq!(text_of(vec![json!({ "table": { "rows": 0, "columns": 0 } })]), "");
        assert_eq!(text_of(vec![json!({ "table": { "tableRows": [{}] } })]), "|  |");
    }

    #[test]
    fn linked_words_cannot_end_their_link_and_seem_to_lead_elsewhere() {
        // Words written to look like a link of their own, to a place they do not lead.
        assert_eq!(
            linked(
                "Sign in](https://accounts.example.test) [here",
                "https://elsewhere.test/a b)c"
            ),
            "[Sign in\\](https://accounts.example.test) \\[here](https://elsewhere.test/a%20b%29c)"
        );
        assert_eq!(
            linked(" plan\n", "https://example.test/plan"),
            " [plan](https://example.test/plan)\n"
        );
    }

    #[test]
    fn linked_words_carry_their_address_once() {
        let address = "https://example.test/plan";
        let content = vec![
            // One link that Google split where its look changes.
            pieces(vec![
                run("See "),
                link("the ", address),
                link("plan", address),
                run(" and "),
                link("the budget", "https://example.test/budget"),
                run("."),
            ]),
            // The words are the address already.
            pieces(vec![link(address, address)]),
            // A link that runs to the end of its paragraph: the newline is not part of it.
            json!({ "paragraph": { "elements": [link(" Read more \n", address)] } }),
            // A link to a heading in the same document has no address to give.
            pieces(vec![
                json!({ "textRun": { "content": "Goals", "textStyle": { "link": { "headingId": "h.abc" } } } }),
            ]),
            pieces(vec![link("   ", address), link("blank address", " ")]),
        ];
        assert_eq!(
            text_of(content),
            "See [the plan](https://example.test/plan) and [the budget](https://example.test/budget).\n\
             https://example.test/plan\n [Read more](https://example.test/plan) \nGoals\n   blank address"
        );
    }

    #[test]
    fn what_is_not_text_is_written_as_what_a_person_sees_in_its_place() {
        let objects = json!({
            "kix.img1": { "objectId": "kix.img1", "inlineObjectProperties": { "embeddedObject": {
                "title": "Floor plan", "description": "The ground floor",
                "imageProperties": { "contentUri": "https://lh3.googleusercontent.com/secret" }
            } } },
            "kix.img2": { "inlineObjectProperties": { "embeddedObject": { "title": "  ", "description": " A chart of sales ", "imageProperties": {} } } },
            "kix.img3": { "inlineObjectProperties": { "embeddedObject": { "imageProperties": {} } } },
            "kix.draw": { "inlineObjectProperties": { "embeddedObject": { "embeddedDrawingProperties": {} } } }
        });
        let footnotes = json!({
            "kix.fn1": { "footnoteId": "kix.fn1", "content": [paragraph(" Counted in March."), paragraph("Again in May.")] }
        });
        let content = body(vec![
            pieces(vec![
                run("Ask "),
                json!({ "person": { "personId": "p1", "personProperties": { "name": "Ada Lovelace", "email": "ada@example.test" } } }),
                run(" or "),
                json!({ "person": { "personProperties": { "email": "grace@example.test" } } }),
                run(" before "),
                json!({ "dateElement": { "dateId": "d1", "dateElementProperties": { "timestamp": "2026-10-12T12:00:00Z", "displayText": "Oct 12, 2026" } } }),
                run(", not "),
                json!({ "dateElement": { "dateElementProperties": { "timestamp": "2026-10-19T12:00:00Z" } } }),
                run("."),
            ]),
            pieces(vec![
                run("Spec: "),
                json!({ "richLink": { "richLinkId": "r1", "richLinkProperties": { "title": "Q3 spec", "uri": "https://docs.google.com/document/d/abc/edit", "mimeType": "application/vnd.google-apps.document" } } }),
                run(" and "),
                json!({ "richLink": { "richLinkProperties": { "uri": "https://example.test/video" } } }),
                run(". Status: "),
                json!({ "dropdown": { "dropdownId": "dd1", "dropdownProperties": { "displayValue": "In review" } } }),
            ]),
            pieces(vec![
                json!({ "inlineObjectElement": { "inlineObjectId": "kix.img1" } }),
                json!({ "inlineObjectElement": { "inlineObjectId": "kix.img2" } }),
                json!({ "inlineObjectElement": { "inlineObjectId": "kix.img3" } }),
                json!({ "inlineObjectElement": { "inlineObjectId": "kix.draw" } }),
                json!({ "inlineObjectElement": { "inlineObjectId": "kix.gone" } }),
            ]),
            pieces(vec![
                run("We have 40 shops"),
                json!({ "footnoteReference": { "footnoteId": "kix.fn1", "footnoteNumber": "1" } }),
                run(" and "),
                json!({ "equation": {} }),
                run(" more"),
                // A footnote the document does not hold is referred to and not written out.
                json!({ "footnoteReference": { "footnoteId": "kix.lost" } }),
                run("."),
            ]),
            pieces(vec![json!({ "horizontalRule": { "textStyle": {} } })]),
        ]);
        let read = read(document(
            json!({ "body": content, "inlineObjects": objects, "footnotes": footnotes }),
        ));
        assert_eq!(
            read.text,
            "Ask Ada Lovelace or grace@example.test before Oct 12, 2026, not 2026-10-19T12:00:00Z.\n\
             Spec: [Q3 spec](https://docs.google.com/document/d/abc/edit) and https://example.test/video. Status: In review\n\
             [image: Floor plan][image: A chart of sales][image][object][object]\n\
             We have 40 shops[^1] and [equation] more[^2].\n\
             ---\n\n\
             [^1]: Counted in March. Again in May."
        );
        assert!(
            !read.text.contains("googleusercontent"),
            "a picture's address expires and is not text"
        );
    }

    #[test]
    fn breaks_hold_no_words_and_leave_the_lines_around_them_whole() {
        let content = vec![
            paragraph("Before the page break."),
            pieces(vec![json!({ "pageBreak": { "textStyle": {} } })]),
            paragraph("After it."),
            json!({ "sectionBreak": { "sectionStyle": { "sectionType": "NEXT_PAGE", "columnSeparatorStyle": "NONE" } } }),
            pieces(vec![
                run("Left column"),
                json!({ "columnBreak": {} }),
                run(" right column"),
            ]),
            pieces(vec![run("Page "), json!({ "autoText": { "type": "PAGE_NUMBER" } })]),
        ];
        assert_eq!(
            text_of(content),
            "Before the page break.\n\nAfter it.\n\nLeft column right column\nPage "
        );
    }

    #[test]
    fn a_table_of_contents_is_read_as_the_lines_it_shows() {
        let entry = |words: &str| {
            json!({ "paragraph": { "elements": [
                { "textRun": { "content": words, "textStyle": { "link": { "heading": { "id": "h.1", "tabId": "t.0" } } } } },
                { "textRun": { "content": "\n" } }
            ] } })
        };
        let content = vec![
            json!({ "tableOfContents": { "content": [entry("Goals"), entry("Risks")] } }),
            styled("Goals", "HEADING_1"),
        ];
        assert_eq!(text_of(content), "Goals\nRisks\n# Goals");
    }

    #[test]
    fn an_element_of_a_kind_that_is_not_known_is_left_out_and_the_rest_is_read() {
        let content = vec![
            paragraph("Before."),
            // A kind of block Google added after this was written.
            json!({ "startIndex": 9, "endIndex": 20, "interactiveCanvas": { "canvasId": "c1", "content": [paragraph("hidden")] } }),
            // A block with nothing set, and a paragraph with no elements.
            json!({ "startIndex": 20, "endIndex": 21 }),
            // A kind of piece that is not known, between two that are.
            pieces(vec![
                run("Vote: "),
                json!({ "poll": { "pollId": "p1", "question": "Lunch?" } }),
                run("today"),
            ]),
            paragraph("After."),
        ];
        assert_eq!(text_of(content), "Before.\nVote: today\nAfter.");
        assert_eq!(text_of(vec![json!({ "paragraph": {} })]), "");

        // A tab of a kind that holds no document is still a tab, with no text.
        let odd = read(json!({
            "documentId": "doc-1",
            "tabs": [{ "tabProperties": { "tabId": "t.9", "title": "Board" }, "whiteboardTab": { "shapes": [1, 2] } }]
        }));
        assert_eq!((odd.tabs[0].tab.title.as_str(), odd.text.as_str()), ("Board", ""));
    }
}
