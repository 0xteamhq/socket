//! Records of any object type: reading, creating, changing and archiving
//! them one at a time.
//!
//! Several at once are in `objects_batch.rs` and searching is in
//! `objects_search.rs`; they are methods of the same group, kept apart as
//! HubSpot keeps its batch and search endpoints apart. What the three files
//! share is here, as methods of the group.
//!
//! The object type is a plain argument, because HubSpot's CRM is one API
//! over all of them: `contacts`, `companies`, `deals`, `tickets`, the
//! engagements `notes`, `calls`, `meetings`, `emails` and `tasks`, and a
//! custom object by its type id, such as `2-3465404`.

use serde_json::{Value, json};
use socketkit_core::{ErrorKind, Page, RawRequest, Result};

use super::{Api, with_query};
use crate::API_VERSION;
use crate::models::{CreateObject, GetObject, ListObjects, Object, Paging, UpdateObject};

/// Records of any object type.
#[derive(Debug, Clone, Copy)]
pub struct Objects<'a>(pub(crate) Api<'a>);

impl Objects<'_> {
    /// Lists the records of an object type, at most 100 a page. Each comes
    /// with the properties named in `options`, beside the few HubSpot always
    /// returns, so a page stays small. `properties.list` says which there are.
    pub async fn list(&self, object_type: &str, options: ListObjects) -> Result<Page<Object>> {
        let request = RawRequest::get(self.path(object_type)?);
        let request = with_query(request, "properties", listed(options.properties.as_deref()));
        let request = with_query(request, "associations", listed(options.associations.as_deref()));
        let request = with_query(
            request,
            "archived",
            options.archived.map(|archived| archived.to_string()),
        );
        let paging = Paging {
            cursor: options.cursor,
            limit: options.limit,
        };
        self.0.page(Some(100), request, &paging, "records").await
    }

    /// Gets one record, by its id or by the value of a unique property.
    pub async fn get(&self, object_type: &str, id: &str, options: GetObject) -> Result<Object> {
        let request = RawRequest::get(self.record_path(object_type, id)?);
        let request = with_query(request, "properties", listed(options.properties.as_deref()));
        let history = listed(options.properties_with_history.as_deref());
        let request = with_query(request, "propertiesWithHistory", history);
        let request = with_query(request, "associations", listed(options.associations.as_deref()));
        let request = with_query(request, "idProperty", self.named(options.id_property.as_deref()));
        let request = with_query(
            request,
            "archived",
            options.archived.map(|archived| archived.to_string()),
        );
        self.record(self.0.send(request).await?)
    }

    /// Creates a record, and associates it with others when asked to.
    pub async fn create(&self, object_type: &str, record: CreateObject) -> Result<Object> {
        let path = self.path(object_type)?;
        let body = self.new_record(&record)?;
        self.record(self.0.send(RawRequest::post(path, body)).await?)
    }

    /// Changes the properties given on a record and leaves the rest.
    pub async fn update(&self, object_type: &str, id: &str, changes: UpdateObject) -> Result<Object> {
        if changes.properties.is_empty() {
            return Err(self
                .0
                .error(ErrorKind::InvalidInput, "`properties` names nothing to change"));
        }
        let request = RawRequest::new("PATCH", self.record_path(object_type, id)?)
            .with_body(json!({ "properties": changes.properties }));
        let request = with_query(request, "idProperty", self.named(changes.id_property.as_deref()));
        self.record(self.0.send(request).await?)
    }

    /// Moves a record to HubSpot's recycling bin, where a person can
    /// restore it for a time: 90 days for a contact, HubSpot says.
    pub async fn archive(&self, object_type: &str, id: &str) -> Result<()> {
        let path = self.record_path(object_type, id)?;
        self.0.send(RawRequest::new("DELETE", path)).await?;
        Ok(())
    }

    /// The address of an object type's records: `crm/objects/2026-09/contacts`.
    pub(super) fn path(&self, object_type: &str) -> Result<String> {
        let object_type = self.0.object_type("`object_type`", object_type)?;
        Ok(format!("crm/objects/{API_VERSION}/{object_type}"))
    }

    /// The address of one record.
    fn record_path(&self, object_type: &str, id: &str) -> Result<String> {
        let id = self.0.segment("`id`", id)?;
        Ok(format!("{}/{id}", self.path(object_type)?))
    }

    /// The body that creates `record`. HubSpot's reference requires the list
    /// of associations, so it is sent when it is empty too.
    pub(super) fn new_record(&self, record: &CreateObject) -> Result<Value> {
        let invalid = |message: &str| self.0.error(ErrorKind::InvalidInput, message);
        if record.properties.is_empty() {
            return Err(invalid("a record needs `properties`"));
        }
        let associations = record.associations.as_deref().unwrap_or_default();
        if associations
            .iter()
            .any(|association| association.to.id.trim().is_empty())
        {
            return Err(invalid("every association needs `to.id`"));
        }
        if associations.iter().any(|association| association.types.is_empty()) {
            return Err(invalid("every association needs one of `types`"));
        }
        Ok(json!({ "properties": record.properties, "associations": associations }))
    }

    /// The name of a property, when one was given that says something.
    pub(super) fn named<'n>(&self, property: Option<&'n str>) -> Option<&'n str> {
        property.map(str::trim).filter(|property| !property.is_empty())
    }

    fn record(&self, body: Value) -> Result<Object> {
        let record: Object = self.0.decode(body, "a record")?;
        if record.id.is_empty() {
            return Err(self.0.error(ErrorKind::Decode, "hubspot answered without a record"));
        }
        Ok(record)
    }
}

/// Names as HubSpot takes a list of them in a query: joined by commas.
/// `None` when there is nothing to name.
fn listed(names: Option<&[String]>) -> Option<String> {
    let names: Vec<&str> = names
        .unwrap_or_default()
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    (!names.is_empty()).then(|| names.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_joined_by_commas_and_blank_ones_are_dropped() {
        let names = |names: &[&str]| names.iter().map(|name| (*name).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            listed(Some(&names(&["email", " firstname ", ""]))),
            Some("email,firstname".into())
        );
        assert_eq!(listed(Some(&names(&[]))), None);
        assert_eq!(listed(Some(&names(&["", " "]))), None);
        assert_eq!(listed(None), None);
    }
}
