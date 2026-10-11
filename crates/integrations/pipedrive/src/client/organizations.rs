//! Organisations. Version 2 of Pipedrive's API throughout.

use socketkit_core::{Page, RawRequest, Result};

use super::{Api, body, filtered};
use crate::models::{
    CreateOrganization, ListOrganizations, Organization, Paging, SearchOrganizations, SearchResult, UpdateOrganization,
};

/// Organisations: the companies a company deals with.
#[derive(Debug, Clone, Copy)]
pub struct Organizations<'a>(pub(crate) Api<'a>);

impl Organizations<'_> {
    /// Lists organisations: all that are not deleted, or those an owner selects.
    ///
    /// A row leaves out the organisation's custom fields unless
    /// `custom_fields` names some. `get` returns them all.
    pub async fn list(&self, options: ListOrganizations) -> Result<Page<Organization>> {
        self.0.custom_field_keys(options.custom_fields.as_deref())?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/organizations"), &options);
        let mut page: Page<Organization> = self.0.page(request, &paging, "organizations").await?;
        // Pipedrive sends every custom field when none is asked for.
        if options.custom_fields.as_ref().is_none_or(Vec::is_empty) {
            page.items
                .iter_mut()
                .for_each(|organization| organization.custom_fields.clear());
        }
        Ok(page)
    }

    /// Gets one organisation, with its custom fields.
    pub async fn get(&self, organization: u64) -> Result<Organization> {
        let request = RawRequest::get(format!("v2/organizations/{organization}"));
        self.0.one(request, "an organization").await
    }

    /// Searches organisations by name, address, notes and custom fields.
    pub async fn search(&self, options: SearchOrganizations) -> Result<Page<SearchResult>> {
        self.0.term(&options.term, options.exact_match)?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/organizations/search"), &options);
        self.0.found(request, &paging, "organizations").await
    }

    /// Creates an organisation.
    pub async fn create(&self, organization: CreateOrganization) -> Result<Organization> {
        self.0.required("`name`", &organization.name)?;
        self.0.custom_values(organization.custom_fields.as_ref())?;
        let request = RawRequest::post("v2/organizations", body(&organization));
        self.0.one(request, "an organization").await
    }

    /// Changes an organisation. What is not set is left as it is.
    pub async fn update(&self, organization: u64, change: UpdateOrganization) -> Result<Organization> {
        self.0.custom_values(change.custom_fields.as_ref())?;
        let request = self.0.change(format!("v2/organizations/{organization}"), &change)?;
        self.0.one(request, "an organization").await
    }

    /// Deletes an organisation. Pipedrive keeps it for 30 days and then removes it for good.
    pub async fn delete(&self, organization: u64) -> Result<()> {
        let request = RawRequest::new("DELETE", format!("v2/organizations/{organization}"));
        self.0.done(request, "a deleted organization").await
    }
}
