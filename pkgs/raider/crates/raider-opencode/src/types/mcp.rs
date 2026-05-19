use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct McpStatus {
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub error: String,
}

pub type McpRegistry = BTreeMap<String, McpStatus>;
