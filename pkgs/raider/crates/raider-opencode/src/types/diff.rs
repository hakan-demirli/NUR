//! Most fields use `#[serde(default)]` so they're cheap to ignore.
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct FileDiff {
    pub file: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub patch: String,
}
