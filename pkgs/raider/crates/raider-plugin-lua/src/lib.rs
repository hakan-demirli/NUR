use std::fmt;
use std::path::PathBuf;

use tokio::sync::mpsc::UnboundedSender;

mod bindings;
mod dispatch;
mod lifecycle;
mod manifest;
mod marshal;
mod registry;
mod runtime;
mod spec;

#[cfg(test)]
mod tests;

pub use lifecycle::{default_plugin_paths, spawn};
pub use manifest::PluginManifest;

pub use raider_tui::{PluginInfo, PluginKind, PluginStatus};

#[derive(Clone, Debug, Default)]
pub struct LuaPluginConfig {
    pub plugin_paths: Vec<PathBuf>,
    pub workspace_directory: Option<String>,
    pub current_session: Option<String>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct PluginId(String);

impl PluginId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Display for PluginId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for PluginId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<&str> for PluginId {
    fn from(value: &str) -> Self {
        Self(value.to_string())
    }
}

#[derive(Clone, Debug)]
pub enum PluginEvent {
    Command { name: String, args: String },
    DialogSelected { callback_id: u64, value: String },
    DialogDismissed { callback_id: u64 },
    SessionChanged { session_id: Option<String> },
    LifecycleToggle(PluginId),
    LifecycleReload(PluginId),
    LifecycleAdd { path: PathBuf },
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
