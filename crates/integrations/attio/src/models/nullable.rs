//! Reading a field that may be `null`.

use serde::{Deserialize, Deserializer};

/// Reads a field that was given as `Some`, a `null` included, so that
/// "set this to nothing" can be told from "leave this as it is".
pub(super) fn given<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// Reads `null` as the type's default. Attio writes `null` where a string
/// or a list has no value.
pub(super) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
