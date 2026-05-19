use async_trait::async_trait;

use raider_opencode::{
    events::SessionStatusKind,
    types::{
        common::SessionId,
        session::{Session, SessionCreatePayload},
    },
    Error,
};

#[async_trait]
pub trait SessionBackend: Send + Sync + 'static {
    async fn sessions_list(&self) -> Result<Vec<Session>, Error>;

    async fn session_get(&self, id: &SessionId) -> Result<Session, Error>;

    async fn session_create(&self, payload: &SessionCreatePayload) -> Result<Session, Error>;

    async fn session_rename(&self, session_id: &SessionId, title: &str) -> Result<Session, Error>;

    async fn session_revert(&self, session_id: &SessionId, message_id: &str) -> Result<(), Error>;

    async fn session_unrevert(&self, session_id: &SessionId) -> Result<(), Error>;

    async fn session_fork(
        &self,
        session_id: &SessionId,
        message_id: Option<&str>,
    ) -> Result<Session, Error>;

    async fn session_delete(&self, session_id: &SessionId) -> Result<(), Error>;

    async fn session_abort(&self, session_id: &SessionId) -> Result<(), Error>;

    async fn session_share(&self, session_id: &SessionId) -> Result<Session, Error>;

    async fn session_unshare(&self, session_id: &SessionId) -> Result<Session, Error>;

    async fn session_summarize(
        &self,
        session_id: &SessionId,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), Error>;

    async fn session_status_map(
        &self,
    ) -> Result<std::collections::HashMap<String, SessionStatusKind>, Error>;
}
