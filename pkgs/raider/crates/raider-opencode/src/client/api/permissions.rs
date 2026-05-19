use crate::error::Result;
use crate::types::permission::{PermissionReply, PermissionReplyBody, PermissionRequest};

use super::super::Client;

impl Client {
    pub async fn permission_list(&self) -> Result<Vec<PermissionRequest>> {
        self.get_json("/permission").await
    }

    pub async fn permission_reply(
        &self,
        request_id: &str,
        reply: PermissionReply,
        message: Option<String>,
    ) -> Result<()> {
        let path = format!("/permission/{request_id}/reply");
        let body = PermissionReplyBody { reply, message };
        let _: serde_json::Value = self.post_json(&path, &body).await?;
        Ok(())
    }
}
