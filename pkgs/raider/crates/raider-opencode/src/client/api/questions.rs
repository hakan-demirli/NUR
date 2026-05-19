use crate::error::Result;
use crate::types::question::{QuestionReplyBody, QuestionRequest};

use super::super::Client;

impl Client {
    pub async fn question_list(&self) -> Result<Vec<QuestionRequest>> {
        self.get_json("/question").await
    }

    pub async fn question_reply(&self, request_id: &str, answers: Vec<Vec<String>>) -> Result<()> {
        let path = format!("/question/{request_id}/reply");
        let body = QuestionReplyBody { answers };
        let _: serde_json::Value = self.post_json(&path, &body).await?;
        Ok(())
    }

    pub async fn question_reject(&self, request_id: &str) -> Result<()> {
        let path = format!("/question/{request_id}/reject");
        let _: serde_json::Value = self.post_json(&path, &serde_json::json!({})).await?;
        Ok(())
    }
}
