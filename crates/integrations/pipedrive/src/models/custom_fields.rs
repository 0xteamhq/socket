//! The fields a company added to its records.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Field;

/// A record's custom fields: each value under its field's 40-character key,
/// as Pipedrive sends it. The key is the `field_code` of a [`Field`], which
/// is where its name is; see [`named_custom_fields`].
pub type CustomFields = BTreeMap<String, Value>;

/// One custom field of a record, with what the company called it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NamedValue {
    /// The field's key in `custom_fields`.
    pub field_code: String,
    /// The field's name, when the list of fields has this key.
    pub field_name: Option<String>,
    /// The value as Pipedrive sent it.
    pub value: Value,
    /// For a field of choices, the label of each choice the value names.
    pub labels: Vec<String>,
}

/// Puts names to a record's custom fields.
///
/// `fields` is the list for the record's kind, from `fields.deal_fields`,
/// `fields.person_fields` or `fields.organization_fields`; a lead has a
/// deal's fields. The list changes rarely, so read it once and use it for
/// many records. A key the list does not have keeps its value and gets no name.
pub fn named_custom_fields(custom_fields: &CustomFields, fields: &[Field]) -> Vec<NamedValue> {
    custom_fields
        .iter()
        .map(|(code, value)| {
            let field = fields.iter().find(|field| field.field_code == *code);
            // A field of choices holds one id or a list of them; with option
            // labels asked for, Pipedrive sends `{ id, label }` in their place.
            let chosen: Vec<&Value> = match value {
                Value::Array(ids) => ids.iter().collect(),
                Value::Null => Vec::new(),
                one => vec![one],
            };
            let labels = field
                .map(|field| {
                    chosen
                        .iter()
                        .map(|id| id.get("id").unwrap_or(id))
                        .filter_map(|id| field.options.iter().find(|option| option.id == *id))
                        .filter_map(|option| option.label.clone())
                        .collect()
                })
                .unwrap_or_default();
            NamedValue {
                field_code: code.clone(),
                field_name: field.and_then(|field| field.field_name.clone()),
                value: value.clone(),
                labels,
            }
        })
        .collect()
}

/// True for the key of a custom field as it is written to: 40 hexadecimal
/// characters and nothing else.
pub(crate) fn is_field_key(key: &str) -> bool {
    key.len() == 40 && key.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// True for the key of a custom field as version 1 returns one: a field's
/// key, or that and a part after `_`, such as the `_currency` beside a sum
/// of money.
fn is_custom_key(key: &str) -> bool {
    let (hash, rest) = key.split_at_checked(40).unwrap_or((key, ""));
    is_field_key(hash) && (rest.is_empty() || rest.starts_with('_'))
}

/// Moves a version 1 record's custom fields under `custom_fields`, where
/// version 2 has them.
///
/// Version 1 writes each custom field beside the record's own fields, under
/// its key. Gathered, a lead reads like a deal, and the model does not have
/// to know the keys.
pub(crate) fn gather_custom_fields(record: &mut Value) {
    let Value::Object(fields) = record else {
        return;
    };
    let keys: Vec<String> = fields.keys().filter(|key| is_custom_key(key)).cloned().collect();
    if keys.is_empty() {
        return;
    }
    let gathered: serde_json::Map<String, Value> = keys
        .into_iter()
        .filter_map(|key| fields.remove(&key).map(|value| (key, value)))
        .collect();
    fields.insert("custom_fields".to_owned(), Value::Object(gathered));
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    const INDUSTRY: &str = "4d1d7a5b1b5a2c5b6a3e9f8d7c6b5a4f3e2d1c0b";
    const BUDGET: &str = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2";

    fn fields() -> Vec<Field> {
        serde_json::from_value(json!([
            { "field_code": INDUSTRY, "field_name": "Industry", "field_type": "enum",
              "options": [{ "id": 12, "label": "Software" }, { "id": 13, "label": "Retail" }] },
            { "field_code": BUDGET, "field_name": "Budget", "field_type": "monetary", "options": null },
            { "field_code": "title", "field_name": "Title", "field_type": "varchar" }
        ]))
        .unwrap()
    }

    #[test]
    fn a_custom_field_gets_its_name_and_the_labels_of_its_choices() {
        let record: CustomFields = serde_json::from_value(json!({
            INDUSTRY: 12,
            BUDGET: { "value": 5000, "currency": "EUR" },
            "ffffffffffffffffffffffffffffffffffffffff": "left by a deleted field"
        }))
        .unwrap();
        let named = named_custom_fields(&record, &fields());
        let find = |code: &str| named.iter().find(|n| n.field_code == code).unwrap();

        assert_eq!(find(INDUSTRY).field_name.as_deref(), Some("Industry"));
        assert_eq!(find(INDUSTRY).labels, ["Software"]);
        assert_eq!(find(INDUSTRY).value, json!(12), "the value itself is kept");
        assert_eq!(find(BUDGET).field_name.as_deref(), Some("Budget"));
        assert!(find(BUDGET).labels.is_empty(), "a sum of money is not a choice");
        let unknown = find("ffffffffffffffffffffffffffffffffffffffff");
        assert_eq!(
            unknown.field_name, None,
            "a key the list lacks keeps its value, unnamed"
        );
        assert_eq!(unknown.value, json!("left by a deleted field"));
    }

    #[test]
    fn several_choices_and_choices_sent_with_their_labels_are_both_read() {
        let several: CustomFields = serde_json::from_value(json!({ INDUSTRY: [13, 12, 99] })).unwrap();
        assert_eq!(
            named_custom_fields(&several, &fields())[0].labels,
            ["Retail", "Software"],
            "an id with no option is left out"
        );
        let labelled: CustomFields =
            serde_json::from_value(json!({ INDUSTRY: { "id": 13, "label": "Retail" } })).unwrap();
        assert_eq!(named_custom_fields(&labelled, &fields())[0].labels, ["Retail"]);
        let unset: CustomFields = serde_json::from_value(json!({ INDUSTRY: null })).unwrap();
        assert!(named_custom_fields(&unset, &fields())[0].labels.is_empty());
    }

    #[test]
    fn only_a_forty_character_hexadecimal_key_is_a_custom_field() {
        assert!(is_custom_key(INDUSTRY));
        assert!(is_custom_key(&format!("{INDUSTRY}_currency")));
        // What is written to is the field's key itself, with nothing after it.
        assert!(is_field_key(INDUSTRY));
        assert!(!is_field_key(&format!("{INDUSTRY}_currency")));
        for own in ["status", "is_deleted", "", &INDUSTRY[..39], &INDUSTRY.to_uppercase()] {
            assert!(!is_field_key(own), "{own:?}");
        }
        for own in [
            "title",
            "id",
            "",
            &INDUSTRY[..39],
            &format!("{INDUSTRY}0"),
            &format!("{INDUSTRY}currency"),
            &INDUSTRY.to_uppercase(),
            "expected_close_date_and_some_more_text_x",
            "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz",
            "ééééééééééééééééééééééééééééééééééééééé",
        ] {
            assert!(!is_custom_key(own), "{own:?}");
        }
    }

    #[test]
    fn a_version_1_records_custom_fields_are_gathered_and_its_own_fields_left() {
        let mut lead = json!({
            "id": "adf21080-0e10-11eb-879b-05d71fb426ec",
            "title": "Jane Doe lead",
            INDUSTRY: 12,
            format!("{BUDGET}_currency"): "EUR",
            BUDGET: 5000
        });
        gather_custom_fields(&mut lead);
        assert_eq!(
            lead,
            json!({
                "id": "adf21080-0e10-11eb-879b-05d71fb426ec",
                "title": "Jane Doe lead",
                "custom_fields": { INDUSTRY: 12, BUDGET: 5000, format!("{BUDGET}_currency"): "EUR" }
            })
        );

        let mut plain = json!({ "id": "x", "title": "No custom fields" });
        gather_custom_fields(&mut plain);
        assert_eq!(plain, json!({ "id": "x", "title": "No custom fields" }));
    }
}
