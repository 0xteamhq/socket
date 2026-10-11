//! Google Drive against a local server that answers as Google does.
//!
//! What every operation sends and returns is in the table in `operations.rs`.
//! This file holds what is particular to Drive.

use std::sync::Arc;

use serde_json::{Map, Value, json};
use socketkit_core::{ConnectionKey, ErrorKind, Integration, Socket};
use socketkit_google::models::{
    DriveCopyFile, DriveCreateFolder, DriveExport, DriveExportFormat, DriveListFiles, Paging,
};
use socketkit_google::{Google, provider};
use socketkit_testkit::wiremock::matchers::{any, method, path};
use socketkit_testkit::wiremock::{Mock, MockServer, Request, ResponseTemplate};
use socketkit_testkit::{connect, point_at};

mod support;
use support::drive::{
    ARCHIVE, DOC, DRIVES_FIELDS, FILES_FIELDS, PERMISSIONS_FIELDS, PLANS, SHARED_DRIVE, doc, doc_with, domain_reader,
    drives, files, folder, of_file, permissions, shared_drive, shared_sheet, writer,
};
use support::{TOKEN, answer, answering, body_of, google, google_error, invoke, only_request, query_of};

/// The id Google gives the top of the account's My Drive.
const ROOT_ID: &str = "0AMyDriveRootId9PVA";

/// Every operation that names a file, with an input that is complete but for the file.
fn operations_on_a_file() -> Vec<(&'static str, Value, &'static str)> {
    vec![
        ("drive_files.get", json!({}), ""),
        ("drive_files.export", json!({ "mimeType": "text/plain" }), "/export"),
        ("drive_files.permissions", json!({}), "/permissions"),
        ("drive_files.copy", json!({}), "/copy"),
        ("drive_files.move_to", json!({ "folder": ARCHIVE }), ""),
        ("drive_files.rename", json!({ "name": "Q4 plan (final)" }), ""),
        ("drive_files.trash", json!({}), ""),
    ]
}

fn with(mut input: Value, name: &str, value: Value) -> Value {
    input[name] = value;
    input
}

// ── What Drive is asked for ──────────────────────────────────────────────────

/// The names in a `fields` parameter, each with the names in its brackets:
/// `a,b(c,d)` is `{ "a": {}, "b": { "c": {}, "d": {} } }`.
fn asked(fields: &str) -> Value {
    fn level(rest: &mut std::str::Chars<'_>) -> Value {
        let mut names = Map::new();
        let mut name = String::new();
        while let Some(c) = rest.next() {
            match c {
                '(' => {
                    let inside = level(rest);
                    names.insert(std::mem::take(&mut name), inside);
                }
                ',' | ')' => {
                    if !name.is_empty() {
                        names.insert(std::mem::take(&mut name), json!({}));
                    }
                    if c == ')' {
                        break;
                    }
                }
                c => name.push(c),
            }
        }
        if !name.is_empty() {
            names.insert(name, json!({}));
        }
        Value::Object(names)
    }
    level(&mut fields.chars())
}

/// The fields of the type the schema at `node` describes, each with the
/// fields of what it holds, in the shape [`asked`] gives.
fn held(root: &Value, node: &Value) -> Value {
    // The schema of an object or a list, behind a reference or beside `null`.
    let mut node = node;
    for _ in 0..8 {
        let named = node["$ref"].as_str().and_then(|name| name.strip_prefix("#/$defs/"));
        let optional = node["anyOf"]
            .as_array()
            .and_then(|arms| arms.iter().find(|arm| arm["type"] != "null"));
        match (named, optional) {
            (Some(name), _) => node = &root["$defs"][name],
            (None, Some(arm)) => node = arm,
            (None, None) => break,
        }
    }
    if let Some(items) = node.get("items") {
        return held(root, items);
    }
    let fields = node["properties"].as_object().into_iter().flatten();
    Value::Object(
        fields
            .map(|(name, schema)| (name.clone(), held(root, schema)))
            .collect(),
    )
}

#[tokio::test]
async fn drive_is_asked_for_exactly_the_fields_the_models_hold() {
    assert_eq!(
        asked("a,b(c,d(e)),f"),
        json!({ "a": {}, "b": { "c": {}, "d": { "e": {} } }, "f": {} }),
        "the reading of `fields` that the rest of this test relies on"
    );

    // Drive returns nothing it was not asked for by name, and what is asked
    // for and cannot be held is thrown away. So for every operation that
    // names fields, the names that reach Google are compared with the fields
    // of what the operation says it returns: a field added to a model and
    // not asked for, or asked for and held nowhere, fails here.
    let operations = Google::new().operations();
    let returned = |name: &str| {
        let operation = operations.iter().find(|o| o.name == format!("google.{name}"));
        operation.unwrap_or_else(|| panic!("{name}")).output_schema.clone()
    };
    for (name, input, list) in [
        ("drive_files.get", json!({ "file": DOC }), None),
        ("drive_files.list", json!({}), Some("files")),
        ("drive_files.permissions", json!({ "file": DOC }), Some("permissions")),
        ("drive_shared_drives.list", json!({}), Some("drives")),
        ("drive_files.create_folder", json!({ "name": "Plans" }), None),
        ("drive_files.copy", json!({ "file": DOC }), None),
        // The read a move begins with. What the move itself asks for is in the table.
        ("drive_files.move_to", json!({ "file": DOC, "folder": ARCHIVE }), None),
        ("drive_files.rename", json!({ "file": DOC, "name": "x" }), None),
        ("drive_files.trash", json!({ "file": DOC }), None),
    ] {
        // Only the request matters here, so the answer is one nothing can be read from.
        let (server, socket, key) = answering(200, json!({})).await;
        invoke(&socket, &key, name, input).await.unwrap_err();
        let query = query_of(&only_request(&server).await);
        let schema = returned(name);
        let holds = match list {
            None => held(&schema, &schema),
            // A page is asked for with what says it is one, and where the next begins.
            Some(list) => json!({
                "kind": {}, "nextPageToken": {}, list: held(&schema, &schema["properties"]["items"])
            }),
        };
        assert_ne!(holds, json!({}), "{name}: the schema was read");
        assert_eq!(asked(query["fields"].as_str().unwrap()), holds, "{name}");
    }
}

#[tokio::test]
async fn what_google_leaves_out_of_a_file_does_not_stop_it_from_being_read() {
    // Nothing but the id: a file the account can barely see.
    let (_server, socket, key) = answering(200, json!({ "id": DOC })).await;
    let bare = invoke(&socket, &key, "drive_files.get", json!({ "file": DOC }))
        .await
        .unwrap();
    assert_eq!(bare["name"], "");
    assert_eq!(bare["parents"], json!([]), "no folder the account can see");
    assert_eq!(bare["owners"], json!([]));
    assert_eq!(bare["trashed"], false);
    assert_eq!(bare["size"], json!(null));

    // What is in a shared drive has no owners, and names its drive.
    let (_server, socket, key) = answering(200, shared_sheet()).await;
    let sheet = invoke(&socket, &key, "drive_files.get", json!({ "file": "x" }))
        .await
        .unwrap();
    assert_eq!(sheet["driveId"], SHARED_DRIVE);
    assert_eq!(sheet["owners"], json!([]));
    assert_eq!(sheet["size"], "4096", "a number in a string, as Google writes it");

    // A shortcut says what it points to, and has no size.
    let shortcut = json!({
        "id": "1Shortcut_aBcDeFgHiJkLmNoPqRsTuV", "name": "Q4 plan", "parents": [ARCHIVE],
        "mimeType": "application/vnd.google-apps.shortcut",
        "shortcutDetails": { "targetId": DOC, "targetMimeType": "application/vnd.google-apps.document" }
    });
    let (_server, socket, key) = answering(200, shortcut).await;
    let read = invoke(&socket, &key, "drive_files.get", json!({ "file": "x" }))
        .await
        .unwrap();
    assert_eq!(
        read["shortcutDetails"],
        json!({ "targetId": DOC, "targetMimeType": "application/vnd.google-apps.document", "targetResourceKey": null })
    );
}

// ── Listing ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_list_sends_only_what_was_set_and_always_reaches_into_shared_drives() {
    for (input, set) in [
        (json!({}), json!({})),
        // A search, a sort or a drive with nothing in it is none at all.
        (json!({ "q": "  ", "orderBy": "", "driveId": " " }), json!({})),
        (json!({ "q": "trashed = false" }), json!({ "q": "trashed = false" })),
        (
            json!({ "orderBy": "folder,modifiedTime desc" }),
            json!({ "orderBy": "folder,modifiedTime desc" }),
        ),
        // One shared drive is searched only when Google is told it is the body of files meant.
        (
            json!({ "driveId": SHARED_DRIVE, "q": "name contains 'budget'" }),
            json!({ "corpora": "drive", "driveId": SHARED_DRIVE, "q": "name contains 'budget'" }),
        ),
    ] {
        let (server, socket, key) = answering(200, files(json!([shared_sheet()]))).await;
        let page = invoke(&socket, &key, "drive_files.list", input.clone()).await.unwrap();
        assert_eq!(page["items"][0]["driveId"], SHARED_DRIVE);
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), "/drive/v3/files");
        let mut expected = json!({
            "fields": FILES_FIELDS, "supportsAllDrives": "true", "includeItemsFromAllDrives": "true"
        });
        for (name, value) in set.as_object().unwrap() {
            expected[name] = value.clone();
        }
        assert_eq!(query_of(&request), expected, "{input}");
    }
}

#[tokio::test]
async fn a_search_reaches_google_exactly_as_it_was_written() {
    for q in [
        "name = 'hello'",
        "fullText contains '\"hello world\"'",
        // A quote and a backslash inside a value, escaped the way Google asks.
        r"name contains 'quinn\'s paper\\essay'",
        "'1FolderOfPlans_aBcDeFgHiJkLmNoPq' in parents and mimeType != 'application/vnd.google-apps.folder'",
        "modifiedTime > '2026-10-01T00:00:00' and 'ada+drive@example.test' in owners",
        // Nothing in a search can become a parameter of its own.
        "name contains 'a&supportsAllDrives=false&fields=*#x' or name contains '100% + more'",
    ] {
        let (server, socket, key) = answering(200, files(json!([]))).await;
        invoke(&socket, &key, "drive_files.list", json!({ "q": q }))
            .await
            .unwrap();
        let query = query_of(&only_request(&server).await);
        assert_eq!(query["q"], q);
        assert_eq!(query["supportsAllDrives"], "true", "{q}");
        assert_eq!(query["fields"], FILES_FIELDS, "{q}");
    }
}

#[tokio::test]
async fn every_list_is_paged_under_googles_names_and_within_googles_limits() {
    for (name, input, most, page) in [
        ("drive_files.list", json!({}), 1000, files(json!([doc()]))),
        (
            "drive_files.permissions",
            json!({ "file": DOC }),
            100,
            permissions(json!([writer()])),
        ),
        (
            "drive_shared_drives.list",
            json!({}),
            100,
            drives(json!([shared_drive()])),
        ),
    ] {
        // A page in the middle: the cursor goes out as Google's token, and Google's comes back as the cursor.
        let (server, socket, key) = answering(200, with(page.clone(), "nextPageToken", json!("~!!~next"))).await;
        let middle = with(with(input.clone(), "cursor", json!("~!!~here")), "limit", json!(most));
        let got = invoke(&socket, &key, name, middle).await.unwrap();
        assert_eq!(got["next_cursor"], "~!!~next", "{name}");
        assert_eq!(got["items"].as_array().unwrap().len(), 1, "{name}");
        let query = query_of(&only_request(&server).await);
        assert_eq!(query["pageToken"], "~!!~here", "{name}");
        assert_eq!(query["pageSize"], most.to_string(), "{name}");

        // The last page has no cursor, and with no paging asked for none is sent.
        let (server, socket, key) = answering(200, page.clone()).await;
        let got = invoke(&socket, &key, name, input.clone()).await.unwrap();
        assert_eq!(got["next_cursor"], json!(null), "{name}");
        let query = query_of(&only_request(&server).await);
        assert_eq!(query.get("pageToken"), None, "{name}");
        assert_eq!(query.get("pageSize"), None, "{name}: Google's own page size applies");

        // A size Google would coerce or refuse is refused here, and says what is allowed.
        let (server, socket, key) = answering(200, page.clone()).await;
        for limit in [0, most + 1] {
            let err = invoke(&socket, &key, name, with(input.clone(), "limit", json!(limit)))
                .await
                .unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {limit}");
            assert!(
                err.message().contains(&format!("from 1 to {most}")),
                "{name}: {}",
                err.message()
            );
        }
        assert!(server.received_requests().await.unwrap().is_empty(), "{name}");
    }
}

#[tokio::test]
async fn an_empty_list_is_empty_and_an_answer_that_is_not_the_list_is_an_error() {
    let secret = "CONFIDENTIAL merger plan";
    for (name, input, kind) in [
        ("drive_files.list", json!({}), "drive#fileList"),
        (
            "drive_files.permissions",
            json!({ "file": DOC }),
            "drive#permissionList",
        ),
        ("drive_shared_drives.list", json!({}), "drive#driveList"),
    ] {
        // Google leaves an empty list out of its answer altogether.
        let (_server, socket, key) = answering(200, json!({ "kind": kind })).await;
        let page = invoke(&socket, &key, name, input.clone()).await.unwrap();
        assert_eq!(page, json!({ "items": [], "next_cursor": null }), "{name}");

        for other in [
            json!({}),
            json!({ "kind": "drive#file", "id": DOC, "name": secret }),
            // Lists that do not say what they are lists of.
            json!({ "files": [{ "name": secret }], "permissions": [{ "displayName": secret }], "drives": [{ "name": secret }] }),
            json!([{ "kind": kind, "name": secret }]),
        ] {
            let (_server, socket, key) = answering(200, other.clone()).await;
            let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
            assert_eq!(err.kind(), ErrorKind::Decode, "{name} {other}: {err}");
            let everything = format!("{err} {err:?} {:?}", err.to_wire());
            assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
        }
    }
}

// ── Ids ──────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn a_file_id_stays_one_segment_of_the_path_whatever_it_contains() {
    for (id, written) in [
        ("a/b", "a%2Fb"),
        ("../about", "..%2Fabout"),
        ("a?fields=*", "a%3Ffields%3D%2A"),
        ("a#b", "a%23b"),
        ("a b", "a%20b"),
        // Space around an id is not part of it.
        (" 1AbC-dEf_9 ", "1AbC-dEf_9"),
    ] {
        for (name, input, after) in operations_on_a_file() {
            let (server, socket, key) = answering(200, doc()).await;
            // What comes back is beside the point: a list is not a file. Where the request went is the point.
            let _ = invoke(&socket, &key, name, with(input, "file", json!(id))).await;
            let received = server.received_requests().await.unwrap();
            assert!(!received.is_empty(), "{name} {id}");
            for request in &received {
                assert_eq!(
                    request.url.path(),
                    format!("/drive/v3/files/{written}{after}"),
                    "{name} {id}"
                );
                assert_eq!(request.url.fragment(), None, "{name} {id}");
            }
        }
    }
}

#[tokio::test]
async fn an_id_that_names_no_file_is_refused_before_google_is_called() {
    let (server, socket, key) = answering(200, doc()).await;
    for id in ["", "   ", ".", ".."] {
        for (name, input, _) in operations_on_a_file() {
            let err = invoke(&socket, &key, name, with(input, "file", json!(id)))
                .await
                .unwrap_err();
            assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {id:?}");
            assert!(err.message().contains("a file id"), "{name} {id:?}: {}", err.message());
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

// ── Export ───────────────────────────────────────────────────────────────────

/// A server that answers every request with `body` as text of type `content_type`.
async fn answering_text(content_type: &str, body: impl Into<Vec<u8>>) -> (MockServer, Socket, ConnectionKey) {
    let (server, socket, key) = google().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_raw(body.into(), content_type))
        .mount(&server)
        .await;
    (server, socket, key)
}

fn export(mime_type: &str) -> Value {
    json!({ "file": DOC, "mimeType": mime_type })
}

#[tokio::test]
async fn an_export_asks_for_one_of_three_text_formats_and_refuses_every_other() {
    for (format, text) in [
        ("text/plain", "Q4 plan\r\n\r\nShip the importer by November.\r\n"),
        ("text/markdown", "# Q4 plan\n\nShip the **importer** by November.\n"),
        ("text/csv", "Team,Budget\r\nPlatform,\"1,200\"\r\n"),
    ] {
        let (server, socket, key) = answering_text(format, text).await;
        let got = invoke(&socket, &key, "drive_files.export", export(format))
            .await
            .unwrap();
        assert_eq!(got, json!({ "mimeType": format, "text": text }));
        let request = only_request(&server).await;
        assert_eq!(request.url.path(), format!("/drive/v3/files/{DOC}/export"));
        assert_eq!(
            query_of(&request),
            json!({ "mimeType": format }),
            "the format, and nothing else"
        );
    }

    // Google exports to much else, all of it bytes or markup. None of it is offered, and none reaches Google.
    let (server, socket, key) = answering_text("application/pdf", "%PDF-1.7").await;
    for other in [
        json!("application/pdf"),
        json!("application/vnd.openxmlformats-officedocument.wordprocessingml.document"),
        json!("application/zip"),
        json!("image/png"),
        json!("text/html"),
        json!("text/tab-separated-values"),
        json!("TEXT/CSV"),
        json!("text/csv&alt=media"),
        json!("csv"),
        json!(""),
        json!(null),
    ] {
        let err = invoke(
            &socket,
            &key,
            "drive_files.export",
            with(export(""), "mimeType", other.clone()),
        )
        .await
        .unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{other}");
        assert!(err.message().contains("mimeType"), "{other}: {}", err.message());
    }
    let err = invoke(&socket, &key, "drive_files.export", json!({ "file": DOC }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput);
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn an_export_is_the_documents_text_from_its_first_character_to_its_last() {
    // Google begins a Doc's plain text with a byte order mark. It is not
    // part of the document; a mark anywhere else is.
    let (_server, socket, key) = answering_text("text/plain", "\u{feff}Q4 plan\r\n\u{feff}still here").await;
    let got = invoke(&socket, &key, "drive_files.export", export("text/plain"))
        .await
        .unwrap();
    assert_eq!(got["text"], "Q4 plan\r\n\u{feff}still here");

    // Text in any script is kept as it is.
    let (_server, socket, key) = answering_text("text/markdown", "# 計画\n\nnaïve café — 10 €\n").await;
    let got = invoke(&socket, &key, "drive_files.export", export("text/markdown"))
        .await
        .unwrap();
    assert_eq!(got["text"], "# 計画\n\nnaïve café — 10 €\n");

    // An empty sheet is exported as nothing at all. That is its text, not a failure.
    let (_server, socket, key) = answering_text("text/csv", "").await;
    let got = invoke(&socket, &key, "drive_files.export", export("text/csv"))
        .await
        .unwrap();
    assert_eq!(got, json!({ "mimeType": "text/csv", "text": "" }));

    // Bytes that are not text are never passed off as text.
    let (_server, socket, key) = answering_text("text/plain", vec![0xff, 0xfe, 0x00, 0x51]).await;
    let err = invoke(&socket, &key, "drive_files.export", export("text/plain"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
}

#[tokio::test]
async fn an_export_google_refuses_says_why_in_words_a_caller_can_act_on() {
    let too_large = "this file is too large to export: the limit is 10 MB of exported content";
    let not_a_document = "this file is not a Google document, so there is nothing to export";
    for (response, kind, says) in [
        // Google reports both of these as a 403, which would otherwise read as a missing permission.
        (
            google_error(403, "exportSizeLimitExceeded", "This file is too large to be exported."),
            ErrorKind::InvalidInput,
            too_large,
        ),
        // The reason says it, however Google words the message.
        (
            google_error(403, "exportSizeLimitExceeded", "The export exceeds the maximum size."),
            ErrorKind::InvalidInput,
            too_large,
        ),
        (
            google_error(403, "fileNotExportable", "Export only supports Docs Editors files."),
            ErrorKind::InvalidInput,
            not_a_document,
        ),
        // Every other refusal is Google's to explain, and arrives as it was.
        (
            google_error(
                403,
                "insufficientFilePermissions",
                "The user does not have sufficient permissions for file 1AbC.",
            ),
            ErrorKind::AccessDenied,
            "The user does not have sufficient permissions for file 1AbC.",
        ),
        (
            google_error(
                403,
                "fileNotExportable",
                "Google Vids does not support files.export. Use files.download with Vids files.",
            ),
            ErrorKind::AccessDenied,
            "Google Vids does not support files.export.",
        ),
        // A Sheet asked for as Markdown, or a Doc as CSV.
        (
            google_error(400, "badRequest", "The requested conversion is not supported."),
            ErrorKind::InvalidInput,
            "The requested conversion is not supported.",
        ),
        (
            google_error(404, "notFound", "File not found: 1AbC."),
            ErrorKind::NotFound,
            "has no such resource",
        ),
    ] {
        let (server, socket, key) = google().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let err = invoke(&socket, &key, "drive_files.export", export("text/markdown"))
            .await
            .unwrap_err();
        assert_eq!(err.kind(), kind, "{err}");
        assert!(err.message().contains(says), "{}", err.message());
        assert_eq!(err.provider().map(|p| p.as_str()), Some("google"), "{err}");
        assert_eq!(server.received_requests().await.unwrap().len(), 1, "{err}");
    }
}

#[tokio::test]
async fn an_export_too_large_to_read_is_reported_as_too_large_to_export() {
    // One byte more than the most the transport reads of any answer.
    let (server, socket, key) = answering_text("text/csv", vec![b'7'; 10 * 1024 * 1024 + 1]).await;
    let err = invoke(&socket, &key, "drive_files.export", export("text/csv"))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::InvalidInput, "{err}");
    assert_eq!(
        err.message(),
        "this file is too large to export: the limit is 10 MB of exported content"
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    // Only an export is told so. Anything else that large is what it was: an answer that could not be read.
    let (_server, socket, key) = answering_text("application/json", vec![b' '; 10 * 1024 * 1024 + 1]).await;
    let err = invoke(&socket, &key, "drive_files.get", json!({ "file": DOC }))
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode, "{err}");
}

// ── Who can see a file ───────────────────────────────────────────────────────

#[tokio::test]
async fn permissions_say_who_can_see_a_file_and_where_the_access_comes_from() {
    let anyone = json!({ "id": "anyoneWithLink", "type": "anyone", "role": "reader", "allowFileDiscovery": false });
    let gone = json!({ "id": "07770000000000000001", "type": "user", "role": "commenter", "deleted": true });
    let guest = json!({
        "id": "07770000000000000002", "type": "group", "role": "reader", "emailAddress": "auditors@example.test",
        "displayName": "Auditors", "expirationTime": "2026-12-31T00:00:00.000Z",
        "permissionDetails": [
            { "permissionType": "member", "inheritedFrom": SHARED_DRIVE, "role": "reader", "inherited": true },
            { "permissionType": "file", "role": "commenter", "inherited": false }
        ]
    });
    let answered = permissions(json!([writer(), domain_reader(), anyone, gone, guest]));
    let (server, socket, key) = answering(200, answered).await;
    let page = invoke(&socket, &key, "drive_files.permissions", json!({ "file": DOC }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(request.url.path(), format!("/drive/v3/files/{DOC}/permissions"));
    assert_eq!(
        query_of(&request),
        json!({ "fields": PERMISSIONS_FIELDS, "supportsAllDrives": "true" })
    );

    let who = page["items"].as_array().unwrap();
    assert_eq!(who.len(), 5);
    // A person, by a grant on the file itself.
    assert_eq!(
        (&who[0]["type"], &who[0]["role"], &who[0]["emailAddress"]),
        (&json!("user"), &json!("writer"), &json!("grace@example.test"))
    );
    assert_eq!(who[0]["permissionDetails"][0]["inherited"], false);
    // A whole domain, by way of the shared drive.
    assert_eq!(who[1]["domain"], "example.test");
    assert_eq!(who[1]["emailAddress"], json!(null));
    assert_eq!(who[1]["permissionDetails"][0]["inheritedFrom"], SHARED_DRIVE);
    assert_eq!(who[1]["permissionDetails"][0]["inherited"], true);
    // Anyone who has the link, and nobody who only searches.
    assert_eq!(who[2]["type"], "anyone");
    assert_eq!(who[2]["allowFileDiscovery"], false);
    assert_eq!(who[2]["displayName"], json!(null));
    assert_eq!(who[2]["permissionDetails"], json!([]));
    // An account that no longer exists still holds its grant.
    assert_eq!(who[3]["deleted"], true);
    assert_eq!(who[0]["deleted"], false);
    // A grant with an end, from two sources.
    assert_eq!(who[4]["expirationTime"], "2026-12-31T00:00:00.000Z");
    assert_eq!(who[4]["permissionDetails"].as_array().unwrap().len(), 2);
    assert_eq!(who[4]["permissionDetails"][1]["role"], "commenter");
}

// ── Moving ───────────────────────────────────────────────────────────────────

/// What a move did: what it returned, the reads it made and the changes it sent.
struct Moved {
    result: socketkit_core::Result<Value>,
    reads: Vec<Request>,
    changes: Vec<Request>,
}

/// Moves the Doc into `folder` on a server where it is as `current` says.
/// A change is answered with the Doc in `folder`.
async fn moving(current: Value, folder: &str) -> Moved {
    let (server, socket, key) = google().await;
    Mock::given(method("GET"))
        .and(path("/drive/v3/files/root"))
        .respond_with(answer(
            200,
            &json!({ "id": ROOT_ID, "name": "My Drive", "mimeType": "application/vnd.google-apps.folder" }),
        ))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path(format!("/drive/v3/files/{DOC}")))
        .respond_with(answer(200, &current))
        .mount(&server)
        .await;
    let after = match folder {
        "root" => ROOT_ID,
        folder => folder,
    };
    Mock::given(method("PATCH"))
        .and(path(format!("/drive/v3/files/{DOC}")))
        .respond_with(answer(200, &doc_with(json!({ "parents": [after] }))))
        .mount(&server)
        .await;
    let input = json!({ "file": DOC, "folder": folder });
    let result = invoke(&socket, &key, "drive_files.move_to", input).await;
    let (reads, changes) = server
        .received_requests()
        .await
        .unwrap()
        .into_iter()
        .partition(|request| request.method.as_str() == "GET");
    Moved { result, reads, changes }
}

#[tokio::test]
async fn a_move_reads_the_file_then_adds_the_new_parent_and_takes_the_old_one_away() {
    let moved = moving(doc(), ARCHIVE).await;
    assert_eq!(moved.result.unwrap()["parents"], json!([ARCHIVE]));

    // The read reaches into shared drives too, or a file in one would seem not to exist.
    assert_eq!(moved.reads.len(), 1);
    assert_eq!(moved.reads[0].url.path(), format!("/drive/v3/files/{DOC}"));
    assert_eq!(query_of(&moved.reads[0]), of_file());

    assert_eq!(moved.changes.len(), 1);
    let change = &moved.changes[0];
    assert_eq!(
        query_of(change),
        json!({
            "addParents": ARCHIVE, "removeParents": PLANS,
            "fields": of_file()["fields"], "supportsAllDrives": "true"
        })
    );
    // Nothing about the file itself is changed, and the request still states a length.
    assert_eq!(body_of(change), json!({}));
    assert_eq!(change.headers.get("content-length").unwrap(), "2");
}

#[tokio::test]
async fn a_move_into_the_folder_a_file_is_already_in_changes_nothing() {
    let moved = moving(doc(), PLANS).await;
    assert_eq!(moved.result.unwrap()["parents"], json!([PLANS]), "the file as it is");
    assert_eq!(moved.reads.len(), 1);
    assert!(moved.changes.is_empty(), "nothing to add and nothing to take away");
}

#[tokio::test]
async fn a_move_of_a_file_whose_folder_cannot_be_seen_only_adds_the_new_parent() {
    // Shared from someone else's Drive: Google names no parent, so there is
    // none to take away. Google moves the file if the account may, and
    // refuses if it may not.
    let mut shared = doc();
    shared.as_object_mut().unwrap().remove("parents");
    let moved = moving(shared, ARCHIVE).await;
    moved.result.unwrap();
    assert_eq!(moved.changes.len(), 1);
    let query = query_of(&moved.changes[0]);
    assert_eq!(query["addParents"], ARCHIVE);
    assert_eq!(query.get("removeParents"), None);
    assert_eq!(query["supportsAllDrives"], "true");
}

#[tokio::test]
async fn a_move_leaves_a_file_in_the_one_folder_it_was_moved_to() {
    // Files from before 2020 may still be in several folders at once.
    let several = doc_with(json!({ "parents": [PLANS, "1AnOlderFolder_aBcDeFgHiJkLmNoP"] }));
    let moved = moving(several, ARCHIVE).await;
    moved.result.unwrap();
    let query = query_of(&moved.changes[0]);
    assert_eq!(query["addParents"], ARCHIVE);
    assert_eq!(
        query["removeParents"],
        format!("{PLANS},1AnOlderFolder_aBcDeFgHiJkLmNoP")
    );

    // Already in the folder and in another: only the other is taken away.
    let moved = moving(doc_with(json!({ "parents": [ARCHIVE, PLANS] })), ARCHIVE).await;
    moved.result.unwrap();
    let query = query_of(&moved.changes[0]);
    assert_eq!(query.get("addParents"), None, "it is there already");
    assert_eq!(query["removeParents"], PLANS);
}

#[tokio::test]
async fn a_move_to_root_is_a_move_to_the_folder_google_means_by_it() {
    // A file names its parent by id and never as `root`, so the name is
    // looked up first: otherwise it could not be told that a file is there.
    let moved = moving(doc(), "root").await;
    assert_eq!(moved.result.unwrap()["parents"], json!([ROOT_ID]));
    let read: Vec<&str> = moved.reads.iter().map(|request| request.url.path()).collect();
    assert_eq!(read, ["/drive/v3/files/root", &format!("/drive/v3/files/{DOC}")]);
    let query = query_of(&moved.changes[0]);
    assert_eq!(query["addParents"], ROOT_ID);
    assert_eq!(query["removeParents"], PLANS);

    // Already at the top of My Drive: it is not added to that folder and taken out of it.
    let moved = moving(doc_with(json!({ "parents": [ROOT_ID] })), "root").await;
    assert_eq!(moved.result.unwrap()["parents"], json!([ROOT_ID]));
    assert!(moved.changes.is_empty());
}

#[tokio::test]
async fn a_move_that_cannot_be_made_changes_nothing() {
    // The file is not there, or not the account's to see: nothing is changed.
    let (server, socket, key) = google().await;
    Mock::given(any())
        .respond_with(google_error(404, "notFound", "File not found: 1AbC."))
        .mount(&server)
        .await;
    let input = json!({ "file": DOC, "folder": ARCHIVE });
    let err = invoke(&socket, &key, "drive_files.move_to", input.clone())
        .await
        .unwrap_err();
    assert_eq!(err.kind(), ErrorKind::NotFound);
    assert_eq!(
        only_request(&server).await.method.as_str(),
        "GET",
        "the read, and no change"
    );

    // Google's refusal of the change itself reaches the caller in Google's words.
    let (server, socket, key) = google().await;
    Mock::given(method("GET"))
        .respond_with(answer(200, &doc()))
        .mount(&server)
        .await;
    let refusal = "The domain administrator has not allowed writers to move items into a shared drive.";
    Mock::given(method("PATCH"))
        .respond_with(google_error(403, "fileWriterTeamDriveMoveInDisabled", refusal))
        .mount(&server)
        .await;
    let err = invoke(&socket, &key, "drive_files.move_to", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::AccessDenied);
    assert!(err.message().ends_with(refusal), "{}", err.message());
}

// ── Making and changing ──────────────────────────────────────────────────────

#[tokio::test]
async fn what_is_not_set_is_not_sent_when_a_folder_is_made_or_a_file_copied() {
    // A folder with a name and nothing else: Google puts it at the top of My Drive.
    let (server, socket, key) = answering(200, folder()).await;
    invoke(&socket, &key, "drive_files.create_folder", json!({ "name": "Plans" }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(
        body_of(&request),
        json!({ "name": "Plans", "mimeType": "application/vnd.google-apps.folder" })
    );
    assert_eq!(query_of(&request), of_file());

    // A copy with nothing said about it: Google names it and puts it beside
    // the original. The request says nothing, and still states a length.
    let (server, socket, key) = answering(200, doc_with(json!({ "id": "1TheCopy" }))).await;
    invoke(&socket, &key, "drive_files.copy", json!({ "file": DOC }))
        .await
        .unwrap();
    let request = only_request(&server).await;
    assert_eq!(body_of(&request), json!({}));
    assert_eq!(request.headers.get("content-length").unwrap(), "2");

    for (input, sent) in [
        (json!({ "name": "Q1 plan" }), json!({ "name": "Q1 plan" })),
        (json!({ "parents": [ARCHIVE] }), json!({ "parents": [ARCHIVE] })),
    ] {
        let (server, socket, key) = answering(200, doc_with(json!({ "id": "1TheCopy" }))).await;
        invoke(&socket, &key, "drive_files.copy", with(input, "file", json!(DOC)))
            .await
            .unwrap();
        assert_eq!(body_of(&only_request(&server).await), sent);
    }
}

#[tokio::test]
async fn what_could_only_fail_is_refused_before_google_is_called() {
    let (server, socket, key) = answering(200, doc()).await;
    for (name, input, says) in [
        // A folder nobody named.
        ("drive_files.create_folder", json!({}), "`name`"),
        ("drive_files.create_folder", json!({ "name": "  " }), "`name`"),
        // A file has one parent, so a list of any other length names no place.
        (
            "drive_files.create_folder",
            json!({ "name": "Plans", "parents": [PLANS, ARCHIVE] }),
            "`parents`",
        ),
        (
            "drive_files.create_folder",
            json!({ "name": "Plans", "parents": [] }),
            "`parents`",
        ),
        (
            "drive_files.create_folder",
            json!({ "name": "Plans", "parents": [" "] }),
            "`parents`",
        ),
        ("drive_files.copy", json!({ "file": DOC, "name": " " }), "`name`"),
        (
            "drive_files.copy",
            json!({ "file": DOC, "parents": [PLANS, ARCHIVE] }),
            "`parents`",
        ),
        ("drive_files.copy", json!({ "file": DOC, "parents": [""] }), "`parents`"),
        ("drive_files.rename", json!({ "file": DOC, "name": "" }), "a name"),
        ("drive_files.rename", json!({ "file": DOC, "name": "  " }), "a name"),
        ("drive_files.rename", json!({ "file": DOC }), "`name`"),
        ("drive_files.move_to", json!({ "file": DOC, "folder": " " }), "a folder"),
        ("drive_files.move_to", json!({ "file": DOC }), "`folder`"),
        (
            "drive_files.move_to",
            json!({ "file": DOC, "folder": format!(" {DOC} ") }),
            "into itself",
        ),
        // A field that is not one is never dropped in silence: the folder
        // would be made at the top of My Drive, and the search would be of everything.
        (
            "drive_files.create_folder",
            json!({ "name": "Plans", "parent": PLANS }),
            "`parent`",
        ),
        ("drive_files.list", json!({ "query": "trashed = false" }), "`query`"),
        ("drive_files.list", json!({ "corpora": "allDrives" }), "`corpora`"),
        (
            "drive_files.trash",
            json!({ "file": DOC, "trashed": false }),
            "`trashed`",
        ),
    ] {
        let err = invoke(&socket, &key, name, input.clone()).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::InvalidInput, "{name} {input}: {err}");
        assert!(err.message().contains(says), "{name} {input}: {}", err.message());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_success_without_the_file_is_an_error_and_never_repeats_what_google_sent() {
    let secret = "CONFIDENTIAL salary review";
    let file = json!({ "file": DOC });
    for (name, input, response, place) in [
        ("drive_files.get", file.clone(), json!({}), "without a file"),
        (
            "drive_files.get",
            file.clone(),
            json!({ "id": "", "name": secret }),
            "without a file",
        ),
        // A field of the wrong kind is named, and what it held is not.
        (
            "drive_files.get",
            file.clone(),
            json!({ "id": DOC, "name": secret, "trashed": secret }),
            "`trashed`",
        ),
        (
            "drive_files.get",
            file.clone(),
            json!({ "id": DOC, "owners": [{ "displayName": secret, "me": secret }] }),
            "`owners[0].me`",
        ),
        (
            "drive_files.list",
            json!({}),
            files(json!([doc(), { "id": "x", "name": secret, "parents": secret }])),
            "`[1].parents`",
        ),
        (
            "drive_files.permissions",
            file.clone(),
            permissions(json!([{ "id": "1", "displayName": secret, "permissionDetails": secret }])),
            "`[0].permissionDetails`",
        ),
        (
            "drive_shared_drives.list",
            json!({}),
            drives(json!([{ "id": "1", "name": secret, "hidden": secret }])),
            "`[0].hidden`",
        ),
        (
            "drive_files.create_folder",
            json!({ "name": "Plans" }),
            json!({ "name": secret }),
            "without a file",
        ),
        (
            "drive_files.copy",
            file.clone(),
            json!({ "name": secret }),
            "without a file",
        ),
        (
            "drive_files.rename",
            json!({ "file": DOC, "name": "x" }),
            json!({ "name": secret }),
            "without a file",
        ),
        (
            "drive_files.trash",
            file.clone(),
            json!({ "name": secret }),
            "without a file",
        ),
        // A file that came back and is not in the bin was not put there.
        (
            "drive_files.trash",
            file.clone(),
            json!({ "id": DOC, "name": secret, "trashed": false }),
            "in the bin",
        ),
        (
            "drive_files.trash",
            file.clone(),
            json!({ "id": DOC, "name": secret }),
            "in the bin",
        ),
    ] {
        let (_server, socket, key) = answering(200, response.clone()).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Decode, "{name} {response}: {err}");
        assert!(err.message().contains(place), "{name}: {}", err.message());
        let everything = format!("{err} {err:?} {:?}", err.to_wire());
        assert!(!everything.contains("CONFIDENTIAL"), "{name}: {everything}");
    }

    // A move whose change comes back without the file.
    let (server, socket, key) = google().await;
    Mock::given(method("GET"))
        .respond_with(answer(200, &doc()))
        .mount(&server)
        .await;
    Mock::given(method("PATCH"))
        .respond_with(answer(200, &json!({ "name": secret })))
        .mount(&server)
        .await;
    let input = json!({ "file": DOC, "folder": ARCHIVE });
    let err = invoke(&socket, &key, "drive_files.move_to", input).await.unwrap_err();
    assert_eq!(err.kind(), ErrorKind::Decode);
    assert!(!format!("{err} {err:?}").contains("CONFIDENTIAL"));
}

#[tokio::test]
async fn a_change_is_sent_once_when_google_fails() {
    for (name, input) in [
        // Sent twice, these would leave two folders and two copies.
        ("drive_files.create_folder", json!({ "name": "Plans" })),
        ("drive_files.copy", json!({ "file": DOC })),
        ("drive_files.move_to", json!({ "file": DOC, "folder": ARCHIVE })),
        ("drive_files.rename", json!({ "file": DOC, "name": "x" })),
        ("drive_files.trash", json!({ "file": DOC })),
    ] {
        let (server, socket, key) = google().await;
        // The read a move begins with succeeds; everything that changes anything fails.
        Mock::given(method("GET"))
            .respond_with(answer(200, &doc()))
            .mount(&server)
            .await;
        Mock::given(any())
            .respond_with(google_error(503, "backendError", "Backend Error"))
            .mount(&server)
            .await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), ErrorKind::Unexpected, "{name}");
        let received = server.received_requests().await.unwrap();
        let changes = received
            .iter()
            .filter(|request| request.method.as_str() != "GET")
            .count();
        assert_eq!(changes, 1, "{name}: it may have been made, so it is not sent again");
    }
}

#[tokio::test]
async fn googles_refusals_reach_the_caller() {
    for (name, input, response, kind, says) in [
        (
            "drive_files.get",
            json!({ "file": DOC }),
            google_error(404, "notFound", "File not found: 1AbC."),
            ErrorKind::NotFound,
            "has no such resource",
        ),
        // Only its owner can put a file in the bin.
        (
            "drive_files.trash",
            json!({ "file": DOC }),
            google_error(
                403,
                "insufficientFilePermissions",
                "The user does not have sufficient permissions for file 1AbC.",
            ),
            ErrorKind::AccessDenied,
            "The user does not have sufficient permissions for file 1AbC.",
        ),
        // With the narrow scope, a file the application was never given.
        (
            "drive_files.rename",
            json!({ "file": DOC, "name": "x" }),
            google_error(
                403,
                "appNotAuthorizedToFile",
                "The user has not granted the app 123 write access to the file 1AbC.",
            ),
            ErrorKind::AccessDenied,
            "The user has not granted the app 123 write access to the file 1AbC.",
        ),
        // A search Google cannot read, or cannot sort, is Google's to explain.
        (
            "drive_files.list",
            json!({ "q": "fullText contains 'plan'", "orderBy": "name" }),
            google_error(
                400,
                "badRequest",
                "Sorting is not supported for queries with fullText terms. Results are always in descending relevance order.",
            ),
            ErrorKind::InvalidInput,
            "Sorting is not supported for queries with fullText terms.",
        ),
        (
            "drive_files.list",
            json!({ "q": "name = " }),
            google_error(400, "invalid", "Invalid Value"),
            ErrorKind::InvalidInput,
            "Invalid Value",
        ),
    ] {
        let (server, socket, key) = google().await;
        Mock::given(any()).respond_with(response).mount(&server).await;
        let err = invoke(&socket, &key, name, input).await.unwrap_err();
        assert_eq!(err.kind(), kind, "{name}: {err}");
        assert!(err.message().contains(says), "{name}: {}", err.message());
    }
}

// ── The typed methods ────────────────────────────────────────────────────────

#[tokio::test]
async fn the_typed_methods_do_what_the_named_operations_do() {
    let server = MockServer::start().await;
    let google = Google::with_spec(point_at(provider(), &server));
    let integration: Arc<dyn Integration> = Arc::new(google.clone());
    let (socket, key) = connect(integration, TOKEN).await;
    let connection = socket.connection(key).await.unwrap();
    let markdown = "# Q4 plan\n\nShip the **importer** by November.\n";
    let after = doc_with(json!({ "name": "Q4 plan (final)", "parents": [ARCHIVE], "trashed": true }));
    for (verb, at, answered) in [
        ("GET", "/drive/v3/files".to_owned(), files(json!([doc()]))),
        ("GET", format!("/drive/v3/files/{DOC}"), doc()),
        (
            "GET",
            format!("/drive/v3/files/{DOC}/permissions"),
            permissions(json!([writer(), domain_reader()])),
        ),
        ("GET", "/drive/v3/drives".to_owned(), drives(json!([shared_drive()]))),
        ("POST", "/drive/v3/files".to_owned(), folder()),
        (
            "POST",
            format!("/drive/v3/files/{DOC}/copy"),
            doc_with(json!({ "id": "1TheCopy", "name": "Copy of Q4 plan" })),
        ),
        ("PATCH", format!("/drive/v3/files/{DOC}"), after),
    ] {
        Mock::given(method(verb))
            .and(path(at))
            .respond_with(answer(200, &answered))
            .mount(&server)
            .await;
    }
    Mock::given(method("GET"))
        .and(path(format!("/drive/v3/files/{DOC}/export")))
        .respond_with(ResponseTemplate::new(200).set_body_raw(markdown, "text/markdown"))
        .mount(&server)
        .await;

    let drive = google.drive_files(&connection);
    let found = drive
        .list(DriveListFiles {
            q: Some("name contains 'plan' and trashed = false".into()),
            drive_id: Some(SHARED_DRIVE.into()),
            limit: Some(10),
            ..DriveListFiles::default()
        })
        .await
        .unwrap();
    assert_eq!(found.items.len(), 1);
    assert_eq!(found.next_cursor, None);
    let file = drive.get(&found.items[0].id).await.unwrap();
    assert_eq!(file.id, DOC);
    assert_eq!(file.mime_type, "application/vnd.google-apps.document");
    assert_eq!(file.parents, [PLANS]);
    assert_eq!(file.size.as_deref(), Some("18342"));
    assert_eq!(file.owners[0].email_address.as_deref(), Some("ada@example.test"));
    assert!(file.owners[0].me);
    assert!(!file.trashed);

    let exported = drive.export(DOC, DriveExportFormat::Markdown).await.unwrap();
    assert_eq!(
        exported,
        DriveExport {
            mime_type: DriveExportFormat::Markdown,
            text: markdown.to_owned()
        }
    );

    let who = drive.permissions(DOC, Paging::default()).await.unwrap();
    assert_eq!(who.items[0].kind, "user");
    assert_eq!(who.items[0].role, "writer");
    assert_eq!(who.items[1].domain.as_deref(), Some("example.test"));
    assert_eq!(
        who.items[1].permission_details[0].inherited_from.as_deref(),
        Some(SHARED_DRIVE)
    );

    let made = drive
        .create_folder(DriveCreateFolder {
            name: Some("Plans".into()),
            parents: None,
        })
        .await
        .unwrap();
    assert_eq!(made.mime_type, "application/vnd.google-apps.folder");
    let copy = drive.copy(DOC, DriveCopyFile::default()).await.unwrap();
    assert_eq!(copy.id, "1TheCopy");
    assert_eq!(drive.move_to(DOC, ARCHIVE).await.unwrap().parents, [ARCHIVE]);
    assert_eq!(
        drive.rename(DOC, "Q4 plan (final)").await.unwrap().name,
        "Q4 plan (final)"
    );
    assert!(drive.trash(DOC).await.unwrap().trashed);

    let shared = google
        .drive_shared_drives(&connection)
        .list(Paging {
            cursor: None,
            limit: Some(5),
        })
        .await
        .unwrap();
    assert_eq!(shared.items[0].name, "Engineering");
    assert!(!shared.items[0].hidden);

    // The same requests the named operations send, in the order they were made.
    let received = server.received_requests().await.unwrap();
    let sent: Vec<(String, String, Value, Value)> = received
        .iter()
        .map(|request| {
            let mut query = query_of(request);
            // The fields are checked where each operation is; here they would only hide the rest.
            query.as_object_mut().unwrap().remove("fields");
            (
                request.method.to_string(),
                request.url.path().to_owned(),
                query,
                body_of(request),
            )
        })
        .collect();
    let file = format!("/drive/v3/files/{DOC}");
    let one = json!({ "supportsAllDrives": "true" });
    let expected = [
        (
            "GET",
            "/drive/v3/files".to_owned(),
            json!({
                "supportsAllDrives": "true", "includeItemsFromAllDrives": "true", "corpora": "drive",
                "driveId": SHARED_DRIVE, "q": "name contains 'plan' and trashed = false", "pageSize": "10"
            }),
            json!(null),
        ),
        ("GET", file.clone(), one.clone(), json!(null)),
        (
            "GET",
            format!("{file}/export"),
            json!({ "mimeType": "text/markdown" }),
            json!(null),
        ),
        ("GET", format!("{file}/permissions"), one.clone(), json!(null)),
        (
            "POST",
            "/drive/v3/files".to_owned(),
            one.clone(),
            json!({ "name": "Plans", "mimeType": "application/vnd.google-apps.folder" }),
        ),
        ("POST", format!("{file}/copy"), one.clone(), json!({})),
        // The move: the read, then the change.
        ("GET", file.clone(), one.clone(), json!(null)),
        (
            "PATCH",
            file.clone(),
            json!({ "supportsAllDrives": "true", "addParents": ARCHIVE, "removeParents": PLANS }),
            json!({}),
        ),
        ("PATCH", file.clone(), one.clone(), json!({ "name": "Q4 plan (final)" })),
        ("PATCH", file.clone(), one.clone(), json!({ "trashed": true })),
        (
            "GET",
            "/drive/v3/drives".to_owned(),
            json!({ "pageSize": "5" }),
            json!(null),
        ),
    ];
    assert_eq!(sent.len(), expected.len());
    for (sent, (verb, at, query, body)) in sent.iter().zip(expected) {
        assert_eq!((sent.0.as_str(), &sent.1, &sent.2, &sent.3), (verb, &at, &query, &body));
    }
    // Every one of them carried the connection's token, and the shared drives' list its fields.
    assert!(
        received
            .iter()
            .all(|request| { request.headers.get("authorization").unwrap() == &format!("Bearer {TOKEN}") })
    );
    assert_eq!(query_of(received.last().unwrap())["fields"], DRIVES_FIELDS);
}
