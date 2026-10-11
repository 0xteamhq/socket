//! Files and folders in Drive: making a folder, and copying, moving,
//! renaming and binning a file.
//!
//! These are methods of [`DriveFiles`], whose struct and reading methods are
//! in `drive_files.rs`. Each returns the file as it is afterwards.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, RawRequest, Result};

use super::drive_files::{DriveFiles, of_file};
use super::set;
use crate::models::{DriveCopyFile, DriveCreateFolder, DriveFile};

/// What Drive calls a folder: a file of this type.
const FOLDER: &str = "application/vnd.google-apps.folder";

/// The name Google reads as the top of the account's own My Drive, wherever
/// a folder's id is asked for.
const ROOT: &str = "root";

impl DriveFiles<'_> {
    /// Creates a folder, at the top of the account's My Drive or inside
    /// another folder.
    pub async fn create_folder(&self, folder: DriveCreateFolder) -> Result<DriveFile> {
        if folder.name.as_deref().is_none_or(|name| name.trim().is_empty()) {
            return Err(self.0.error(ErrorKind::InvalidInput, "a folder needs a `name`"));
        }
        self.one_parent(folder.parents.as_deref())?;
        let mut content = set(&folder);
        content.insert("mimeType".to_owned(), json!(FOLDER));
        let request = RawRequest::post("drive/v3/files", Value::Object(content));
        let body = self.0.send(of_file(request)).await?;
        self.file(body)
    }

    /// Makes a copy of a file, beside it or in another folder. Google does
    /// not copy a folder.
    pub async fn copy(&self, file: &str, copy: DriveCopyFile) -> Result<DriveFile> {
        let path = format!("{}/copy", self.item(file)?);
        if copy.name.as_deref().is_some_and(|name| name.trim().is_empty()) {
            return Err(self.0.error(
                ErrorKind::InvalidInput,
                "`name` is blank: leave it out for the name Google gives a copy",
            ));
        }
        self.one_parent(copy.parents.as_deref())?;
        // With nothing set this is an empty object, which says nothing and
        // still gives the request a length.
        let request = RawRequest::post(path, Value::Object(set(&copy)));
        let body = self.0.send(of_file(request)).await?;
        self.file(body)
    }

    /// Moves a file or a folder into `folder`, out of wherever it is.
    /// `folder` is a folder's id, or `root` for the top of the account's My
    /// Drive.
    ///
    /// Google moves a file by being told which parent to add and which to
    /// take away, so the file is read first. A file that is already in the
    /// folder, and nowhere else, is returned as it is and nothing is changed.
    /// A file whose folder the account cannot see is only given the new
    /// parent: Google then moves it if the account may, and refuses if not.
    pub async fn move_to(&self, file: &str, folder: &str) -> Result<DriveFile> {
        let path = self.item(file)?;
        self.0.required("a folder", folder)?;
        // Google reads a comma here as the start of a second folder.
        if folder.contains(',') {
            return Err(self.0.error(ErrorKind::InvalidInput, "a folder is one id"));
        }
        if folder.trim() == file.trim() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "a folder cannot be moved into itself"));
        }
        // A file names its parent by id and never as `root`. Left as it is,
        // the name would never match, and a file at the top of My Drive
        // would be both added to that folder and taken out of it.
        let folder = match folder.trim() {
            ROOT => self.get(ROOT).await?.id,
            id => id.to_owned(),
        };
        let current = self.get(file).await?;
        let elsewhere: Vec<&str> = current
            .parents
            .iter()
            .map(String::as_str)
            .filter(|parent| *parent != folder)
            .collect();
        let there = elsewhere.len() < current.parents.len();
        if there && elsewhere.is_empty() {
            return Ok(current);
        }
        let mut request = RawRequest::new("PATCH", path).with_body(json!({}));
        if !there {
            request = request.with_query("addParents", folder);
        }
        if !elsewhere.is_empty() {
            request = request.with_query("removeParents", elsewhere.join(","));
        }
        let body = self.0.send(of_file(request)).await?;
        self.file(body)
    }

    /// Gives a file or a folder another name. It stays where it is, under
    /// the same id.
    pub async fn rename(&self, file: &str, name: &str) -> Result<DriveFile> {
        let path = self.item(file)?;
        self.0.required("a name", name)?;
        self.change(path, json!({ "name": name })).await
    }

    /// Puts a file or a folder in the bin, with everything inside a folder.
    /// It can be taken out again until Google empties the bin, 30 days later.
    pub async fn trash(&self, file: &str) -> Result<DriveFile> {
        let binned = self.change(self.item(file)?, json!({ "trashed": true })).await?;
        if !binned.trashed {
            return Err(self.0.error(
                ErrorKind::Decode,
                "google answered without saying the file is in the bin",
            ));
        }
        Ok(binned)
    }

    /// Changes the file at `path`: only the fields in `changes`.
    async fn change(&self, path: String, changes: Value) -> Result<DriveFile> {
        let request = RawRequest::new("PATCH", path).with_body(changes);
        let body = self.0.send(of_file(request)).await?;
        self.file(body)
    }

    /// Checks the folder something is to be put in. Google takes it as a
    /// list, and a file has one parent: a list of any other length could
    /// only be refused, or put the file somewhere nobody named.
    fn one_parent(&self, parents: Option<&[String]>) -> Result<()> {
        match parents {
            None => Ok(()),
            Some([parent]) if !parent.trim().is_empty() => Ok(()),
            Some([_]) => Err(self.0.error(ErrorKind::InvalidInput, "`parents` holds a blank id")),
            Some(_) => Err(self.0.error(
                ErrorKind::InvalidInput,
                "`parents` holds exactly one folder id: a file has one parent",
            )),
        }
    }
}
