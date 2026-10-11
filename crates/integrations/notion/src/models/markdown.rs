//! A page's blocks written as Markdown.
//!
//! Paragraphs, headings, lists, to-dos, toggles, quotes, callouts, code,
//! tables, dividers, equations and links to pages are written as Markdown
//! writes them. Any other block is a line that names its kind in square
//! brackets, so that nothing on the page goes missing without a trace.

use serde_json::Value;

use super::Block;
use super::rich_text::{markdown, plain_text, runs};
use super::tree::Tree;

impl Tree {
    pub(crate) fn markdown(&self) -> String {
        let mut lines = Vec::new();
        self.inside(Self::PAGE, "", None, &mut lines);
        lines.join("\n")
    }

    /// Writes the blocks inside `node`, each line after `margin`. `before`
    /// is the kind of what was written just above them, if anything was.
    fn inside<'a>(&'a self, node: usize, margin: &str, mut before: Option<&'a str>, lines: &mut Vec<String>) {
        let apart = |lines: &mut Vec<String>| lines.push(margin.trim_end().to_owned());
        let mut number = 0;
        for &place in &self.nodes[node].inside {
            let Some(block) = &self.nodes[place].block else {
                continue;
            };
            let kind = block.kind();
            number = match (kind, number) {
                ("numbered_list_item", 0) => block.held()["list_start_index"].as_u64().unwrap_or(1),
                ("numbered_list_item", last) => last + 1,
                _ => 0,
            };
            let mut written = Vec::new();
            self.block(place, block, number, margin, &mut written);
            if written.is_empty() {
                continue;
            }
            // Items of one list follow each other; anything else stands apart.
            if before.is_some() && !(listed(kind) && before.is_some_and(listed)) {
                apart(lines);
            }
            lines.append(&mut written);
            before = Some(kind);
        }
        if let Some(cut) = self.nodes[node].cut {
            if before.is_some() {
                apart(lines);
            }
            lines.push(format!("{margin}{}", cut.note()));
        }
    }

    fn block(&self, place: usize, block: &Block, number: u64, margin: &str, lines: &mut Vec<String>) {
        let (kind, held) = (block.kind(), block.held());
        let text = markdown(&runs(&held["rich_text"]));
        let caption = markdown(&runs(&held["caption"]));
        let label = |fallback: &str| match (caption.is_empty(), held["name"].as_str()) {
            (false, _) => caption.clone(),
            (true, Some(name)) if !name.is_empty() => name.to_owned(),
            (true, _) => fallback.to_owned(),
        };
        // The margin of the blocks nested inside this one.
        let mut nested = margin.to_owned();
        match kind {
            "paragraph" => put(lines, margin, margin, &text),
            "heading_1" | "heading_2" | "heading_3" | "heading_4" => {
                let level = kind.trim_start_matches("heading_").parse().unwrap_or(1);
                let heading = format!("{margin}{} ", "#".repeat(level));
                put(lines, &heading, margin, &text.replace('\n', " "));
            }
            "bulleted_list_item" | "toggle" => nested = item(lines, margin, "- ", &text),
            "numbered_list_item" => nested = item(lines, margin, &format!("{number}. "), &text),
            "to_do" if held["checked"] == true => nested = item(lines, margin, "- [x] ", &text),
            "to_do" => nested = item(lines, margin, "- [ ] ", &text),
            "quote" | "callout" => {
                nested = format!("{margin}> ");
                let said = match held["icon"]["emoji"].as_str() {
                    Some(emoji) => format!("{emoji} {text}").trim_end().to_owned(),
                    None => text,
                };
                put(lines, &nested, &nested, &said);
            }
            "code" => {
                let source = plain_text(&runs(&held["rich_text"]));
                // A fence longer than any run of backticks in what it encloses.
                let longest = source.split(|c| c != '`').map(str::len).max().unwrap_or(0);
                let fence = "`".repeat(longest.max(2) + 1);
                let language = held["language"].as_str().filter(|language| *language != "plain text");
                lines.push(format!("{margin}{fence}{}", language.unwrap_or_default()));
                lines.extend(source.split('\n').map(|line| format!("{margin}{line}")));
                lines.push(format!("{margin}{fence}"));
                put(lines, margin, margin, &caption);
            }
            "divider" => lines.push(format!("{margin}---")),
            "equation" => lines.push(format!(
                "{margin}$$ {} $$",
                held["expression"].as_str().unwrap_or_default()
            )),
            "table" => return self.table(place, held, margin, lines),
            "child_page" | "child_database" => {
                let title = held["title"].as_str().filter(|title| !title.is_empty());
                lines.push(format!(
                    "{margin}[{}]({})",
                    title.unwrap_or("Untitled"),
                    address(&block.id)
                ));
            }
            "link_to_page" => match held["page_id"].as_str().or(held["database_id"].as_str()) {
                Some(id) => lines.push(format!("{margin}[Linked page]({})", address(id))),
                None => lines.push(format!("{margin}[link_to_page]")),
            },
            "bookmark" | "embed" | "link_preview" => match held["url"].as_str().filter(|url| !url.is_empty()) {
                Some(url) => lines.push(format!("{margin}[{}]({url})", label(url))),
                None => lines.push(format!("{margin}[{kind}]")),
            },
            // A file Notion keeps is given at an address that expires within
            // the hour, so only a file kept elsewhere is linked.
            "image" | "video" | "audio" | "file" | "pdf" => match held["external"]["url"].as_str() {
                Some(url) if kind == "image" => lines.push(format!("{margin}![{}]({url})", label(""))),
                Some(url) => lines.push(format!("{margin}[{}]({url})", label(kind))),
                None => lines.push(format!("{margin}[{kind}{}]", described(&label("")))),
            },
            // These only arrange the blocks inside them.
            "column_list" | "column" | "synced_block" | "tab" => {}
            "unsupported" => {
                let what = held["block_type"].as_str().unwrap_or_default();
                lines.push(format!("{margin}[unsupported{}]", described(what)));
            }
            other => lines.push(format!("{margin}[{other}] {text}").trim_end().to_owned()),
        }
        let before = (!lines.is_empty()).then_some(kind);
        self.inside(place, &nested, before, lines);
    }

    /// A table, whose rows are the blocks inside it.
    fn table(&self, place: usize, held: &Value, margin: &str, lines: &mut Vec<String>) {
        let cell = |cell: &Value| markdown(&runs(cell)).replace('|', "\\|").replace('\n', "<br>");
        let mut rows: Vec<Vec<String>> = self.nodes[place]
            .inside
            .iter()
            .filter_map(|&row| self.nodes[row].block.as_ref())
            .filter(|row| row.kind() == "table_row")
            .map(|row| row.held()["cells"].as_array().into_iter().flatten().map(cell).collect())
            .collect();
        let widest = rows.iter().map(Vec::len).max().unwrap_or(0);
        let width =
            usize::try_from(held["table_width"].as_u64().unwrap_or(0)).map_or(widest, |width| width.max(widest));
        if rows.is_empty() || width == 0 {
            lines.push(format!("{margin}[table]"));
        } else {
            let line = |cells: &[String]| {
                let cells: Vec<&str> = (0..width).map(|at| cells.get(at).map_or("", String::as_str)).collect();
                format!("{margin}| {} |", cells.join(" | "))
            };
            // Markdown has no table without a heading row, so one without
            // gets an empty one.
            let heading = if held["has_column_header"] == true {
                rows.remove(0)
            } else {
                Vec::new()
            };
            lines.push(line(&heading));
            lines.push(format!("{margin}|{}", " --- |".repeat(width)));
            lines.extend(rows.iter().map(|row| line(row)));
        }
        if let Some(cut) = self.nodes[place].cut {
            lines.push(margin.trim_end().to_owned());
            lines.push(format!("{margin}{}", cut.note()));
        }
    }
}

fn listed(kind: &str) -> bool {
    matches!(kind, "bulleted_list_item" | "numbered_list_item" | "to_do" | "toggle")
}

/// The address that opens a page or a database in Notion.
fn address(id: &str) -> String {
    format!("https://www.notion.so/{}", id.replace('-', ""))
}

/// `what` as it follows a kind inside square brackets, or nothing.
fn described(what: &str) -> String {
    if what.is_empty() {
        String::new()
    } else {
        format!(": {what}")
    }
}

/// Writes `text` a line at a time: the first after `first`, the rest after `rest`.
fn put(lines: &mut Vec<String>, first: &str, rest: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    for (at, line) in text.split('\n').enumerate() {
        lines.push(format!("{}{line}", if at == 0 { first } else { rest }));
    }
}

/// Writes a list item behind `mark`, and returns the margin of what is nested in it.
fn item(lines: &mut Vec<String>, margin: &str, mark: &str, text: &str) -> String {
    // What is nested lines up with the item's text. A to-do's box is part of
    // its text, so that is two columns in, as for any item behind a dash.
    let indent = if mark.starts_with('-') { 2 } else { mark.len() };
    let nested = format!("{margin}{}", " ".repeat(indent));
    if text.is_empty() {
        lines.push(format!("{margin}{mark}").trim_end().to_owned());
    } else {
        put(lines, &format!("{margin}{mark}"), &nested, text);
    }
    nested
}
