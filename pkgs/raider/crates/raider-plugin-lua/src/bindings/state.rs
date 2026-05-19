use mlua::{Lua, Table};

pub(crate) fn install(
    lua: &Lua,
    api: &Table,
    workspace_directory: Option<String>,
) -> mlua::Result<()> {
    let state_table = lua.create_table()?;
    let path = lua.create_table()?;
    path.set("state", "")?;
    path.set("config", "")?;
    path.set("worktree", "")?;
    path.set("directory", workspace_directory.unwrap_or_default())?;
    state_table.set("path", path)?;
    api.set("state", state_table)?;
    Ok(())
}
