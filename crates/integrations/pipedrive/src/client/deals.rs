//! Deals. Version 2 of Pipedrive's API throughout.

use socketkit_core::{Page, RawRequest, Result};

use super::{Api, body, filtered};
use crate::models::{CreateDeal, Deal, ListDeals, Paging, SearchDeals, SearchResult, UpdateDeal};

/// Deals.
#[derive(Debug, Clone, Copy)]
pub struct Deals<'a>(pub(crate) Api<'a>);

impl Deals<'_> {
    /// Lists deals: all that are not deleted, or those an owner, a person,
    /// an organisation, a pipeline, a stage or a status selects.
    ///
    /// A row leaves out the deal's custom fields unless `custom_fields`
    /// names some. `get` returns them all.
    pub async fn list(&self, options: ListDeals) -> Result<Page<Deal>> {
        self.0.custom_field_keys(options.custom_fields.as_deref())?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/deals"), &options);
        let mut page: Page<Deal> = self.0.page(request, &paging, "deals").await?;
        // Pipedrive sends every custom field when none is asked for.
        if options.custom_fields.as_ref().is_none_or(Vec::is_empty) {
            page.items.iter_mut().for_each(|deal| deal.custom_fields.clear());
        }
        Ok(page)
    }

    /// Gets one deal, with its custom fields.
    pub async fn get(&self, deal: u64) -> Result<Deal> {
        self.0.one(RawRequest::get(format!("v2/deals/{deal}")), "a deal").await
    }

    /// Searches deals by title, notes and custom fields.
    pub async fn search(&self, options: SearchDeals) -> Result<Page<SearchResult>> {
        self.0.term(&options.term, options.exact_match)?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/deals/search"), &options);
        self.0.found(request, &paging, "deals").await
    }

    /// Creates a deal.
    pub async fn create(&self, deal: CreateDeal) -> Result<Deal> {
        self.0.required("`title`", &deal.title)?;
        self.0.custom_values(deal.custom_fields.as_ref())?;
        let request = RawRequest::post("v2/deals", body(&deal));
        self.0.one(request, "a deal").await
    }

    /// Changes a deal: its stage, its status, its value, or anything else
    /// that is set. What is not set is left as it is.
    pub async fn update(&self, deal: u64, change: UpdateDeal) -> Result<Deal> {
        self.0.custom_values(change.custom_fields.as_ref())?;
        let request = self.0.change(format!("v2/deals/{deal}"), &change)?;
        self.0.one(request, "a deal").await
    }

    /// Deletes a deal. Pipedrive keeps it for 30 days and then removes it for good.
    pub async fn delete(&self, deal: u64) -> Result<()> {
        let request = RawRequest::new("DELETE", format!("v2/deals/{deal}"));
        self.0.done(request, "a deleted deal").await
    }
}
