//! Shared drives: the drives that belong to an organisation and not to one person.

use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{Paging, SharedDrive};

/// What is asked for of a shared drive: every field of [`SharedDrive`].
const DRIVE_FIELDS: &str = "id,name,createdTime,hidden";

/// The most shared drives Google returns in one page.
const MOST_DRIVES: u32 = 100;

/// The shared drives the account is a member of.
#[derive(Debug, Clone, Copy)]
pub struct DriveSharedDrives<'a>(pub(crate) Api<'a>);

impl DriveSharedDrives<'_> {
    /// Lists the shared drives the account is a member of. Google returns
    /// ten in a page unless `limit` says otherwise.
    ///
    /// A drive's id is what `DriveListFiles::drive_id` takes to list what is
    /// in it, and is also the id of the drive's top folder.
    pub async fn list(&self, paging: Paging) -> Result<Page<SharedDrive>> {
        let request = RawRequest::get("drive/v3/drives")
            .with_query("fields", format!("kind,nextPageToken,drives({DRIVE_FIELDS})"));
        let request = self.0.paged(request, &paging, "pageSize", MOST_DRIVES)?;
        let body = self.0.send(request).await?;
        self.0.kind(&body, "drive#driveList", "shared drives")?;
        self.0.page(body, "drives", "shared drives")
    }
}
