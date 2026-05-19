use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;

use raider_opencode::types::{
    common::{MessageId, SessionId},
    id::{ascending, Prefix},
    session::{
        PromptFilePart, PromptFileSource, PromptModel, PromptPart, PromptPayload, PromptTextPart,
        SessionCreateModel, SessionCreatePayload,
    },
};
use raider_tui::{Action, HostAction, ModelRef, UserFileAttachment};

use crate::backend::Backend;

#[derive(Clone, Debug)]
pub(super) struct PromptRequest {
    pub text: String,
    pub files: Vec<UserFileAttachment>,
}

pub(super) struct PromptTask<B: Backend> {
    pub backend: Arc<B>,
    pub action_tx: UnboundedSender<Action>,
    pub prompt_rx: UnboundedReceiver<PromptRequest>,
    pub active_tx: watch::Sender<Option<SessionId>>,
    pub model_rx: watch::Receiver<Option<ModelRef>>,
    pub variant_rx: watch::Receiver<Option<String>>,
    pub agent_rx: watch::Receiver<String>,
    pub sidebar_refetch_tx: UnboundedSender<SessionId>,
    pub plugin_handle: Option<raider_plugin_lua::LuaPluginHandle>,
}

pub(super) async fn prompt_task<B: Backend>(ctx: PromptTask<B>) {
    let PromptTask {
        backend,
        action_tx,
        mut prompt_rx,
        active_tx,
        model_rx,
        variant_rx,
        agent_rx,
        sidebar_refetch_tx,
        plugin_handle,
    } = ctx;

    while let Some(req) = prompt_rx.recv().await {
        let model_snapshot = model_rx.borrow().clone();
        let variant_snapshot = variant_rx.borrow().clone();
        let agent_snapshot = agent_rx.borrow().clone();
        let active_snapshot = active_tx.borrow().clone();

        let model = match model_snapshot {
            Some(m) => m,
            None => {
                let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
                    "Pick a model with /models before sending a message".to_string(),
                )));
                let _ =
                    action_tx.send(Action::Host(HostAction::AssistantDone { message_id: None }));
                continue;
            }
        };
        let variant = variant_snapshot;
        let agent = agent_snapshot;

        let session_id = match active_snapshot {
            Some(id) => id,
            None => match create_session(&*backend, &model, &variant, &agent).await {
                Ok(id) => {
                    let _ = active_tx.send(Some(id.clone()));
                    let _ = action_tx.send(Action::Host(HostAction::SetCurrentSession(Some(
                        id.as_str().to_string(),
                    ))));
                    if let Some(plugin) = &plugin_handle {
                        plugin.send(raider_plugin_lua::PluginEvent::SessionChanged {
                            session_id: Some(id.as_str().to_string()),
                        });
                    }
                    let _ = sidebar_refetch_tx.send(id.clone());
                    id
                }
                Err(e) => {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                        "Failed to create session: {e}"
                    ))));
                    let _ = action_tx
                        .send(Action::Host(HostAction::AssistantDone { message_id: None }));
                    continue;
                }
            },
        };

        let message_id = MessageId::new(ascending(Prefix::Message));
        let mut parts: Vec<PromptPart> = Vec::with_capacity(1 + req.files.len());
        parts.push(PromptPart::Text(PromptTextPart {
            id: None,
            text: req.text,
        }));
        for file in req.files {
            let url = format!("data:{};base64,{}", file.mime, file.base64);
            parts.push(PromptPart::File(PromptFilePart {
                mime: file.mime,
                filename: if file.filename.is_empty() {
                    None
                } else {
                    Some(file.filename)
                },
                url,
                source: Some(PromptFileSource {
                    kind: "file".to_string(),
                    path: file.filepath,
                    text: None,
                }),
            }));
        }
        let payload = PromptPayload {
            message_id: Some(message_id),
            model: Some(PromptModel {
                provider_id: model.provider_id.clone(),
                model_id: model.model_id.clone(),
            }),
            agent: Some(agent),
            variant: variant.filter(|v| !v.is_empty()),
            parts,
        };

        if let Err(e) = backend.session_prompt(&session_id, &payload).await {
            tracing::warn!(error = %e, "session_prompt failed");
            let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                "Failed to send prompt: {e}"
            ))));
            let _ = action_tx.send(Action::Host(HostAction::AssistantDone { message_id: None }));
        }
    }
}

async fn create_session<B: Backend>(
    backend: &B,
    model: &ModelRef,
    variant: &Option<String>,
    agent: &str,
) -> Result<SessionId, raider_opencode::Error> {
    let payload = SessionCreatePayload {
        agent: Some(agent.to_string()),
        model: Some(SessionCreateModel {
            id: model.model_id.clone(),
            provider_id: model.provider_id.clone(),
            variant: variant.clone().filter(|v| !v.is_empty()),
        }),
        title: None,
    };
    let session = backend.session_create(&payload).await?;
    Ok(session.id)
}
