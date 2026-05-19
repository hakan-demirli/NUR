use mlua::{Lua, Table};
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::{Action, ViewAction};

use crate::dispatch::route_current_table;
use crate::marshal::optional_string;

pub(crate) fn install(
    lua: &Lua,
    api: &Table,
    action_tx: UnboundedSender<Action>,
    current_session: Option<String>,
) -> mlua::Result<()> {
    let route = lua.create_table()?;
    route.set("current", route_current_table(lua, current_session)?)?;
    let navigate_action_tx = action_tx;
    route.set(
        "navigate",
        lua.create_function(move |_, (name, params): (String, Option<Table>)| {
            if name == "session" {
                if let Some(params) = params {
                    if let Some(session_id) = optional_string(&params, "sessionID")?
                        .or(optional_string(&params, "session_id")?)
                    {
                        let _ = navigate_action_tx
                            .send(Action::View(ViewAction::PluginNavigateSession(session_id)));
                    }
                }
            }
            Ok(())
        })?,
    )?;
    api.set("route", route)?;
    Ok(())
}
