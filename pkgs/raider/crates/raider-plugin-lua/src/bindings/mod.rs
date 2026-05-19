use std::sync::{Arc, Mutex};
use std::time::Duration;

use mlua::Lua;
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::Action;

use crate::runtime::RuntimeState;

pub(crate) mod command;
pub(crate) mod http;
pub(crate) mod json;
pub(crate) mod process;
pub(crate) mod route;
pub(crate) mod state;
pub(crate) mod ui;

pub(crate) fn install_api(
    lua: &Lua,
    state: Arc<Mutex<RuntimeState>>,
    action_tx: UnboundedSender<Action>,
    workspace_directory: Option<String>,
    current_session: Option<String>,
) -> mlua::Result<()> {
    let api = lua.create_table()?;

    command::install(lua, &api, Arc::clone(&state), action_tx.clone())?;
    ui::install(lua, &api, Arc::clone(&state), action_tx.clone())?;
    route::install(lua, &api, action_tx.clone(), current_session)?;
    state::install(lua, &api, workspace_directory)?;
    json::install(lua, &api)?;
    http::install(lua, &api)?;
    process::install(lua, &api)?;

    api.set(
        "sleep",
        lua.create_function(|_, millis: u64| {
            std::thread::sleep(Duration::from_millis(millis));
            Ok(())
        })?,
    )?;

    lua.globals().set("api", api)?;
    Ok(())
}
