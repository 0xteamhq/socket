//! Limits on fetching a file.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// How much of a file to accept, and how long to wait for it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct Download {
    /// The most bytes to read: ten megabytes unless set. A file that is
    /// larger is refused with the error `too_large`; it is never cut short.
    pub max_bytes: Option<usize>,
    /// The longest to wait for the whole file, in seconds. Thirty unless set.
    pub timeout_secs: Option<u64>,
}

/// How much text to accept.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default, rename_all = "camelCase")]
pub struct TextLimit {
    /// The most bytes of text to read: one megabyte unless set, and at most
    /// ten. Text that is longer is refused with the error `too_large`; it is
    /// never cut short.
    pub max_bytes: Option<usize>,
}
