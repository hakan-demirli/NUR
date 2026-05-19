use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

use raider_opencode::{
    client::Client,
    events::{SessionStatusKind, StreamItem},
    types::{
        common::SessionId,
        diff::FileDiff,
        lsp::LspStatus,
        mcp::McpRegistry,
        message::MessageWithParts,
        permission::{PermissionReply, PermissionRequest},
        provider::ProviderList,
        question::QuestionRequest,
        session::{PromptPayload, Session, SessionCreatePayload},
        todo::Todo,
    },
    Error,
};

use super::{
    events::EventBackend, message::MessageBackend, permission::PermissionBackend,
    prompt::PromptBackend, provider::ProviderBackend, question::QuestionBackend,
    session::SessionBackend, tooling::ToolingBackend,
};

pub struct OpencodeBackend {
    client: Client,
}

impl OpencodeBackend {
    pub fn new(client: Client) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &Client {
        &self.client
    }
}

#[async_trait]
impl SessionBackend for OpencodeBackend {
    async fn sessions_list(&self) -> Result<Vec<Session>, Error> {
        self.client.sessions_list().await
    }

    async fn session_get(&self, id: &SessionId) -> Result<Session, Error> {
        self.client.session_get(id).await
    }

    async fn session_create(&self, payload: &SessionCreatePayload) -> Result<Session, Error> {
        self.client.session_create(payload).await
    }

    async fn session_rename(&self, session_id: &SessionId, title: &str) -> Result<Session, Error> {
        self.client.session_rename(session_id, title).await
    }

    async fn session_revert(&self, session_id: &SessionId, message_id: &str) -> Result<(), Error> {
        self.client.session_revert(session_id, message_id).await
    }

    async fn session_unrevert(&self, session_id: &SessionId) -> Result<(), Error> {
        self.client.session_unrevert(session_id).await
    }

    async fn session_fork(
        &self,
        session_id: &SessionId,
        message_id: Option<&str>,
    ) -> Result<Session, Error> {
        self.client.session_fork(session_id, message_id).await
    }

    async fn session_delete(&self, session_id: &SessionId) -> Result<(), Error> {
        self.client.session_delete(session_id).await
    }

    async fn session_abort(&self, session_id: &SessionId) -> Result<(), Error> {
        self.client.session_abort(session_id).await
    }

    async fn session_share(&self, session_id: &SessionId) -> Result<Session, Error> {
        self.client.session_share(session_id).await
    }

    async fn session_unshare(&self, session_id: &SessionId) -> Result<Session, Error> {
        self.client.session_unshare(session_id).await
    }

    async fn session_summarize(
        &self,
        session_id: &SessionId,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), Error> {
        self.client
            .session_summarize(session_id, provider_id, model_id)
            .await
    }

    async fn session_status_map(
        &self,
    ) -> Result<std::collections::HashMap<String, SessionStatusKind>, Error> {
        self.client.session_status_map().await
    }
}

#[async_trait]
impl MessageBackend for OpencodeBackend {
    async fn session_messages(&self, id: &SessionId) -> Result<Vec<MessageWithParts>, Error> {
        self.client.session_messages(id).await
    }

    async fn session_diff(&self, id: &SessionId) -> Result<Vec<FileDiff>, Error> {
        self.client.session_diff(id).await
    }

    async fn session_todo(&self, id: &SessionId) -> Result<Vec<Todo>, Error> {
        self.client.session_todo(id).await
    }
}

#[async_trait]
impl PromptBackend for OpencodeBackend {
    async fn session_prompt(
        &self,
        session_id: &SessionId,
        payload: &PromptPayload,
    ) -> Result<(), Error> {
        self.client.session_prompt(session_id, payload).await
    }
}

#[async_trait]
impl ProviderBackend for OpencodeBackend {
    async fn provider_list(&self) -> Result<ProviderList, Error> {
        self.client.provider_list().await
    }
}

#[async_trait]
impl ToolingBackend for OpencodeBackend {
    async fn mcp_status(&self) -> Result<McpRegistry, Error> {
        self.client.mcp_status().await
    }

    async fn lsp_status(&self) -> Result<Vec<LspStatus>, Error> {
        self.client.lsp_status().await
    }

    async fn config_get(&self) -> Result<raider_opencode::types::config::AppConfig, Error> {
        self.client.config_get().await
    }

    async fn sync_start(&self, directory: Option<&str>) -> Result<bool, Error> {
        self.client.sync_start(directory).await
    }
}

#[async_trait]
impl PermissionBackend for OpencodeBackend {
    async fn permission_list(&self) -> Result<Vec<PermissionRequest>, Error> {
        self.client.permission_list().await
    }

    async fn permission_reply(
        &self,
        request_id: &str,
        reply: PermissionReply,
        message: Option<String>,
    ) -> Result<(), Error> {
        self.client
            .permission_reply(request_id, reply, message)
            .await
    }
}

#[async_trait]
impl QuestionBackend for OpencodeBackend {
    async fn question_list(&self) -> Result<Vec<QuestionRequest>, Error> {
        self.client.question_list().await
    }

    async fn question_reply(
        &self,
        request_id: &str,
        answers: Vec<Vec<String>>,
    ) -> Result<(), Error> {
        self.client.question_reply(request_id, answers).await
    }

    async fn question_reject(&self, request_id: &str) -> Result<(), Error> {
        self.client.question_reject(request_id).await
    }
}

impl EventBackend for OpencodeBackend {
    fn events(&self) -> Pin<Box<dyn Stream<Item = StreamItem> + Send>> {
        Box::pin(self.client.events())
    }
}
