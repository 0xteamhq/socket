//! Reading the values HubSpot writes in more than one way.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Reads `null` as the type's default. HubSpot writes `null` where a string,
/// a flag or a list has no value.
pub(super) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Reads an id as text, whether HubSpot wrote it as a string or as a number.
///
/// Its reference says a record id is a string, and its own examples of an
/// association write one as a number. Both are the same id.
pub(super) fn id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::String(text) => Ok(text),
        Value::Number(number) => Ok(number.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err(serde::de::Error::custom("an id is a string or a number")),
    }
}

#[cfg(test)]
mod tests {
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Debug, Default, Deserialize)]
    #[serde(default)]
    struct Row {
        #[serde(deserialize_with = "super::id")]
        id: String,
        #[serde(deserialize_with = "super::nullable")]
        labels: Vec<String>,
    }

    #[test]
    fn an_id_reads_the_same_as_a_string_or_a_number() {
        let text: Row = serde_json::from_value(json!({ "id": "5790939450" })).unwrap();
        let number: Row = serde_json::from_value(json!({ "id": 5_790_939_450_u64 })).unwrap();
        assert_eq!(text.id, "5790939450");
        assert_eq!(number.id, "5790939450");
    }

    #[test]
    fn what_is_null_or_missing_is_empty_and_what_is_not_an_id_is_refused() {
        let empty: Row = serde_json::from_value(json!({ "id": null, "labels": null })).unwrap();
        assert_eq!((empty.id.as_str(), empty.labels.len()), ("", 0));
        let missing: Row = serde_json::from_value(json!({})).unwrap();
        assert_eq!((missing.id.as_str(), missing.labels.len()), ("", 0));
        assert!(serde_json::from_value::<Row>(json!({ "id": { "nested": 1 } })).is_err());
        assert!(serde_json::from_value::<Row>(json!({ "id": true })).is_err());
    }
}
