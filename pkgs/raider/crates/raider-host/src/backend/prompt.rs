use async_trait::async_trait;

use raider_opencode::{
    types::{common::SessionId, session::PromptPayload},
    Error,
};

#[async_trait]
pub trait PromptBackend: Send + Sync + 'static {
    async fn session_prompt(
        &self,
        session_id: &SessionId,
        payload: &PromptPayload,
    ) -> Result<(), Error>;
}
