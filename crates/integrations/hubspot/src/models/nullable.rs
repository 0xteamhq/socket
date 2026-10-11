//! Reading fields HubSpot writes in more than one way.

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

/// Reads an id as text. HubSpot documents ids as strings, and its own
/// examples of associations write them as numbers. `null` reads as no id,
/// which the caller reports.
pub(super) fn id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<String, D::Error> {
    match Value::deserialize(deserializer)? {
        Value::String(id) => Ok(id),
        Value::Number(id) => Ok(id.to_string()),
        Value::Null => Ok(String::new()),
        _ => Err(serde::de::Error::custom("an id is a string or a number")),
    }
}
