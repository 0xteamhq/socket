//! The cells of a spreadsheet: a range of values as Google returns it, and
//! the options and content used to read and write one.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The values of one range of cells.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct ValueRange {
    /// The range that was asked for, in A1 notation with its sheet:
    /// `Sheet1!A1:C10`. The values may cover less of it; see `values`.
    pub range: String,
    /// `ROWS` when each inner list is a row, `COLUMNS` when it is a column.
    pub major_dimension: Option<String>,
    /// One list for each row, or for each column, and in it one value for
    /// each cell: a string, a number, a boolean, or an empty string for an
    /// empty cell. Google leaves out the empty rows and columns at the end,
    /// and the empty cells at the end of each row, so the lists may be of
    /// different lengths, and a range with nothing in it has none.
    pub values: Vec<Vec<Value>>,
}

/// The values of several ranges.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SheetsValueRanges {
    pub spreadsheet_id: String,
    /// One for each range asked for, in the order they were asked for.
    pub value_ranges: Vec<ValueRange>,
}

/// Whether each inner list of `values` is a row or a column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SheetsDimension {
    Rows,
    Columns,
}

/// How the value of a cell is returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SheetsValueRenderOption {
    /// As the cell shows it, a string: `$1.23`.
    FormattedValue,
    /// What the cell works out to, before formatting: the number `1.23`.
    UnformattedValue,
    /// What was typed into the cell: `=A1` for a formula.
    Formula,
}

/// How a date, a time or a duration is returned when values are not formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SheetsDateTimeRenderOption {
    /// A number: the days since 30 December 1899, with the time of day as
    /// its fraction.
    SerialNumber,
    /// A string, as the cell's own format writes it.
    FormattedString,
}

/// How values that are written are taken.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SheetsValueInputOption {
    /// Stored exactly as given. A string stays a string, whatever it says.
    Raw,
    /// Read as if a person had typed them into the cell: a string that
    /// starts with `=` becomes a formula, and one that looks like a number
    /// or a date becomes one.
    UserEntered,
}

/// What appending rows does to what lies under the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SheetsInsertDataOption {
    /// The new rows are written into the cells under the table, over
    /// anything that is already in them.
    Overwrite,
    /// New rows are inserted for the data, and what was under the table
    /// moves down.
    InsertRows,
}

/// How to return the values that are read.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SheetsGetValues {
    /// `ROWS` for a list of rows, `COLUMNS` for a list of columns. Rows
    /// when not given.
    pub major_dimension: Option<SheetsDimension>,
    /// `FORMATTED_VALUE` for each cell as it is shown, `UNFORMATTED_VALUE`
    /// for numbers as numbers, or `FORMULA` for what was typed. As shown
    /// when not given.
    pub value_render_option: Option<SheetsValueRenderOption>,
    /// `SERIAL_NUMBER` or `FORMATTED_STRING`, for dates and times. Google
    /// ignores it unless `valueRenderOption` is `UNFORMATTED_VALUE` or
    /// `FORMULA`. A serial number when not given.
    pub date_time_render_option: Option<SheetsDateTimeRenderOption>,
}

/// Values to write over a range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SheetsUpdateValues {
    /// One list for each row, and in it one value for each cell: a string, a
    /// number or a boolean. `null` leaves a cell as it is, and an empty
    /// string empties it.
    pub values: Vec<Vec<Value>>,
    /// `RAW` or `USER_ENTERED`. It has to be chosen, because neither is safe
    /// for every caller: `USER_ENTERED` turns text that starts with `=` into
    /// a formula, which text taken from someone else must not become, and
    /// `RAW` stores `2026-10-12` and `=SUM(A1:A9)` as the text they are.
    pub value_input_option: SheetsValueInputOption,
    /// `ROWS` when each inner list of `values` is a row, `COLUMNS` when it
    /// is a column. Rows when not given.
    pub major_dimension: Option<SheetsDimension>,
}

impl SheetsUpdateValues {
    /// These rows, each cell stored exactly as it is given.
    pub fn raw(values: Vec<Vec<Value>>) -> Self {
        Self {
            values,
            value_input_option: SheetsValueInputOption::Raw,
            major_dimension: None,
        }
    }

    /// These rows, each cell read as if a person had typed it: a formula
    /// where it starts with `=`.
    pub fn user_entered(values: Vec<Vec<Value>>) -> Self {
        Self {
            value_input_option: SheetsValueInputOption::UserEntered,
            ..Self::raw(values)
        }
    }
}

/// Rows to add under a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SheetsAppendValues {
    /// One list for each row to add, and in it one value for each cell: a
    /// string, a number or a boolean.
    pub values: Vec<Vec<Value>>,
    /// `RAW` or `USER_ENTERED`. It has to be chosen, because neither is safe
    /// for every caller: `USER_ENTERED` turns text that starts with `=` into
    /// a formula, which text taken from someone else must not become, and
    /// `RAW` stores `2026-10-12` and `=SUM(A1:A9)` as the text they are.
    pub value_input_option: SheetsValueInputOption,
    /// `OVERWRITE` writes the rows into the cells under the table, over
    /// anything already there. `INSERT_ROWS` inserts new rows for them, so
    /// nothing under the table is written over. Google overwrites when not
    /// given.
    pub insert_data_option: Option<SheetsInsertDataOption>,
    /// `ROWS` when each inner list of `values` is a row, `COLUMNS` when it
    /// is a column. Rows when not given.
    pub major_dimension: Option<SheetsDimension>,
}

impl SheetsAppendValues {
    /// These rows, each cell stored exactly as it is given.
    pub fn raw(values: Vec<Vec<Value>>) -> Self {
        Self {
            values,
            value_input_option: SheetsValueInputOption::Raw,
            insert_data_option: None,
            major_dimension: None,
        }
    }

    /// These rows, each cell read as if a person had typed it: a formula
    /// where it starts with `=`.
    pub fn user_entered(values: Vec<Vec<Value>>) -> Self {
        Self {
            value_input_option: SheetsValueInputOption::UserEntered,
            ..Self::raw(values)
        }
    }
}

/// What Google reports it wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SheetsUpdatedValues {
    pub spreadsheet_id: String,
    /// The range that was written, in A1 notation with its sheet.
    pub updated_range: Option<String>,
    /// How many rows had at least one cell written.
    pub updated_rows: i32,
    /// How many columns had at least one cell written.
    pub updated_columns: i32,
    pub updated_cells: i32,
}

/// What Google reports it appended, and to which table.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct SheetsAppendedValues {
    pub spreadsheet_id: String,
    /// The table the rows were added under, as it was before they were
    /// added. Absent when the range held no table.
    pub table_range: Option<String>,
    /// Where the rows went, and how many cells were written.
    pub updates: SheetsUpdatedValues,
}
