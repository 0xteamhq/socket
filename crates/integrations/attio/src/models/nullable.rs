//! Reading a field that Attio may send as `null`.

use serde::{Deserialize, Deserializer};

/// Reads `null` as the type's default. A default on a field covers one that
/// is left out, and not one that is sent as `null`; a list, a flag or a
/// number that arrives so would otherwise make the whole answer unreadable.
pub(super) fn nullable<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
