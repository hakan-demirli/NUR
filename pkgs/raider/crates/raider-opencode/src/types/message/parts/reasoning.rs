use serde::{Deserialize, Serialize};

use crate::types::common::{MessageId, PartId};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ReasoningPart {
    pub id: PartId,
    #[serde(default)]
    pub text: String,
    #[serde(rename = "messageID", default)]
    pub message_id: Option<MessageId>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
