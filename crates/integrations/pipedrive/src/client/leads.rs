//! Leads. Pipedrive offers them in version 1 of its API, and their search in version 2.

use serde_json::Value;
use socketkit_core::{Page, RawRequest, Result};

use super::{Api, body, filtered, is_uuid};
use crate::models::{CreateLead, Lead, ListLeads, Paging, SearchLeads, SearchResult, UpdateLead, gather_custom_fields};

/// Leads: possible deals that are not yet in a pipeline.
#[derive(Debug, Clone, Copy)]
pub struct Leads<'a>(pub(crate) Api<'a>);

impl Leads<'_> {
    /// Lists the leads that are not archived: all of them, or those an
    /// owner, a person or an organisation selects.
    ///
    /// A row leaves out the lead's custom fields. `get` returns them.
    pub async fn list(&self, options: ListLeads) -> Result<Page<Lead>> {
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v1/leads"), &options);
        let page = self.0.page_by_offset(request, &paging, "leads").await?;
        // Version 1 writes custom fields beside a lead's own, where reading
        // a row passes over them.
        Ok(Page {
            items: self.0.decode(Value::Array(page.items), "leads")?,
            next_cursor: page.next_cursor,
        })
    }

    /// Gets one lead, with its custom fields.
    pub async fn get(&self, lead: &str) -> Result<Lead> {
        self.lead(RawRequest::get(self.path(lead)?)).await
    }

    /// Searches leads by title, notes and custom fields.
    pub async fn search(&self, options: SearchLeads) -> Result<Page<SearchResult>> {
        self.0.term(&options.term, options.exact_match)?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/leads/search"), &options);
        self.0.found(request, &paging, "leads").await
    }

    /// Creates a lead, linked to a person, an organisation or both.
    pub async fn create(&self, lead: CreateLead) -> Result<Lead> {
        self.0.required("`title`", &lead.title)?;
        if lead.person_id.is_none() && lead.organization_id.is_none() {
            return Err(self.0.invalid("a lead needs `person_id`, `organization_id` or both"));
        }
        self.lead(RawRequest::post("v1/leads", body(&lead))).await
    }

    /// Changes a lead, or archives it. What is not set is left as it is.
    pub async fn update(&self, lead: &str, change: UpdateLead) -> Result<Lead> {
        self.lead(self.0.change(self.path(lead)?, &change)?).await
    }

    /// Deletes a lead.
    pub async fn delete(&self, lead: &str) -> Result<()> {
        let request = RawRequest::new("DELETE", self.path(lead)?);
        self.0.done(request, "a deleted lead").await
    }

    /// Sends `request` and reads the lead it answers with, in the shape of version 2.
    async fn lead(&self, request: RawRequest) -> Result<Lead> {
        let mut record = self.0.record(request, "a lead").await?;
        gather_custom_fields(&mut record);
        self.0.decode(record, "a lead")
    }

    /// The path of one lead. Its id is a UUID and is written into the path
    /// only as one: a letter, a digit or a hyphen cannot add a segment, a
    /// query or a fragment.
    fn path(&self, lead: &str) -> Result<String> {
        let lead = lead.trim();
        if !is_uuid(lead) {
            return Err(self
                .0
                .invalid("a lead id is a UUID, such as adf21080-0e10-11eb-879b-05d71fb426ec"));
        }
        Ok(format!("v1/leads/{}", lead.to_ascii_lowercase()))
    }
}
