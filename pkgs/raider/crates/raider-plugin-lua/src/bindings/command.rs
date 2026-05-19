use std::sync::{Arc, Mutex};

use mlua::{Lua, Table, Value};
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::{Action, HostAction, PluginCommand};

use crate::marshal::{optional_function, optional_string, optional_table, string_vec_field};
use crate::runtime::{lock_error, CommandCallback, CommandCallbackMode, RuntimeState};

pub(crate) fn install(
    lua: &Lua,
    api: &Table,
    state: Arc<Mutex<RuntimeState>>,
    action_tx: UnboundedSender<Action>,
) -> mlua::Result<()> {
    let command = lua.create_table()?;
    let register_state = Arc::clone(&state);
    let register_action_tx = action_tx.clone();
    command.set(
        "register",
        lua.create_function(move |lua, spec: Value| {
            let mut commands = Vec::new();
            let specs = command_specs_from_value(spec)?;
            {
                let mut state = register_state.lock().map_err(lock_error)?;
                for spec in specs {
                    let (command, callback) = plugin_command_from_table(&spec)?;
                    if let Some(callback) = callback {
                        state.commands.insert(command.name.clone(), callback);
                    }
                    commands.push(command);
                }
            }
            if !commands.is_empty() {
                let _ = register_action_tx
                    .send(Action::Host(HostAction::RegisterPluginCommands(commands)));
            }
            lua.create_function(|_, ()| Ok(()))
        })?,
    )?;
    api.set("command", command)?;
    Ok(())
}

pub(crate) fn command_specs_from_value(spec: Value) -> mlua::Result<Vec<Table>> {
    match spec {
        Value::Function(callback) => command_specs_from_value(callback.call::<Value>(())?),
        Value::Table(table) => {
            if table.contains_key("value")? || table.contains_key("name")? {
                return Ok(vec![table]);
            }
            let mut out = Vec::new();
            for command in table.sequence_values::<Table>() {
                out.push(command?);
            }
            Ok(out)
        }
        _ => Err(mlua::Error::external(
            "api.command.register expects a command table or callback",
        )),
    }
}

pub(crate) fn plugin_command_from_table(
    spec: &Table,
) -> mlua::Result<(PluginCommand, Option<CommandCallback>)> {
    let name = optional_string(spec, "name")?
        .or(optional_string(spec, "value")?)
        .ok_or_else(|| mlua::Error::external("plugin command requires name"))?;
    let title = optional_string(spec, "title")?.unwrap_or_else(|| name.clone());
    let description = optional_string(spec, "description")?;
    let category = optional_string(spec, "category")?;
    let mut slash_name =
        optional_string(spec, "slash_name")?.or(optional_string(spec, "slashName")?);
    let mut slash_aliases = string_vec_field(spec, "slash_aliases")?;
    if slash_aliases.is_empty() {
        slash_aliases = string_vec_field(spec, "slashAliases")?;
    }

    if let Some(slash) = optional_table(spec, "slash")? {
        if slash_name.is_none() {
            slash_name = optional_string(&slash, "name")?;
        }
        if slash_aliases.is_empty() {
            slash_aliases = string_vec_field(&slash, "aliases")?;
        }
    }

    let callback = optional_function(spec, "onSelect")?
        .map(|function| CommandCallback {
            function,
            mode: CommandCallbackMode::Dialog,
        })
        .or(
            optional_function(spec, "run")?.map(|function| CommandCallback {
                function,
                mode: CommandCallbackMode::Context,
            }),
        )
        .or(
            optional_function(spec, "callback")?.map(|function| CommandCallback {
                function,
                mode: CommandCallbackMode::Context,
            }),
        );

    Ok((
        PluginCommand {
            name,
            title,
            description,
            category,
            slash_name,
            slash_aliases,
        },
        callback,
    ))
}
