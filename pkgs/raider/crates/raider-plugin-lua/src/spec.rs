use mlua::{Table, Value};

use raider_tui::{Toast, ToastVariant};

use crate::marshal::{optional_string, optional_u64, value_to_string, values_to_string};

pub(crate) fn dialog_spec_from_value(spec: Value) -> mlua::Result<Table> {
    match spec {
        Value::Function(render) => match render.call::<Value>(())? {
            Value::Table(table) => Ok(table),
            _ => Err(mlua::Error::external(
                "api.ui.dialog.replace render callback must return a dialog table",
            )),
        },
        Value::Table(table) => Ok(table),
        _ => Err(mlua::Error::external(
            "api.ui.dialog.replace expects a dialog table or render callback",
        )),
    }
}

pub(crate) fn alert_args(args: &[Value]) -> mlua::Result<(String, String)> {
    if let Some(Value::Table(spec)) = args.first() {
        let title = optional_string(spec, "title")?.unwrap_or_else(|| "Alert".to_string());
        let message = optional_string(spec, "message")?
            .or(optional_string(spec, "body")?)
            .unwrap_or_default();
        return Ok((title, message));
    }
    let title = args
        .first()
        .map(value_to_string)
        .transpose()?
        .unwrap_or_else(|| "Alert".to_string());
    let rest = if args.len() > 1 { &args[1..] } else { &[] };
    let message = values_to_string(rest)?;
    Ok((title, message))
}

pub(crate) fn toast_from_args(args: &[Value]) -> mlua::Result<Option<Toast>> {
    if let Some(Value::Table(spec)) = args.first() {
        let message = optional_string(spec, "message")?.unwrap_or_default();
        if message.is_empty() {
            return Ok(None);
        }
        let variant = optional_string(spec, "variant")?
            .as_deref()
            .map(toast_variant)
            .unwrap_or(ToastVariant::Info);
        let mut toast = Toast::new(message, variant);
        if let Some(title) = optional_string(spec, "title")? {
            if !title.is_empty() {
                toast = toast.with_title(title);
            }
        }
        if let Some(duration) = optional_u64(spec, "duration")? {
            let ticks = ((duration.saturating_add(49)) / 50).clamp(1, u16::MAX as u64) as u16;
            toast.ttl_ticks = ticks;
        }
        return Ok(Some(toast));
    }

    let message = values_to_string(args)?;
    if message.is_empty() {
        Ok(None)
    } else {
        Ok(Some(Toast::new(message, ToastVariant::Info)))
    }
}

pub(crate) fn toast_variant(value: &str) -> ToastVariant {
    match value {
        "success" => ToastVariant::Success,
        "warning" => ToastVariant::Warning,
        "error" => ToastVariant::Error,
        _ => ToastVariant::Info,
    }
}

pub(crate) fn headers_from_init(init: &Table) -> mlua::Result<Vec<(String, String)>> {
    let Value::Table(headers) = init.get::<Value>("headers")? else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for pair in headers.pairs::<Value, Value>() {
        let (key, value) = pair?;
        out.push((value_to_string(&key)?, value_to_string(&value)?));
    }
    Ok(out)
}
