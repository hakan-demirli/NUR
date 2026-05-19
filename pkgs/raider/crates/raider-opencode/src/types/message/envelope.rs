use serde::{Deserialize, Serialize};

use crate::types::common::{MessageId, SessionId};

use super::part::MessagePart;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Message {
    pub id: MessageId,
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<SessionId>,
    pub role: MessageRole,

    #[serde(default)]
    pub time: MessageTime,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct MessageTime {
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub completed: Option<i64>,
}

/// the original M1 code used `#[serde(flatten)] info` here, which
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessageWithParts {
    pub info: Message,
    #[serde(default)]
    pub parts: Vec<MessagePart>,
}
