//! The folders of a person's mailbox.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::Api;
use crate::models::{ListFolders, MailFolder, Paging};

/// The folders of a person's mailbox.
#[derive(Debug, Clone, Copy)]
pub struct MailFolders<'a>(pub(crate) Api<'a>);

impl MailFolders<'_> {
    /// Lists the folders at the top of the mailbox, or those inside one folder.
    pub async fn list(&self, options: ListFolders) -> Result<Page<MailFolder>> {
        let path = match &options.parent {
            Some(parent) => format!("me/mailFolders/{}/childFolders", self.0.segment("a folder", parent)?),
            None => "me/mailFolders".to_owned(),
        };
        let mut request = RawRequest::get(path);
        if options.include_hidden == Some(true) {
            request = request.with_query("includeHiddenFolders", "true");
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.page(request, &paging, "folders").await
    }

    /// Gets one folder: by its id, or by a well-known name such as `inbox`.
    pub async fn get(&self, folder: &str) -> Result<MailFolder> {
        let folder = self.0.segment("a folder", folder)?;
        let body = self.0.send(RawRequest::get(format!("me/mailFolders/{folder}"))).await?;
        let folder: MailFolder = self.0.decode(body, "a folder")?;
        if folder.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "microsoft answered without a folder"));
        }
        Ok(folder)
    }
}
