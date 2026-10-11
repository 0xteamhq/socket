//! Owners: the users and queues records are assigned to.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, area};
use crate::models::{GetOwner, ListOwners, Owner, OwnerIdProperty, Paging};

/// The people and queues a record can be assigned to.
#[derive(Debug, Clone, Copy)]
pub struct Owners<'a>(pub(crate) Api<'a>);

impl Owners<'_> {
    /// Lists the account's owners, or the one with an email address.
    pub async fn list(&self, options: ListOwners) -> Result<Page<Owner>> {
        let mut request = RawRequest::get(area("owners"));
        if let Some(email) = &options.email {
            self.0.required("`email`", email)?;
            request = request.with_query("email", email.as_str());
        }
        if let Some(archived) = options.archived {
            request = request.with_query("archived", archived.to_string());
        }
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        // HubSpot returns 100 a page when none is asked for.
        self.0.page(500, request, &paging, "owners").await
    }

    /// Gets one owner: by the id a record's `hubspot_owner_id` holds, or by
    /// the id of the user behind it.
    pub async fn get(&self, owner: &str, options: GetOwner) -> Result<Owner> {
        let id = self.0.segment("an owner id", owner)?;
        let mut request = RawRequest::get(format!("{}/{id}", area("owners")));
        if let Some(property) = options.id_property {
            let name = match property {
                OwnerIdProperty::Id => "id",
                OwnerIdProperty::UserId => "userId",
            };
            request = request.with_query("idProperty", name);
        }
        if let Some(archived) = options.archived {
            request = request.with_query("archived", archived.to_string());
        }
        let owner: Owner = self.0.decode(self.0.send(request).await?, "an owner")?;
        if owner.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without an owner"));
        }
        Ok(owner)
    }
}
