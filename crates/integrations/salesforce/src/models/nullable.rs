//! Reading a property that may be `null`.

use serde::{Deserialize, Deserializer};

/// Reads `null` as the type's default. Salesforce writes `null` where a
/// string, a flag or a list has no value: an object with no key prefix, a
/// field that is not a relationship.
pub(super) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
