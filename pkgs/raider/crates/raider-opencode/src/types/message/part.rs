use serde::{Deserialize, Serialize};

use crate::types::common::{MessageId, PartId};

use super::parts::{
    compaction::CompactionPart, reasoning::ReasoningPart, step::StepBoundaryPart, text::TextPart,
    tool::ToolPart,
};

/// We don't use `#[serde(tag = "type")]` + `#[serde(other)]` because
#[derive(Clone, Debug, Serialize)]
pub enum MessagePart {
    Text(TextPart),
    Reasoning(ReasoningPart),
    Tool(ToolPart),
    StepStart(StepBoundaryPart),
    StepFinish(StepBoundaryPart),
    Compaction(CompactionPart),
    Other,
}

impl<'de> Deserialize<'de> for MessagePart {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let ty = value
            .as_object()
            .and_then(|m| m.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        fn pick<T: serde::de::DeserializeOwned, E: serde::de::Error>(
            v: serde_json::Value,
        ) -> std::result::Result<T, E> {
            serde_json::from_value(v).map_err(serde::de::Error::custom)
        }

        Ok(match ty.as_str() {
            "text" => MessagePart::Text(pick(value)?),
            "reasoning" => MessagePart::Reasoning(pick(value)?),
            "tool" => MessagePart::Tool(pick(value)?),
            "step-start" => MessagePart::StepStart(pick(value)?),
            "step-finish" => MessagePart::StepFinish(pick(value)?),
            "compaction" => MessagePart::Compaction(pick(value)?),
            _ => MessagePart::Other,
        })
    }
}

impl MessagePart {
    pub fn text(&self) -> Option<&str> {
        match self {
            MessagePart::Text(t) => Some(&t.text),
            _ => None,
        }
    }

    pub fn reasoning(&self) -> Option<&str> {
        match self {
            MessagePart::Reasoning(r) => Some(&r.text),
            _ => None,
        }
    }

    pub fn message_id(&self) -> Option<&MessageId> {
        match self {
            MessagePart::Text(t) => t.message_id.as_ref(),
            MessagePart::Reasoning(r) => r.message_id.as_ref(),
            MessagePart::Tool(t) => t.message_id.as_ref(),
            MessagePart::Compaction(c) => c.message_id.as_ref(),
            MessagePart::StepStart(_) | MessagePart::StepFinish(_) | MessagePart::Other => None,
        }
    }

    pub fn part_id(&self) -> Option<&PartId> {
        match self {
            MessagePart::Text(t) => Some(&t.id),
            MessagePart::Reasoning(r) => Some(&r.id),
            MessagePart::Tool(t) => Some(&t.id),
            MessagePart::StepStart(s) | MessagePart::StepFinish(s) => Some(&s.id),
            MessagePart::Compaction(c) => Some(&c.id),
            MessagePart::Other => None,
        }
    }
}
