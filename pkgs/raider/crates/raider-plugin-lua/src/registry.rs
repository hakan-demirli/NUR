use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use mlua::Lua;

use crate::manifest::PluginManifest;
use crate::runtime::RuntimeState;
use crate::{PluginId, PluginInfo, PluginKind, PluginStatus};

struct PluginEntry {
    id: PluginId,
    path: PathBuf,
    source: String,
    manifest: PluginManifest,
    kind: PluginKind,
    status: PluginStatus,
}

impl PluginEntry {
    fn info(&self) -> PluginInfo {
        PluginInfo {
            id: self.id.as_str().to_string(),
            title: self
                .manifest
                .title
                .clone()
                .unwrap_or_else(|| self.id.as_str().to_string()),
            description: self.manifest.description.clone(),
            version: self.manifest.version.clone(),
            kind: self.kind,
            source: self.path.display().to_string(),
            status: self.status.clone(),
        }
    }
}

pub(crate) struct PluginRegistry {
    entries: Vec<PluginEntry>,
}

impl PluginRegistry {
    pub(crate) fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    pub(crate) fn load_path(
        &mut self,
        lua: &Lua,
        state: &Arc<Mutex<RuntimeState>>,
        path: PathBuf,
        kind: PluginKind,
    ) -> Result<PluginId, String> {
        let source =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let manifest = PluginManifest::parse(&source);
        let id = self.allocate_id(manifest.id.clone(), &path);

        let mut entry = PluginEntry {
            id: id.clone(),
            path: path.clone(),
            source,
            manifest,
            kind,
            status: PluginStatus::Inactive,
        };

        match exec_with_owner(lua, state, &id, &entry.path, &entry.source) {
            Ok(()) => entry.status = PluginStatus::Active,
            Err(error) => {
                entry.status = PluginStatus::Error(error.clone());
                self.entries.push(entry);
                return Err(error);
            }
        }
        self.entries.push(entry);
        Ok(id)
    }

    pub(crate) fn activate(
        &mut self,
        lua: &Lua,
        state: &Arc<Mutex<RuntimeState>>,
        id: &PluginId,
    ) -> Result<(), String> {
        let Some(idx) = self.position(id) else {
            return Err(format!("unknown plugin: {id}"));
        };
        if matches!(self.entries[idx].status, PluginStatus::Active) {
            return Ok(());
        }
        let path = self.entries[idx].path.clone();
        let source = self.entries[idx].source.clone();
        match exec_with_owner(lua, state, id, &path, &source) {
            Ok(()) => {
                self.entries[idx].status = PluginStatus::Active;
                Ok(())
            }
            Err(error) => {
                self.entries[idx].status = PluginStatus::Error(error.clone());
                Err(error)
            }
        }
    }

    pub(crate) fn deactivate(
        &mut self,
        state: &Arc<Mutex<RuntimeState>>,
        id: &PluginId,
    ) -> Result<Vec<String>, String> {
        let Some(idx) = self.position(id) else {
            return Err(format!("unknown plugin: {id}"));
        };
        if matches!(self.entries[idx].status, PluginStatus::Inactive) {
            return Ok(Vec::new());
        }
        let dropped = {
            let mut state = state
                .lock()
                .map_err(|e| format!("runtime state lock poisoned: {e}"))?;
            state.drop_owned_by(id)
        };
        self.entries[idx].status = PluginStatus::Inactive;
        Ok(dropped)
    }

    pub(crate) fn reload(
        &mut self,
        lua: &Lua,
        state: &Arc<Mutex<RuntimeState>>,
        id: &PluginId,
    ) -> Result<Vec<String>, String> {
        let Some(idx) = self.position(id) else {
            return Err(format!("unknown plugin: {id}"));
        };
        let dropped = {
            let mut state = state
                .lock()
                .map_err(|e| format!("runtime state lock poisoned: {e}"))?;
            state.drop_owned_by(id)
        };
        self.entries[idx].status = PluginStatus::Inactive;

        let path = self.entries[idx].path.clone();
        let reread =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        self.entries[idx].manifest = PluginManifest::parse(&reread);
        self.entries[idx].source = reread.clone();

        match exec_with_owner(lua, state, id, &path, &reread) {
            Ok(()) => {
                self.entries[idx].status = PluginStatus::Active;
                Ok(dropped)
            }
            Err(error) => {
                self.entries[idx].status = PluginStatus::Error(error.clone());
                Err(error)
            }
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<PluginInfo> {
        self.entries.iter().map(PluginEntry::info).collect()
    }

    pub(crate) fn status(&self, id: &PluginId) -> Option<PluginStatus> {
        self.position(id)
            .map(|idx| self.entries[idx].status.clone())
    }

    fn position(&self, id: &PluginId) -> Option<usize> {
        self.entries.iter().position(|e| &e.id == id)
    }

    fn allocate_id(&self, manifest_id: Option<String>, path: &Path) -> PluginId {
        let candidate = manifest_id
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| derive_id_from_path(path));
        if !self.id_taken(&candidate) {
            return PluginId::new(candidate);
        }
        for suffix in 2..=u32::MAX {
            let attempt = format!("{candidate}#{suffix}");
            if !self.id_taken(&attempt) {
                return PluginId::new(attempt);
            }
        }
        PluginId::new(candidate)
    }

    fn id_taken(&self, id: &str) -> bool {
        self.entries.iter().any(|e| e.id.as_str() == id)
    }
}

fn exec_with_owner(
    lua: &Lua,
    state: &Arc<Mutex<RuntimeState>>,
    owner: &PluginId,
    path: &Path,
    source: &str,
) -> Result<(), String> {
    {
        let mut state = state
            .lock()
            .map_err(|e| format!("runtime state lock poisoned: {e}"))?;
        state.current_owner = Some(owner.clone());
    }
    let result = lua
        .load(source)
        .set_name(path.display().to_string())
        .exec()
        .map_err(|e| format!("{e}"));
    if let Ok(mut state) = state.lock() {
        state.current_owner = None;
    }
    result
}

fn derive_id_from_path(path: &Path) -> String {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("plugin")
        .to_string();
    if stem.is_empty() {
        return "plugin".to_string();
    }
    sanitize_id(&stem)
}

fn sanitize_id(input: &str) -> String {
    let cleaned: String = input
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() {
        "plugin".to_string()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_id_handles_dots_and_extension() {
        let path = Path::new("/tmp/raider/plugins/judge.daemon.lua");
        assert_eq!(derive_id_from_path(path), "judge.daemon");
    }

    #[test]
    fn allocate_id_uniqueifies_on_collision() {
        let mut registry = PluginRegistry::new();
        registry.entries.push(PluginEntry {
            id: PluginId::new("judge"),
            path: PathBuf::from("/tmp/judge.lua"),
            source: String::new(),
            manifest: PluginManifest::default(),
            kind: PluginKind::Configured,
            status: PluginStatus::Active,
        });
        let next = registry.allocate_id(None, Path::new("/somewhere/else/judge.lua"));
        assert_eq!(next.as_str(), "judge#2");
    }
}
