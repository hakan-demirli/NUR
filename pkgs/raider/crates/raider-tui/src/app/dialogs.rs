use std::collections::HashSet;

use crate::action::{PluginCommand, Toast};
use crate::completion::{CompletionManager, SlashEntry};
use crate::dialog::Dialog;

use super::builtin::Command;

pub struct DialogState {
    pub dialog: Option<Dialog>,
    pub commands: Vec<Command>,
    pub plugin_commands: Vec<PluginCommand>,
    pub toast: Option<Toast>,
}

impl DialogState {
    pub fn new(commands: Vec<Command>) -> Self {
        Self {
            dialog: None,
            commands,
            plugin_commands: Vec::new(),
            toast: None,
        }
    }

    pub fn show_toast(&mut self, toast: Toast) {
        self.toast = Some(toast);
    }

    pub fn tick_toast(&mut self) {
        let Some(toast) = &mut self.toast else {
            return;
        };
        toast.ttl_ticks = toast.ttl_ticks.saturating_sub(1);
        if toast.ttl_ticks == 0 {
            self.toast = None;
        }
    }

    pub fn register_plugin_commands(&mut self, commands: Vec<PluginCommand>) -> bool {
        let mut changed = false;
        for command in commands.into_iter().filter(|c| !c.name.is_empty()) {
            if let Some(existing) = self
                .plugin_commands
                .iter_mut()
                .find(|existing| existing.name == command.name)
            {
                if *existing != command {
                    *existing = command;
                    changed = true;
                }
            } else {
                self.plugin_commands.push(command);
                changed = true;
            }
        }
        changed
    }

    pub fn plugin_command_for_slash(&self, slash: &str) -> Option<&PluginCommand> {
        self.plugin_commands.iter().find(|command| {
            command.slash_name.as_deref().map(normalize_plugin_slash) == Some(slash)
                || command
                    .slash_aliases
                    .iter()
                    .map(String::as_str)
                    .map(normalize_plugin_slash)
                    .any(|alias| alias == slash)
        })
    }

    pub fn rebuild_slash_completion(&self, completion: &mut CompletionManager) {
        let mut seen: HashSet<String> = HashSet::new();
        let mut entries = Vec::new();

        for command in &self.commands {
            let Some(slash_name) = command.slash_name.as_deref() else {
                continue;
            };
            if slash_name.is_empty() {
                continue;
            }
            let slash = format!("/{slash_name}");
            if seen.insert(slash.clone()) {
                entries.push(SlashEntry::new(slash, command.title.clone()));
            }
        }

        for command in &self.plugin_commands {
            let description = command
                .description
                .as_ref()
                .filter(|description| !description.is_empty())
                .cloned()
                .unwrap_or_else(|| command.title.clone());
            let slashes = command
                .slash_name
                .iter()
                .map(String::as_str)
                .chain(command.slash_aliases.iter().map(String::as_str));
            for slash_name in slashes {
                let slash_name = normalize_plugin_slash(slash_name);
                if slash_name.is_empty() {
                    continue;
                }
                let slash = format!("/{slash_name}");
                if seen.insert(slash.clone()) {
                    entries.push(SlashEntry::new(slash, description.clone()));
                }
            }
        }

        completion.set_commands(entries);
    }

    pub fn dialog_kind(&self) -> Option<crate::dialog::DialogKind> {
        self.dialog.as_ref().map(|d| d.kind())
    }
}

impl Default for DialogState {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

pub fn normalize_plugin_slash(slash: &str) -> &str {
    slash.trim().trim_start_matches('/')
}
