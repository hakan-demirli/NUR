use async_trait::async_trait;

use raider_opencode::{
    types::permission::{PermissionReply, PermissionRequest},
    Error,
};

#[async_trait]
pub trait PermissionBackend: Send + Sync + 'static {
    async fn permission_list(&self) -> Result<Vec<PermissionRequest>, Error>;

    async fn permission_reply(
        &self,
        request_id: &str,
        reply: PermissionReply,
        message: Option<String>,
    ) -> Result<(), Error>;
}
