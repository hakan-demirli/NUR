use mlua::{Function, Table, Value};

pub(crate) fn optional_table(table: &Table, key: &str) -> mlua::Result<Option<Table>> {
    match table.get::<Value>(key)? {
        Value::Nil => Ok(None),
        Value::Table(table) => Ok(Some(table)),
        _ => Ok(None),
    }
}

pub(crate) fn optional_function(table: &Table, key: &str) -> mlua::Result<Option<Function>> {
    match table.get::<Value>(key)? {
        Value::Nil => Ok(None),
        Value::Function(function) => Ok(Some(function)),
        _ => Ok(None),
    }
}

pub(crate) fn optional_string(table: &Table, key: &str) -> mlua::Result<Option<String>> {
    match table.get::<Value>(key)? {
        Value::Nil => Ok(None),
        value => value_to_string(&value).map(Some),
    }
}

pub(crate) fn optional_bool(table: &Table, key: &str) -> mlua::Result<Option<bool>> {
    match table.get::<Value>(key)? {
        Value::Nil => Ok(None),
        Value::Boolean(value) => Ok(Some(value)),
        _ => Ok(None),
    }
}

pub(crate) fn optional_u64(table: &Table, key: &str) -> mlua::Result<Option<u64>> {
    match table.get::<Value>(key)? {
        Value::Nil => Ok(None),
        Value::Integer(value) if value >= 0 => Ok(Some(value as u64)),
        Value::Number(value) if value >= 0.0 => Ok(Some(value as u64)),
        Value::String(value) => Ok(value.to_string_lossy().parse::<u64>().ok()),
        _ => Ok(None),
    }
}

pub(crate) fn string_vec_field(table: &Table, key: &str) -> mlua::Result<Vec<String>> {
    match table.get::<Value>(key)? {
        Value::Nil => Ok(Vec::new()),
        value @ (Value::String(_) | Value::Integer(_) | Value::Number(_) | Value::Boolean(_)) => {
            Ok(vec![value_to_string(&value)?])
        }
        Value::Table(values) => {
            let mut out = Vec::new();
            for value in values.sequence_values::<Value>() {
                out.push(value_to_string(&value?)?);
            }
            Ok(out)
        }
        _ => Ok(Vec::new()),
    }
}

pub(crate) fn values_to_string(values: &[Value]) -> mlua::Result<String> {
    values
        .iter()
        .map(value_to_string)
        .collect::<mlua::Result<Vec<_>>>()
        .map(|parts| parts.join(" "))
}

pub(crate) fn value_to_string(value: &Value) -> mlua::Result<String> {
    match value {
        Value::Nil => Ok(String::new()),
        Value::Boolean(value) => Ok(value.to_string()),
        Value::Integer(value) => Ok(value.to_string()),
        Value::Number(value) => Ok(value.to_string()),
        Value::String(value) => Ok(value.to_string_lossy()),
        Value::Table(_) => Ok("table".to_string()),
        Value::Function(_) => Ok("function".to_string()),
        Value::Thread(_) => Ok("thread".to_string()),
        Value::UserData(_) => Ok("userdata".to_string()),
        Value::LightUserData(_) => Ok("userdata".to_string()),
        Value::Error(error) => Ok(error.to_string()),
        Value::Other(_) => Ok("value".to_string()),
    }
}
