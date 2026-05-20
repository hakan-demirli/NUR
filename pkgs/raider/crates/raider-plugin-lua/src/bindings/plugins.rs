//! Exposes lifecycle controls to plugin authors. Calls produced from Lua
use std::path::PathBuf;

use mlua::{Lua, Table, Value};
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::{Action, ViewAction};

use crate::marshal::value_to_string;

pub(crate) fn install(
    lua: &Lua,
    api: &Table,
    action_tx: UnboundedSender<Action>,
) -> mlua::Result<()> {
    let plugins = lua.create_table()?;

    {
        let action_tx = action_tx.clone();
        plugins.set(
            "open",
            lua.create_function(move |_, ()| {
                let _ = action_tx.send(Action::View(ViewAction::OpenPluginManager));
                Ok(())
            })?,
        )?;
    }

    {
        let action_tx = action_tx.clone();
        plugins.set(
            "install_prompt",
            lua.create_function(move |_, ()| {
                let _ = action_tx.send(Action::View(ViewAction::OpenPluginInstallPrompt));
                Ok(())
            })?,
        )?;
    }

    {
        let action_tx = action_tx.clone();
        plugins.set(
            "activate",
            lua.create_function(move |_, id: Value| {
                let id = value_to_string(&id)?;
                if id.is_empty() {
                    return Err(mlua::Error::external("api.plugins.activate requires an id"));
                }
                let _ = action_tx.send(Action::View(ViewAction::TogglePlugin(id)));
                Ok(())
            })?,
        )?;
    }

    {
        let action_tx = action_tx.clone();
        plugins.set(
            "deactivate",
            lua.create_function(move |_, id: Value| {
                let id = value_to_string(&id)?;
                if id.is_empty() {
                    return Err(mlua::Error::external(
                        "api.plugins.deactivate requires an id",
                    ));
                }
                let _ = action_tx.send(Action::View(ViewAction::TogglePlugin(id)));
                Ok(())
            })?,
        )?;
    }

    {
        let action_tx = action_tx.clone();
        plugins.set(
            "reload",
            lua.create_function(move |_, id: Value| {
                let id = value_to_string(&id)?;
                if id.is_empty() {
                    return Err(mlua::Error::external("api.plugins.reload requires an id"));
                }
                let _ = action_tx.send(Action::View(ViewAction::ReloadPlugin(id)));
                Ok(())
            })?,
        )?;
    }

    {
        plugins.set(
            "add",
            lua.create_function(move |_, path: Value| {
                let path = value_to_string(&path)?;
                if path.is_empty() {
                    return Err(mlua::Error::external("api.plugins.add requires a path"));
                }
                let path = PathBuf::from(path);
                let _ = action_tx.send(Action::View(ViewAction::AddPluginPath(
                    path.to_string_lossy().into_owned(),
                )));
                Ok(())
            })?,
        )?;
    }

    api.set("plugins", plugins)?;
    Ok(())
}
