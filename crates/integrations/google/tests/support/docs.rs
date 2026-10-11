//! What Google answers for Docs and Sheets, as the tests need it: fixtures and constants.

use serde_json::{Value, json};

/// A document's id, as it stands in its address.
pub const DOCUMENT: &str = "1AbC_dEf-GhIjKlMnOpQrStUvWxYz012345";
/// A spreadsheet's id.
pub const SPREADSHEET: &str = "1BxiMVs0XRA5nFMdKvBdBZjgmUUqptlbs74OgvE2upms";

/// The fields `docs_documents.get` and `.create` ask for: a document's own
/// names and its tabs', four tabs deep, and nothing written in them.
pub const DOCUMENT_FIELDS: &str = "documentId,title,revisionId,tabs(tabProperties,childTabs(tabProperties,childTabs(tabProperties,childTabs(tabProperties))))";

/// The fields `sheets_spreadsheets.get` asks for: no cell is among them.
pub const SPREADSHEET_FIELDS: &str = "spreadsheetId,spreadsheetUrl,properties(title,locale,timeZone),sheets.properties(sheetId,title,index,sheetType,hidden,gridProperties(rowCount,columnCount,frozenRowCount,frozenColumnCount))";

/// The revision Google names in these answers.
pub const REVISION: &str = "ALBJ4LuVbV5yNfrJKqs3BU8rV9rHv2q4";

fn tab_properties(id: &str, title: &str, index: i32, parent: Option<&str>) -> Value {
    match parent {
        Some(parent) => {
            json!({ "tabId": id, "title": title, "parentTabId": parent, "index": index, "nestingLevel": 1 })
        }
        // Google leaves out a nesting level of 0.
        None => json!({ "tabId": id, "title": title, "index": index }),
    }
}

/// A document as `documents.get` answers when asked for [`DOCUMENT_FIELDS`]:
/// two tabs at the top, and one inside the first.
pub fn document() -> Value {
    json!({
        "documentId": DOCUMENT,
        "title": "Q3 plan",
        "revisionId": REVISION,
        "tabs": [
            {
                "tabProperties": tab_properties("t.0", "Plan", 0, None),
                "childTabs": [{ "tabProperties": tab_properties("t.8f2k", "Budget", 0, Some("t.0")) }]
            },
            { "tabProperties": tab_properties("t.x1n4", "Notes", 1, None) }
        ]
    })
}

/// A blank document as `documents.create` answers.
pub fn created_document() -> Value {
    json!({
        "documentId": DOCUMENT,
        "title": "Q3 plan",
        "revisionId": REVISION,
        "tabs": [{ "tabProperties": tab_properties("t.0", "Tab 1", 0, None) }]
    })
}

/// A paragraph as Google writes one: runs of text, the last ending in the
/// newline that closes it, and the style it is set in.
pub fn paragraph(words: &str, style: &str) -> Value {
    json!({
        "startIndex": 1,
        "endIndex": 1 + words.encode_utf16().count(),
        "paragraph": {
            "elements": [{ "startIndex": 1, "endIndex": 2, "textRun": { "content": format!("{words}\n"), "textStyle": {} } }],
            "paragraphStyle": { "namedStyleType": style, "direction": "LEFT_TO_RIGHT" }
        }
    })
}

fn item(words: &str, list: &str, depth: i32) -> Value {
    let mut item = paragraph(words, "NORMAL_TEXT");
    item["paragraph"]["bullet"] = json!({ "listId": list, "nestingLevel": depth, "textStyle": {} });
    item
}

fn cell(words: &str) -> Value {
    json!({ "startIndex": 1, "endIndex": 2, "content": [paragraph(words, "NORMAL_TEXT")], "tableCellStyle": { "rowSpan": 1, "columnSpan": 1 } })
}

/// What a tab holds, behind the section break every body opens with.
pub fn tab_content(mut blocks: Vec<Value>) -> Value {
    blocks.insert(
        0,
        json!({ "endIndex": 1, "sectionBreak": { "sectionStyle": { "columnSeparatorStyle": "NONE", "contentDirection": "LEFT_TO_RIGHT", "sectionType": "CONTINUOUS" } } }),
    );
    json!({
        "body": { "content": blocks },
        "documentStyle": { "pageSize": { "height": { "magnitude": 792, "unit": "PT" }, "width": { "magnitude": 612, "unit": "PT" } } },
        "namedStyles": { "styles": [{ "namedStyleType": "NORMAL_TEXT", "textStyle": { "fontSize": { "magnitude": 11, "unit": "PT" } } }] }
    })
}

/// The same document as `documents.get` answers with `includeTabsContent`:
/// each tab with what is written in it.
pub fn document_with_content() -> Value {
    let mut plan = tab_content(vec![
        paragraph("Q3 plan", "TITLE"),
        paragraph("Goals", "HEADING_1"),
        item("Open two shops", "kix.goals", 0),
        item("One in Paris", "kix.goals", 1),
        item("Hire a manager", "kix.goals", 0),
        json!({
            "startIndex": 60, "endIndex": 90,
            "table": {
                "rows": 2, "columns": 2,
                "tableRows": [
                    { "startIndex": 61, "endIndex": 75, "tableCells": [cell("Area"), cell("Owner")], "tableRowStyle": { "minRowHeight": { "unit": "PT" } } },
                    { "startIndex": 75, "endIndex": 89, "tableCells": [cell("Shops"), cell("Ada")], "tableRowStyle": { "minRowHeight": { "unit": "PT" } } }
                ],
                "tableStyle": { "tableColumnProperties": [{ "widthType": "EVENLY_DISTRIBUTED" }, { "widthType": "EVENLY_DISTRIBUTED" }] }
            }
        }),
        paragraph("", "NORMAL_TEXT"),
    ]);
    plan["lists"] = json!({
        "kix.goals": { "listProperties": { "nestingLevels": [
            { "bulletAlignment": "START", "glyphType": "DECIMAL", "glyphFormat": "%0.", "startNumber": 1 },
            { "bulletAlignment": "START", "glyphSymbol": "○", "glyphFormat": "%1" }
        ] } }
    });
    json!({
        "documentId": DOCUMENT,
        "title": "Q3 plan",
        "revisionId": REVISION,
        "suggestionsViewMode": "PREVIEW_WITHOUT_SUGGESTIONS",
        "tabs": [
            {
                "tabProperties": tab_properties("t.0", "Plan", 0, None),
                "childTabs": [{
                    "tabProperties": tab_properties("t.8f2k", "Budget", 0, Some("t.0")),
                    "documentTab": tab_content(vec![paragraph("Rent is the largest cost.", "NORMAL_TEXT")])
                }],
                "documentTab": plan
            },
            {
                "tabProperties": tab_properties("t.x1n4", "Notes", 1, None),
                "documentTab": tab_content(vec![paragraph("", "NORMAL_TEXT")])
            }
        ]
    })
}

/// [`document_with_content`], tab by tab, as plain text.
pub const PLAN_TEXT: &str =
    "# Q3 plan\n# Goals\n1. Open two shops\n  - One in Paris\n2. Hire a manager\n| Area | Owner |\n| Shops | Ada |";
pub const BUDGET_TEXT: &str = "Rent is the largest cost.";

/// [`document_with_content`] as one text.
pub fn document_text() -> String {
    format!("[tab: Plan]\n\n{PLAN_TEXT}\n\n[tab: Plan > Budget]\n\n{BUDGET_TEXT}\n\n[tab: Notes]")
}

/// What `documents.batchUpdate` answers for one request that has no reply of its own.
pub fn document_updated() -> Value {
    json!({
        "documentId": DOCUMENT,
        "replies": [{}],
        "writeControl": { "requiredRevisionId": REVISION }
    })
}

/// A spreadsheet as `spreadsheets.get` answers when asked for
/// [`SPREADSHEET_FIELDS`]: two sheets of cells, and one that holds a chart.
pub fn spreadsheet() -> Value {
    json!({
        "spreadsheetId": SPREADSHEET,
        "properties": { "title": "Stock", "locale": "en_GB", "timeZone": "Europe/London" },
        "sheets": [
            { "properties": {
                "sheetId": 0, "title": "Sheet1", "index": 0, "sheetType": "GRID",
                "gridProperties": { "rowCount": 1000, "columnCount": 26, "frozenRowCount": 1 }
            } },
            { "properties": {
                "sheetId": 1837264519, "title": "Q3 plan/final", "index": 1, "sheetType": "GRID", "hidden": true,
                "gridProperties": { "rowCount": 200, "columnCount": 8 }
            } },
            { "properties": { "sheetId": 771203, "title": "Chart1", "index": 2, "sheetType": "OBJECT" } }
        ],
        "spreadsheetUrl": format!("https://docs.google.com/spreadsheets/d/{SPREADSHEET}/edit")
    })
}

/// The values of a range as Google returns them formatted: every cell a
/// string, an empty cell an empty string, and a row cut short where the
/// rest of it is empty.
pub fn value_range() -> Value {
    json!({
        "range": "Sheet1!A1:C4",
        "majorDimension": "ROWS",
        "values": [["Item", "Qty", "In stock"], ["Bolts", "40", "TRUE"], ["Nuts"], ["", "12"]]
    })
}

/// The same cells unformatted: numbers and booleans as what they are.
pub fn unformatted_values() -> Value {
    json!({
        "range": "Sheet1!A1:C4",
        "majorDimension": "ROWS",
        "values": [["Item", "Qty", "In stock"], ["Bolts", 40, true], ["Nuts"], ["", 12.5]]
    })
}

/// What `values.batchGet` answers for two ranges, the second with nothing in it.
pub fn value_ranges() -> Value {
    json!({
        "spreadsheetId": SPREADSHEET,
        "valueRanges": [
            { "range": "Sheet1!A1:A4", "majorDimension": "COLUMNS", "values": [["Item", "Bolts", "Nuts"]] },
            { "range": "'Q3 plan/final'!B2", "majorDimension": "COLUMNS" }
        ]
    })
}

/// What `values.update` answers.
pub fn values_updated() -> Value {
    json!({
        "spreadsheetId": SPREADSHEET,
        "updatedRange": "Sheet1!A2:C2",
        "updatedRows": 1,
        "updatedColumns": 3,
        "updatedCells": 3
    })
}

/// What `values.append` answers.
pub fn values_appended() -> Value {
    json!({
        "spreadsheetId": SPREADSHEET,
        "tableRange": "Sheet1!A1:C4",
        "updates": {
            "spreadsheetId": SPREADSHEET,
            "updatedRange": "Sheet1!A5:C5",
            "updatedRows": 1,
            "updatedColumns": 3,
            "updatedCells": 3
        }
    })
}

/// An error as Google's newer APIs, Docs and Sheets among them, write one.
pub fn status_error(code: u16, status: &str, message: &str) -> Value {
    json!({ "error": { "code": code, "message": message, "status": status } })
}
