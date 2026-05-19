use serde::{Deserialize, Serialize};

use super::common::{MessageId, PartId, SessionId};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Session {
    pub id: SessionId,
    #[serde(default)]
    pub title: String,

    #[serde(rename = "parentID", default)]
    pub parent_id: Option<SessionId>,

    #[serde(default)]
    pub time: SessionTime,

    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct SessionTime {
    #[serde(default)]
    pub created: Option<i64>,
    #[serde(default)]
    pub updated: Option<i64>,
    #[serde(default)]
    pub archived: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SessionCreatePayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<SessionCreateModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct SessionCreateModel {
    pub id: String,
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct PromptPayload {
    #[serde(rename = "messageID", skip_serializing_if = "Option::is_none")]
    pub message_id: Option<MessageId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<PromptModel>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<String>,
    pub parts: Vec<PromptPart>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PromptModel {
    #[serde(rename = "providerID")]
    pub provider_id: String,
    #[serde(rename = "modelID")]
    pub model_id: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PromptPart {
    Text(PromptTextPart),
    File(PromptFilePart),
}

#[derive(Clone, Debug, Serialize)]
pub struct PromptTextPart {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<PartId>,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PromptFilePart {
    pub mime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filename: Option<String>,
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<PromptFileSource>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PromptFileSource {
    #[serde(rename = "type")]
    pub kind: String,
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<PromptFileSourceText>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PromptFileSourceText {
    pub start: usize,
    pub end: usize,
    pub value: String,
}
