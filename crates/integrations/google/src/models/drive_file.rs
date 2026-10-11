//! Files and folders in Drive: what Google returns for them, and the filters and content used to find, make and copy them.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A file or a folder in Drive. A Google Doc, a Sheet and a shortcut are
/// files too; `mimeType` says which.
///
/// Drive returns only the fields it is asked for, and it is asked for
/// exactly these.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriveFile {
    pub id: String,
    /// Not unique: one folder can hold two files of the same name.
    pub name: String,
    /// What it is. A folder is `application/vnd.google-apps.folder`, a Doc
    /// `application/vnd.google-apps.document`, a Sheet
    /// `application/vnd.google-apps.spreadsheet`, a shortcut
    /// `application/vnd.google-apps.shortcut`. Anything else is a file with
    /// content of its own, such as `application/pdf`.
    pub mime_type: String,
    /// The folder it is in: one id. Empty when the account cannot see that
    /// folder, as with a file someone shared from their own Drive.
    pub parents: Vec<String>,
    /// RFC 3339, in UTC: `2026-10-09T08:15:00.000Z`.
    pub created_time: Option<String>,
    /// When anyone last changed it.
    pub modified_time: Option<String>,
    /// Its size in bytes, written as Google writes it: a number in a string.
    /// A folder and a shortcut have none.
    pub size: Option<String>,
    /// Who owns it. Empty for a file in a shared drive, which belongs to the drive.
    pub owners: Vec<DriveUser>,
    /// The address that opens it in a browser.
    pub web_view_link: Option<String>,
    /// Whether it is in the bin, put there itself or with a folder above it.
    pub trashed: bool,
    /// The shared drive it is in. Absent for a file in someone's My Drive.
    pub drive_id: Option<String>,
    /// What a shortcut points to. Only a shortcut has it.
    pub shortcut_details: Option<DriveShortcutDetails>,
}

/// A person, as Drive names one on a file.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriveUser {
    pub display_name: Option<String>,
    /// Absent when the person does not show it to the account.
    pub email_address: Option<String>,
    /// The person's id as a permission names it.
    pub permission_id: Option<String>,
    /// Whether this is the account the connection is authorised as.
    pub me: bool,
}

/// The file a shortcut points to. Read that file by its `targetId`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriveShortcutDetails {
    pub target_id: Option<String>,
    /// What the target was when the shortcut was made.
    pub target_mime_type: Option<String>,
    pub target_resource_key: Option<String>,
}

/// A file that is text, as text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriveFileText {
    /// The media type Google served it as, such as `text/csv`.
    pub content_type: Option<String>,
    /// The file, exactly as it was written.
    pub text: String,
}

/// Which files to list.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriveListFiles {
    /// A search in Drive's query language, passed to Google as it is:
    /// `name contains 'budget' and trashed = false`,
    /// `fullText contains 'quarterly plan'`,
    /// `mimeType = 'application/vnd.google-apps.spreadsheet'`,
    /// `'FOLDER_ID' in parents`, `modifiedTime > '2026-10-01T00:00:00'`.
    /// A value goes in single quotes, with `\'` for a quote inside it and
    /// `\\` for a backslash. Everything the account can see when not given,
    /// what is in the bin included.
    pub q: Option<String>,
    /// What to sort by: Google's keys with commas between them, each
    /// followed by ` desc` to reverse it, such as `folder,modifiedTime desc`.
    /// The keys are `createdTime`, `folder`, `modifiedByMeTime`,
    /// `modifiedTime`, `name`, `name_natural`, `quotaBytesUsed`, `recency`,
    /// `sharedWithMeTime`, `starred` and `viewedByMeTime`. Google refuses a
    /// sort together with a `fullText` search.
    pub order_by: Option<String>,
    /// The id of one shared drive, to list only what is in it.
    pub drive_id: Option<String>,
    /// The `next_cursor` of the page before, unchanged; absent for the first page.
    pub cursor: Option<String>,
    /// The most files to return in one page, from 1 to 1000. Google may return fewer.
    pub limit: Option<u32>,
}

/// A folder to create.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriveCreateFolder {
    /// The folder's name. A folder is refused without one.
    pub name: Option<String>,
    /// The id of the folder to create it in, as a list of that one id: a
    /// folder has one parent. The top of the account's My Drive when not given.
    pub parents: Option<Vec<String>>,
}

/// How a copy differs from the file it is made of.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct DriveCopyFile {
    /// The copy's name. Google names it "Copy of …" when not given.
    pub name: Option<String>,
    /// The id of the folder to put the copy in, as a list of that one id.
    /// Beside the original when not given.
    pub parents: Option<Vec<String>>,
}
