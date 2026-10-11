//! Google Sheets: a spreadsheet and the sheets in it, without their cells.
//!
//! Reading and writing cells is in `value_range.rs`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A spreadsheet and its sheets, without what is in their cells.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Spreadsheet {
    /// The id in the spreadsheet's address: `docs.google.com/spreadsheets/d/{id}/edit`.
    pub spreadsheet_id: String,
    pub properties: SpreadsheetProperties,
    /// The sheets, in the order of their tabs.
    pub sheets: Vec<Sheet>,
    /// The address that opens the spreadsheet in a browser.
    pub spreadsheet_url: Option<String>,
}

/// What is set for a spreadsheet as a whole.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SpreadsheetProperties {
    pub title: String,
    /// The language and region numbers and dates are formatted for: `en_US`.
    pub locale: Option<String>,
    /// The zone dates and times are in: `Europe/London`. A zone Google does
    /// not know by name is an offset, such as `GMT-07:00`.
    pub time_zone: Option<String>,
}

/// One sheet of a spreadsheet.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Sheet {
    pub properties: SheetProperties,
}

/// What a sheet is called, where it stands and how large it is.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SheetProperties {
    /// The number after `gid=` in the sheet's address. The first sheet of a
    /// new spreadsheet is 0. It stays the same when the sheet is renamed.
    pub sheet_id: i32,
    /// The name a range is written with: `Sheet1!A1:B2`. A name with a
    /// space or a symbol in it goes between single quotes: `'Q3 plan'!A1:B2`.
    pub title: String,
    /// Where the sheet stands among the tabs, from 0.
    pub index: i32,
    /// `GRID` for a sheet of cells, `OBJECT` for one that holds a chart, or
    /// `DATA_SOURCE` for one connected to data kept elsewhere.
    pub sheet_type: Option<String>,
    pub hidden: bool,
    /// How large the sheet is. Absent for a sheet that has no cells.
    pub grid_properties: Option<SheetGridProperties>,
}

/// The size of a sheet. Every row and column is counted, empty ones included.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SheetGridProperties {
    pub row_count: i32,
    pub column_count: i32,
    /// How many rows at the top stay in place when the sheet is scrolled.
    /// Often the row of headings.
    pub frozen_row_count: i32,
    pub frozen_column_count: i32,
}
