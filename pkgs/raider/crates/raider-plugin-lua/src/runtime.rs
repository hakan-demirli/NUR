use std::collections::HashMap;

use mlua::{Function, Lua, Table};

use raider_tui::PluginDialogOption;

use crate::PluginId;

#[derive(Default)]
pub(crate) struct RuntimeState {
    pub(crate) commands: HashMap<String, CommandCallback>,
    pub(crate) command_owners: HashMap<String, PluginId>,
    pub(crate) dialog_callbacks: HashMap<u64, DialogCallback>,
    pub(crate) dialog_callback_owners: HashMap<u64, PluginId>,
    pub(crate) next_callback_id: u64,
    pub(crate) current_owner: Option<PluginId>,
}

impl RuntimeState {
    pub(crate) fn drop_owned_by(&mut self, owner: &PluginId) -> Vec<String> {
        let mut dropped = Vec::new();
        let command_names: Vec<String> = self
            .command_owners
            .iter()
            .filter(|(_, id)| *id == owner)
            .map(|(name, _)| name.clone())
            .collect();
        for name in command_names {
            self.commands.remove(&name);
            self.command_owners.remove(&name);
            dropped.push(name);
        }
        let dialog_ids: Vec<u64> = self
            .dialog_callback_owners
            .iter()
            .filter(|(_, plugin)| *plugin == owner)
            .map(|(id, _)| *id)
            .collect();
        for id in dialog_ids {
            self.dialog_callbacks.remove(&id);
            self.dialog_callback_owners.remove(&id);
        }
        dropped
    }
}

#[derive(Clone)]
pub(crate) struct CommandCallback {
    pub(crate) function: Function,
    pub(crate) mode: CommandCallbackMode,
}

#[derive(Clone, Copy)]
pub(crate) enum CommandCallbackMode {
    Context,
    Dialog,
}

pub(crate) struct DialogCallback {
    pub(crate) function: Function,
    pub(crate) mode: DialogCallbackMode,
    pub(crate) options: HashMap<String, LuaDialogOption>,
}

#[derive(Clone, Copy)]
pub(crate) enum DialogCallbackMode {
    Value,
    Option,
}

#[derive(Clone, Debug)]
pub(crate) struct LuaDialogOption {
    pub(crate) title: String,
    pub(crate) value: String,
    pub(crate) description: Option<String>,
    pub(crate) category: Option<String>,
    pub(crate) disabled: bool,
}

impl LuaDialogOption {
    pub(crate) fn to_plugin_option(&self) -> PluginDialogOption {
        PluginDialogOption {
            title: self.title.clone(),
            value: self.value.clone(),
            description: self.description.clone(),
            category: self.category.clone(),
            disabled: self.disabled,
        }
    }

    pub(crate) fn to_lua_table(&self, lua: &Lua) -> mlua::Result<Table> {
        let table = lua.create_table()?;
        table.set("title", self.title.clone())?;
        table.set("value", self.value.clone())?;
        if let Some(description) = &self.description {
            table.set("description", description.clone())?;
        }
        if let Some(category) = &self.category {
            table.set("category", category.clone())?;
        }
        table.set("disabled", self.disabled)?;
        Ok(table)
    }
}

pub(crate) fn lock_error<T>(_: std::sync::PoisonError<T>) -> mlua::Error {
    mlua::Error::external("lua plugin runtime state lock poisoned")
}
