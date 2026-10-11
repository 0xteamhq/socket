//! Google Sheets against a local server that answers as Google does.
//!
//! What every operation sends and returns is in the table in `operations.rs`.
//! This file holds what is particular to Sheets.

use std::sync::Arc;

use serde_json::{Value, json};
use socketkit_core::ErrorKind;
use socketkit_google::models::{
    SheetsAppendValues, SheetsDateTimeRenderOption, SheetsDimension, SheetsGetValues, SheetsInsertDataOption,
    SheetsUpdateValues, SheetsValueInputOption, SheetsValueRenderOption,
};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::{any, method};
use socketkit_testkit::wiremock::{Mock, MockServer};
use socketkit_testkit::{connect, point_at};

mod support;
use support::docs::{
    SPREADSHEET, SPREADSHEET_FIELDS, spreadsheet, status_error, unformatted_values, value_range, value_ranges,
    values_appended, values_updated,
};
use support::{answer, answering, body_of, google, invoke, only_request, query_of};

/// The last segment of a path, as it was before it was written into one.
fn decoded(segment: &str) -> String {
    let mut bytes = Vec::new();
    let mut rest = segment.as_bytes();
    while let [first, tail @ ..] = rest {
        match (first, tail) {
            (b'%', [high, low, after @ ..]) => {
                let pair = [*high, *low];
                bytes.push(u8::from_str_radix(std::str::from_utf8(&pair).unwrap(), 16).unwrap());
                rest = after;
            }
            _ => {
                bytes.push(*first);
                rest = tail;
            }
        }
    }
    String::from_utf8(bytes).unwrap()
}

// ── Reading ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn getting_a_spreadsheet_names_the_fields_it_wants_and_reads_no_cell() {
    let (server, socket, key) = answering(200, spreadsheet()).await;
    let found = invoke(
        &socket,
        &key,
        "sheets_spreadsheets.get",
        json!({ "spreadsheet": SPREADSHEET }),
    )
    .await
    .unwrap();
    // Without a field mask Google would also send formats, merges and
    // charts; with one it ignores `includeGridData`, which is never sent.
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "fields": SPREADSHEET_FIELDS })
    );
    for cells in ["sheets.data", "rowData", "includeGridData", "*"] {
        assert!(!SPREADSHEET_FIELDS.contains(cells), "{cells}");
    }
    let sizes: Vec<_> = found["sheets"]
        .as_array()
        .unwrap()
        .iter()
        .map(|sheet| {
            let sheet = &sheet["properties"];
            (
                sheet["title"].as_str().unwrap(),
                sheet["sheetId"].as_i64().unwrap(),
                sheet["gridProperties"]["rowCount"].as_i64(),
                sheet["gridProperties"]["columnCount"].as_i64(),
            )
        })
        .collect();
    assert_eq!(
        sizes,
        [
            ("Sheet1", 0, Some(1000), Some(26)),
            ("Q3 plan/final", 1_837_264_519, Some(200), Some(8)),
            // A sheet that holds a chart has no cells, and so no size.
            ("Chart1", 771_203, None, None),
        ]
    );
}

#[tokio::test]
async fn a_range_is_one_segment_of_the_path_whatever_its_sheet_is_called() {
    for range in [
        "Sheet1!A1:B2",
        "'Q3 plan/final'!A1:B2",
        "'Übersicht 2026 – größe'!A:A",
        "'Jon''s \"data\"'!1:2",
        "'What? #1 & 50%+'!C3",
        "'a/../../drive/v3/files'!A1",
        "計画",
        "  Sheet1!A1  ",
    ] {
        let wanted = range.trim();
        for (name, verb, suffix, input, response) in [
            ("sheets_spreadsheets.values_get", "GET", "", json!({}), value_range()),
            (
                "sheets_spreadsheets.values_update",
                "PUT",
                "",
                json!({ "values": [["x"]], "valueInputOption": "RAW" }),
                values_updated(),
            ),
            (
                "sheets_spreadsheets.values_append",
                "POST",
                ":append",
                json!({ "values": [["x"]], "valueInputOption": "RAW" }),
                values_appended(),
            ),
        ] {
            let (server, socket, key) = answering(200, response).await;
            let mut input = input;
            input["spreadsheet"] = json!(SPREADSHEET);
            input["range"] = json!(range);
            invoke(&socket, &key, name, input)
                .await
                .unwrap_or_else(|e| panic!("{name} {range}: {e}"));
            let request = only_request(&server).await;
            assert_eq!(request.method.as_str(), verb, "{name}");
            let path = request.url.path();
            let segment = path
                .strip_prefix(&format!("/v4/spreadsheets/{SPREADSHEET}/values/"))
                .unwrap_or_else(|| panic!("{name} {range}: {path}"));
            let segment = segment
                .strip_suffix(suffix)
                .unwrap_or_else(|| panic!("{name} {range}: {path}"));
            assert!(
                segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~' | b'%')),
                "{name} {range}: nothing in {segment} can end the segment or begin a query"
            );
            assert_eq!(decoded(segment), wanted, "{name}: the range arrives as it was given");
            assert_eq!(request.url.fragment(), None, "{name} {range}");
            assert_eq!(
                query_of(&request)
                    .as_object()
                    .unwrap()
                    .keys()
                    .filter(|name| *name != "valueInputOption")
                    .count(),
                0,
                "{name} {range}: nothing of the range became a parameter"
            );
        }
    }
    // The one the issue names, to the letter.
    let (server, socket, key) = answering(200, value_range()).await;
    let input = json!({ "spreadsheet": SPREADSHEET, "range": "'Q3 plan/final'!A1:B2" });
    invoke(&socket, &key, "sheets_spreadsheets.values_get", input)
        .await
        .unwrap();
    assert_eq!(
        only_request(&server).await.url.path(),
        format!("/v4/spreadsheets/{SPREADSHEET}/values/%27Q3%20plan%2Ffinal%27%21A1%3AB2")
    );

    // A spreadsheet id is the caller's text too.
    let (server, socket, key) = answering(200, spreadsheet()).await;
    invoke(
        &socket,
        &key,
        "sheets_spreadsheets.get",
        json!({ "spreadsheet": "a/b?c#d" }),
    )
    .await
    .unwrap();
    assert_eq!(only_request(&server).await.url.path(), "/v4/spreadsheets/a%2Fb%3Fc%23d");
    let (server, socket, key) = answering(200, value_ranges()).await;
    let input = json!({ "spreadsheet": "../x", "ranges": ["A1", "B2"] });
    invoke(&socket, &key, "sheets_spreadsheets.values_batch_get", input)
        .await
        .unwrap();
    assert_eq!(
        only_request(&server).await.url.path(),
        "/v4/spreadsheets/..%2Fx/values:batchGet"
    );
}

#[tokio::test]
async fn options_that_are_not_set_are_not_sent_and_those_that_are_go_under_googles_names() {
    let (server, socket, key) = answering(200, value_range()).await;
    let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1:C4" });
    invoke(&socket, &key, "sheets_spreadsheets.values_get", input)
        .await
        .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({}),
        "Google's own defaults apply"
    );

    let (server, socket, key) = answering(200, value_ranges()).await;
    let input = json!({ "spreadsheet": SPREADSHEET, "ranges": ["A1", "B2"], "majorDimension": null });
    invoke(&socket, &key, "sheets_spreadsheets.values_batch_get", input)
        .await
        .unwrap();
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "ranges": ["A1", "B2"] })
    );

    for (options, sent) in [
        (json!({ "majorDimension": "ROWS" }), json!({ "majorDimension": "ROWS" })),
        (
            json!({ "majorDimension": "COLUMNS" }),
            json!({ "majorDimension": "COLUMNS" }),
        ),
        (
            json!({ "valueRenderOption": "FORMATTED_VALUE" }),
            json!({ "valueRenderOption": "FORMATTED_VALUE" }),
        ),
        (
            json!({ "valueRenderOption": "FORMULA" }),
            json!({ "valueRenderOption": "FORMULA" }),
        ),
        (
            json!({ "valueRenderOption": "UNFORMATTED_VALUE", "dateTimeRenderOption": "SERIAL_NUMBER", "majorDimension": "COLUMNS" }),
            json!({ "valueRenderOption": "UNFORMATTED_VALUE", "dateTimeRenderOption": "SERIAL_NUMBER", "majorDimension": "COLUMNS" }),
        ),
        (
            json!({ "dateTimeRenderOption": "FORMATTED_STRING" }),
            json!({ "dateTimeRenderOption": "FORMATTED_STRING" }),
        ),
    ] {
        let (server, socket, key) = answering(200, value_range()).await;
        let mut input = options.clone();
        input["spreadsheet"] = json!(SPREADSHEET);
        input["range"] = json!("Sheet1!A1:C4");
        invoke(&socket, &key, "sheets_spreadsheets.values_get", input)
            .await
            .unwrap();
        assert_eq!(query_of(&only_request(&server).await), sent, "{options}");

        let (server, socket, key) = answering(200, value_ranges()).await;
        let mut input = options.clone();
        input["spreadsheet"] = json!(SPREADSHEET);
        input["ranges"] = json!(["A1", "B2"]);
        invoke(&socket, &key, "sheets_spreadsheets.values_batch_get", input)
            .await
            .unwrap();
        let mut sent = sent;
        sent["ranges"] = json!(["A1", "B2"]);
        assert_eq!(query_of(&only_request(&server).await), sent, "{options}");
    }

    // An option Google does not have is refused here, not passed on.
    let (server, socket, key) = google().await;
    for options in [
        json!({ "valueRenderOption": "formatted_value" }),
        json!({ "valueRenderOption": "VALUE" }),
        json!({ "dateTimeRenderOption": "ISO_8601" }),
        json!({ "majorDimension": "DIMENSION_UNSPECIFIED" }),
        json!({ "majorDimension": "ROWS&valueRenderOption=FORMULA" }),
        json!({ "includeGridData": true }),
    ] {
        let mut input = options.clone();
        input["spreadsheet"] = json!(SPREADSHEET);
        input["range"] = json!("A1");
        let err = invoke(&socket, &key, "sheets_spreadsheets.values_get", input)
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{options}: {err}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn several_ranges_are_one_parameter_sent_once_for_each_in_the_order_given() {
    let ranges = json!(["Sheet1!A1:A4", " 'Q3 plan/final'!B2 ", "'a&b=c'!A1", "Totals"]);
    let answered = json!({
        "spreadsheetId": SPREADSHEET,
        "valueRanges": [
            { "range": "Sheet1!A1:A4", "majorDimension": "ROWS", "values": [["Item"], ["Bolts"]] },
            { "range": "'Q3 plan/final'!B2", "majorDimension": "ROWS" },
            { "range": "'a&b=c'!A1", "majorDimension": "ROWS", "values": [[7]] },
            { "range": "Totals!A1:Z1000", "majorDimension": "ROWS", "values": [["Sum", 52.5]] }
        ]
    });
    let (server, socket, key) = answering(200, answered).await;
    let input = json!({ "spreadsheet": SPREADSHEET, "ranges": ranges });
    let read = invoke(&socket, &key, "sheets_spreadsheets.values_batch_get", input)
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(
        request.url.path(),
        format!("/v4/spreadsheets/{SPREADSHEET}/values:batchGet")
    );
    assert_eq!(
        query_of(&request),
        json!({ "ranges": ["Sheet1!A1:A4", "'Q3 plan/final'!B2", "'a&b=c'!A1", "Totals"] }),
        "each range whole, an `&` in a sheet's name included"
    );
    let read: Vec<_> = read["valueRanges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|range| (range["range"].as_str().unwrap(), range["values"].clone()))
        .collect();
    assert_eq!(
        read,
        [
            ("Sheet1!A1:A4", json!([["Item"], ["Bolts"]])),
            ("'Q3 plan/final'!B2", json!([])),
            ("'a&b=c'!A1", json!([[7]])),
            ("Totals!A1:Z1000", json!([["Sum", 52.5]])),
        ]
    );

    let (server, socket, key) = google().await;
    for ranges in [json!([]), json!([""]), json!(["A1", "  "])] {
        let input = json!({ "spreadsheet": SPREADSHEET, "ranges": ranges });
        let err = invoke(&socket, &key, "sheets_spreadsheets.values_batch_get", input.clone())
            .await
            .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{input}");
        assert_eq!(err.message(), "`ranges` needs at least one range, and none blank");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn cells_of_any_kind_and_rows_of_any_length_are_kept_as_google_sends_them() {
    let values = json!([
        ["Item", "Qty", "In stock", "Updated"],
        ["Bolts", 40, true, 46_307.5],
        [],
        ["Nuts"],
        ["", "", false],
        ["Total", -1.5e-3, "=SUM(B2:B5)", "#DIV/0!"]
    ]);
    let (_server, socket, key) = answering(
        200,
        json!({ "range": "Sheet1!A1:D6", "majorDimension": "ROWS", "values": values }),
    )
    .await;
    let input =
        json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1:D6", "valueRenderOption": "UNFORMATTED_VALUE" });
    let read = invoke(&socket, &key, "sheets_spreadsheets.values_get", input)
        .await
        .unwrap();
    assert_eq!(
        read["values"], values,
        "nothing padded, nothing dropped, no number made a string"
    );
    assert_eq!(read["majorDimension"], "ROWS");

    // A range with nothing in it comes without `values`, and is no error.
    let (_server, socket, key) = answering(200, json!({ "range": "Sheet1!F1:G9", "majorDimension": "ROWS" })).await;
    let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!F1:G9" });
    let empty = invoke(&socket, &key, "sheets_spreadsheets.values_get", input)
        .await
        .unwrap();
    assert_eq!(
        empty,
        json!({ "range": "Sheet1!F1:G9", "majorDimension": "ROWS", "values": [] })
    );
}

// ── Writing ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_write_has_to_say_how_its_values_are_to_be_taken() {
    // The same text is a formula one way and a string the other, so neither
    // is chosen for the caller.
    let (server, socket, key) = google().await;
    for name in ["sheets_spreadsheets.values_update", "sheets_spreadsheets.values_append"] {
        let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1", "values": [["=IMPORTXML(A1)"]] });
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name}");
        assert_eq!(err.message(), "missing field `valueInputOption`", "{name}");
        for option in [
            json!("INPUT_VALUE_OPTION_UNSPECIFIED"),
            json!("raw"),
            json!(""),
            json!(null),
            json!(true),
        ] {
            let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1", "values": [["x"]], "valueInputOption": option });
            let err = invoke(&socket, &key, name, input).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {option}: {err}");
        }
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "nothing was written"
    );

    for (name, response) in [
        ("sheets_spreadsheets.values_update", values_updated()),
        ("sheets_spreadsheets.values_append", values_appended()),
    ] {
        for option in ["RAW", "USER_ENTERED"] {
            let (server, socket, key) = answering(200, response.clone()).await;
            let values = json!([["=SUM(A1:A9)", "2026-10-12", "007", 7, true, null, ""]]);
            let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1", "values": values, "valueInputOption": option });
            invoke(&socket, &key, name, input).await.unwrap();
            let request = only_request(&server).await;
            assert_eq!(query_of(&request), json!({ "valueInputOption": option }), "{name}");
            // The values go as they were given either way: a `null`, which
            // leaves a cell alone, stays apart from an empty string, which
            // empties it. Nothing but `values` is in the body.
            assert_eq!(body_of(&request), json!({ "values": values }), "{name} {option}");
        }
    }
}

#[tokio::test]
async fn an_append_says_where_new_rows_go_only_when_asked_and_values_may_be_columns() {
    for (options, query, body) in [
        (
            json!({}),
            json!({ "valueInputOption": "RAW" }),
            json!({ "values": [["Washers", 12]] }),
        ),
        (
            json!({ "insertDataOption": "INSERT_ROWS" }),
            json!({ "valueInputOption": "RAW", "insertDataOption": "INSERT_ROWS" }),
            json!({ "values": [["Washers", 12]] }),
        ),
        (
            json!({ "insertDataOption": "OVERWRITE", "majorDimension": "COLUMNS" }),
            json!({ "valueInputOption": "RAW", "insertDataOption": "OVERWRITE" }),
            json!({ "values": [["Washers", 12]], "majorDimension": "COLUMNS" }),
        ),
    ] {
        let (server, socket, key) = answering(200, values_appended()).await;
        let mut input = options.clone();
        input["spreadsheet"] = json!(SPREADSHEET);
        input["range"] = json!("Sheet1");
        input["values"] = json!([["Washers", 12]]);
        input["valueInputOption"] = json!("RAW");
        let appended = invoke(&socket, &key, "sheets_spreadsheets.values_append", input)
            .await
            .unwrap();
        let request = only_request(&server).await;
        assert_eq!(request.method.as_str(), "POST");
        assert_eq!(
            request.url.path(),
            format!("/v4/spreadsheets/{SPREADSHEET}/values/Sheet1:append")
        );
        assert_eq!(query_of(&request), query, "{options}");
        assert_eq!(body_of(&request), body, "{options}");
        assert_eq!(appended["tableRange"], "Sheet1!A1:C4");
        assert_eq!(appended["updates"]["updatedRange"], "Sheet1!A5:C5");
    }

    // An update takes columns too, and has no word to say on inserting.
    let (server, socket, key) = answering(200, values_updated()).await;
    let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!B2:B3", "values": [[38, 12]], "valueInputOption": "RAW", "majorDimension": "COLUMNS" });
    invoke(&socket, &key, "sheets_spreadsheets.values_update", input)
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.method.as_str(), "PUT");
    assert_eq!(
        body_of(&request),
        json!({ "values": [[38, 12]], "majorDimension": "COLUMNS" })
    );
    let (server, socket, key) = google().await;
    let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!B2", "values": [[1]], "valueInputOption": "RAW", "insertDataOption": "INSERT_ROWS" });
    let err = invoke(&socket, &key, "sheets_spreadsheets.values_update", input)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_write_with_nothing_to_write_or_to_nowhere_is_refused_before_google_is_called() {
    let (server, socket, key) = google().await;
    let no_cell = "`values` needs at least one cell";
    let not_a_cell = "a cell of `values` is a string, a number, a boolean or null";
    for name in ["sheets_spreadsheets.values_update", "sheets_spreadsheets.values_append"] {
        for (range, values, says) in [
            ("Sheet1!A1", json!([]), no_cell),
            ("Sheet1!A1", json!([[]]), no_cell),
            ("Sheet1!A1", json!([[], []]), no_cell),
            // A cell holds one value. A list or an object in one is a mistake Google would refuse.
            ("Sheet1!A1", json!([["ok", ["CONFIDENTIAL"]]]), not_a_cell),
            ("Sheet1!A1", json!([[{ "formula": "CONFIDENTIAL" }]]), not_a_cell),
            ("", json!([["x"]]), "a range is required"),
            ("  ", json!([["x"]]), "a range is required"),
            ("..", json!([["x"]]), "a range is not valid"),
        ] {
            let input =
                json!({ "spreadsheet": SPREADSHEET, "range": range, "values": values, "valueInputOption": "RAW" });
            let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
            assert_eq!(err.message(), says, "{name} {input}");
            assert!(!format!("{err:?}").contains("CONFIDENTIAL"), "{name}: {err:?}");
        }
        let input = json!({ "spreadsheet": "", "range": "A1", "values": [["x"]], "valueInputOption": "RAW" });
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.message(), "a spreadsheet id is required", "{name}");
        let input = json!({ "spreadsheet": SPREADSHEET, "range": "A1", "valueInputOption": "RAW" });
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.message(), "missing field `values`", "{name}");
    }
    for (name, input, says) in [
        (
            "sheets_spreadsheets.get",
            json!({ "spreadsheet": " " }),
            "a spreadsheet id is required",
        ),
        (
            "sheets_spreadsheets.get",
            json!({ "spreadsheet": ".." }),
            "a spreadsheet id is not valid",
        ),
        (
            "sheets_spreadsheets.values_get",
            json!({ "spreadsheet": SPREADSHEET, "range": "" }),
            "a range is required",
        ),
        (
            "sheets_spreadsheets.values_get",
            json!({ "spreadsheet": "", "range": "A1" }),
            "a spreadsheet id is required",
        ),
        (
            "sheets_spreadsheets.values_batch_get",
            json!({ "spreadsheet": ".", "ranges": ["A1"] }),
            "a spreadsheet id is not valid",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}");
        assert_eq!(err.message(), says, "{name} {input}");
    }
    assert!(
        server.received_requests().await.unwrap().is_empty(),
        "none of these reached Google"
    );
}

#[tokio::test]
async fn rows_are_appended_once_when_google_fails() {
    let (server, socket, key) = google().await;
    Mock::given(any())
        .respond_with(answer(
            503,
            &status_error(503, "UNAVAILABLE", "The service is currently unavailable."),
        ))
        .mount(&server)
        .await;
    let input = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1", "values": [["Washers", 12]], "valueInputOption": "RAW" });
    let err = invoke(&socket, &key, "sheets_spreadsheets.values_append", input)
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Unexpected);
    assert_eq!(
        server.received_requests().await.unwrap().len(),
        1,
        "the rows may have been added, and adding them again would add them twice"
    );
    // `values_update` is not here. On this branch the transport still
    // repeats a PUT after a server error. Writing the same values over the
    // same cells a second time leaves what the first time left; see the guide.
}

// ── What Google answers ──────────────────────────────────────────────────────

#[tokio::test]
async fn a_success_without_what_was_asked_for_is_an_error() {
    let get = json!({ "spreadsheet": SPREADSHEET });
    let one = json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1:C4" });
    let two = json!({ "spreadsheet": SPREADSHEET, "ranges": ["Sheet1!A1:A4", "Sheet1!B1:B4"] });
    let write =
        json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1", "values": [["x"]], "valueInputOption": "RAW" });
    for (name, input, response, says) in [
        (
            "sheets_spreadsheets.get",
            &get,
            json!({}),
            "google answered without a spreadsheet",
        ),
        (
            "sheets_spreadsheets.get",
            &get,
            json!({ "properties": { "title": "Stock" }, "sheets": [] }),
            "google answered without a spreadsheet",
        ),
        (
            "sheets_spreadsheets.values_get",
            &one,
            json!({}),
            "google answered without a range of values",
        ),
        (
            "sheets_spreadsheets.values_get",
            &one,
            json!({ "values": [["Item"]] }),
            "google answered without a range of values",
        ),
        (
            "sheets_spreadsheets.values_batch_get",
            &two,
            json!({}),
            "google answered without the ranges of values",
        ),
        (
            "sheets_spreadsheets.values_batch_get",
            &two,
            json!({ "valueRanges": [{ "range": "Sheet1!A1:A4" }, { "range": "Sheet1!B1:B4" }] }),
            "google answered without the ranges of values",
        ),
        // One answer for two ranges would be paired with the wrong one.
        (
            "sheets_spreadsheets.values_batch_get",
            &two,
            json!({ "spreadsheetId": SPREADSHEET, "valueRanges": [{ "range": "Sheet1!B1:B4", "values": [["Qty"]] }] }),
            "google answered without the ranges of values",
        ),
        (
            "sheets_spreadsheets.values_batch_get",
            &two,
            json!({ "spreadsheetId": SPREADSHEET }),
            "google answered without the ranges of values",
        ),
        (
            "sheets_spreadsheets.values_update",
            &write,
            json!({}),
            "google answered without what it updated",
        ),
        (
            "sheets_spreadsheets.values_update",
            &write,
            json!({ "updatedCells": 1 }),
            "google answered without what it updated",
        ),
        (
            "sheets_spreadsheets.values_append",
            &write,
            json!({}),
            "google answered without what it appended",
        ),
        (
            "sheets_spreadsheets.values_append",
            &write,
            json!({ "updates": { "updatedCells": 1 } }),
            "google answered without what it appended",
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
        assert_eq!(err.message(), says, "{name} {response}");
    }
}

#[tokio::test]
async fn an_answer_that_cannot_be_read_names_the_place_and_never_what_is_in_the_cells() {
    let secret = "CONFIDENTIAL: salary 140k";
    let get = json!({ "spreadsheet": SPREADSHEET });
    let one = json!({ "spreadsheet": SPREADSHEET, "range": "Pay!A1:B2" });
    let two = json!({ "spreadsheet": SPREADSHEET, "ranges": ["Pay!A1", "Pay!B1"] });
    let write = json!({ "spreadsheet": SPREADSHEET, "range": "Pay!A1", "values": [["x"]], "valueInputOption": "RAW" });
    for (name, input, response, says) in [
        (
            "sheets_spreadsheets.get",
            &get,
            json!({ "spreadsheetId": SPREADSHEET, "properties": { "title": secret }, "sheets": [
                { "properties": { "sheetId": 0, "title": secret, "gridProperties": { "rowCount": 10 } } },
                { "properties": { "sheetId": 1, "title": secret, "gridProperties": { "rowCount": secret } } }
            ] }),
            "google sent a spreadsheet that could not be read, at `sheets[1].properties.gridProperties.rowCount`",
        ),
        (
            "sheets_spreadsheets.get",
            &get,
            json!({ "spreadsheetId": SPREADSHEET, "properties": { "title": [secret] } }),
            "google sent a spreadsheet that could not be read, at `properties.title`",
        ),
        (
            "sheets_spreadsheets.values_get",
            &one,
            json!({ "range": "Pay!A1:B2", "values": [[secret, 140_000], secret] }),
            "google sent a range of values that could not be read, at `values[1]`",
        ),
        (
            "sheets_spreadsheets.values_get",
            &one,
            json!({ "range": "Pay!A1:B2", "values": secret }),
            "google sent a range of values that could not be read, at `values`",
        ),
        (
            "sheets_spreadsheets.values_batch_get",
            &two,
            json!({ "spreadsheetId": SPREADSHEET, "valueRanges": [{ "range": "Pay!A1", "values": [[secret]] }, { "range": { "is": secret } }] }),
            "google sent ranges of values that could not be read, at `valueRanges[1].range`",
        ),
        (
            "sheets_spreadsheets.values_update",
            &write,
            json!({ "spreadsheetId": SPREADSHEET, "updatedRange": "Pay!A1", "updatedCells": secret }),
            "google sent what was updated that could not be read, at `updatedCells`",
        ),
        (
            "sheets_spreadsheets.values_append",
            &write,
            json!({ "spreadsheetId": SPREADSHEET, "updates": { "updatedRange": [secret] } }),
            "google sent what was appended that could not be read, at `updates.updatedRange`",
        ),
    ] {
        let (_server, socket, key) = answering(200, response).await;
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name}: {err}");
        assert_eq!(err.message(), says, "{name}");
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
        assert!(!everything.contains("140"), "{name}: {everything}");
    }
}

#[tokio::test]
async fn googles_refusals_of_a_spreadsheet_reach_the_caller() {
    let one = json!({ "spreadsheet": SPREADSHEET, "range": "Nope!A1" });
    let write =
        json!({ "spreadsheet": SPREADSHEET, "range": "Sheet1!A1", "values": [["x"]], "valueInputOption": "RAW" });
    for (name, input, status, word, message, kind, ends) in [
        (
            "sheets_spreadsheets.get",
            json!({ "spreadsheet": SPREADSHEET }),
            404,
            "NOT_FOUND",
            "Requested entity was not found.",
            ErrorKind::NotFound,
            "has no such resource",
        ),
        // A range Google cannot read, such as a sheet that is not there.
        (
            "sheets_spreadsheets.values_get",
            one,
            400,
            "INVALID_ARGUMENT",
            "Unable to parse range: Nope!A1",
            ErrorKind::InvalidInput,
            "Unable to parse range: Nope!A1",
        ),
        // The provider's default scopes do not cover Sheets. A token
        // without the scope is refused in these words.
        (
            "sheets_spreadsheets.get",
            json!({ "spreadsheet": SPREADSHEET }),
            403,
            "PERMISSION_DENIED",
            "Request had insufficient authentication scopes.",
            ErrorKind::AccessDenied,
            "Request had insufficient authentication scopes.",
        ),
        // A sheet or a range someone protected.
        (
            "sheets_spreadsheets.values_update",
            write.clone(),
            400,
            "INVALID_ARGUMENT",
            "You are trying to edit a protected cell or object.",
            ErrorKind::InvalidInput,
            "You are trying to edit a protected cell or object.",
        ),
        (
            "sheets_spreadsheets.values_append",
            write,
            403,
            "PERMISSION_DENIED",
            "The caller does not have permission",
            ErrorKind::AccessDenied,
            "The caller does not have permission",
        ),
    ] {
        let (server, socket, key) = google().await;
        Mock::given(any())
            .respond_with(answer(status, &status_error(status, word, message)))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{name}: {err}");
        assert!(err.message().ends_with(ends), "{name}: {}", err.message());
    }
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let google = Google::with_spec(point_at(provider(), &server));
    let (socket, key) = connect(Arc::new(google.clone()), "ya29.good").await;
    let connection = socket.connection(key).await.unwrap();
    let sheets = google.sheets_spreadsheets(&connection);
    let answers = |verb: &'static str, body: Value| {
        let server = &server;
        async move {
            server.reset().await;
            Mock::given(method(verb))
                .respond_with(answer(200, &body))
                .mount(server)
                .await;
        }
    };

    answers("GET", spreadsheet()).await;
    let found = sheets.get(SPREADSHEET).await.unwrap();
    assert_eq!(found.spreadsheet_id, SPREADSHEET);
    assert_eq!(found.properties.title, "Stock");
    assert_eq!(found.properties.locale.as_deref(), Some("en_GB"));
    assert_eq!(found.properties.time_zone.as_deref(), Some("Europe/London"));
    let second = &found.sheets[1].properties;
    assert_eq!(
        (second.sheet_id, second.title.as_str(), second.index),
        (1_837_264_519, "Q3 plan/final", 1)
    );
    assert!(second.hidden);
    let size = second.grid_properties.as_ref().unwrap();
    assert_eq!((size.row_count, size.column_count), (200, 8));
    assert_eq!(
        found.sheets[0]
            .properties
            .grid_properties
            .as_ref()
            .unwrap()
            .frozen_row_count,
        1
    );
    assert_eq!(found.sheets[2].properties.grid_properties, None);
    assert_eq!(found.sheets[2].properties.sheet_type.as_deref(), Some("OBJECT"));
    assert_eq!(
        only_request(&server).await.url.path(),
        format!("/v4/spreadsheets/{SPREADSHEET}")
    );

    // A sheet's name, as `get` returned it, quoted into a range.
    let range = format!("'{}'!A1:C4", second.title);
    answers("GET", unformatted_values()).await;
    let options = SheetsGetValues {
        value_render_option: Some(SheetsValueRenderOption::UnformattedValue),
        date_time_render_option: Some(SheetsDateTimeRenderOption::FormattedString),
        major_dimension: Some(SheetsDimension::Rows),
    };
    let read = sheets.values_get(SPREADSHEET, &range, options.clone()).await.unwrap();
    assert_eq!(read.values[1], [json!("Bolts"), json!(40), json!(true)]);
    assert_eq!(read.values[2], [json!("Nuts")], "a row cut short stays short");
    assert_eq!(read.major_dimension.as_deref(), Some("ROWS"));
    let request = only_request(&server).await;
    assert_eq!(
        request.url.path(),
        format!("/v4/spreadsheets/{SPREADSHEET}/values/%27Q3%20plan%2Ffinal%27%21A1%3AC4")
    );
    assert_eq!(
        query_of(&request),
        json!({ "valueRenderOption": "UNFORMATTED_VALUE", "dateTimeRenderOption": "FORMATTED_STRING", "majorDimension": "ROWS" })
    );

    answers("GET", value_ranges()).await;
    let several = sheets
        .values_batch_get(
            SPREADSHEET,
            &["Sheet1!A1:A4", "'Q3 plan/final'!B2"],
            SheetsGetValues::default(),
        )
        .await
        .unwrap();
    assert_eq!(several.value_ranges.len(), 2);
    assert_eq!(
        several.value_ranges[0].values,
        [[json!("Item"), json!("Bolts"), json!("Nuts")]]
    );
    assert!(several.value_ranges[1].values.is_empty());
    assert_eq!(
        query_of(&only_request(&server).await),
        json!({ "ranges": ["Sheet1!A1:A4", "'Q3 plan/final'!B2"] })
    );

    answers("PUT", values_updated()).await;
    let updated = sheets
        .values_update(
            SPREADSHEET,
            "Sheet1!A2:C2",
            SheetsUpdateValues::raw(vec![vec![json!("Bolts"), json!(38), json!(true)]]),
        )
        .await
        .unwrap();
    assert_eq!(updated.updated_range.as_deref(), Some("Sheet1!A2:C2"));
    assert_eq!(
        (updated.updated_rows, updated.updated_columns, updated.updated_cells),
        (1, 3, 3)
    );
    let request = only_request(&server).await;
    assert_eq!(query_of(&request), json!({ "valueInputOption": "RAW" }));
    assert_eq!(body_of(&request), json!({ "values": [["Bolts", 38, true]] }));

    answers("POST", values_appended()).await;
    let append = SheetsAppendValues {
        insert_data_option: Some(SheetsInsertDataOption::InsertRows),
        ..SheetsAppendValues::user_entered(vec![vec![json!("Washers"), json!("=6*2")]])
    };
    assert_eq!(append.value_input_option, SheetsValueInputOption::UserEntered);
    let appended = sheets.values_append(SPREADSHEET, "Sheet1!A:C", append).await.unwrap();
    assert_eq!(appended.table_range.as_deref(), Some("Sheet1!A1:C4"));
    assert_eq!(appended.updates.updated_range.as_deref(), Some("Sheet1!A5:C5"));
    assert_eq!(appended.updates.updated_cells, 3);
    let request = only_request(&server).await;
    assert_eq!(
        request.url.path(),
        format!("/v4/spreadsheets/{SPREADSHEET}/values/Sheet1%21A%3AC:append")
    );
    assert_eq!(
        query_of(&request),
        json!({ "valueInputOption": "USER_ENTERED", "insertDataOption": "INSERT_ROWS" })
    );
    assert_eq!(body_of(&request), json!({ "values": [["Washers", "=6*2"]] }));
    assert_eq!(
        SheetsUpdateValues::user_entered(vec![]).value_input_option,
        SheetsValueInputOption::UserEntered
    );
    assert_eq!(
        SheetsAppendValues::raw(vec![]).value_input_option,
        SheetsValueInputOption::Raw
    );
}
