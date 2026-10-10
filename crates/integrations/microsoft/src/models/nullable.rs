//! Reading a field that may be `null`.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Reads a field that was given as `Some`, a `null` included, so that
/// "set this to nothing" can be told from "leave this as it is".
pub(super) fn given<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}

/// Reads `null` as the type's default. Graph writes `null` where a string,
/// a flag or a list has no value, and a private event has many of those.
pub(super) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
