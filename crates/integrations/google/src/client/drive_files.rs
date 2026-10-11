//! Files and folders in Drive: finding them, reading what describes one,
//! exporting a Google document as text, and who can see a file.
//!
//! Making, copying, moving, renaming and binning are in
//! `drive_files_change.rs`; they are methods of the same group, kept apart
//! only for the length of the file.
//!
//! Two things hold for every request that names a file or lists files.
//! It says `supportsAllDrives=true`: without it Google answers as if what is
//! in a shared drive did not exist. And it names the fields it wants: Drive
//! returns next to nothing unless `fields` asks.

use serde_json::Value;
use socketkit_core::{Error, ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{DriveExport, DriveExportFormat, DriveFile, DriveListFiles, DrivePermission, Paging};

/// What is asked for of a file: every field of [`DriveFile`], and no other.
const FILE_FIELDS: &str = "id,name,mimeType,parents,createdTime,modifiedTime,size,\
    owners(displayName,emailAddress,permissionId,me),webViewLink,trashed,driveId,\
    shortcutDetails(targetId,targetMimeType,targetResourceKey)";

/// What is asked for of a permission: every field of [`DrivePermission`].
const PERMISSION_FIELDS: &str = "id,type,role,emailAddress,domain,displayName,deleted,allowFileDiscovery,\
    expirationTime,permissionDetails(permissionType,inheritedFrom,role,inherited)";

/// The most files Google returns in one page, and the most permissions.
const MOST_FILES: u32 = 1000;
const MOST_PERMISSIONS: u32 = 100;

/// `request` for one file, answered with the fields of [`DriveFile`],
/// wherever the file is: in someone's My Drive or in a shared drive.
pub(super) fn of_file(request: RawRequest) -> RawRequest {
    request
        .with_query("fields", FILE_FIELDS)
        .with_query("supportsAllDrives", "true")
}

/// Files and folders in Drive.
#[derive(Debug, Clone, Copy)]
pub struct DriveFiles<'a>(pub(crate) Api<'a>);

impl DriveFiles<'_> {
    /// Lists the files that match a search, or everything the account can
    /// see. What is in shared drives is included.
    ///
    /// The search is Drive's own query language and goes to Google as it was
    /// given: nothing here reads it.
    pub async fn list(&self, options: DriveListFiles) -> Result<Page<DriveFile>> {
        let set = |value: &Option<String>| {
            value
                .as_deref()
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        };
        let mut request = RawRequest::get("drive/v3/files")
            .with_query("fields", format!("kind,nextPageToken,files({FILE_FIELDS})"))
            .with_query("supportsAllDrives", "true")
            .with_query("includeItemsFromAllDrives", "true");
        if let Some(q) = set(&options.q) {
            request = request.with_query("q", q);
        }
        if let Some(order) = set(&options.order_by) {
            request = request.with_query("orderBy", order);
        }
        // One shared drive is a body of files of its own, which Google
        // searches only when told that this is the body meant.
        if let Some(drive) = set(&options.drive_id) {
            request = request.with_query("corpora", "drive").with_query("driveId", drive);
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        let request = self.0.paged(request, &paging, "pageSize", MOST_FILES)?;
        let body = self.0.send(request).await?;
        self.0.kind(&body, "drive#fileList", "files")?;
        self.0.page(body, "files", "files")
    }

    /// Gets what describes one file or folder: its name, what it is, where
    /// it is, who owns it. Not its content.
    pub async fn get(&self, file: &str) -> Result<DriveFile> {
        let body = self.0.send(of_file(RawRequest::get(self.item(file)?))).await?;
        self.file(body)
    }

    /// Returns a Google document as text: a Doc as plain text or Markdown, a
    /// Sheet as CSV, which holds its first sheet only.
    ///
    /// Google exports at most 10 MB. A file that holds content of its own,
    /// such as a PDF, is not a Google document and has nothing to export.
    pub async fn export(&self, file: &str, format: DriveExportFormat) -> Result<DriveExport> {
        let request = RawRequest::get(format!("{}/export", self.item(file)?))
            .with_query("mimeType", format.mime_type())
            .as_text();
        let text = match self.0.send(request).await.map_err(|e| self.not_exported(e))? {
            Value::String(text) => text,
            // An empty sheet is exported as nothing at all, and that is its text.
            _ => String::new(),
        };
        // Google begins a Doc's plain text with a byte order mark, which is
        // no part of what the document says.
        let text = match text.strip_prefix('\u{feff}') {
            Some(rest) => rest.to_owned(),
            None => text,
        };
        Ok(DriveExport {
            mime_type: format,
            text,
        })
    }

    /// Lists who can see a file, and in what role: people, groups, whole
    /// domains, and anyone at all.
    pub async fn permissions(&self, file: &str, paging: Paging) -> Result<Page<DrivePermission>> {
        let request = RawRequest::get(format!("{}/permissions", self.item(file)?))
            .with_query("fields", format!("kind,nextPageToken,permissions({PERMISSION_FIELDS})"))
            .with_query("supportsAllDrives", "true");
        let request = self.0.paged(request, &paging, "pageSize", MOST_PERMISSIONS)?;
        let body = self.0.send(request).await?;
        self.0.kind(&body, "drive#permissionList", "permissions")?;
        self.0.page(body, "permissions", "permissions")
    }

    /// The path of one file.
    pub(super) fn item(&self, file: &str) -> Result<String> {
        Ok(format!("drive/v3/files/{}", self.0.segment("a file id", file)?))
    }

    pub(super) fn file(&self, body: Value) -> Result<DriveFile> {
        let file: DriveFile = self.0.decode(body, "a file")?;
        if file.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "google answered without a file"));
        }
        Ok(file)
    }

    /// Why an export failed, where the reason is one a caller can act on.
    ///
    /// Google refuses both cases below with a 403, which reads as a missing
    /// permission and is not one. The error that arrives here carries Google's
    /// message and not the `reason` beside it, so the two are told by their
    /// wording; a refusal worded any other way is passed on as it came.
    fn not_exported(&self, error: Error) -> Error {
        let said = error.message();
        let too_large = match error.kind() {
            // Google's own limit on what it exports.
            ErrorKind::AccessDenied => said.contains("too large to be exported"),
            // The most the transport reads of any answer, which is as much.
            ErrorKind::Decode => said.contains("too large to read"),
            _ => false,
        };
        if too_large {
            return self.0.error(
                ErrorKind::InvalidInput,
                "this file is too large to export: the limit is 10 MB of exported content",
            );
        }
        if error.kind() == ErrorKind::AccessDenied && said.contains("only supports Docs Editors files") {
            return self.0.error(
                ErrorKind::InvalidInput,
                "this file is not a Google document, so there is nothing to export: \
                 only a Google Doc, Sheet or Slides presentation is exported",
            );
        }
        error
    }
}
