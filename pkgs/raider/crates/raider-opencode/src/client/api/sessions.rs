use crate::error::Result;
use crate::types::common::SessionId;
use crate::types::diff::FileDiff;
use crate::types::message::MessageWithParts;
use crate::types::session::{PromptPayload, Session, SessionCreatePayload};

use super::super::Client;

impl Client {
    pub async fn sessions_list(&self) -> Result<Vec<Session>> {
        self.get_json("/session").await
    }

    pub async fn session_get(&self, id: &SessionId) -> Result<Session> {
        let path = format!("/session/{}", id.as_str());
        self.get_json(&path).await
    }

    pub async fn session_messages(&self, id: &SessionId) -> Result<Vec<MessageWithParts>> {
        let path = format!("/session/{}/message", id.as_str());
        self.get_json(&path).await
    }

    pub async fn session_diff(&self, id: &SessionId) -> Result<Vec<FileDiff>> {
        let path = format!("/session/{}/diff", id.as_str());
        self.get_json(&path).await
    }

    pub async fn session_todo(&self, id: &SessionId) -> Result<Vec<crate::types::todo::Todo>> {
        let path = format!("/session/{}/todo", id.as_str());
        self.get_json(&path).await
    }

    pub async fn session_create(&self, payload: &SessionCreatePayload) -> Result<Session> {
        self.post_json("/session", payload).await
    }

    pub async fn session_prompt(
        &self,
        session_id: &SessionId,
        payload: &PromptPayload,
    ) -> Result<()> {
        let path = format!("/session/{}/prompt_async", session_id.as_str());
        let _: serde_json::Value = self.post_json(&path, payload).await?;
        Ok(())
    }

    pub async fn session_summarize(
        &self,
        session_id: &SessionId,
        provider_id: &str,
        model_id: &str,
    ) -> Result<()> {
        let path = format!("/session/{}/summarize", session_id.as_str());
        let body = serde_json::json!({
            "providerID": provider_id,
            "modelID": model_id,
        });
        let _: serde_json::Value = self.post_json(&path, &body).await?;
        Ok(())
    }

    pub async fn session_status_map(
        &self,
    ) -> Result<std::collections::HashMap<String, crate::events::SessionStatusKind>> {
        let path = "/session/status";
        self.get_json(path).await
    }

    pub async fn session_rename(
        &self,
        session_id: &SessionId,
        title: &str,
    ) -> Result<crate::types::session::Session> {
        let path = format!("/session/{}", session_id.as_str());
        let body = serde_json::json!({ "title": title });
        self.patch_json(&path, &body).await
    }

    pub async fn session_delete(&self, session_id: &SessionId) -> Result<()> {
        let path = format!("/session/{}", session_id.as_str());
        let _: serde_json::Value = self.delete_json(&path).await?;
        Ok(())
    }

    pub async fn session_abort(&self, session_id: &SessionId) -> Result<()> {
        let path = format!("/session/{}/abort", session_id.as_str());
        let _: serde_json::Value = self.post_json(&path, &serde_json::json!({})).await?;
        Ok(())
    }

    pub async fn session_revert(&self, session_id: &SessionId, message_id: &str) -> Result<()> {
        let path = format!("/session/{}/revert", session_id.as_str());
        let body = serde_json::json!({ "messageID": message_id });
        let _: serde_json::Value = self.post_json(&path, &body).await?;
        Ok(())
    }

    pub async fn session_unrevert(&self, session_id: &SessionId) -> Result<()> {
        let path = format!("/session/{}/unrevert", session_id.as_str());
        let _: serde_json::Value = self.post_json(&path, &serde_json::json!({})).await?;
        Ok(())
    }

    pub async fn session_fork(
        &self,
        session_id: &SessionId,
        message_id: Option<&str>,
    ) -> Result<crate::types::session::Session> {
        let path = format!("/session/{}/fork", session_id.as_str());
        let body = match message_id {
            Some(mid) => serde_json::json!({ "messageID": mid }),
            None => serde_json::json!({}),
        };
        self.post_json(&path, &body).await
    }

    pub async fn session_share(
        &self,
        session_id: &SessionId,
    ) -> Result<crate::types::session::Session> {
        let path = format!("/session/{}/share", session_id.as_str());
        self.post_json(&path, &serde_json::json!({})).await
    }

    pub async fn session_unshare(
        &self,
        session_id: &SessionId,
    ) -> Result<crate::types::session::Session> {
        let path = format!("/session/{}/share", session_id.as_str());
        self.delete_json(&path).await
    }
}
