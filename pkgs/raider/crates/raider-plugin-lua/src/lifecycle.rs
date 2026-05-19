use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mlua::Lua;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::task::JoinHandle;

use raider_tui::{Action, HostAction};

use crate::bindings::install_api;
use crate::dispatch::handle_event;
use crate::runtime::RuntimeState;
use crate::{LuaPluginConfig, LuaPluginHandle, PluginEvent};

pub fn spawn(
    config: LuaPluginConfig,
    action_tx: UnboundedSender<Action>,
) -> Option<(LuaPluginHandle, JoinHandle<()>)> {
    let plugin_paths = expand_plugin_paths(config.plugin_paths);
    if plugin_paths.is_empty() {
        return None;
    }

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    let handle = LuaPluginHandle { tx };
    let task = tokio::spawn(async move {
        let config = LuaPluginConfig {
            plugin_paths,
            workspace_directory: config.workspace_directory,
            current_session: config.current_session,
        };
        if let Err(error) = run(config, action_tx.clone(), rx).await {
            tracing::warn!(error = %error, "lua plugin runtime stopped");
            let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                "Lua plugin runtime stopped: {error}"
            ))));
        }
    });

    Some((handle, task))
}

pub fn default_plugin_paths() -> Vec<PathBuf> {
    let Some(config_dir) = config_dir() else {
        return Vec::new();
    };
    lua_files_in_dir(&config_dir.join("raider").join("plugins"))
}

async fn run(
    config: LuaPluginConfig,
    action_tx: UnboundedSender<Action>,
    mut rx: UnboundedReceiver<PluginEvent>,
) -> anyhow::Result<()> {
    let lua = Lua::new();
    let state = Arc::new(Mutex::new(RuntimeState {
        next_callback_id: 1,
        ..RuntimeState::default()
    }));

    install_api(
        &lua,
        Arc::clone(&state),
        action_tx.clone(),
        config.workspace_directory.clone(),
        config.current_session.clone(),
    )?;

    for path in &config.plugin_paths {
        let source = std::fs::read_to_string(path)?;
        lua.load(&source)
            .set_name(path.display().to_string())
            .exec()?;
        tracing::info!(path = %path.display(), "loaded lua plugin");
    }

    while let Some(event) = rx.recv().await {
        if let Err(error) = handle_event(&lua, Arc::clone(&state), &action_tx, event) {
            tracing::warn!(error = %error, "lua plugin event failed");
            let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                "Lua plugin error: {error}"
            ))));
        }
    }

    Ok(())
}

fn expand_plugin_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for path in paths {
        if path.is_dir() {
            out.extend(lua_files_in_dir(&path));
        } else if is_lua_file(&path) {
            out.push(path);
        }
    }
    out.sort();
    out.dedup();
    out
}

fn lua_files_in_dir(dir: &Path) -> Vec<PathBuf> {
    let mut paths = match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| is_lua_file(path))
            .collect::<Vec<_>>(),
        Err(_) => Vec::new(),
    };
    paths.sort();
    paths
}

fn is_lua_file(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("lua")
}

fn config_dir() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        return Some(PathBuf::from(path));
    }
    std::env::var_os("HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .map(|home| home.join(".config"))
}
