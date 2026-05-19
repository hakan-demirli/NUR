use async_trait::async_trait;

use raider_opencode::{
    types::{common::SessionId, diff::FileDiff, message::MessageWithParts, todo::Todo},
    Error,
};

#[async_trait]
pub trait MessageBackend: Send + Sync + 'static {
    async fn session_messages(&self, id: &SessionId) -> Result<Vec<MessageWithParts>, Error>;

    async fn session_diff(&self, id: &SessionId) -> Result<Vec<FileDiff>, Error>;

    async fn session_todo(&self, id: &SessionId) -> Result<Vec<Todo>, Error>;
}
