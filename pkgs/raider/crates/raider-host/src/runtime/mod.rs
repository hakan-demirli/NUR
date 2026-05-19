use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{watch, Mutex};
use tokio::task::JoinHandle;

use raider_opencode::types::common::SessionId;
use raider_tui::{Action, ModelRef};

use crate::backend::Backend;

mod bootstrap;
mod event_task;
mod helpers;
mod message_task;
mod prompt_task;
mod sidebar_task;
mod ui_events;

use bootstrap::{provider_task, session_task};
use event_task::event_task;
use helpers::{fetch_pending_permissions_for_session, fetch_pending_questions_for_session};
use message_task::message_task;
use prompt_task::{prompt_task, PromptRequest, PromptTask};
use sidebar_task::{catalog_watcher_task, sidebar_task};
use ui_events::{ui_event_task, UiEventTask};

#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    pub initial_session: Option<SessionId>,
    pub disconnect_warning_threshold: u32,
    pub workspace_directory: Option<String>,
    pub lua_plugin_paths: Vec<PathBuf>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            initial_session: None,
            disconnect_warning_threshold: 3,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
        }
    }
}

pub struct HostHandle {
    pub ui_events: UnboundedSender<raider_tui::Event>,
    pub actions: Arc<Mutex<UnboundedReceiver<Action>>>,
    join_handles: Vec<JoinHandle<()>>,
}

impl HostHandle {
    pub fn shutdown(&mut self) {
        for h in self.join_handles.drain(..) {
            h.abort();
        }
    }
}

impl Drop for HostHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub struct Runtime;

impl Runtime {
    pub fn spawn<B: Backend + 'static>(backend: Arc<B>, config: RuntimeConfig) -> HostHandle {
        let lua_plugin_paths = config.lua_plugin_paths.clone();
        Self::spawn_with_lua_plugins(backend, config, lua_plugin_paths)
    }

    pub fn spawn_with_lua_plugins<B: Backend + 'static>(
        backend: Arc<B>,
        config: RuntimeConfig,
        lua_plugin_paths: Vec<PathBuf>,
    ) -> HostHandle {
        let (action_tx, action_rx) = tokio::sync::mpsc::unbounded_channel::<Action>();
        let (ui_tx, ui_rx) = tokio::sync::mpsc::unbounded_channel::<raider_tui::Event>();
        let (active_tx, active_rx) =
            watch::channel::<Option<SessionId>>(config.initial_session.clone());

        let (model_tx, model_rx) = watch::channel::<Option<ModelRef>>(None);
        let (variant_tx, variant_rx) = watch::channel::<Option<String>>(None);
        let (agent_tx, agent_rx) = watch::channel::<String>("build".to_string());
        let (catalog_tx, catalog_rx) = watch::channel::<Option<raider_tui::ModelCatalog>>(None);

        let (prompt_tx, prompt_rx) = tokio::sync::mpsc::unbounded_channel::<PromptRequest>();

        let (refetch_tx, refetch_rx) = tokio::sync::mpsc::unbounded_channel::<SessionId>();

        let (sidebar_refetch_tx, sidebar_refetch_rx) =
            tokio::sync::mpsc::unbounded_channel::<SessionId>();

        let mut handles = Vec::new();

        let plugin_handle = raider_plugin_lua::spawn(
            raider_plugin_lua::LuaPluginConfig {
                plugin_paths: lua_plugin_paths,
                workspace_directory: config.workspace_directory.clone(),
                current_session: config
                    .initial_session
                    .as_ref()
                    .map(|id| id.as_str().to_string()),
            },
            action_tx.clone(),
        )
        .map(|(handle, task)| {
            handles.push(task);
            handle
        });

        {
            let backend = Arc::clone(&backend);
            let directory = config.workspace_directory.clone();
            handles.push(tokio::spawn(async move {
                match backend.sync_start(directory.as_deref()).await {
                    Ok(true) => tracing::info!("sync.start succeeded"),
                    Ok(false) => tracing::info!("sync.start declined by server"),
                    Err(e) => tracing::warn!(error = %e, "sync.start failed"),
                }
            }));
        }

        {
            let backend = Arc::clone(&backend);
            let action_tx = action_tx.clone();
            let initial = config.initial_session.clone();
            handles.push(tokio::spawn(async move {
                session_task(backend, action_tx, initial).await;
            }));
        }
        {
            let backend = Arc::clone(&backend);
            let action_tx = action_tx.clone();
            let model_tx = model_tx.clone();
            let catalog_tx = catalog_tx.clone();
            handles.push(tokio::spawn(async move {
                provider_task(backend, action_tx, model_tx, catalog_tx).await;
            }));
        }

        {
            let backend = Arc::clone(&backend);
            let active_tx = active_tx.clone();
            let action_tx = action_tx.clone();
            let model_tx = model_tx.clone();
            let variant_tx = variant_tx.clone();
            let agent_tx = agent_tx.clone();
            let prompt_tx = prompt_tx.clone();
            let refetch_tx = refetch_tx.clone();
            let plugin_handle = plugin_handle.clone();
            handles.push(tokio::spawn(async move {
                ui_event_task(UiEventTask {
                    backend,
                    ui_rx,
                    active_tx,
                    action_tx,
                    model_tx,
                    variant_tx,
                    agent_tx,
                    prompt_tx,
                    refetch_tx,
                    plugin_handle,
                })
                .await;
            }));
        }

        {
            let backend = Arc::clone(&backend);
            let action_tx = action_tx.clone();
            let catalog_rx = catalog_rx.clone();
            let active_rx = active_tx.subscribe();
            handles.push(tokio::spawn(async move {
                message_task(backend, action_tx, refetch_rx, catalog_rx, active_rx).await;
            }));
        }

        {
            let backend = Arc::clone(&backend);
            let action_tx = action_tx.clone();
            let catalog_rx = catalog_rx.clone();
            handles.push(tokio::spawn(async move {
                sidebar_task(backend, action_tx, sidebar_refetch_rx, catalog_rx).await;
            }));
        }

        {
            let backend = Arc::clone(&backend);
            let action_tx = action_tx.clone();
            let active_rx = active_tx.subscribe();
            let catalog_rx = catalog_rx.clone();
            handles.push(tokio::spawn(async move {
                catalog_watcher_task(backend, action_tx, active_rx, catalog_rx).await;
            }));
        }

        if let Some(initial) = config.initial_session.clone() {
            let _ = refetch_tx.send(initial.clone());
            fetch_pending_permissions_for_session(&backend, &action_tx, initial.clone());
            fetch_pending_questions_for_session(&backend, &action_tx, initial);
        }
        let refetch_tx_keepalive = refetch_tx;
        let _active_rx = active_rx;

        {
            let backend = Arc::clone(&backend);
            let action_tx = action_tx.clone();
            let active_tx = active_tx.clone();
            let sidebar_refetch_tx = sidebar_refetch_tx.clone();
            let plugin_handle = plugin_handle.clone();
            handles.push(tokio::spawn(async move {
                prompt_task(PromptTask {
                    backend,
                    action_tx,
                    prompt_rx,
                    active_tx,
                    model_rx,
                    variant_rx,
                    agent_rx,
                    sidebar_refetch_tx,
                    plugin_handle,
                })
                .await;
            }));
        }

        {
            let backend = Arc::clone(&backend);
            let action_tx = action_tx.clone();
            let active_rx = active_tx.subscribe();
            let refetch_tx = refetch_tx_keepalive.clone();
            let sidebar_refetch_tx = sidebar_refetch_tx.clone();
            let threshold = config.disconnect_warning_threshold;
            handles.push(tokio::spawn(async move {
                event_task(
                    backend,
                    action_tx,
                    active_rx,
                    refetch_tx,
                    sidebar_refetch_tx,
                    threshold,
                )
                .await;
            }));
        }
        let _sidebar_refetch_tx_keepalive = sidebar_refetch_tx;
        let _refetch_tx_keepalive = refetch_tx_keepalive;

        HostHandle {
            ui_events: ui_tx,
            actions: Arc::new(Mutex::new(action_rx)),
            join_handles: handles,
        }
    }
}
