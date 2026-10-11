//! Google Sheets: a spreadsheet's sheets, and reading and writing its cells.
//!
//! A range is written in A1 notation: `Sheet1!A1:B2`, `A:A`, or a sheet's
//! name alone for every cell of it. For one range it is part of the path,
//! and a sheet's name is its owner's own text, so it is always written with
//! [`Api::segment`].

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::{Api, SHEETS, set, with_query};
use crate::models::{
    SheetsAppendValues, SheetsAppendedValues, SheetsGetValues, SheetsUpdateValues, SheetsUpdatedValues,
    SheetsValueRanges, Spreadsheet, ValueRange,
};

/// What describes a spreadsheet and its sheets. Naming the fields is also
/// what keeps the cells out of the answer.
const LIGHT: &str = "spreadsheetId,spreadsheetUrl,properties(title,locale,timeZone),\
    sheets.properties(sheetId,title,index,sheetType,hidden,\
    gridProperties(rowCount,columnCount,frozenRowCount,frozenColumnCount))";

/// Google Sheets spreadsheets.
#[derive(Debug, Clone, Copy)]
pub struct SheetsSpreadsheets<'a>(pub(crate) Api<'a>);

impl SheetsSpreadsheets<'_> {
    /// Gets a spreadsheet's title, locale and time zone, and its sheets
    /// with their names and sizes. No cell is read.
    pub async fn get(&self, spreadsheet: &str) -> Result<Spreadsheet> {
        let request = RawRequest::get(self.path(spreadsheet)?).with_query("fields", LIGHT);
        let found: Spreadsheet = self.0.decode(self.0.send(request).await?, "a spreadsheet")?;
        if found.spreadsheet_id.is_empty() {
            return Err(self.missing("a spreadsheet"));
        }
        Ok(found)
    }

    /// Reads the values of one range.
    pub async fn values_get(&self, spreadsheet: &str, range: &str, options: SheetsGetValues) -> Result<ValueRange> {
        let request = with_query(RawRequest::get(self.values(spreadsheet, range)?), &options);
        let values: ValueRange = self.0.decode(self.0.send(request).await?, "a range of values")?;
        if values.range.is_empty() {
            return Err(self.missing("a range of values"));
        }
        Ok(values)
    }

    /// Reads the values of several ranges in one call. They come back in
    /// the order they were asked for.
    pub async fn values_batch_get<R: AsRef<str>>(
        &self,
        spreadsheet: &str,
        ranges: &[R],
        options: SheetsGetValues,
    ) -> Result<SheetsValueRanges> {
        let ranges: Vec<&str> = ranges.iter().map(|range| range.as_ref().trim()).collect();
        if ranges.is_empty() || ranges.iter().any(|range| range.is_empty()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`ranges` needs at least one range, and none blank",
            ));
        }
        let path = format!("{}/values:batchGet", self.path(spreadsheet)?);
        let request = with_query(RawRequest::get(path), &json!({ "ranges": ranges }));
        let body = self.0.send(with_query(request, &options)).await?;
        let values: SheetsValueRanges = self.0.decode(body, "ranges of values")?;
        // A caller pairs each answer with the range it asked for by its
        // place, so a list that is short would pair them wrongly.
        if values.spreadsheet_id.is_empty() || values.value_ranges.len() != ranges.len() {
            return Err(self.missing("the ranges of values"));
        }
        Ok(values)
    }

    /// Writes values over the cells of a range. What was in them is gone.
    pub async fn values_update(
        &self,
        spreadsheet: &str,
        range: &str,
        update: SheetsUpdateValues,
    ) -> Result<SheetsUpdatedValues> {
        self.cells(&update.values)?;
        let path = self.values(spreadsheet, range)?;
        let mut body = set(&json!({ "majorDimension": update.major_dimension }));
        body.insert("values".to_owned(), json!(update.values));
        let request = with_query(
            RawRequest::new("PUT", path).with_body(Value::Object(body)),
            &json!({ "valueInputOption": update.value_input_option }),
        );
        let updated: SheetsUpdatedValues = self.0.decode(self.0.send(request).await?, "what was updated")?;
        if updated.spreadsheet_id.is_empty() {
            return Err(self.missing("what it updated"));
        }
        Ok(updated)
    }

    /// Adds rows under a table. Google looks for the table in `range` and
    /// writes after its last row, starting at its first column.
    ///
    /// New rows are inserted for what is added (`insertDataOption=INSERT_ROWS`),
    /// and whatever lay under the table moves down. Left to its default,
    /// Google would write over it, and an append would not only add. To
    /// write over cells, use `values_update`.
    pub async fn values_append(
        &self,
        spreadsheet: &str,
        range: &str,
        append: SheetsAppendValues,
    ) -> Result<SheetsAppendedValues> {
        self.cells(&append.values)?;
        let path = format!("{}:append", self.values(spreadsheet, range)?);
        let mut body = set(&json!({ "majorDimension": append.major_dimension }));
        body.insert("values".to_owned(), json!(append.values));
        let request = with_query(
            RawRequest::post(path, Value::Object(body)),
            &json!({ "valueInputOption": append.value_input_option, "insertDataOption": "INSERT_ROWS" }),
        );
        let appended: SheetsAppendedValues = self.0.decode(self.0.send(request).await?, "what was appended")?;
        if appended.spreadsheet_id.is_empty() {
            return Err(self.missing("what it appended"));
        }
        Ok(appended)
    }

    /// The path of one spreadsheet.
    fn path(&self, spreadsheet: &str) -> Result<String> {
        let spreadsheet = self.0.segment("a spreadsheet id", spreadsheet)?;
        Ok(self.0.on(SHEETS, &format!("v4/spreadsheets/{spreadsheet}")))
    }

    /// The path of the values of one range.
    fn values(&self, spreadsheet: &str, range: &str) -> Result<String> {
        let path = self.path(spreadsheet)?;
        Ok(format!("{path}/values/{}", self.0.segment("a range", range)?))
    }

    /// Refuses values Google could only refuse, or would write nothing for.
    fn cells(&self, values: &[Vec<Value>]) -> Result<()> {
        if values.iter().all(Vec::is_empty) {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`values` needs at least one cell"));
        }
        if values.iter().flatten().any(|cell| cell.is_array() || cell.is_object()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "a cell of `values` is a string, a number, a boolean or null",
            ));
        }
        Ok(())
    }

    fn missing(&self, what: &str) -> socketkit_core::Error {
        self.0
            .error(ErrorKind::Decode, format!("google answered without {what}"))
    }
}
