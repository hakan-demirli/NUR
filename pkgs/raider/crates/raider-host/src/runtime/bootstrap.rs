use std::sync::Arc;

use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;

use raider_opencode::types::common::SessionId;
use raider_tui::{Action, HostAction, ModelRef};

use crate::backend::Backend;
use crate::bridge;

use super::helpers::retry_with_backoff;

const MAX_ATTEMPTS: u32 = 8;

pub(super) async fn session_task<B: Backend>(
    backend: Arc<B>,
    action_tx: UnboundedSender<Action>,
    initial_session: Option<SessionId>,
) {
    let sessions =
        match retry_with_backoff("sessions_list", || backend.sessions_list(), MAX_ATTEMPTS).await {
            Ok(s) => s,
            Err(e) => {
                let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                    "Failed to list sessions after {MAX_ATTEMPTS} attempts: {e}"
                ))));
                return;
            }
        };
    for a in bridge::sessions_refresh_actions(&sessions, initial_session.as_ref()) {
        let _ = action_tx.send(a);
    }

    if let Ok(status_map) = backend.session_status_map().await {
        for (sid, status) in status_map {
            let tui_status = bridge::session_status_to_tui(&status);
            let busy = status.is_working();
            let _ = action_tx.send(Action::Host(HostAction::SetSessionStatus {
                session_id: sid.clone(),
                status: tui_status,
            }));
            let _ = action_tx.send(Action::Host(HostAction::SetSessionBusy {
                session_id: sid,
                busy,
            }));
        }
    }
}

pub(super) async fn provider_task<B: Backend>(
    backend: Arc<B>,
    action_tx: UnboundedSender<Action>,
    model_tx: watch::Sender<Option<ModelRef>>,
    catalog_tx: watch::Sender<Option<raider_tui::ModelCatalog>>,
) {
    let list =
        match retry_with_backoff("provider_list", || backend.provider_list(), MAX_ATTEMPTS).await {
            Ok(l) => l,
            Err(e) => {
                let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                    "Failed to list providers after {MAX_ATTEMPTS} attempts: {e}"
                ))));
                return;
            }
        };
    let actions = bridge::provider_refresh_actions(&list, None);
    for a in &actions {
        if let Action::Host(HostAction::SetCurrentModel(picked)) = a {
            let _ = model_tx.send(picked.clone());
        }
        if let Action::Host(HostAction::SetCatalog(cat)) = a {
            let _ = catalog_tx.send(Some(cat.clone()));
        }
    }
    for a in actions {
        let _ = action_tx.send(a);
    }
}
