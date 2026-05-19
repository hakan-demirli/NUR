use mlua::{Lua, Table, Value};

use crate::marshal::value_to_string;

pub(crate) fn install(lua: &Lua, api: &Table) -> mlua::Result<()> {
    let json = lua.create_table()?;
    json.set(
        "stringify",
        lua.create_function(|_, value: Value| {
            let json = lua_to_json(value)?;
            serde_json::to_string(&json).map_err(mlua::Error::external)
        })?,
    )?;
    json.set(
        "parse",
        lua.create_function(|lua, raw: String| {
            let json: serde_json::Value =
                serde_json::from_str(&raw).map_err(mlua::Error::external)?;
            json_to_lua(lua, &json)
        })?,
    )?;
    api.set("json", json.clone())?;
    lua.globals().set("JSON", json)?;
    Ok(())
}

pub(crate) fn lua_to_json(value: Value) -> mlua::Result<serde_json::Value> {
    match value {
        Value::Nil => Ok(serde_json::Value::Null),
        Value::Boolean(value) => Ok(serde_json::Value::Bool(value)),
        Value::Integer(value) => Ok(serde_json::Value::Number(value.into())),
        Value::Number(value) => serde_json::Number::from_f64(value)
            .map(serde_json::Value::Number)
            .ok_or_else(|| mlua::Error::external("cannot encode non-finite number as JSON")),
        Value::String(value) => Ok(serde_json::Value::String(value.to_string_lossy())),
        Value::Table(table) => lua_table_to_json(table),
        _ => Err(mlua::Error::external("unsupported Lua value for JSON")),
    }
}

pub(crate) fn lua_table_to_json(table: Table) -> mlua::Result<serde_json::Value> {
    let mut entries = Vec::new();
    let mut array_like = true;
    let mut max_index = 0usize;
    for pair in table.pairs::<Value, Value>() {
        let (key, value) = pair?;
        if let Value::Integer(index) = key {
            if index > 0 {
                max_index = max_index.max(index as usize);
                entries.push((Some(index as usize), String::new(), value));
                continue;
            }
        }
        array_like = false;
        entries.push((None, value_to_string(&key)?, value));
    }

    if array_like && entries.len() == max_index {
        let mut values = vec![serde_json::Value::Null; max_index];
        for (index, _, value) in entries {
            if let Some(index) = index {
                values[index - 1] = lua_to_json(value)?;
            }
        }
        return Ok(serde_json::Value::Array(values));
    }

    let mut object = serde_json::Map::new();
    for (_, key, value) in entries {
        if !key.is_empty() {
            object.insert(key, lua_to_json(value)?);
        }
    }
    Ok(serde_json::Value::Object(object))
}

pub(crate) fn json_to_lua(lua: &Lua, value: &serde_json::Value) -> mlua::Result<Value> {
    match value {
        serde_json::Value::Null => Ok(Value::Nil),
        serde_json::Value::Bool(value) => Ok(Value::Boolean(*value)),
        serde_json::Value::Number(value) => {
            if let Some(value) = value.as_i64() {
                Ok(Value::Integer(value))
            } else if let Some(value) = value.as_f64() {
                Ok(Value::Number(value))
            } else {
                Ok(Value::Nil)
            }
        }
        serde_json::Value::String(value) => Ok(Value::String(lua.create_string(value)?)),
        serde_json::Value::Array(values) => {
            let table = lua.create_table()?;
            for (idx, value) in values.iter().enumerate() {
                table.set(idx + 1, json_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
        serde_json::Value::Object(values) => {
            let table = lua.create_table()?;
            for (key, value) in values {
                table.set(key.as_str(), json_to_lua(lua, value)?)?;
            }
            Ok(Value::Table(table))
        }
    }
}
