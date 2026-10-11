//! Files and folders in Drive: finding them, reading what describes one,
//! downloading a file, exporting a Google document as text, and who can see
//! a file.
//!
//! Making, copying, moving, renaming and binning are in
//! `drive_files_change.rs`; they are methods of the same group, kept apart
//! only for the length of the file.
//!
//! Two things hold for every request that names a file or lists files.
//! It says `supportsAllDrives=true`: without it Google answers as if what is
//! in a shared drive did not exist. And where it asks for what describes a
//! file, it names the fields it wants: Drive returns next to nothing unless
//! `fields` asks. An export takes neither: Google gives it no such parameter.

use serde_json::Value;
use socketkit_core::{Content, ContentRequest, Error, ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{
    Download, DriveExport, DriveExportFormat, DriveFile, DriveFileText, DriveListFiles, DrivePermission, Paging,
    TextLimit,
};

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

    /// The file itself: its bytes, unchanged, with the type Google serves
    /// them as. For a file that has content of its own, such as a PDF, an
    /// image or a text file. A Google Doc, Sheet or Slides presentation has
    /// none, and is refused with a message that points to [`Self::export`].
    ///
    /// Ten megabytes are read unless `limits` allows more, and a larger file
    /// is refused whole. This is a typed method only: an operation called by
    /// name never returns bytes.
    pub async fn download(&self, file: &str, limits: Download) -> Result<Content> {
        let fetched = self.0.fetch(self.media(file)?, &limits).await;
        fetched.map_err(|e| self.not_downloaded(e))
    }

    /// A file that is text, as text: a CSV file, a text file, a JSON file.
    /// Anything Google does not serve as text is refused with `unsupported`,
    /// and nothing of it is returned.
    ///
    /// One megabyte is read unless `limit` allows more, up to ten.
    pub async fn download_text(&self, file: &str, limit: TextLimit) -> Result<DriveFileText> {
        let fetched = self.within(self.media(file)?, &limit).await;
        let content = fetched.map_err(|e| self.not_downloaded(e))?;
        let content_type = content.content_type.clone();
        let text = content.into_text(&self.0.connection.provider().id)?;
        Ok(DriveFileText { content_type, text })
    }

    /// Returns a Google document as text: a Doc as plain text or Markdown, a
    /// Sheet as CSV, which holds its first sheet only.
    ///
    /// One megabyte is read unless `limit` allows more, up to ten, which is
    /// also the most Google exports. A file that holds content of its own,
    /// such as a PDF, is not a Google document and has nothing to export.
    pub async fn export(&self, file: &str, format: DriveExportFormat, limit: TextLimit) -> Result<DriveExport> {
        let request =
            ContentRequest::get(format!("{}/export", self.item(file)?)).with_query("mimeType", format.mime_type());
        let content = self.within(request, &limit).await.map_err(|e| self.not_exported(e))?;
        let text = if content.is_empty() {
            // An empty sheet is exported as nothing at all, and that is its text.
            String::new()
        } else {
            content.into_text(&self.0.connection.provider().id)?
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

    /// The request for a file's own content, wherever the file is.
    fn media(&self, file: &str) -> Result<ContentRequest> {
        Ok(ContentRequest::get(self.item(file)?)
            .with_query("alt", "media")
            .with_query("supportsAllDrives", "true"))
    }

    /// Fetches what goes back as text, within what `limit` allows. Content
    /// over the limit is refused whole, with what the caller can do about it.
    async fn within(&self, request: ContentRequest, limit: &TextLimit) -> Result<Content> {
        let limits = self.0.text_limits(limit)?;
        let most = limits.max_bytes.unwrap_or(Content::MAX_INLINE_BYTES);
        self.0.fetch(request, &limits).await.map_err(|error| {
            if error.kind() != ErrorKind::TooLarge {
                return error;
            }
            let ceiling = ContentRequest::DEFAULT_MAX_BYTES;
            let advice = if most < ceiling {
                format!("`maxBytes` can be raised, up to {ceiling}")
            } else {
                "that is the most that is returned as text".to_owned()
            };
            self.0.error(
                ErrorKind::TooLarge,
                format!("google has content larger than the limit of {most} bytes set for this request; {advice}"),
            )
        })
    }

    // Google refuses the two cases below with a 403, which reads as a
    // missing permission and is not one. The error that arrives here carries
    // Google's message and not the `reason` beside it, so each is told by its
    // wording; a refusal worded any other way is passed on as it came.

    /// Why a download failed, where the reason is one a caller can act on.
    fn not_downloaded(&self, error: Error) -> Error {
        let said = error.message();
        if error.kind() == ErrorKind::AccessDenied && said.contains("Only files with binary content can be downloaded")
        {
            return self.0.error(
                ErrorKind::InvalidInput,
                "this file has no content of its own to download: \
                 a Google Doc, Sheet or Slides presentation is read with `export`",
            );
        }
        error
    }

    /// Why an export failed, where the reason is one a caller can act on.
    /// An export over Google's own limit is told apart earlier, by its
    /// `reason`, where every answer of Google's is read.
    fn not_exported(&self, error: Error) -> Error {
        if error.kind() == ErrorKind::AccessDenied && error.message().contains("only supports Docs Editors files") {
            return self.0.error(
                ErrorKind::InvalidInput,
                "this file is not a Google document, so there is nothing to export: \
                 only a Google Doc, Sheet or Slides presentation is exported",
            );
        }
        error
    }
}
