//! Reading a field Graph may send as `null`.

use serde::{Deserialize, Deserializer};

/// Reads `null` as the type's default. Graph writes `null` where a string,
/// a flag or a list has no value, and a private event has many of those.
pub(super) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
