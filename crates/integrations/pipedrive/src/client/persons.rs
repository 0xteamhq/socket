//! Persons. Version 2 of Pipedrive's API throughout.

use socketkit_core::{Page, RawRequest, Result};

use super::{Api, body, filtered};
use crate::models::{CreatePerson, ListPersons, Paging, Person, SearchPersons, SearchResult, UpdatePerson};

/// Persons: the people a company deals with.
#[derive(Debug, Clone, Copy)]
pub struct Persons<'a>(pub(crate) Api<'a>);

impl Persons<'_> {
    /// Lists persons: all that are not deleted, or those an owner, an
    /// organisation or a deal selects.
    ///
    /// A row leaves out the person's custom fields unless `custom_fields`
    /// names some. `get` returns them all.
    pub async fn list(&self, options: ListPersons) -> Result<Page<Person>> {
        self.0.custom_field_keys(options.custom_fields.as_deref())?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/persons"), &options);
        let mut page: Page<Person> = self.0.page(request, &paging, "persons").await?;
        // Pipedrive sends every custom field when none is asked for.
        if options.custom_fields.as_ref().is_none_or(Vec::is_empty) {
            page.items.iter_mut().for_each(|person| person.custom_fields.clear());
        }
        Ok(page)
    }

    /// Gets one person, with their custom fields.
    pub async fn get(&self, person: u64) -> Result<Person> {
        let request = RawRequest::get(format!("v2/persons/{person}"));
        self.0.one(request, "a person").await
    }

    /// Searches persons by name, email, phone, notes and custom fields.
    pub async fn search(&self, options: SearchPersons) -> Result<Page<SearchResult>> {
        self.0.term(&options.term, options.exact_match)?;
        let paging = Paging {
            cursor: options.cursor.clone(),
            limit: options.limit,
        };
        let request = filtered(RawRequest::get("v2/persons/search"), &options);
        self.0.found(request, &paging, "persons").await
    }

    /// Creates a person.
    pub async fn create(&self, person: CreatePerson) -> Result<Person> {
        self.0.required("`name`", &person.name)?;
        self.0.custom_values(person.custom_fields.as_ref())?;
        let request = RawRequest::post("v2/persons", body(&person));
        self.0.one(request, "a person").await
    }

    /// Changes a person. What is not set is left as it is.
    pub async fn update(&self, person: u64, change: UpdatePerson) -> Result<Person> {
        self.0.custom_values(change.custom_fields.as_ref())?;
        let request = self.0.change(format!("v2/persons/{person}"), &change)?;
        self.0.one(request, "a person").await
    }

    /// Deletes a person. Pipedrive keeps them for 30 days and then removes them for good.
    pub async fn delete(&self, person: u64) -> Result<()> {
        let request = RawRequest::new("DELETE", format!("v2/persons/{person}"));
        self.0.done(request, "a deleted person").await
    }
}
