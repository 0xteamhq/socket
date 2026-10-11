//! Owners: the people and queues a record can be assigned to.

use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, with_query};
use crate::API_VERSION;
use crate::models::{GetOwner, ListOwners, Owner, OwnerIdProperty, Paging};

/// The owners of an account.
#[derive(Debug, Clone, Copy)]
pub struct Owners<'a>(pub(crate) Api<'a>);

impl Owners<'_> {
    /// Lists the account's owners, or finds the one with an email address.
    pub async fn list(&self, options: ListOwners) -> Result<Page<Owner>> {
        let request = RawRequest::get(format!("crm/owners/{API_VERSION}"));
        let email = options
            .email
            .as_deref()
            .map(str::trim)
            .filter(|email| !email.is_empty());
        let request = with_query(request, "email", email);
        let request = with_query(
            request,
            "archived",
            options.archived.map(|archived| archived.to_string()),
        );
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.page(None, request, &paging, "owners").await
    }

    /// Gets one owner, by the owner's id or by the id of the user behind it.
    pub async fn get(&self, owner: &str, options: GetOwner) -> Result<Owner> {
        let owner = self.0.segment("`owner`", owner)?;
        let request = RawRequest::get(format!("crm/owners/{API_VERSION}/{owner}"));
        let id_property = options.id_property.map(|property| match property {
            OwnerIdProperty::Id => "id",
            OwnerIdProperty::UserId => "userId",
        });
        let request = with_query(request, "idProperty", id_property);
        let request = with_query(
            request,
            "archived",
            options.archived.map(|archived| archived.to_string()),
        );
        let owner: Owner = self.0.decode(self.0.send(request).await?, "an owner")?;
        if owner.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without an owner"));
        }
        Ok(owner)
    }
}
