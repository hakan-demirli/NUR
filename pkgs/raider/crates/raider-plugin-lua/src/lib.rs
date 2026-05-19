use std::path::PathBuf;

use tokio::sync::mpsc::UnboundedSender;

mod bindings;
mod dispatch;
mod lifecycle;
mod marshal;
mod runtime;
mod spec;

#[cfg(test)]
mod tests;

pub use lifecycle::{default_plugin_paths, spawn};

#[derive(Clone, Debug, Default)]
pub struct LuaPluginConfig {
    pub plugin_paths: Vec<PathBuf>,
    pub workspace_directory: Option<String>,
    pub current_session: Option<String>,
}

#[derive(Clone, Debug)]
pub enum PluginEvent {
    Command { name: String, args: String },
    DialogSelected { callback_id: u64, value: String },
    DialogDismissed { callback_id: u64 },
    SessionChanged { session_id: Option<String> },
}

#[derive(Clone)]
pub struct LuaPluginHandle {
    pub(crate) tx: UnboundedSender<PluginEvent>,
}

impl LuaPluginHandle {
    pub fn send(&self, event: PluginEvent) {
        let _ = self.tx.send(event);
    }
}
