use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{watch, Mutex};

use raider_opencode::types::common::SessionId;
use raider_tui::{Action, HostAction};

use crate::backend::Backend;
use crate::bridge;

use super::helpers::fetch_session_bundle;

pub(super) async fn message_task<B: Backend>(
    backend: Arc<B>,
    action_tx: UnboundedSender<Action>,
    mut refetch_rx: UnboundedReceiver<SessionId>,
    catalog_rx: watch::Receiver<Option<raider_tui::ModelCatalog>>,
    active_rx: watch::Receiver<Option<SessionId>>,
) {
    let cache: Arc<Mutex<HashMap<String, Vec<Action>>>> = Arc::new(Mutex::new(HashMap::new()));

    while let Some(id) = refetch_rx.recv().await {
        let key = id.as_str().to_string();
        let cached = { cache.lock().await.get(&key).cloned() };
        if let Some(actions) = cached {
            send_if_active(
                &action_tx,
                &active_rx,
                &id,
                actions
                    .into_iter()
                    .filter(|a| !matches!(a, Action::Host(HostAction::ReplaceMessages(_)))),
            );
        } else if is_active(&active_rx, &id) {
            let _ = action_tx.send(Action::Host(HostAction::ReplaceMessages(Vec::new())));
        }

        let backend = Arc::clone(&backend);
        let action_tx = action_tx.clone();
        let active_rx = active_rx.clone();
        let catalog_rx = catalog_rx.clone();
        let cache = Arc::clone(&cache);
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            let bundle = fetch_session_bundle(&backend, &id, "message_task").await;
            let catalog_snapshot = catalog_rx.borrow().clone();
            let actions = session_view_actions(&id, bundle, catalog_snapshot.as_ref());
            let was_cached = {
                let mut cache = cache.lock().await;
                let was_cached = cache.contains_key(&key);
                cache.insert(key, actions.clone());
                was_cached
            };
            tracing::debug!(
                session = id.as_str(),
                elapsed_ms = started.elapsed().as_millis(),
                cached = was_cached,
                "session view refreshed",
            );
            if was_cached {
                send_if_active(
                    &action_tx,
                    &active_rx,
                    &id,
                    actions
                        .into_iter()
                        .filter(|a| !matches!(a, Action::Host(HostAction::ReplaceMessages(_)))),
                );
            } else {
                send_if_active(&action_tx, &active_rx, &id, actions.into_iter());
            }
        });
    }
}

fn session_view_actions(
    id: &SessionId,
    bundle: super::helpers::SessionBundle,
    catalog_snapshot: Option<&raider_tui::ModelCatalog>,
) -> Vec<Action> {
    let messages_for_sidebar = match &bundle.messages {
        Ok(list) => list.clone(),
        Err(e) => {
            tracing::warn!(error = %e, "session_messages failed; sidebar Context will read 0 tokens");
            Vec::new()
        }
    };

    let mut actions = Vec::new();
    match &bundle.session {
        Ok(session) => {
            actions.extend(bridge::sidebar_actions_for_session(
                session,
                catalog_snapshot,
                &messages_for_sidebar,
                &bundle.diff,
                &bundle.todo,
                &bundle.mcp,
                &bundle.lsp,
                bundle.config.lsp_enabled(),
            ));
        }
        Err(e) => {
            tracing::warn!(error = %e, "session_get failed; sidebar will fall back to id");
            actions.push(Action::Host(HostAction::SetSidebarTitle(
                id.as_str().to_string(),
            )));
            actions.push(Action::Host(HostAction::SetSidebarSubtitle(None)));
            actions.push(Action::Host(HostAction::SetSidebarSections(Vec::new())));
            actions.push(Action::Host(HostAction::SetSidebarVisible(true)));
        }
    }

    match bundle.messages {
        Ok(messages) => {
            actions.extend(bridge::messages_refresh_actions(&messages));
        }
        Err(e) => {
            actions.push(Action::Host(HostAction::SystemMessage(format!(
                "Failed to load messages: {e}"
            ))));
        }
    }
    actions
}

fn is_active(active_rx: &watch::Receiver<Option<SessionId>>, id: &SessionId) -> bool {
    active_rx.borrow().as_ref() == Some(id)
}

fn send_if_active(
    action_tx: &UnboundedSender<Action>,
    active_rx: &watch::Receiver<Option<SessionId>>,
    id: &SessionId,
    actions: impl IntoIterator<Item = Action>,
) {
    if !is_active(active_rx, id) {
        return;
    }
    for action in actions {
        let _ = action_tx.send(action);
    }
}
