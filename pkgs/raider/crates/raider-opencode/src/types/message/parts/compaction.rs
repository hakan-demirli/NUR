use serde::{Deserialize, Serialize};

use crate::types::common::{MessageId, PartId};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct CompactionPart {
    pub id: PartId,
    #[serde(rename = "messageID", default)]
    pub message_id: Option<MessageId>,
    #[serde(default)]
    pub auto: bool,
    #[serde(default)]
    pub overflow: Option<bool>,
    #[serde(rename = "tail_start_id", default)]
    pub tail_start_id: Option<MessageId>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
