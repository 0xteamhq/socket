//! Reading a field that may be `null`, or that Pipedrive writes in two ways.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Reads `null` as the type's default. Pipedrive writes `null` where a list
/// or a set of custom fields has no value.
pub(super) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// Reads a value Pipedrive writes as a string in one version of its API and
/// as a number in the other, such as a lead's `visible_to`.
pub(super) fn text_or_number<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::Null => Ok(None),
        Value::String(text) => Ok(Some(text)),
        Value::Number(number) => Ok(Some(number.to_string())),
        _ => Err(serde::de::Error::custom("expected a string or a number")),
    }
}
