use std::sync::{Arc, Mutex};

use mlua::{Lua, Table, Value, Variadic};
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::{Action, HostAction, PluginDialogOption, ViewAction};

use crate::marshal::{
    optional_bool, optional_function, optional_string, optional_table, value_to_string,
};
use crate::runtime::{
    lock_error, DialogCallback, DialogCallbackMode, LuaDialogOption, RuntimeState,
};
use crate::spec::{alert_args, dialog_spec_from_value, toast_from_args};

pub(crate) fn install(
    lua: &Lua,
    api: &Table,
    state: Arc<Mutex<RuntimeState>>,
    action_tx: UnboundedSender<Action>,
) -> mlua::Result<()> {
    let ui = lua.create_table()?;
    let dialog = lua.create_table()?;

    let replace_state = Arc::clone(&state);
    let replace_action_tx = action_tx.clone();
    dialog.set(
        "replace",
        lua.create_function(move |_, spec: Value| {
            let spec = dialog_spec_from_value(spec)?;
            open_dialog_from_table(&spec, Arc::clone(&replace_state), &replace_action_tx)
        })?,
    )?;
    let clear_action_tx = action_tx.clone();
    dialog.set(
        "clear",
        lua.create_function(move |_, ()| {
            let _ = clear_action_tx.send(Action::Host(HostAction::ClearPluginDialog));
            Ok(())
        })?,
    )?;
    ui.set("dialog", dialog)?;

    ui.set(
        "DialogAlert",
        lua.create_function(|_, spec: Table| {
            spec.set("kind", "alert")?;
            Ok(spec)
        })?,
    )?;
    ui.set(
        "DialogSelect",
        lua.create_function(|_, spec: Table| {
            spec.set("kind", "select")?;
            Ok(spec)
        })?,
    )?;

    let toast_action_tx = action_tx.clone();
    ui.set(
        "toast",
        lua.create_function(move |_, args: Variadic<Value>| {
            if let Some(toast) = toast_from_args(args.as_slice())? {
                let _ = toast_action_tx.send(Action::View(ViewAction::ShowToast(toast)));
            }
            Ok(())
        })?,
    )?;

    let alert_action_tx = action_tx.clone();
    ui.set(
        "alert",
        lua.create_function(move |_, args: Variadic<Value>| {
            let (title, message) = alert_args(args.as_slice())?;
            let _ =
                alert_action_tx.send(Action::Host(HostAction::OpenPluginAlert { title, message }));
            Ok(())
        })?,
    )?;

    let select_state = Arc::clone(&state);
    let select_action_tx = action_tx.clone();
    ui.set(
        "select",
        lua.create_function(move |_, spec: Table| {
            open_select_from_table(&spec, Arc::clone(&select_state), &select_action_tx)
        })?,
    )?;

    api.set("ui", ui)?;
    Ok(())
}

pub(crate) fn open_dialog_from_table(
    spec: &Table,
    state: Arc<Mutex<RuntimeState>>,
    action_tx: &UnboundedSender<Action>,
) -> mlua::Result<()> {
    match optional_string(spec, "kind")?
        .or(optional_string(spec, "type")?)
        .as_deref()
    {
        Some("alert") => {
            let title = optional_string(spec, "title")?.unwrap_or_else(|| "Alert".to_string());
            let message = optional_string(spec, "message")?
                .or(optional_string(spec, "body")?)
                .unwrap_or_default();
            let _ = action_tx.send(Action::Host(HostAction::OpenPluginAlert { title, message }));
            Ok(())
        }
        Some("select") => open_select_from_table(spec, state, action_tx),
        Some(other) => Err(mlua::Error::external(format!(
            "unsupported plugin dialog kind: {other}"
        ))),
        None => Err(mlua::Error::external("plugin dialog requires kind")),
    }
}

pub(crate) fn open_select_from_table(
    spec: &Table,
    state: Arc<Mutex<RuntimeState>>,
    action_tx: &UnboundedSender<Action>,
) -> mlua::Result<()> {
    let title = optional_string(spec, "title")?.unwrap_or_else(|| "Select".to_string());
    let placeholder = optional_string(spec, "placeholder")?;
    let (options, lua_options) = optional_table(spec, "options")?
        .map(|options| plugin_dialog_options(&options))
        .transpose()?
        .unwrap_or_default();
    let callback = optional_function(spec, "onSelect")?
        .map(|function| (function, DialogCallbackMode::Option))
        .or(optional_function(spec, "on_select")?
            .map(|function| (function, DialogCallbackMode::Value)))
        .or(optional_function(spec, "callback")?
            .map(|function| (function, DialogCallbackMode::Value)));
    let callback_id = if let Some((function, mode)) = callback {
        let mut state = state.lock().map_err(lock_error)?;
        let callback_id = state.next_callback_id;
        state.next_callback_id = state.next_callback_id.saturating_add(1).max(1);
        state.dialog_callbacks.insert(
            callback_id,
            DialogCallback {
                function,
                mode,
                options: lua_options
                    .into_iter()
                    .map(|option| (option.value.clone(), option))
                    .collect(),
            },
        );
        callback_id
    } else {
        0
    };

    let _ = action_tx.send(Action::Host(HostAction::OpenPluginSelect {
        callback_id,
        title,
        placeholder,
        options,
    }));
    Ok(())
}

fn plugin_dialog_options(
    options: &Table,
) -> mlua::Result<(Vec<PluginDialogOption>, Vec<LuaDialogOption>)> {
    let mut out = Vec::new();
    let mut lua_out = Vec::new();
    for option in options.sequence_values::<Value>() {
        let lua_option = match option? {
            Value::Table(option) => {
                let value = optional_string(&option, "value")?
                    .or(optional_string(&option, "id")?)
                    .unwrap_or_default();
                let title = optional_string(&option, "title")?.unwrap_or_else(|| value.clone());
                let description = optional_string(&option, "description")?;
                let category = optional_string(&option, "category")?;
                let disabled = optional_bool(&option, "disabled")?.unwrap_or(false);
                LuaDialogOption {
                    title,
                    value,
                    description,
                    category,
                    disabled,
                }
            }
            value => {
                let value = value_to_string(&value)?;
                LuaDialogOption {
                    title: value.clone(),
                    value,
                    description: None,
                    category: None,
                    disabled: false,
                }
            }
        };
        out.push(lua_option.to_plugin_option());
        lua_out.push(lua_option);
    }
    Ok((out, lua_out))
}
