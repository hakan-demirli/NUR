use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::watch;

use raider_opencode::events::StreamItem;
use raider_opencode::types::common::SessionId;
use raider_tui::{Action, HostAction};

use crate::backend::Backend;
use crate::bridge::{self, PartMirror};

struct DisconnectState {
    consecutive_errors: u32,
    warned: bool,
    last_msg_at: Option<std::time::Instant>,
    had_live_stream: bool,
    threshold: u32,
}

impl DisconnectState {
    const DEDUP_WINDOW: Duration = Duration::from_secs(30);

    fn new(threshold: u32) -> Self {
        Self {
            consecutive_errors: 0,
            warned: false,
            last_msg_at: None,
            had_live_stream: false,
            threshold,
        }
    }

    fn on_event(&mut self) -> bool {
        self.consecutive_errors = 0;
        self.had_live_stream = true;
        if self.warned {
            self.warned = false;
            true
        } else {
            false
        }
    }

    fn on_error(&mut self) -> bool {
        self.consecutive_errors = self.consecutive_errors.saturating_add(1);
        let should_warn = self.consecutive_errors >= self.threshold
            && !self.warned
            && self
                .last_msg_at
                .is_none_or(|t| t.elapsed() >= Self::DEDUP_WINDOW);
        if should_warn {
            self.warned = true;
            self.last_msg_at = Some(std::time::Instant::now());
        }
        should_warn
    }

    fn on_reconnecting(&mut self) -> bool {
        let had = self.had_live_stream;
        self.had_live_stream = false;
        had
    }
}

pub(super) async fn event_task<B: Backend>(
    backend: Arc<B>,
    action_tx: UnboundedSender<Action>,
    active_rx: watch::Receiver<Option<SessionId>>,
    _refetch_tx: UnboundedSender<SessionId>,
    sidebar_refetch_tx: UnboundedSender<SessionId>,
    disconnect_threshold: u32,
) {
    let mut stream = backend.events();
    let mut mirror = PartMirror::new();
    let mut state = DisconnectState::new(disconnect_threshold);

    loop {
        let item = tokio::select! {
            i = stream.next() => i,
            _ = tokio::time::sleep(Duration::from_secs(3600)) => continue,
        };

        let Some(item) = item else {
            tracing::info!("event stream ended");
            break;
        };

        match item {
            StreamItem::Event(ev) => {
                if state.on_event() {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(
                        "reconnected to opencode server".to_string(),
                    )));
                }
                let active = active_rx.borrow().clone();
                let translation = bridge::translate(*ev, active.as_ref(), &mut mirror);
                for line in translation.log {
                    tracing::debug!(line = %line, "event log");
                }
                let turn_finished = translation
                    .actions
                    .iter()
                    .any(|a| matches!(a, Action::Host(HostAction::AssistantDone)));
                for action in translation.actions {
                    let _ = action_tx.send(action);
                }
                if turn_finished {
                    if let Some(sid) = active {
                        let _ = sidebar_refetch_tx.send(sid);
                    }
                }
            }
            StreamItem::Error(e) => {
                tracing::warn!(
                    error = %e,
                    attempts = state.consecutive_errors + 1,
                    "event stream error",
                );
                tracing::debug!(error_debug = ?e, "event stream error (debug)");
                if state.on_error() {
                    let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                        "disconnected from opencode ({e}); retrying…"
                    ))));
                }
            }
            StreamItem::Reconnecting { attempt } => {
                tracing::debug!(attempt, "reconnecting to event stream");
                if state.on_reconnecting() {
                    mirror.clear();
                }
            }
        }
    }
}
