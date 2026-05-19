use serde::{Deserialize, Serialize};

use crate::types::common::{MessageId, PartId, SessionId};
use crate::types::message::{MessagePart, MessageWithParts};
use crate::types::permission::{PermissionRepliedProps, PermissionRequest};
use crate::types::question::{QuestionRejectedProps, QuestionRepliedProps, QuestionRequest};
use crate::types::session::Session;

#[derive(Clone, Debug, Serialize)]
pub enum ServerEvent {
    SessionUpdated(SessionUpdatedProps),
    SessionDeleted(SessionDeletedProps),
    SessionIdle(SessionIdleProps),
    SessionError(SessionErrorProps),
    MessageUpdated(MessageUpdatedProps),

    MessagePartUpdated(MessagePartUpdatedProps),

    MessagePartDelta(MessagePartDeltaProps),

    VcsBranchUpdated(VcsBranchUpdatedProps),

    SessionStatus(SessionStatusProps),

    MessageRemoved(MessageRemovedProps),

    MessagePartRemoved(MessagePartRemovedProps),

    PermissionAsked(PermissionRequest),
    PermissionReplied(PermissionRepliedProps),
    QuestionAsked(QuestionRequest),
    QuestionReplied(QuestionRepliedProps),
    QuestionRejected(QuestionRejectedProps),
    Unknown(String),
}

impl<'de> Deserialize<'de> for ServerEvent {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let payload = unwrap_envelope(value);

        fn pick<T: serde::de::DeserializeOwned, E: serde::de::Error>(
            v: serde_json::Value,
        ) -> std::result::Result<T, E> {
            serde_json::from_value(v).map_err(serde::de::Error::custom)
        }

        let ty = payload
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let normalized = strip_version_suffix(&ty);
        let properties = payload
            .get("properties")
            .cloned()
            .or_else(|| payload.get("data").cloned())
            .unwrap_or(serde_json::Value::Null);

        Ok(match normalized.as_str() {
            "session.updated" => ServerEvent::SessionUpdated(pick(properties)?),
            "session.created" => ServerEvent::SessionUpdated(pick(properties)?),
            "session.deleted" => ServerEvent::SessionDeleted(pick(properties)?),
            "session.idle" => ServerEvent::SessionIdle(pick(properties)?),
            "session.error" => ServerEvent::SessionError(pick(properties)?),
            "message.updated" => ServerEvent::MessageUpdated(pick(properties)?),
            "message.part.updated" => ServerEvent::MessagePartUpdated(pick(properties)?),
            "message.part.delta" => ServerEvent::MessagePartDelta(pick(properties)?),
            "message.removed" => ServerEvent::MessageRemoved(pick(properties)?),
            "message.part.removed" => ServerEvent::MessagePartRemoved(pick(properties)?),
            "session.status" => ServerEvent::SessionStatus(pick(properties)?),
            "vcs.branch.updated" => ServerEvent::VcsBranchUpdated(pick(properties)?),
            "permission.asked" => ServerEvent::PermissionAsked(pick(properties)?),
            "permission.replied" => ServerEvent::PermissionReplied(pick(properties)?),
            "question.asked" => ServerEvent::QuestionAsked(pick(properties)?),
            "question.replied" => ServerEvent::QuestionReplied(pick(properties)?),
            "question.rejected" => ServerEvent::QuestionRejected(pick(properties)?),
            "permission.requested" => ServerEvent::PermissionAsked(pick(properties)?),
            other => ServerEvent::Unknown(other.to_string()),
        })
    }
}

pub(crate) fn unwrap_envelope(mut v: serde_json::Value) -> serde_json::Value {
    for _ in 0..2 {
        let next = if let Some(p) = v.get("payload") {
            if p.is_object() {
                Some(p.clone())
            } else {
                None
            }
        } else if let Some(sync_inner) = v.get("syncEvent").filter(|s| s.is_object()).cloned() {
            let mut flat = serde_json::Map::new();
            if let Some(ty) = sync_inner.get("type").cloned() {
                flat.insert("type".into(), ty);
            }
            if let Some(data) = sync_inner.get("data").cloned() {
                flat.insert("properties".into(), data);
            }
            Some(serde_json::Value::Object(flat))
        } else {
            None
        };
        match next {
            Some(n) => v = n,
            None => break,
        }
    }
    v
}

pub(crate) fn strip_version_suffix(ty: &str) -> String {
    if let Some(idx) = ty.rfind('.') {
        let tail = &ty[idx + 1..];
        if tail.chars().all(|c| c.is_ascii_digit()) && !tail.is_empty() {
            return ty[..idx].to_string();
        }
    }
    ty.to_string()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionUpdatedProps {
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<SessionId>,
    pub info: Session,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionDeletedProps {
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<SessionId>,
    #[serde(default)]
    pub info: Option<Session>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionIdleProps {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionErrorProps {
    #[serde(rename = "sessionID", default)]
    pub session_id: Option<SessionId>,
    #[serde(default)]
    pub error: serde_json::Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessageUpdatedProps {
    #[serde(deserialize_with = "deserialize_bare_message_as_with_parts")]
    pub info: MessageWithParts,
}

fn deserialize_bare_message_as_with_parts<'de, D>(
    deserializer: D,
) -> std::result::Result<MessageWithParts, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use crate::types::message::Message;
    let v = serde_json::Value::deserialize(deserializer)?;
    let looks_nested = v
        .as_object()
        .map(|m| m.contains_key("info"))
        .unwrap_or(false);
    if looks_nested {
        return serde_json::from_value(v).map_err(serde::de::Error::custom);
    }
    let info: Message = serde_json::from_value(v).map_err(serde::de::Error::custom)?;
    Ok(MessageWithParts {
        info,
        parts: Vec::new(),
    })
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessagePartUpdatedProps {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(rename = "messageID", default)]
    pub message_id: Option<MessageId>,
    pub part: MessagePart,
    #[serde(rename = "partID", default)]
    pub part_id: Option<PartId>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum SessionStatusKind {
    Idle,
    Busy,
    Retry {
        #[serde(default)]
        attempt: Option<u32>,
        #[serde(default)]
        message: Option<String>,
        #[serde(default)]
        next: Option<i64>,
    },
}

impl SessionStatusKind {
    pub fn is_working(&self) -> bool {
        !matches!(self, SessionStatusKind::Idle)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct VcsBranchUpdatedProps {
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SessionStatusProps {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    pub status: SessionStatusKind,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessageRemovedProps {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessagePartRemovedProps {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
    #[serde(rename = "partID")]
    pub part_id: PartId,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct MessagePartDeltaProps {
    #[serde(rename = "sessionID")]
    pub session_id: SessionId,
    #[serde(rename = "messageID")]
    pub message_id: MessageId,
    #[serde(rename = "partID")]
    pub part_id: PartId,
    pub field: String,
    pub delta: String,
}
