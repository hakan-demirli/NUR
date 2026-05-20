use crate::action::{PluginInfo, PluginStatus, Toast, ToastVariant};
use crate::dialog::{Dialog, DialogAction, DialogOption, DialogPayload, PluginInstallScope};
use crate::event::Event;

use super::App;

impl App {
    pub(crate) fn open_plugin_manager(&mut self) {
        let plugins = self.dialogs.plugins.clone();
        let options = plugin_manager_options(&plugins);
        let initial_value = plugins
            .first()
            .filter(|info| info.status != PluginStatus::Inactive)
            .map(|info| info.id.clone())
            .unwrap_or_else(|| {
                plugins
                    .first()
                    .map(|info| info.id.clone())
                    .unwrap_or_default()
            });
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::PluginManager {
                current: v.to_string(),
            });
        let actions = vec![
            DialogAction {
                label: "toggle".to_string(),
                key_hint: "space".to_string(),
            },
            DialogAction {
                label: "reload".to_string(),
                key_hint: "ctrl+r".to_string(),
            },
            DialogAction {
                label: "install".to_string(),
                key_hint: "shift+i".to_string(),
            },
        ];
        self.dialogs.dialog = Some(
            Dialog::new(
                "Plugins",
                DialogPayload::PluginManager {
                    current: initial_value,
                },
                options,
                parser,
            )
            .with_actions(actions),
        );
    }

    pub(crate) fn open_plugin_install_prompt(&mut self) {
        let parser: Box<dyn Fn(&str) -> DialogPayload + Send + Sync> =
            Box::new(|v: &str| DialogPayload::PluginInstall {
                path: v.to_string(),
                scope: PluginInstallScope::Global,
            });
        self.dialogs.dialog = Some(Dialog::prompt(
            "Install plugin",
            DialogPayload::PluginInstall {
                path: String::new(),
                scope: PluginInstallScope::Global,
            },
            parser,
        ));
    }

    pub(crate) fn toggle_plugin_install_scope(&mut self) {
        let Some(dialog) = self.dialogs.dialog.as_mut() else {
            return;
        };
        let (next_path, next_scope) = match &dialog.payload {
            DialogPayload::PluginInstall { path, scope } => {
                let next_scope = match scope {
                    PluginInstallScope::Global => PluginInstallScope::Local,
                    PluginInstallScope::Local => PluginInstallScope::Global,
                };
                (path.clone(), next_scope)
            }
            _ => return,
        };
        dialog.payload = DialogPayload::PluginInstall {
            path: next_path.clone(),
            scope: next_scope,
        };
        dialog.current_value = next_path;
    }

    pub(crate) fn refresh_plugin_manager_if_open(&mut self) {
        let Some(dialog) = self.dialogs.dialog.as_mut() else {
            return;
        };
        if !matches!(dialog.payload, DialogPayload::PluginManager { .. }) {
            return;
        }
        let options = plugin_manager_options(&self.dialogs.plugins);
        dialog.replace_options(options);
    }

    pub(crate) fn confirm_plugin_manager(&mut self, current: String) {
        if current.is_empty() {
            return;
        }
        if !self.dialogs.plugins.iter().any(|p| p.id == current) {
            self.dialogs.show_toast(Toast::new(
                format!("Unknown plugin: {current}"),
                ToastVariant::Error,
            ));
            return;
        }
        let dialog_was_consumed = self.dialogs.dialog.is_none();
        self.runtime.push(Event::TogglePlugin(current));
        if dialog_was_consumed {
            self.open_plugin_manager();
        }
    }

    pub(crate) fn confirm_plugin_install(&mut self, path: String, scope: PluginInstallScope) {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.dialogs.show_toast(Toast::new(
                "Plugin path cannot be empty",
                ToastVariant::Error,
            ));
            return;
        }
        self.runtime.push(Event::InstallPluginPath {
            path: trimmed.to_string(),
            scope,
        });
    }

    pub(crate) fn toggle_plugin(&mut self, id: String) {
        self.runtime.push(Event::TogglePlugin(id));
    }

    pub(crate) fn reload_plugin(&mut self, id: String) {
        self.runtime.push(Event::ReloadPlugin(id));
    }

    pub(crate) fn add_plugin_path(&mut self, path: String) {
        let trimmed = path.trim();
        if trimmed.is_empty() {
            self.dialogs.show_toast(Toast::new(
                "Plugin path cannot be empty",
                ToastVariant::Error,
            ));
            return;
        }
        self.runtime.push(Event::InstallPluginPath {
            path: trimmed.to_string(),
            scope: PluginInstallScope::Global,
        });
    }
}

fn plugin_manager_options(plugins: &[PluginInfo]) -> Vec<DialogOption> {
    if plugins.is_empty() {
        return vec![DialogOption {
            title: "No plugins loaded".to_string(),
            value: String::new(),
            description: Some(
                "Add a .lua file under ~/.config/raider/plugins/ or press shift+i to install."
                    .to_string(),
            ),
            category: None,
            disabled: true,
            is_header: false,
        }];
    }

    let mut options = Vec::new();
    let mut emitted_active = false;
    let mut emitted_inactive = false;
    let mut emitted_error = false;

    for status in [
        PluginStatus::Active,
        PluginStatus::Inactive,
        PluginStatus::Error(String::new()),
    ] {
        for plugin in plugins {
            if !same_status_bucket(&plugin.status, &status) {
                continue;
            }
            match &status {
                PluginStatus::Active if !emitted_active => {
                    options.push(DialogOption::header("Active"));
                    emitted_active = true;
                }
                PluginStatus::Inactive if !emitted_inactive => {
                    options.push(DialogOption::header("Inactive"));
                    emitted_inactive = true;
                }
                PluginStatus::Error(_) if !emitted_error => {
                    options.push(DialogOption::header("Errors"));
                    emitted_error = true;
                }
                _ => {}
            }
            options.push(plugin_to_option(plugin));
        }
    }
    options
}

fn same_status_bucket(a: &PluginStatus, b: &PluginStatus) -> bool {
    matches!(
        (a, b),
        (PluginStatus::Active, PluginStatus::Active)
            | (PluginStatus::Inactive, PluginStatus::Inactive)
            | (PluginStatus::Error(_), PluginStatus::Error(_))
    )
}

fn plugin_to_option(plugin: &PluginInfo) -> DialogOption {
    let title = match (&plugin.version, &plugin.title) {
        (Some(version), title) => format!("{title}  v{version}"),
        (None, title) => title.clone(),
    };
    let mut description_parts: Vec<String> = Vec::new();
    if let Some(desc) = plugin.description.as_ref().filter(|s| !s.trim().is_empty()) {
        description_parts.push(desc.clone());
    }
    description_parts.push(format!("source: {}", plugin.source));
    if let PluginStatus::Error(reason) = &plugin.status {
        description_parts.push(format!("error: {reason}"));
    }
    DialogOption {
        title,
        value: plugin.id.clone(),
        description: Some(description_parts.join("  ·  ")),
        category: Some(plugin.kind.label().to_string()),
        disabled: false,
        is_header: false,
    }
}
