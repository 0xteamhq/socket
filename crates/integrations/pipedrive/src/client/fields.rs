//! The fields of each kind of record. Version 2 of Pipedrive's API throughout.
//!
//! Without these a custom field cannot be read: a record holds its value
//! under a 40-character key, and only the list of fields says what the
//! company called it and what its choices are.

use socketkit_core::{Page, RawRequest, Result};

use super::Api;
use crate::models::{Field, Paging};

/// The fields a company's deals, persons and organisations have.
#[derive(Debug, Clone, Copy)]
pub struct Fields<'a>(pub(crate) Api<'a>);

impl Fields<'_> {
    /// Lists the fields of a deal. A lead has the same custom fields.
    pub async fn deal_fields(&self, paging: Paging) -> Result<Page<Field>> {
        self.list("v2/dealFields", paging).await
    }

    /// Lists the fields of a person.
    pub async fn person_fields(&self, paging: Paging) -> Result<Page<Field>> {
        self.list("v2/personFields", paging).await
    }

    /// Lists the fields of an organisation.
    pub async fn organization_fields(&self, paging: Paging) -> Result<Page<Field>> {
        self.list("v2/organizationFields", paging).await
    }

    async fn list(&self, path: &str, paging: Paging) -> Result<Page<Field>> {
        self.0.page(RawRequest::get(path), &paging, "fields").await
    }
}
