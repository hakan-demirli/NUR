use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;

use raider_opencode::types::common::SessionId;
use raider_tui::{Action, HostAction, ModelRef, Toast, ToastVariant, ViewAction};

use crate::backend::Backend;
use crate::bridge::session_to_entry;

use super::helpers::{
    fetch_pending_permissions_for_session, fetch_pending_questions_for_session,
    report_backend_error, spawn_backend_call,
};
use super::prompt_task::PromptRequest;

pub(super) struct UiEventTask<B: Backend> {
    pub backend: Arc<B>,
    pub ui_rx: UnboundedReceiver<raider_tui::Event>,
    pub active_tx: watch::Sender<Option<SessionId>>,
    pub action_tx: UnboundedSender<Action>,
    pub model_tx: watch::Sender<Option<ModelRef>>,
    pub variant_tx: watch::Sender<Option<String>>,
    pub agent_tx: watch::Sender<String>,
    pub prompt_tx: UnboundedSender<PromptRequest>,
    pub refetch_tx: UnboundedSender<SessionId>,
    pub plugin_handle: Option<raider_plugin_lua::LuaPluginHandle>,
}

pub(super) async fn ui_event_task<B: Backend>(ctx: UiEventTask<B>) {
    let UiEventTask {
        backend,
        mut ui_rx,
        active_tx,
        action_tx,
        model_tx,
        variant_tx,
        agent_tx,
        prompt_tx,
        refetch_tx,
        plugin_handle,
    } = ctx;

    while let Some(ev) = ui_rx.recv().await {
        match ev {
            raider_tui::Event::SessionSwitched(id) => {
                handle_session_switched(
                    &backend,
                    &action_tx,
                    &active_tx,
                    &refetch_tx,
                    plugin_handle.as_ref(),
                    id,
                );
            }
            raider_tui::Event::SubagentNavigate(id) => {
                handle_session_switched(
                    &backend,
                    &action_tx,
                    &active_tx,
                    &refetch_tx,
                    plugin_handle.as_ref(),
                    id,
                );
            }
            raider_tui::Event::Quit => break,
            raider_tui::Event::PluginCommand { name, args } => {
                if let Some(plugin) = &plugin_handle {
                    plugin.send(raider_plugin_lua::PluginEvent::Command { name, args });
                } else {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
                        "Lua plugin runtime is not enabled.".to_string(),
                    )));
                }
            }
            raider_tui::Event::PluginDialogSelected { callback_id, value } => {
                if let Some(plugin) = &plugin_handle {
                    plugin.send(raider_plugin_lua::PluginEvent::DialogSelected {
                        callback_id,
                        value,
                    });
                }
            }
            raider_tui::Event::PluginDialogDismissed { callback_id } => {
                if let Some(plugin) = &plugin_handle {
                    plugin.send(raider_plugin_lua::PluginEvent::DialogDismissed { callback_id });
                }
            }
            raider_tui::Event::UserMessage(text) => {
                let _ = action_tx.send(Action::Host(HostAction::SetBusy(true)));
                let _ = prompt_tx.send(PromptRequest {
                    text,
                    files: Vec::new(),
                });
            }
            raider_tui::Event::UserMessageWithFiles { text, files } => {
                let _ = action_tx.send(Action::Host(HostAction::SetBusy(true)));
                let _ = prompt_tx.send(PromptRequest { text, files });
            }
            raider_tui::Event::ModelChanged { model, variant } => {
                let _ = model_tx.send(Some(model));
                let _ = variant_tx.send(variant);
            }
            raider_tui::Event::VariantChanged(variant) => {
                let _ = variant_tx.send(variant);
            }
            raider_tui::Event::AgentChanged(name) => {
                let _ = agent_tx.send(name);
            }
            raider_tui::Event::Interrupt => {
                let Some(session_id) = active_tx.borrow().clone() else {
                    continue;
                };
                let backend_c = Arc::clone(&backend);
                let action_tx_c = action_tx.clone();
                tokio::spawn(async move {
                    if let Err(e) = backend_c.session_abort(&session_id).await {
                        tracing::warn!(error = %e, "session.abort failed");
                        report_backend_error(&action_tx_c, "Failed to interrupt session", &e);
                    } else {
                        let _ = action_tx_c.send(Action::Host(HostAction::SetBusy(false)));
                        let _ = action_tx_c
                            .send(Action::Host(HostAction::AssistantDone { message_id: None }));
                    }
                });
            }
            raider_tui::Event::PermissionReply {
                request_id,
                reply,
                message,
            } => {
                let wire_reply = match reply {
                    raider_tui::PermissionReplyChoice::Once => {
                        raider_opencode::PermissionReply::Once
                    }
                    raider_tui::PermissionReplyChoice::Always => {
                        raider_opencode::PermissionReply::Always
                    }
                    raider_tui::PermissionReplyChoice::Reject => {
                        raider_opencode::PermissionReply::Reject
                    }
                };
                let rid = request_id.clone();
                spawn_backend_call(
                    &backend,
                    &action_tx,
                    move |b| async move { b.permission_reply(&request_id, wire_reply, message).await },
                    move |e, tx| {
                        tracing::warn!(error = %e, request_id = %rid, "permission.reply failed");
                        report_backend_error(&tx, "Failed to reply to permission", &e);
                    },
                );
            }
            raider_tui::Event::QuestionReply {
                request_id,
                answers,
            } => {
                let rid = request_id.clone();
                spawn_backend_call(
                    &backend,
                    &action_tx,
                    move |b| async move { b.question_reply(&request_id, answers).await },
                    move |e, tx| {
                        tracing::warn!(error = %e, request_id = %rid, "question.reply failed");
                        report_backend_error(&tx, "Failed to reply to question", &e);
                    },
                );
            }
            raider_tui::Event::QuestionReject { request_id } => {
                let rid = request_id.clone();
                spawn_backend_call(
                    &backend,
                    &action_tx,
                    move |b| async move { b.question_reject(&request_id).await },
                    move |e, tx| {
                        tracing::warn!(error = %e, request_id = %rid, "question.reject failed");
                        report_backend_error(&tx, "Failed to reject question", &e);
                    },
                );
            }
            raider_tui::Event::Command { name, args: _ } if name == "new" => {
                handle_new_command(&backend, &action_tx, &active_tx, plugin_handle.as_ref());
            }
            raider_tui::Event::Command { name, args: _ }
                if name == "share" || name == "unshare" =>
            {
                handle_share_command(&backend, &action_tx, &active_tx, name);
            }
            raider_tui::Event::Undo { message_id } => {
                let Some(session_id) = active_tx.borrow().clone() else {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
                        "No active session to undo.".to_string(),
                    )));
                    continue;
                };
                spawn_backend_call(
                    &backend,
                    &action_tx,
                    move |b| async move { b.session_revert(&session_id, &message_id).await },
                    |e, tx| {
                        tracing::warn!(error = %e, "session.revert failed");
                        report_backend_error(&tx, "Failed to undo", &e);
                    },
                );
            }
            raider_tui::Event::Redo => {
                let Some(session_id) = active_tx.borrow().clone() else {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
                        "No active session to redo.".to_string(),
                    )));
                    continue;
                };
                spawn_backend_call(
                    &backend,
                    &action_tx,
                    move |b| async move { b.session_unrevert(&session_id).await },
                    |e, tx| {
                        tracing::warn!(error = %e, "session.unrevert failed");
                        report_backend_error(&tx, "Failed to redo", &e);
                    },
                );
            }
            raider_tui::Event::DeleteSession { session_id } => {
                let sid = SessionId::new(session_id);
                spawn_backend_call(
                    &backend,
                    &action_tx,
                    move |b| async move { b.session_delete(&sid).await },
                    |e, tx| {
                        tracing::warn!(error = %e, "session.delete failed");
                        report_backend_error(&tx, "Failed to delete", &e);
                    },
                );
            }
            raider_tui::Event::ForkSession { message_id } => {
                let Some(session_id) = active_tx.borrow().clone() else {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
                        "No active session to fork.".to_string(),
                    )));
                    continue;
                };
                let backend_c = Arc::clone(&backend);
                let action_tx_c = action_tx.clone();
                tokio::spawn(async move {
                    let mid = message_id.as_deref();
                    match backend_c.session_fork(&session_id, mid).await {
                        Ok(new_session) => {
                            let _ = action_tx_c.send(Action::View(ViewAction::SwitchSession(
                                new_session.id.as_str().to_string(),
                            )));
                            let _ = action_tx_c.send(Action::Host(HostAction::SystemMessage(
                                format!("Forked into new session {}", new_session.id.as_str()),
                            )));
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "session.fork failed");
                            report_backend_error(&action_tx_c, "Failed to fork", &e);
                        }
                    }
                });
            }
            raider_tui::Event::RenameSession { session_id, title } => {
                handle_rename_session(
                    &backend,
                    &action_tx,
                    &active_tx,
                    SessionId::new(session_id),
                    title,
                );
            }
            raider_tui::Event::Command { name, args } if name == "rename" => {
                let Some(session_id) = active_tx.borrow().clone() else {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
                        "No active session to rename.".to_string(),
                    )));
                    continue;
                };
                handle_rename_session(&backend, &action_tx, &active_tx, session_id, args);
            }
            raider_tui::Event::Command { name, args: _ } if name == "compact" => {
                handle_compact_command(&backend, &action_tx, &active_tx, &model_tx);
            }
            _ => {}
        }
    }
}

fn handle_rename_session<B: Backend>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    active_tx: &watch::Sender<Option<SessionId>>,
    session_id: SessionId,
    title: String,
) {
    let active_id = active_tx
        .borrow()
        .as_ref()
        .map(|id| id.as_str().to_string());
    let backend_c = Arc::clone(backend);
    let action_tx_c = action_tx.clone();
    tokio::spawn(async move {
        match backend_c.session_rename(&session_id, &title).await {
            Ok(session) => {
                let is_active = active_id.as_deref() == Some(session.id.as_str());
                let sidebar_title = if session.title.trim().is_empty() {
                    session.id.as_str().to_string()
                } else {
                    session.title.clone()
                };
                let entry = session_to_entry(&session, active_id.as_deref());
                let _ = action_tx_c.send(Action::Host(HostAction::UpsertSession(entry)));
                if is_active {
                    let _ =
                        action_tx_c.send(Action::Host(HostAction::SetSidebarTitle(sidebar_title)));
                }
                let _ = action_tx_c.send(Action::View(ViewAction::ShowToast(Toast::new(
                    format!("Renamed session to: {title}"),
                    ToastVariant::Success,
                ))));
            }
            Err(e) => {
                tracing::warn!(error = %e, "session.rename failed");
                report_backend_error(&action_tx_c, "Failed to rename", &e);
            }
        }
    });
}

fn handle_session_switched<B: Backend>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    active_tx: &watch::Sender<Option<SessionId>>,
    refetch_tx: &UnboundedSender<SessionId>,
    plugin_handle: Option<&raider_plugin_lua::LuaPluginHandle>,
    id: String,
) {
    let sid = SessionId::new(id.clone());
    let _ = active_tx.send(Some(sid.clone()));
    let _ = action_tx.send(Action::Host(HostAction::SetCurrentSession(Some(id))));
    if let Some(plugin) = plugin_handle {
        plugin.send(raider_plugin_lua::PluginEvent::SessionChanged {
            session_id: Some(sid.as_str().to_string()),
        });
    }
    let _ = action_tx.send(Action::Host(HostAction::SetBusy(false)));
    let _ = action_tx.send(Action::Host(HostAction::SetUsage(None)));
    let _ = action_tx.send(Action::Host(HostAction::AssistantDone { message_id: None }));
    let _ = refetch_tx.send(sid.clone());
    fetch_pending_permissions_for_session(backend, action_tx, sid.clone());
    fetch_pending_questions_for_session(backend, action_tx, sid);
}

fn handle_new_command<B: Backend>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    active_tx: &watch::Sender<Option<SessionId>>,
    plugin_handle: Option<&raider_plugin_lua::LuaPluginHandle>,
) {
    if let Some(outgoing) = active_tx.borrow().clone() {
        let backend_c = Arc::clone(backend);
        tokio::spawn(async move {
            if let Err(e) = backend_c.session_abort(&outgoing).await {
                tracing::debug!(error = %e, "session.abort on /new failed (likely already idle)");
            }
        });
    }
    let _ = active_tx.send(None);
    let _ = action_tx.send(Action::Host(HostAction::SetCurrentSession(None)));
    if let Some(plugin) = plugin_handle {
        plugin.send(raider_plugin_lua::PluginEvent::SessionChanged { session_id: None });
    }
    let _ = action_tx.send(Action::Host(HostAction::ReplaceMessages(Vec::new())));
    let _ = action_tx.send(Action::Host(HostAction::SetSidebarTitle(
        "Session".to_string(),
    )));
    let _ = action_tx.send(Action::Host(HostAction::SetSidebarSubtitle(None)));
    let _ = action_tx.send(Action::Host(HostAction::SetSidebarSections(Vec::new())));
    let _ = action_tx.send(Action::Host(HostAction::SetBusy(false)));
    let _ = action_tx.send(Action::Host(HostAction::SetUsage(None)));
    let _ = action_tx.send(Action::Host(HostAction::AssistantDone { message_id: None }));
}

fn handle_share_command<B: Backend>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    active_tx: &watch::Sender<Option<SessionId>>,
    name: String,
) {
    let active = active_tx.borrow().clone();
    let Some(session_id) = active else {
        let _ = action_tx.send(Action::View(ViewAction::ShowToast(raider_tui::Toast::new(
            format!("No active session to /{name} — start a conversation first."),
            raider_tui::ToastVariant::Error,
        ))));
        return;
    };
    let backend_c = Arc::clone(backend);
    let action_tx_c = action_tx.clone();
    let is_share = name == "share";
    tokio::spawn(async move {
        let result = if is_share {
            backend_c.session_share(&session_id).await
        } else {
            backend_c.session_unshare(&session_id).await
        };
        match result {
            Ok(session) => {
                if is_share {
                    let url = session
                        .extra
                        .get("share")
                        .and_then(|v| v.get("url"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                    if let Some(url) = url {
                        let _ = action_tx_c.send(Action::View(ViewAction::CopyToClipboard {
                            text: url,
                            success_message: "Share URL copied to clipboard!".into(),
                            error_message: "Failed to copy URL to clipboard".into(),
                        }));
                    } else {
                        let _ = action_tx_c.send(Action::View(ViewAction::ShowToast(
                            raider_tui::Toast::new(
                                "Session shared (server did not return a URL)",
                                raider_tui::ToastVariant::Warning,
                            ),
                        )));
                    }
                } else {
                    let _ = action_tx_c.send(Action::View(ViewAction::ShowToast(
                        raider_tui::Toast::new(
                            "Session unshared successfully",
                            raider_tui::ToastVariant::Success,
                        ),
                    )));
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, command = name, "share/unshare failed");
                let _ =
                    action_tx_c.send(Action::View(ViewAction::ShowToast(raider_tui::Toast::new(
                        format!("Failed to /{name}: {e}"),
                        raider_tui::ToastVariant::Error,
                    ))));
            }
        }
    });
}

fn handle_compact_command<B: Backend>(
    backend: &Arc<B>,
    action_tx: &UnboundedSender<Action>,
    active_tx: &watch::Sender<Option<SessionId>>,
    model_tx: &watch::Sender<Option<ModelRef>>,
) {
    let active = active_tx.borrow().clone();
    let model = model_tx.borrow().clone();
    let Some(session_id) = active else {
        let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
            "No active session to compact — start a conversation first.".to_string(),
        )));
        return;
    };
    let Some(model_ref) = model else {
        let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
            "Pick a model with /models before running /compact.".to_string(),
        )));
        return;
    };
    spawn_backend_call(
        backend,
        action_tx,
        move |b| async move {
            b.session_summarize(&session_id, &model_ref.provider_id, &model_ref.model_id)
                .await
        },
        |e, _tx| {
            tracing::warn!(
                error = %e,
                "session.summarize HTTP call returned an error (compaction may still \
                 succeed server-side; outcome arrives via SSE message.updated)",
            );
        },
    );
}
