//! Reading a field that may be `null`.

use serde::{Deserialize, Deserializer};
use serde_json::Value;

/// Reads a field that was given as `Some`, a `null` included, so that
/// "set this to nothing" can be told from "leave this as it is".
pub(super) fn given<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Value>, D::Error> {
    Value::deserialize(deserializer).map(Some)
}
