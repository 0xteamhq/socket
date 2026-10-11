//! Records: one row of any object type.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::is_record_id;

/// One record of any object type.
///
/// Salesforce writes a record as its fields side by side with an
/// `attributes` entry that names the type. Here the type and the id are
/// taken out, and `fields` holds everything else, exactly as Salesforce
/// wrote it. A relationship that was asked for comes back nested and is
/// kept that way: the record a lookup leads to under the relationship's
/// name (`Owner`), and the records of a subquery under theirs (`Contacts`).
/// [`Record::parent`] and [`Record::children`] read them.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Record {
    /// The object type, such as `Account` or `Invoice__c`. A row of a query
    /// that counts or groups is an `AggregateResult`.
    #[serde(rename = "type")]
    pub object_type: String,
    /// The record's id. Absent for a row that is not a stored record, such
    /// as an `AggregateResult`.
    pub id: Option<String>,
    /// The fields that were asked for, by API name.
    pub fields: Map<String, Value>,
}

impl Record {
    /// Reads a record as Salesforce writes one: an object with `attributes`
    /// beside its fields. `None` when `value` is not that.
    ///
    /// The id is the `Id` field. When the fields asked for did not include
    /// it, it is read from the record's own address in `attributes`.
    pub fn from_wire(value: Value) -> Option<Self> {
        let Value::Object(mut fields) = value else {
            return None;
        };
        let attributes = fields.remove("attributes")?;
        let object_type = attributes["type"].as_str().filter(|name| !name.is_empty())?.to_owned();
        let selected = fields.remove("Id").and_then(|id| id.as_str().map(str::to_owned));
        let addressed = || {
            let last = attributes["url"].as_str()?.rsplit('/').next()?;
            is_record_id(last).then(|| last.to_owned())
        };
        let id = selected.filter(|id| !id.is_empty()).or_else(addressed);
        Some(Self {
            object_type,
            id,
            fields,
        })
    }

    /// The record a relationship field leads to, when the query asked for
    /// one of its fields: `Owner` after `SELECT Owner.Name FROM Account`.
    /// `None` when it was not asked for, or the lookup is empty.
    pub fn parent(&self, relationship: &str) -> Option<Record> {
        Record::from_wire(self.fields.get(relationship)?.clone())
    }

    /// The records of a child relationship that a subquery asked for:
    /// `Contacts` after `SELECT (SELECT LastName FROM Contacts) FROM Account`.
    /// Empty when it was not asked for, or there are none.
    pub fn children(&self, relationship: &str) -> Vec<Record> {
        self.fields
            .get(relationship)
            .and_then(|related| related["records"].as_array())
            .map(|records| records.iter().cloned().filter_map(Record::from_wire).collect())
            .unwrap_or_default()
    }
}

/// The fields of a record to write.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RecordFields {
    /// The values to set, by field API name: `{ "Name": "Acme", "Industry":
    /// "Energy" }`. `null` clears a field. A lookup can be set through the
    /// other record's external id: `{ "Account": { "ERP_Id__c": "A-17" } }`.
    pub fields: Map<String, Value>,
}

/// Which fields of a record to return.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct GetRecord {
    /// The API names of the fields to return, such as `["Name",
    /// "Industry"]`. Every field the account can see when not given, which
    /// for a large object is a great deal: name the fields that are needed.
    pub fields: Option<Vec<String>>,
}

/// What a create or an upsert did.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Saved {
    /// The id of the record that was created or changed.
    pub id: String,
    /// True when a record was created, false when one that existed was changed.
    pub created: bool,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn account() -> Value {
        json!({
            "attributes": { "type": "Account", "url": "/services/data/v67.0/sobjects/Account/001xx000003DGb2AAG" },
            "Id": "001xx000003DGb2AAG",
            "Name": "Acme",
            "AnnualRevenue": 1250000.5,
            "BillingAddress": { "city": "Paris", "country": "France", "street": null },
            "Owner": {
                "attributes": { "type": "User", "url": "/services/data/v67.0/sobjects/User/005xx000001SvogAAC" },
                "Name": "Ada Lovelace",
                "Manager": { "attributes": { "type": "User", "url": "/services/data/v67.0/sobjects/User/005xx000001Sv7hAAC" }, "Email": "grace@acme.example" }
            },
            "Parent": null,
            "Contacts": {
                "totalSize": 2, "done": true,
                "records": [
                    { "attributes": { "type": "Contact", "url": "/services/data/v67.0/sobjects/Contact/003xx000004TmiQAAS" }, "Id": "003xx000004TmiQAAS", "LastName": "Hopper" },
                    { "attributes": { "type": "Contact", "url": "/services/data/v67.0/sobjects/Contact/003xx000004TmiRAAS" }, "LastName": "Turing" }
                ]
            }
        })
    }

    #[test]
    fn a_record_is_its_type_its_id_and_its_fields() {
        let record = Record::from_wire(account()).unwrap();
        assert_eq!(record.object_type, "Account");
        assert_eq!(record.id.as_deref(), Some("001xx000003DGb2AAG"));
        assert_eq!(record.fields["Name"], "Acme");
        assert_eq!(record.fields["AnnualRevenue"], 1250000.5);
        assert_eq!(
            record.fields["BillingAddress"]["city"], "Paris",
            "a compound field is kept whole"
        );
        assert!(
            record.fields["Parent"].is_null(),
            "an empty lookup stays in the fields, as null"
        );
        assert!(!record.fields.contains_key("attributes"));
        assert!(!record.fields.contains_key("Id"), "the id is said once");
    }

    #[test]
    fn relationships_stay_nested_and_survive_being_written_and_read_again() {
        let record = Record::from_wire(account()).unwrap();
        let written = serde_json::to_value(&record).unwrap();
        assert_eq!(written["type"], "Account");
        assert_eq!(written["id"], "001xx000003DGb2AAG");
        assert_eq!(written["fields"]["Owner"]["Manager"]["Email"], "grace@acme.example");
        assert_eq!(written["fields"]["Contacts"]["records"][1]["LastName"], "Turing");
        let read: Record = serde_json::from_value(written).unwrap();
        assert_eq!(read, record, "nothing is lost on the way");

        let owner = read.parent("Owner").unwrap();
        assert_eq!(
            (owner.object_type.as_str(), owner.id.as_deref()),
            ("User", Some("005xx000001SvogAAC"))
        );
        assert_eq!(owner.fields["Name"], "Ada Lovelace");
        let manager = owner.parent("Manager").unwrap();
        assert_eq!(manager.fields["Email"], "grace@acme.example");
        assert_eq!(read.parent("Parent"), None, "an empty lookup leads nowhere");
        assert_eq!(read.parent("BillingAddress"), None, "an address is not a record");
        assert_eq!(read.parent("Nothing"), None);

        let contacts = read.children("Contacts");
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].fields["LastName"], "Hopper");
        // The second was selected without its id, which its address still gives.
        assert_eq!(contacts[1].id.as_deref(), Some("003xx000004TmiRAAS"));
        assert!(read.children("Owner").is_empty(), "a lookup is not a list");
        assert!(read.children("Opportunities").is_empty());
    }

    #[test]
    fn a_row_that_is_not_a_stored_record_has_no_id() {
        let counted = json!({ "attributes": { "type": "AggregateResult" }, "expr0": 42, "Industry": "Energy" });
        let row = Record::from_wire(counted).unwrap();
        assert_eq!((row.object_type.as_str(), row.id), ("AggregateResult", None));
        assert_eq!(row.fields["expr0"], 42);
        // An address that does not end in an id gives none.
        let odd =
            json!({ "attributes": { "type": "Account", "url": "/services/data/v67.0/sobjects/Account/describe" } });
        assert_eq!(Record::from_wire(odd).unwrap().id, None);
    }

    #[test]
    fn what_is_not_a_record_is_not_read_as_one() {
        for not_a_record in [
            json!(null),
            json!("001xx000003DGb2AAG"),
            json!([]),
            json!({}),
            json!({ "Id": "001xx000003DGb2AAG", "Name": "Acme" }),
            json!({ "attributes": {}, "Name": "Acme" }),
            json!({ "attributes": { "type": "" } }),
            json!({ "attributes": { "type": 7 } }),
        ] {
            assert_eq!(Record::from_wire(not_a_record.clone()), None, "{not_a_record}");
        }
    }

    #[test]
    fn an_id_is_fifteen_or_eighteen_letters_and_digits() {
        assert!(is_record_id("001xx000003DGb2"));
        assert!(is_record_id("001xx000003DGb2AAG"));
        for bad in [
            "",
            "001",
            "001xx000003DGb2A",
            "001xx000003DGb2AA/",
            "001xx000003DGb2AAG7",
            "001xx000003DG/2",
            "001xx000003DGb2AA é",
        ] {
            assert!(!is_record_id(bad), "{bad:?}");
        }
    }
}
