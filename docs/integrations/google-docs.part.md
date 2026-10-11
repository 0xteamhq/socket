### Docs and Sheets: `docs_documents` and `sheets_spreadsheets`

A Google Doc is read as plain text, tabs included, and can be created and added to. A Google Sheet is described by its sheets and their sizes, and its cells are read and written by range. Both take the id from the file's address, the part after `/d/`; `google.resource.resolve` turns a pasted Docs or Sheets link into that id and confirms the account can open the file.

| Group | Method | What it does | Effect | Scope |
| --- | --- | --- | --- | --- |
| `docs_documents` | `get(document)` | `Document`: title, revision and tabs, without what is written in it | read | `documents.readonly` |
| `docs_documents` | `read(document)` | `DocumentText`: the whole document as plain text, and each tab's own | read | `documents.readonly` |
| `docs_documents` | `create(DocsCreateDocument)` | `Document`: a new blank document with a title | write | `documents` |
| `docs_documents` | `append_text(document, DocsAppendText)` | `DocumentUpdate`: adds text at the end of the document, or of one tab | write | `documents` |
| `sheets_spreadsheets` | `get(spreadsheet)` | `Spreadsheet`: title, locale, time zone, and each sheet's id, name, position and size | read | `spreadsheets.readonly` |
| `sheets_spreadsheets` | `values_get(spreadsheet, range, SheetsGetValues)` | `ValueRange`: the values of one range | read | `spreadsheets.readonly` |
| `sheets_spreadsheets` | `values_batch_get(spreadsheet, ranges, SheetsGetValues)` | `SheetsValueRanges`: the values of several ranges, in the order asked for | read | `spreadsheets.readonly` |
| `sheets_spreadsheets` | `values_update(spreadsheet, range, SheetsUpdateValues)` | `SheetsUpdatedValues`: writes values over the cells of a range | destructive | `spreadsheets` |
| `sheets_spreadsheets` | `values_append(spreadsheet, range, SheetsAppendValues)` | `SheetsAppendedValues`: adds rows under a table | write | `spreadsheets` |

```rust
use serde_json::json;
use socketkit::google::models::{DocsAppendText, SheetsAppendValues, SheetsGetValues};

// A meeting's notes as text, and a line added under them.
let docs = google.docs_documents(&connection);
let notes = docs.read(document_id).await?;
println!("{}", notes.text);
docs.append_text(document_id, DocsAppendText {
    text: "\nDecision: open in Paris first.".into(),
    tab_id: None,
}).await?;

// The sheets of a spreadsheet, the cells of one, and a row added to it.
let sheets = google.sheets_spreadsheets(&connection);
let stock = sheets.get(spreadsheet_id).await?;
let first = &stock.sheets[0].properties.title;
let cells = sheets.values_get(spreadsheet_id, &format!("'{first}'!A1:C100"), SheetsGetValues::default()).await?;
for row in &cells.values {
    println!("{row:?}");
}
sheets.values_append(spreadsheet_id, first, SheetsAppendValues::raw(vec![vec![json!("Washers"), json!(12)]])).await?;
```

**Scopes.** The provider's default scopes cover reading Docs (`documents.readonly`) and not Sheets: an application that reads spreadsheets has to ask for `spreadsheets.readonly` in `GoogleOAuth::scopes`, and one that writes has to ask for `documents` or `spreadsheets`. A token without the scope is refused by Google with a 403, which arrives as `AccessDenied` with Google's words, "Request had insufficient authentication scopes." The same error with "The caller does not have permission" means the account may not open that file.

**`get` is light.** Both `get` methods name the fields they want, so a long document or a large spreadsheet answers in a few hundred bytes. A document's tabs come back as one list in the order a person sees them, a child tab after the tab it is inside, each with `tabId`, `title`, `index`, `nestingLevel` and `parentTabId`. A spreadsheet's sheets each carry `sheetId`, `title`, `index`, `sheetType`, `hidden` and `gridProperties` (`rowCount`, `columnCount`, `frozenRowCount`, `frozenColumnCount`); a sheet that holds a chart has no `gridProperties`. `revisionId` is sent only to an account that may edit the document.

**A document as text.** `read` asks Google for every tab's content and writes it as the text a person reads. `text` is the whole document, and `tabs` has each tab with its own `text`. With one tab, `text` is that tab's text. With more, each tab's text follows a line that names it: `[tab: Plan > Budget]` for a tab called Budget inside one called Plan. Socket writes the text; Google does not send it.

- **A paragraph is a line**, and a line break inside a paragraph stays one. Empty paragraphs are kept between lines and dropped at the two ends.
- **A heading is a Markdown heading**: `#` for a title and for a first heading, `##` to `######` for the five below. A subtitle is an ordinary line.
- **A list item keeps its marker and its depth**: `-` for a bullet, the item's number and a full stop for a numbered item, behind two spaces for each list it is inside. Letters and Roman numerals are written as numbers. A checklist item is written as a bullet; Google does not say whether it is ticked.
- **A table has one line for each row**, its cells between `|`. What a cell holds is put on one line, and a `|` in a cell is written `\|`.
- **Linked words** are `[words](address)`. A smart chip for a file or a page is written the same way from its title and address, a person's chip is the name (or the address when there is no name), a date's chip is the date as the document shows it, and a dropdown is the option chosen.
- **A picture** is `[image: its title or description]`, or `[image]` when it has none; a drawing is `[object]`. A footnote is `[^1]` where it is referred to and `[^1]: …` after the text. A rule across the page is `---`, an equation is `[equation]`.
- **Breaks hold no words.** A page break or a column break leaves the line it is on as it was, and a section break leaves an empty line.
- **Headers, footers and page numbers are left out.**
- **An element of a kind Socket does not know is skipped**, and the rest of the document is read. A document is never refused for holding one.

**Suggestions.** A document is read as it stands: text someone has only suggested adding is left out, and text someone has only suggested deleting is kept. Google's own default depends on the account, with suggestions shown to an account that may edit and hidden from one that may only read, so `read` always asks for `PREVIEW_WITHOUT_SUGGESTIONS`. Comments are not read.

**Creating and adding.** `create` takes a `title` and nothing else: Google makes the document blank and ignores any content sent with it. `append_text` puts `text` at the end of the body of the first tab, or of the tab named in `tabId` (a tab's `tabId` from `get`). The text is sent exactly as given. Google puts it before the newline that ends the document, so it carries on the last paragraph: start it with `\n` to begin a new one. A new paragraph takes the look of the one before it, so text added after a heading or a list item is a heading or a list item too. Empty text and an empty title are refused before Google is called. A tab the document does not have is Google's to refuse, as `InvalidInput`.

**Ranges.** A range is in A1 notation: `Sheet1!A1:C10`, `Sheet1!A:A`, `A1:B2` for the first visible sheet, or a sheet's name alone for all of it. A name with a space or a symbol goes between single quotes: `'Q3 plan/final'!A1:B2`. For `values_get`, `values_update` and `values_append` the range is part of the address, and Socket writes it as one segment whatever it holds, so a `/`, a `?` or a `#` in a sheet's name cannot change where the request goes. A range Google cannot read, such as a sheet that is not there, arrives as `InvalidInput` with Google's words.

**Values.** `values` is a list of rows, each a list of cells, and a cell is whatever JSON Google sent: a string, a number or a boolean. Google leaves out the empty rows and columns at the end of a range and the empty cells at the end of a row, so rows differ in length and an empty range has no rows; Socket pads nothing. `SheetsGetValues` has three options, none sent unless set:

- `valueRenderOption`: `FORMATTED_VALUE` (the default: every cell as the string it shows, `$1.23`), `UNFORMATTED_VALUE` (numbers as numbers) or `FORMULA` (what was typed, `=A1`).
- `dateTimeRenderOption`: `SERIAL_NUMBER` (the default: days since 30 December 1899) or `FORMATTED_STRING`. Google ignores it when values are formatted.
- `majorDimension`: `ROWS` (the default) or `COLUMNS`, for a list of columns.

**Writing values.** `valueInputOption` is required and has no default, because neither value is safe for every caller. `USER_ENTERED` reads each value as if a person had typed it: text that starts with `=` becomes a formula, and text that looks like a number or a date becomes one. Text taken from an email or a form must not be written that way. `RAW` stores every value as it is given, so `2026-10-12` and `=SUM(A1:A9)` stay text. In Rust, `SheetsUpdateValues::raw(values)` and `::user_entered(values)` name the choice, and the same two exist on `SheetsAppendValues`. In `values`, `null` leaves a cell as it is and an empty string empties it. A write with no cell in it, or with a list or an object where a cell should be, is refused before Google is called.

**`values_update` overwrites.** What was in the cells is gone, so it is `destructive` and a host asks a person first. It reports what Google wrote: `updatedRange`, `updatedRows`, `updatedColumns` and `updatedCells`. It is sent as a PUT, and Socket's transport still repeats a PUT after a server error; repeating it writes the same values over the same cells, which changes nothing further.

**`values_append` adds rows.** Google looks in `range` for a table and writes after its last row, from its first column. It reports `tableRange`, the table as it was, and `updates`, where the rows went. New rows are always inserted for what is added (`insertDataOption=INSERT_ROWS`), and whatever lay under the table moves down. Google's own default, `OVERWRITE`, would write the rows over the cells there; that is not offered, so that an append only adds. To write over cells, use `values_update`. An append is a POST and is never repeated after a server error, so a failed append may or may not have added its rows; read the range before trying again.

**Limits Google sets.** Docs allows 300 reads and 60 writes a minute for each user of an application; Sheets allows 60 of each. Past that Google answers 429, which arrives as `RateLimited`. Google recommends keeping a Sheets request under 2 MB, and stops one it has worked on for 180 seconds. Socket reads an answer of at most 10 MB: a document whose structure is larger than that is refused as `Decode`, and a large range is better read in parts.

#### Confirmed against Google's documentation, and not

Read from developers.google.com in October 2026. Nothing was run against a live Google account.

Confirmed:

- `GET https://docs.googleapis.com/v1/documents/{documentId}`, its parameters `includeTabsContent` and `suggestionsViewMode`, and its scopes, `documents.readonly` among them ([documents.get](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/get)).
- That `suggestionsViewMode` defaults to `DEFAULT_FOR_CURRENT_ACCESS`, which shows suggestions inline to an account that may edit and hides them from one that may only view, and that `PREVIEW_WITHOUT_SUGGESTIONS` returns the document with every suggestion rejected ([documents](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents)).
- That with `includeTabsContent` the content is in `tabs[].documentTab` and the fields at the top are empty, that without it they hold the first tab's content, and that child tabs are in `childTabs` ([tabs](https://developers.google.com/workspace/docs/api/how-tos/tabs)).
- The `fields` parameter and its syntax, with commas, dots and parentheses, and that a mask that names `tabs` is treated as `includeTabsContent` ([field masks](https://developers.google.com/workspace/docs/api/how-tos/field-masks), [documents.get](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/get)).
- The names Socket reads from a document: `tabProperties` (`tabId`, `title`, `parentTabId`, `index`, `nestingLevel`, `iconEmoji`), `body.content`, the four kinds of structural element (`paragraph`, `sectionBreak`, `table`, `tableOfContents`), the twelve kinds of paragraph element, `paragraphStyle.namedStyleType` and its values, `bullet.listId` and `nestingLevel`, `lists[].listProperties.nestingLevels[]` with `glyphType`, `glyphSymbol` and `startNumber`, `tableRows[].tableCells[].content`, `textStyle.link.url`, `personProperties`, `richLinkProperties`, `dateElementProperties.displayText`, `dropdownProperties.displayValue`, `inlineObjects[].inlineObjectProperties.embeddedObject` and `footnotes[].content` ([documents](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents)).
- That `revisionId` is sent only to an account that may edit, and is good for 24 hours for that account.
- `POST https://docs.googleapis.com/v1/documents`, that it uses the title and ignores everything else, and its scopes ([documents.create](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/create)).
- `POST https://docs.googleapis.com/v1/documents/{documentId}:batchUpdate`, its body `requests`, its answer `documentId`, `replies` and `writeControl`, and its scopes ([documents.batchUpdate](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/batchUpdate)).
- `insertText` with `text` and `endOfSegmentLocation`, that an empty `segmentId` is the body, that `tabId` names the tab and the first tab is used without it, that the text goes immediately before the last newline of the segment, and that a newline in the text starts a paragraph in the style of the one before ([requests](https://developers.google.com/workspace/docs/api/reference/rest/v1/documents/request)).
- `GET https://sheets.googleapis.com/v4/spreadsheets/{spreadsheetId}`, that cell data is not returned by default, that `includeGridData` is ignored when a field mask is set, and its scopes ([spreadsheets.get](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/get)).
- The fields of a spreadsheet, of `SpreadsheetProperties`, of `SheetProperties` and of `GridProperties`, and the three sheet types ([spreadsheets](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets), [sheets](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets/sheets)).
- `GET …/values/{range}` and `GET …/values:batchGet` with `ranges` repeated, their three options with their values and defaults, that the answers come in the order asked for, and their scopes ([values.get](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/get), [values.batchGet](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/batchGet)).
- `PUT …/values/{range}` and `POST …/values/{range}:append`, `valueInputOption` with `RAW` and `USER_ENTERED` and what each does, `insertDataOption` with `OVERWRITE` and `INSERT_ROWS`, the body as a `ValueRange`, the two answers with their fields, and their scopes ([values.update](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/update), [values.append](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values/append), [UpdateValuesResponse](https://developers.google.com/workspace/sheets/api/reference/rest/v4/UpdateValuesResponse)).
- `ValueRange`: that trailing empty rows and columns are left out, that a `null` in what is written is skipped and an empty string empties a cell, and that a cell written is a boolean, a string or a number ([values](https://developers.google.com/workspace/sheets/api/reference/rest/v4/spreadsheets.values)).
- A1 notation, and that single quotes are required around a sheet name with spaces or special characters ([concepts](https://developers.google.com/workspace/sheets/api/guides/concepts)).
- The quotas above, the 429, the 2 MB recommendation and the 180 seconds ([Docs limits](https://developers.google.com/workspace/docs/api/limits), [Sheets limits](https://developers.google.com/workspace/sheets/api/limits)).

Not confirmed:

- **How deep tabs can nest.** No page gives a limit. A field mask cannot say "at every depth", so `get` names four: a tab and three inside one another under it. A tab deeper than that is missing from the list `get` returns. `read` uses no mask and returns every tab at any depth.
- **Whether `documents.create` answers with the new document's tabs.** Socket asks for them with the same field mask as `get`. If Google sends none, `create` returns a document with an empty `tabs`, and `get` returns them.
- **That a body opens with a section break**, and that the newline that ends a paragraph is in its last text run. Google's examples show both and no page states them. Socket skips a section break only where it is the first element, and removes one newline from the end of a paragraph.
- **That a line break inside a paragraph is a vertical tab** (`U+000B`) in a text run. No page says so. If Google writes it another way, the break is kept as whatever character Google sent.
- **Whether a checklist item's tick can be read.** No field for it was found, so a checklist reads as bullets.
- **An empty `endOfSegmentLocation`.** Socket sends `{}` for the end of the first tab's body. The page says an empty `segmentId` is the body and an omitted `tabId` is the first tab; it does not show the object with neither.
- **What `append_text` does to a document that ends in a table**, or in another element text cannot follow.
- **A range written with `%21` and `%3A`** for `!` and `:`. This is ordinary URL encoding of a path segment, not confirmed for Sheets specifically, and neither is a `/` in a sheet's name written as `%2F`.
- **How an apostrophe inside a sheet's name is written in A1 notation.** Google's own example, `'Jon's_Data'!A1:D5`, does not double it. Socket passes the range on as it is given.
- **That an empty cell between two values is an empty string**, and that the cells at the end of a row are left out. The page speaks only of trailing rows and columns. Socket returns what Google sends either way.
- **What Google answers for a write of only `null` values**, and whether `updatedCells` is then absent. An absent count reads as 0.
- **The words of Google's errors** used in the tests ("Unable to parse range", "insufficient authentication scopes"). Socket goes by the status, and passes the words on.
- **Whether the quotas count a `batchGet` as one read.**

#### Not supported yet

- **Editing a document** beyond adding text at its end: inserting at a place, replacing, deleting, styling, tables, images, and writing as a suggestion. `batchUpdate` is used for the one request only.
- **Adding a tab, renaming one or deleting one.**
- **Comments and suggestions** in a document. Suggested text is not marked; it is left out.
- **A document's headers and footers**, and the tick of a checklist item.
- **Reading a document in a format other than text.** Drive's export gives Markdown, HTML and PDF; see `drive_files.export`.
- **Reading part of a document.** `read` always returns every tab, and a document whose structure is over 10 MB cannot be read.
- **Creating a spreadsheet, and adding, renaming or deleting a sheet.**
- **Clearing a range** (`values.clear`), **writing several ranges at once** (`values.batchUpdate`), and returning the written values (`includeValuesInResponse`).
- **Formats, merges, notes, charts, named ranges, filters and protected ranges.** `spreadsheets.get` is asked only for what describes the sheets.
- **Ranges in R1C1 notation.** Google accepts them where it accepts A1 notation and Socket passes a range on as given, but nothing here was tested with one.
