//! Files shared in the workspace.

use serde_json::json;
use socketkit_core::{ErrorKind, Result};

use super::{Api, with};
use crate::models::{File, ListFiles};

/// Files shared in the workspace.
#[derive(Debug, Clone, Copy)]
pub struct Files<'a>(pub(crate) Api<'a>);

impl Files<'_> {
    /// One file's details.
    pub async fn info(&self, file: &str) -> Result<File> {
        self.0.required("a file", file)?;
        let body = self.0.get("files.info", json!({ "file": file })).await?;
        let file: File = self.0.field(&body, "file")?;
        if file.id.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::Decode, "slack answered with a file that has no id"));
        }
        Ok(file)
    }

    /// Lists files, optionally those of one channel or member.
    pub async fn list(&self, options: ListFiles) -> Result<Vec<File>> {
        let body = self.0.get("files.list", with(json!({}), &options)).await?;
        self.0.field(&body, "files")
    }

    /// Deletes a file.
    pub async fn delete(&self, file: &str) -> Result<()> {
        self.0.required("a file", file)?;
        self.0.post("files.delete", json!({ "file": file })).await.map(drop)
    }
}
