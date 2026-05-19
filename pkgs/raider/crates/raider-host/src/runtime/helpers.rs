use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc::UnboundedSender;

use raider_opencode::types::{
    common::SessionId, config::AppConfig, diff::FileDiff, lsp::LspStatus, mcp::McpRegistry,
    message::MessageWithParts, session::Session, todo::Todo,
};
use raider_tui::{Action, HostAction};

use crate::backend::Backend;
use crate::bridge;

pub(super) struct SessionBundle {
    pub session: Result<Session, raider_opencode::Error>,
    pub messages: Result<Vec<MessageWithParts>, raider_opencode::Error>,
    pub diff: Vec<FileDiff>,
    pub todo: Vec<Todo>,
    pub mcp: McpRegistry,
    pub lsp: Vec<LspStatus>,
    pub config: AppConfig,
}

pub(super) async fn fetch_session_bundle<B: Backend>(
    backend: &Arc<B>,
    id: &SessionId,
    context: &'static str,
) -> SessionBundle {
    let (session_res, messages_res, diff_res, todo_res, mcp_res, lsp_res, config_res) = tokio::join!(
        backend.session_get(id),
        backend.session_messages(id),
        backend.session_diff(id),
        backend.session_todo(id),
        backend.mcp_status(),
        backend.lsp_status(),
        backend.config_get(),
    );

    let diff = diff_res.unwrap_or_else(|e| {
        tracing::warn!(error = %e, context, "session_diff failed");
        Vec::new()
    });
    let todo = todo_res.unwrap_or_else(|e| {
        tracing::warn!(error = %e, context, "session_todo failed");
        Vec::new()
    });
    let mcp = mcp_res.unwrap_or_else(|e| {
        tracing::warn!(error = %e, context, "mcp_status failed");
        Default::default()
    });
    let lsp = lsp_res.unwrap_or_else(|e| {
        tracing::warn!(error = %e, context, "lsp_status failed");
        Vec::new()
    });
    let config = config_res.unwrap_or_else(|e| {
        tracing::warn!(error = %e, context, "config_get failed");
        AppConfig::default()
    });
    SessionBundle {
        session: session_res,
        messages: messages_res,
        diff,
        todo,
        mcp,
        lsp,
        config,
    }
}

pub(super) async fn retry_with_backoff<T, F, Fut>(
    name: &'static str,
    mut op: F,
    max_attempts: u32,
) -> Result<T, raider_opencode::Error>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, raider_opencode::Error>>,
{
    let mut attempt: u32 = 1;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt < max_attempts => {
                tracing::warn!(error = %e, attempt, op = name, "operation failed; retrying with backoff");
                let secs = (1u64 << (attempt - 1).min(5)).min(30);
                tokio::time::sleep(Duration::from_secs(secs)).await;
                attempt += 1;
            }
            Err(e) => {
                tracing::warn!(error = %e, op = name, "operation failed (giving up after retries)");
                return Err(e);
            }
        }
    }
}

pub(super) fn spawn_backend_call<B, F, Fut, OnErr>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    op: F,
    on_error: OnErr,
) where
    B: Backend,
    F: FnOnce(Arc<B>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), raider_opencode::Error>> + Send,
    OnErr: FnOnce(raider_opencode::Error, UnboundedSender<Action>) + Send + 'static,
{
    let backend = Arc::clone(backend);
    let action_tx = action_tx.clone();
    tokio::spawn(async move {
        if let Err(e) = op(backend).await {
            on_error(e, action_tx);
        }
    });
}

pub(super) fn report_backend_error(
    action_tx: &UnboundedSender<Action>,
    context: &'static str,
    e: &raider_opencode::Error,
) {
    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
        "{context}: {e}"
    ))));
}

pub(super) fn fetch_pending_permissions_for_session<B: Backend>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    sid: SessionId,
) {
    let backend = Arc::clone(backend);
    let action_tx = action_tx.clone();
    tokio::spawn(async move {
        match backend.permission_list().await {
            Ok(requests) => {
                for req in requests {
                    if req.session_id == sid {
                        let prompt = bridge::permission_to_prompt(&req);
                        let _ = action_tx.send(Action::Host(HostAction::PermissionAsked(prompt)));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "permission_list failed");
            }
        }
    });
}

pub(super) fn fetch_pending_questions_for_session<B: Backend>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    sid: SessionId,
) {
    let backend = Arc::clone(backend);
    let action_tx = action_tx.clone();
    tokio::spawn(async move {
        match backend.question_list().await {
            Ok(requests) => {
                for req in requests {
                    if req.session_id == sid {
                        let prompt = bridge::question_to_prompt(&req);
                        let _ = action_tx.send(Action::Host(HostAction::QuestionAsked(prompt)));
                    }
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "question_list failed");
            }
        }
    });
}
