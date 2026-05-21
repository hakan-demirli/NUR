use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::dialog::DialogOption;
use crate::session::{SessionEntry, SessionList, SessionStatus};

#[derive(Default, Debug, Serialize, Deserialize)]
struct PersistedSessionState {
    #[serde(default)]
    pinned: Vec<String>,
}

fn session_state_path() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("RAIDER_STATE_DIR") {
        let s = dir.to_string_lossy();
        if s.is_empty() {
            return None;
        }
        return Some(PathBuf::from(dir).join("session.json"));
    }
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        return Some(PathBuf::from(xdg).join("raider").join("session.json"));
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("raider")
            .join("session.json"),
    )
}

pub struct SessionState {
    pub sessions: SessionList,
    pub pinned_sessions: Vec<String>,
    pub session_delete_armed: Option<String>,
    pub statuses: HashMap<String, SessionStatus>,
}

impl SessionState {
    pub fn new() -> Self {
        Self {
            sessions: SessionList::default(),
            pinned_sessions: Vec::new(),
            session_delete_armed: None,
            statuses: HashMap::new(),
        }
    }

    pub fn set_sessions(&mut self, entries: Vec<SessionEntry>) {
        let busy: std::collections::HashSet<String> = self
            .sessions
            .entries
            .iter()
            .filter(|e| e.busy)
            .map(|e| e.id.clone())
            .collect();
        self.sessions.entries = entries;
        for e in &mut self.sessions.entries {
            if let Some(status) = self.statuses.get(&e.id) {
                e.status = status.clone();
                e.busy = status.is_working();
            } else if busy.contains(&e.id) {
                e.busy = true;
            }
        }
    }

    pub fn upsert_session(&mut self, entry: SessionEntry) {
        if let Some(existing) = self.sessions.entries.iter_mut().find(|e| e.id == entry.id) {
            let was_busy = existing.busy;
            let status = existing.status.clone();
            *existing = entry;
            existing.busy = was_busy;
            existing.status = status;
            return;
        }
        let mut entry = entry;
        if let Some(status) = self.statuses.get(&entry.id) {
            entry.status = status.clone();
            entry.busy = status.is_working();
        }
        self.sessions.entries.insert(0, entry);
    }

    pub fn remove_session(&mut self, id: &str) {
        self.sessions.entries.retain(|e| e.id != id);
        self.pinned_sessions.retain(|s| s != id);
    }

    pub fn set_session_busy(&mut self, id: &str, busy: bool) {
        if let Some(entry) = self.sessions.entries.iter_mut().find(|e| e.id == id) {
            entry.busy = busy;
            if !busy {
                entry.status = SessionStatus::Idle;
                self.statuses.remove(id);
            } else if !entry.status.is_retry() {
                entry.status = SessionStatus::Busy;
                self.statuses.insert(id.to_string(), SessionStatus::Busy);
            }
        } else if !busy {
            self.statuses.remove(id);
        } else if !matches!(self.statuses.get(id), Some(SessionStatus::Retry { .. })) {
            self.statuses.insert(id.to_string(), SessionStatus::Busy);
        }
    }

    pub fn set_session_status(&mut self, id: &str, status: SessionStatus) {
        if matches!(status, SessionStatus::Idle) {
            self.statuses.remove(id);
        } else {
            self.statuses.insert(id.to_string(), status.clone());
        }
        if let Some(entry) = self.sessions.entries.iter_mut().find(|e| e.id == id) {
            entry.busy = status.is_working();
            entry.status = status;
        }
    }

    pub fn current_status(&self) -> Option<&SessionStatus> {
        let current = self.sessions.current.as_deref()?;
        self.statuses.get(current).or_else(|| {
            self.sessions
                .get(current)
                .and_then(|e| (!matches!(e.status, SessionStatus::Idle)).then_some(&e.status))
        })
    }

    pub fn current_busy(&self) -> bool {
        let Some(current) = self.sessions.current.as_deref() else {
            return false;
        };
        self.statuses
            .get(current)
            .map(SessionStatus::is_working)
            .unwrap_or_else(|| self.sessions.get(current).map(|e| e.busy).unwrap_or(false))
    }

    pub fn set_current(&mut self, current: Option<String>) {
        self.sessions.current = current;
    }

    pub fn toggle_pin(&mut self, id: String) {
        if let Some(pos) = self.pinned_sessions.iter().position(|s| s == &id) {
            self.pinned_sessions.remove(pos);
        } else {
            self.pinned_sessions.push(id);
        }
    }

    pub fn has_session(&self, id: &str) -> bool {
        self.sessions.entries.iter().any(|s| s.id == id)
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    pub fn build_picker_options(&self) -> Vec<DialogOption> {
        let mut options: Vec<DialogOption> = Vec::new();
        let pinned_existing: Vec<&str> = self
            .pinned_sessions
            .iter()
            .filter(|id| self.sessions.entries.iter().any(|s| &s.id == *id))
            .map(|s| s.as_str())
            .collect();

        if !pinned_existing.is_empty() {
            options.push(DialogOption::header("Pinned"));
            for id in &pinned_existing {
                if let Some(s) = self.sessions.entries.iter().find(|s| &s.id == id) {
                    let mut opt = DialogOption::new(s.display_title(), s.id.clone());
                    opt.footer = Some(s.updated_label.clone());
                    options.push(opt);
                }
            }
        }

        let mut emitted_others = false;
        for s in &self.sessions.entries {
            if pinned_existing.contains(&s.id.as_str()) {
                continue;
            }
            if !emitted_others && !pinned_existing.is_empty() {
                options.push(DialogOption::header("Sessions"));
                emitted_others = true;
            }
            let mut opt = DialogOption::new(s.display_title(), s.id.clone());
            opt.footer = Some(s.updated_label.clone());
            options.push(opt);
        }
        options
    }

    pub fn save_to_disk(&self) -> std::io::Result<()> {
        let Some(path) = session_state_path() else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let payload = PersistedSessionState {
            pinned: self.pinned_sessions.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&payload).map_err(std::io::Error::other)?;
        std::fs::write(&path, bytes)
    }

    pub fn load_from_disk(&mut self) {
        let Some(path) = session_state_path() else {
            return;
        };
        let Ok(bytes) = std::fs::read(&path) else {
            return;
        };
        let Ok(state) = serde_json::from_slice::<PersistedSessionState>(&bytes) else {
            return;
        };
        self.pinned_sessions = state.pinned;
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::new()
    }
}
