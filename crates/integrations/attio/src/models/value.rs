//! The values of an attribute, as a record or a list entry carries them.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::Actor;
use super::nullable::nullable;

/// Every attribute's values, by the attribute's slug. Attio keeps a list for
/// each attribute, whether it holds one value or several.
pub type Values = BTreeMap<String, Vec<AttributeValue>>;

/// One value of an attribute, with the time it has held since.
///
/// What else it carries depends on `attribute_type`, and is kept in `fields`
/// under Attio's own names: `value` for text, a number, a date or a checkbox;
/// `email_address`, `domain`, `phone_number` or `full_name` for those kinds;
/// `option` for a select and `status` for a status; `target_object` and
/// `target_record_id` for a reference to another record.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct AttributeValue {
    /// When this value began to hold.
    pub active_from: Option<String>,
    /// When it stopped holding; absent while it still does.
    pub active_until: Option<String>,
    pub created_by_actor: Option<Actor>,
    /// `text`, `number`, `select`, `record-reference` and so on.
    #[serde(deserialize_with = "nullable")]
    pub attribute_type: String,
    /// The rest of the value, as Attio sent it.
    #[serde(flatten)]
    pub fields: Map<String, Value>,
}

impl AttributeValue {
    /// Whether the value still holds.
    pub fn is_current(&self) -> bool {
        self.active_until.is_none()
    }

    /// The value in its plainest form: the text, the number, the address,
    /// the title of the chosen option. A kind with no single plain form,
    /// such as a location or a reference to another record, is given whole.
    pub fn plain(&self) -> Value {
        let field = |name: &str| self.fields.get(name).filter(|value| !value.is_null()).cloned();
        let titled = |name: &str| {
            self.fields
                .get(name)
                .and_then(|inner| inner["title"].as_str())
                .map(Value::from)
        };
        let plain = match self.attribute_type.as_str() {
            "currency" => field("currency_value"),
            "domain" => field("domain"),
            "email-address" => field("email_address"),
            "phone-number" => field("phone_number"),
            "personal-name" => field("full_name"),
            "select" => titled("option"),
            "status" => titled("status"),
            _ => self.fields.get("value").cloned(),
        };
        plain.unwrap_or_else(|| Value::Object(self.fields.clone()))
    }
}

/// What each attribute holds now: nothing, one value, or a list of them.
///
/// This is what nearly every reader wants, and what `values` makes them work
/// for. An attribute that may hold several values and holds one is given as
/// that one, not as a list of one.
pub(super) fn current(values: &Values) -> BTreeMap<String, Value> {
    values
        .iter()
        .map(|(attribute, held)| {
            let mut now: Vec<Value> = held
                .iter()
                .filter(|value| value.is_current())
                .map(AttributeValue::plain)
                .collect();
            let plain = match now.len() {
                0 => Value::Null,
                1 => now.remove(0),
                _ => Value::Array(now),
            };
            (attribute.clone(), plain)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn held(values: Value) -> Values {
        serde_json::from_value(values).unwrap()
    }

    #[test]
    fn each_kind_of_value_has_a_plain_form() {
        let at = json!({ "active_from": "2026-01-01T00:00:00.000000000Z", "active_until": null });
        let of = |kind: &str, rest: Value| {
            let mut value = at.clone();
            value["attribute_type"] = json!(kind);
            value.as_object_mut().unwrap().extend(rest.as_object().unwrap().clone());
            vec![value]
        };
        let values = held(json!({
            "name": of("personal-name", json!({ "first_name": "Ada", "last_name": "Lovelace", "full_name": "Ada Lovelace" })),
            "email_addresses": of("email-address", json!({ "email_address": "ada@example.test", "email_domain": "example.test" })),
            "domains": of("domain", json!({ "domain": "example.test", "root_domain": "example.test" })),
            "phone_numbers": of("phone-number", json!({ "phone_number": "+15558675309", "country_code": "US" })),
            "job_title": of("text", json!({ "value": "Analyst" })),
            "employees": of("number", json!({ "value": 42 })),
            "is_customer": of("checkbox", json!({ "value": false })),
            "value": of("currency", json!({ "currency_value": 1200.5, "currency_code": "USD" })),
            "stage": of("status", json!({ "status": { "title": "In progress", "is_archived": false } })),
            "categories": of("select", json!({ "option": { "title": "SaaS", "is_archived": false } })),
            "company": of("record-reference", json!({ "target_object": "companies", "target_record_id": "rec-2" })),
            "notes": [],
        }));
        assert_eq!(
            serde_json::to_value(current(&values)).unwrap(),
            json!({
                "name": "Ada Lovelace",
                "email_addresses": "ada@example.test",
                "domains": "example.test",
                "phone_numbers": "+15558675309",
                "job_title": "Analyst",
                "employees": 42,
                "is_customer": false,
                "value": 1200.5,
                "stage": "In progress",
                "categories": "SaaS",
                "company": { "target_object": "companies", "target_record_id": "rec-2" },
                "notes": null,
            })
        );
    }

    #[test]
    fn several_current_values_are_a_list_and_a_value_that_ended_is_left_out() {
        let values = held(json!({
            "email_addresses": [
                { "attribute_type": "email-address", "active_until": null, "email_address": "ada@example.test" },
                { "attribute_type": "email-address", "active_until": null, "email_address": "ada@work.test" },
                { "attribute_type": "email-address", "active_until": "2025-06-01T00:00:00.000000000Z", "email_address": "old@example.test" },
            ],
            "job_title": [
                { "attribute_type": "text", "active_until": "2025-06-01T00:00:00.000000000Z", "value": "Intern" },
            ],
        }));
        assert_eq!(
            serde_json::to_value(current(&values)).unwrap(),
            json!({ "email_addresses": ["ada@example.test", "ada@work.test"], "job_title": null })
        );
    }

    #[test]
    fn a_value_without_its_plain_field_is_given_whole_and_not_as_nothing() {
        let values = held(json!({
            "categories": [{ "attribute_type": "select", "option": { "id": { "option_id": "opt-1" } } }],
            "falsy": [{ "attribute_type": "checkbox", "value": false }],
        }));
        assert_eq!(
            serde_json::to_value(current(&values)).unwrap(),
            json!({ "categories": { "option": { "id": { "option_id": "opt-1" } } }, "falsy": false })
        );
    }
}
