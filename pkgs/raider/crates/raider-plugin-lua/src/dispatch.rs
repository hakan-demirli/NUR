use std::sync::{Arc, Mutex};

use mlua::{Lua, Table, Value};
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::{Action, HostAction};

use crate::runtime::{lock_error, CommandCallbackMode, DialogCallbackMode, RuntimeState};
use crate::PluginEvent;

pub(crate) fn handle_event(
    lua: &Lua,
    state: Arc<Mutex<RuntimeState>>,
    action_tx: &UnboundedSender<Action>,
    event: PluginEvent,
) -> mlua::Result<()> {
    match event {
        PluginEvent::Command { name, args } => {
            let callback = {
                let state = state.lock().map_err(lock_error)?;
                state.commands.get(&name).cloned()
            };
            let Some(callback) = callback else {
                let _ = action_tx.send(Action::Host(HostAction::SystemMessage(format!(
                    "Plugin command has no handler: {name}"
                ))));
                return Ok(());
            };
            let ctx = lua.create_table()?;
            ctx.set("name", name)?;
            ctx.set("args", args.clone())?;
            ctx.set("input", args)?;
            match callback.mode {
                CommandCallbackMode::Context => callback.function.call::<()>(ctx)?,
                CommandCallbackMode::Dialog => {
                    let api: Table = lua.globals().get("api")?;
                    let ui: Table = api.get("ui")?;
                    let dialog: Table = ui.get("dialog")?;
                    callback.function.call::<()>(dialog)?;
                }
            }
        }
        PluginEvent::DialogSelected { callback_id, value } => {
            call_dialog_callback(lua, state, callback_id, Some(value))?;
        }
        PluginEvent::DialogDismissed { callback_id } => {
            call_dialog_callback(lua, state, callback_id, None)?;
        }
        PluginEvent::SessionChanged { session_id } => {
            set_route_current(lua, session_id)?;
        }
    }
    Ok(())
}

pub(crate) fn call_dialog_callback(
    lua: &Lua,
    state: Arc<Mutex<RuntimeState>>,
    callback_id: u64,
    value: Option<String>,
) -> mlua::Result<()> {
    let callback = {
        let mut state = state.lock().map_err(lock_error)?;
        state.dialog_callbacks.remove(&callback_id)
    };
    if let Some(callback) = callback {
        match value {
            Some(value) => match callback.mode {
                DialogCallbackMode::Value => callback.function.call::<()>(value)?,
                DialogCallbackMode::Option => {
                    if let Some(option) = callback.options.get(&value) {
                        callback.function.call::<()>(option.to_lua_table(lua)?)?;
                    } else {
                        callback.function.call::<()>(Value::Nil)?;
                    }
                }
            },
            None => callback.function.call::<()>(Value::Nil)?,
        }
    }
    Ok(())
}

pub(crate) fn set_route_current(lua: &Lua, session_id: Option<String>) -> mlua::Result<()> {
    let api: Table = lua.globals().get("api")?;
    let route: Table = api.get("route")?;
    route.set("current", route_current_table(lua, session_id)?)
}

pub(crate) fn route_current_table(lua: &Lua, session_id: Option<String>) -> mlua::Result<Table> {
    let current = lua.create_table()?;
    match session_id.filter(|id| !id.is_empty()) {
        Some(session_id) => {
            current.set("name", "session")?;
            let params = lua.create_table()?;
            params.set("sessionID", session_id)?;
            current.set("params", params)?;
        }
        None => {
            current.set("name", "home")?;
        }
    }
    Ok(current)
}
