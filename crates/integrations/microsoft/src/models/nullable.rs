//! Graph writes `null` where a list is empty or was not asked for.

use serde::{Deserialize, Deserializer};

/// Reads `null` as the type's default, so a list Graph left out is an empty list.
pub(super) fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
