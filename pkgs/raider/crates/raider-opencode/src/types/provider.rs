//! `#[serde(rename = "...")]` for the camelCase ones we do read.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ProviderList {
    #[serde(default)]
    pub all: Vec<ProviderInfo>,
    #[serde(default)]
    pub default: std::collections::HashMap<String, String>,
    #[serde(default)]
    pub connected: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProviderInfo {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub models: std::collections::HashMap<String, ModelInfo>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelInfo {
    pub id: String,
    #[serde(rename = "providerID", default)]
    pub provider_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub cost: Option<ModelCost>,
    #[serde(default)]
    pub limit: Option<ModelLimit>,
    #[serde(default)]
    pub variants: std::collections::HashMap<String, serde_json::Value>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelLimit {
    #[serde(default)]
    pub context: u64,
    #[serde(default)]
    pub output: u64,
}

impl ModelInfo {
    pub fn is_zero_input_cost(&self) -> bool {
        self.cost.as_ref().is_some_and(|c| c.input == 0.0)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelCost {
    #[serde(default)]
    pub input: f64,
    #[serde(default)]
    pub output: f64,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
