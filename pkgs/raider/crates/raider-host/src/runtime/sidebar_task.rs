use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::watch;

use raider_opencode::types::common::SessionId;
use raider_tui::Action;

use crate::backend::Backend;
use crate::bridge;

use super::helpers::fetch_session_bundle;

pub(super) async fn sidebar_task<B: Backend>(
    backend: Arc<B>,
    action_tx: UnboundedSender<Action>,
    mut sidebar_refetch_rx: UnboundedReceiver<SessionId>,
    catalog_rx: watch::Receiver<Option<raider_tui::ModelCatalog>>,
) {
    while let Some(id) = sidebar_refetch_rx.recv().await {
        let bundle = fetch_session_bundle(&backend, &id, "sidebar_task").await;
        let catalog_snapshot = catalog_rx.borrow().clone();

        let messages_list = match bundle.messages {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "sidebar refresh: session_messages failed");
                Vec::new()
            }
        };

        match bundle.session {
            Ok(session) => {
                for a in bridge::sidebar_actions_for_session(
                    &session,
                    catalog_snapshot.as_ref(),
                    &messages_list,
                    &bundle.diff,
                    &bundle.todo,
                    &bundle.mcp,
                    &bundle.lsp,
                    bundle.config.lsp_enabled(),
                ) {
                    let _ = action_tx.send(a);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "sidebar refresh: session_get failed");
            }
        }
    }
}

pub(super) async fn catalog_watcher_task<B: Backend>(
    backend: Arc<B>,
    action_tx: UnboundedSender<Action>,
    active_rx: watch::Receiver<Option<SessionId>>,
    mut catalog_rx: watch::Receiver<Option<raider_tui::ModelCatalog>>,
) {
    while catalog_rx.changed().await.is_ok() {
        let catalog = match catalog_rx.borrow().clone() {
            Some(c) => c,
            None => continue,
        };
        let active = active_rx.borrow().clone();
        let Some(sid) = active else {
            continue;
        };
        let bundle = fetch_session_bundle(&backend, &sid, "catalog_watcher").await;
        let messages_list = match bundle.messages {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e, "catalog-driven session_messages failed");
                Vec::new()
            }
        };
        match bundle.session {
            Ok(session) => {
                for a in bridge::sidebar_actions_for_session(
                    &session,
                    Some(&catalog),
                    &messages_list,
                    &bundle.diff,
                    &bundle.todo,
                    &bundle.mcp,
                    &bundle.lsp,
                    bundle.config.lsp_enabled(),
                ) {
                    let _ = action_tx.send(a);
                }
            }
            Err(e) => {
                tracing::warn!(error = %e, "catalog-driven sidebar refresh failed");
            }
        }
    }
}
