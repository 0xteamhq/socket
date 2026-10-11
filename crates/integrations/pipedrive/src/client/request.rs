//! What goes into a request: the query a list's filters make, the body a
//! create or an update carries, and the one id that is not a number.
//!
//! This is part of the shared access to the API, not a group of methods; it
//! has its own file only to keep `mod.rs` short.

use serde::Serialize;
use serde_json::{Map, Value};
use socketkit_core::RawRequest;

/// `request` with the set fields of `options` as its query, each under its
/// own name. A list is written as Pipedrive reads one, joined by commas.
///
/// `cursor` and `limit` are left out: paging writes them, once it has
/// checked them.
pub(crate) fn filtered(request: RawRequest, options: &impl Serialize) -> RawRequest {
    let Ok(Value::Object(fields)) = serde_json::to_value(options) else {
        return request;
    };
    fields
        .into_iter()
        .filter(|(name, _)| name != "cursor" && name != "limit")
        .fold(request, |request, (name, value)| match written(&value) {
            Some(text) => request.with_query(name, text),
            None => request,
        })
}

/// A value as it is written in a query, if it has one: unset, an empty list
/// and an object have none.
fn written(value: &Value) -> Option<String> {
    match value {
        Value::Null | Value::Object(_) => None,
        Value::String(text) => Some(text.clone()),
        Value::Bool(_) | Value::Number(_) => Some(value.to_string()),
        Value::Array(items) => {
            let items: Vec<String> = items.iter().filter_map(written).collect();
            (!items.is_empty()).then(|| items.join(","))
        }
    }
}

/// `text` with everything percent-encoded but the characters a URL always
/// leaves alone: letters, digits and `-._~`.
pub(crate) fn encoded(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

/// The JSON body for a create or an update: the set fields of `content`.
///
/// Unset fields are left out at every depth, so Pipedrive applies its own
/// defaults and a change touches only what was named. Inside `custom_fields`
/// a `null` is kept: there it is the caller's own, and means "clear this
/// field".
pub(crate) fn body(content: &impl Serialize) -> Value {
    match serde_json::to_value(content) {
        Ok(Value::Object(fields)) => without_nulls(fields),
        _ => Value::Object(Map::new()),
    }
}

fn without_nulls(fields: Map<String, Value>) -> Value {
    fn inside(value: Value) -> Value {
        match value {
            Value::Object(fields) => without_nulls(fields),
            Value::Array(items) => Value::Array(items.into_iter().map(inside).collect()),
            other => other,
        }
    }
    Value::Object(
        fields
            .into_iter()
            .filter(|(_, value)| !value.is_null())
            .map(|(name, value)| {
                let kept = if name == "custom_fields" { value } else { inside(value) };
                (name, kept)
            })
            .collect(),
    )
}

/// True for a UUID as Pipedrive writes one: 8-4-4-4-12, in hexadecimal.
pub(crate) fn is_uuid(text: &str) -> bool {
    text.split('-').map(str::len).eq([8, 4, 4, 4, 12]) && text.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-')
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::models::{DealStatusFilter, ListDeals, UpdateDeal};

    fn query_of(options: &impl Serialize) -> Vec<(String, String)> {
        filtered(RawRequest::get("v2/deals"), options).query
    }

    #[test]
    fn set_filters_become_the_query_and_paging_is_left_to_paging() {
        let options = ListDeals {
            owner_id: Some(7),
            status: Some(vec![DealStatusFilter::Open, DealStatusFilter::Won]),
            updated_since: Some("2026-10-01T00:00:00Z".into()),
            custom_fields: Some(Vec::new()),
            cursor: Some("eyJpZCI6MX0".into()),
            limit: Some(50),
            ..ListDeals::default()
        };
        assert_eq!(
            query_of(&options),
            [
                ("owner_id".to_owned(), "7".to_owned()),
                ("status".to_owned(), "open,won".to_owned()),
                ("updated_since".to_owned(), "2026-10-01T00:00:00Z".to_owned()),
            ],
            "an empty list, an unset field, the cursor and the limit are not written"
        );
        assert!(query_of(&ListDeals::default()).is_empty());
    }

    #[test]
    fn everything_but_plain_characters_is_percent_encoded() {
        assert_eq!(encoded("Acme & Sons/é?#"), "Acme%20%26%20Sons%2F%C3%A9%3F%23");
        assert_eq!(encoded("a-b.c_d~e"), "a-b.c_d~e");
    }

    #[test]
    fn a_body_drops_what_is_unset_and_keeps_a_null_that_clears_a_custom_field() {
        let change = UpdateDeal {
            stage_id: Some(4),
            custom_fields: Some(
                [
                    ("a".repeat(40), json!(null)),
                    ("b".repeat(40), json!({ "value": 10, "currency": null })),
                ]
                .into(),
            ),
            ..UpdateDeal::default()
        };
        assert_eq!(
            body(&change),
            json!({
                "stage_id": 4,
                "custom_fields": { "a".repeat(40): null, "b".repeat(40): { "value": 10, "currency": null } }
            })
        );
        assert_eq!(body(&UpdateDeal::default()), json!({}));
    }

    #[test]
    fn unset_fields_are_dropped_inside_a_list_and_an_object_too() {
        let content = json!({ "location": { "value": "1 Main St", "country": null }, "participants": [{ "person_id": 3, "primary": null }] });
        assert_eq!(
            body(&content),
            json!({ "location": { "value": "1 Main St" }, "participants": [{ "person_id": 3 }] })
        );
    }

    #[test]
    fn a_lead_id_is_a_uuid_and_nothing_else() {
        assert!(is_uuid("adf21080-0e10-11eb-879b-05d71fb426ec"));
        assert!(is_uuid("ADF21080-0E10-11EB-879B-05D71FB426EC"));
        for bad in [
            "",
            "42",
            "adf21080-0e10-11eb-879b-05d71fb426e",
            "adf21080-0e10-11eb-879b-05d71fb426ecc",
            "adf210800e1011eb879b05d71fb426ec",
            "adf21080-0e10-11eb-879b-05d71fb426eg",
            "adf21080-0e10-11eb-879b-05d71fb4/6ec",
            "adf21080-0e10-11eb-879b-..%2Fdeals%2F",
            "../deals/1",
            "adf21080-0e10-11eb-879b-05d71fb426ec/notes",
            "adf21080-0e10-11eb-879b-05d71fb426ec?x=1",
            "adf21080-0e10-11eb-879b-05d71fb426e\u{e9}",
        ] {
            assert!(!is_uuid(bad), "{bad:?}");
        }
    }
}
