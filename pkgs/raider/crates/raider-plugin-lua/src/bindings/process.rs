use mlua::{Lua, Table, Value, Variadic};

use crate::marshal::{optional_string, optional_table, value_to_string};

pub(crate) fn install(lua: &Lua, api: &Table) -> mlua::Result<()> {
    let process = lua.create_table()?;
    process.set(
        "run",
        lua.create_function(|lua, args: Variadic<Value>| {
            let argv = argv_from_process_args(args.as_slice())?;
            if argv.is_empty() {
                return Err(mlua::Error::external("api.process.run requires argv"));
            }
            let output = std::process::Command::new(&argv[0])
                .args(&argv[1..])
                .output()
                .map_err(mlua::Error::external)?;
            let table = lua.create_table()?;
            table.set("code", output.status.code().unwrap_or(-1))?;
            table.set("success", output.status.success())?;
            table.set(
                "stdout",
                String::from_utf8_lossy(&output.stdout).to_string(),
            )?;
            table.set(
                "stderr",
                String::from_utf8_lossy(&output.stderr).to_string(),
            )?;
            Ok(table)
        })?,
    )?;
    api.set("process", process)?;
    Ok(())
}

fn argv_from_process_args(args: &[Value]) -> mlua::Result<Vec<String>> {
    match args.first() {
        Some(Value::Table(table)) => {
            let mut argv = Vec::new();
            for value in table.sequence_values::<Value>() {
                argv.push(value_to_string(&value?)?);
            }
            if !argv.is_empty() {
                return Ok(argv);
            }
            let command = optional_string(table, "command")?.or(optional_string(table, "cmd")?);
            let Some(command) = command else {
                return Ok(Vec::new());
            };
            argv.push(command);
            if let Some(arg_table) = optional_table(table, "args")? {
                for value in arg_table.sequence_values::<Value>() {
                    argv.push(value_to_string(&value?)?);
                }
            }
            Ok(argv)
        }
        Some(Value::String(command)) => {
            let mut argv = vec![command.to_string_lossy()];
            if let Some(Value::Table(args)) = args.get(1) {
                for value in args.sequence_values::<Value>() {
                    argv.push(value_to_string(&value?)?);
                }
            }
            Ok(argv)
        }
        _ => Ok(Vec::new()),
    }
}
