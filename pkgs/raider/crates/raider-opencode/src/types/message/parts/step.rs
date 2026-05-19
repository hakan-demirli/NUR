use serde::{Deserialize, Serialize};

use crate::types::common::PartId;

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct StepBoundaryPart {
    pub id: PartId,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
