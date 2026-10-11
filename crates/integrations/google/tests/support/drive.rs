//! What Google answers for Drive, as the tests need it: fixtures and constants.

use serde_json::{Value, json};

/// A Google Doc's id, and the folder it is in.
pub const DOC: &str = "1AbC_dEf-GhIjKlMnOpQrStUvWxYz012345";
pub const PLANS: &str = "1FolderOfPlans_aBcDeFgHiJkLmNoPq";
/// Another folder, and a shared drive.
pub const ARCHIVE: &str = "1FolderOfOldThings_zYxWvUtSrQpOn";
pub const SHARED_DRIVE: &str = "0AEngineeringDrive9PVA";

/// Exactly what Drive is asked for of a file, of a page of files, of a page
/// of permissions and of a page of shared drives. Drive returns nothing
/// that is not named here.
pub const FILE_FIELDS: &str = "id,name,mimeType,parents,createdTime,modifiedTime,size,\
    owners(displayName,emailAddress,permissionId,me),webViewLink,trashed,driveId,\
    shortcutDetails(targetId,targetMimeType,targetResourceKey)";
pub const FILES_FIELDS: &str = "kind,nextPageToken,files(id,name,mimeType,parents,createdTime,modifiedTime,size,\
    owners(displayName,emailAddress,permissionId,me),webViewLink,trashed,driveId,\
    shortcutDetails(targetId,targetMimeType,targetResourceKey))";
pub const PERMISSIONS_FIELDS: &str = "kind,nextPageToken,permissions(id,type,role,emailAddress,domain,displayName,\
    deleted,allowFileDiscovery,expirationTime,permissionDetails(permissionType,inheritedFrom,role,inherited))";
pub const DRIVES_FIELDS: &str = "kind,nextPageToken,drives(id,name,createdTime,hidden)";

/// The query of a request for one file: its fields, with shared drives in reach.
pub fn of_file() -> Value {
    json!({ "fields": FILE_FIELDS, "supportsAllDrives": "true" })
}

fn ada() -> Value {
    json!({ "displayName": "Ada Lovelace", "emailAddress": "ada@example.test", "permissionId": "08412345678901234567", "me": true })
}

/// A Google Doc in a folder of the account's My Drive, as Drive answers
/// when asked for [`FILE_FIELDS`].
pub fn doc() -> Value {
    json!({
        "id": DOC,
        "name": "Q4 plan",
        "mimeType": "application/vnd.google-apps.document",
        "parents": [PLANS],
        "createdTime": "2026-09-30T14:02:11.000Z",
        "modifiedTime": "2026-10-09T08:15:00.000Z",
        "size": "18342",
        "owners": [ada()],
        "webViewLink": format!("https://docs.google.com/document/d/{DOC}/edit?usp=drivesdk"),
        "trashed": false
    })
}

/// The same Doc with something about it changed.
pub fn doc_with(changes: Value) -> Value {
    let mut doc = doc();
    for (name, value) in changes.as_object().unwrap() {
        doc[name] = value.clone();
    }
    doc
}

/// A folder. It has no size.
pub fn folder() -> Value {
    json!({
        "id": PLANS,
        "name": "Plans",
        "mimeType": "application/vnd.google-apps.folder",
        "parents": ["0AMyDriveRootId9PVA"],
        "createdTime": "2026-01-12T09:00:00.000Z",
        "modifiedTime": "2026-10-09T08:15:00.000Z",
        "owners": [ada()],
        "webViewLink": format!("https://drive.google.com/drive/folders/{PLANS}"),
        "trashed": false
    })
}

/// A spreadsheet in a shared drive: it has no owners, and names its drive.
pub fn shared_sheet() -> Value {
    json!({
        "id": "1SheetInASharedDrive_aBcDeFgHiJk",
        "name": "Budget",
        "mimeType": "application/vnd.google-apps.spreadsheet",
        "parents": [SHARED_DRIVE],
        "createdTime": "2026-03-02T11:30:00.000Z",
        "modifiedTime": "2026-10-08T16:45:12.000Z",
        "size": "4096",
        "webViewLink": "https://docs.google.com/spreadsheets/d/1SheetInASharedDrive_aBcDeFgHiJk/edit?usp=drivesdk",
        "trashed": false,
        "driveId": SHARED_DRIVE
    })
}

/// A page of files, as Drive answers when asked for [`FILES_FIELDS`].
pub fn files(files: Value) -> Value {
    json!({ "kind": "drive#fileList", "files": files })
}

/// A person who may edit, set on the file itself.
pub fn writer() -> Value {
    json!({
        "id": "08412345678901234567",
        "type": "user",
        "role": "writer",
        "emailAddress": "grace@example.test",
        "displayName": "Grace Hopper",
        "deleted": false,
        "permissionDetails": [{ "permissionType": "file", "role": "writer", "inherited": false }]
    })
}

/// Everyone in a domain may read, by way of the shared drive above the file.
pub fn domain_reader() -> Value {
    json!({
        "id": "12345678901234567890k",
        "type": "domain",
        "role": "reader",
        "domain": "example.test",
        "displayName": "Example",
        "allowFileDiscovery": false,
        "permissionDetails": [
            { "permissionType": "member", "inheritedFrom": SHARED_DRIVE, "role": "reader", "inherited": true }
        ]
    })
}

/// A page of permissions, as Drive answers when asked for [`PERMISSIONS_FIELDS`].
pub fn permissions(permissions: Value) -> Value {
    json!({ "kind": "drive#permissionList", "permissions": permissions })
}

pub fn shared_drive() -> Value {
    json!({ "id": SHARED_DRIVE, "name": "Engineering", "createdTime": "2025-11-04T10:00:00.000Z", "hidden": false })
}

/// A page of shared drives, as Drive answers when asked for [`DRIVES_FIELDS`].
pub fn drives(drives: Value) -> Value {
    json!({ "kind": "drive#driveList", "drives": drives })
}
