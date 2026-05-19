pub(crate) fn extract_provider(
    extra: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    if let Some(id) = extra.get("providerID").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    if let Some(model) = extra.get("model").and_then(|v| v.as_object()) {
        if let Some(id) = model.get("providerID").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
    }
    None
}

pub(crate) fn extract_agent(extra: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    extra
        .get("agent")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

pub(crate) fn extract_assistant_error(
    extra: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    let raw = extra.get("error")?;
    unwrap_error_message(raw)
}

pub(crate) fn unwrap_error_message(raw: &serde_json::Value) -> Option<String> {
    if raw.is_null() {
        return None;
    }
    if let Some(s) = raw.as_str() {
        let trimmed = s.trim();
        return if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        };
    }
    if let Some(obj) = raw.as_object() {
        if let Some(data) = obj.get("data").and_then(|v| v.as_object()) {
            if let Some(msg) = data.get("message").and_then(|v| v.as_str()) {
                let trimmed = msg.trim();
                if !trimmed.is_empty() {
                    return Some(trimmed.to_string());
                }
            }
        }
        if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
            let trimmed = name.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    let s = serde_json::to_string(raw).ok()?;
    let trimmed = s.trim();
    if matches!(trimmed, "null" | "{}" | "[]" | "") {
        None
    } else {
        Some(s)
    }
}

pub(crate) fn is_message_aborted_error(raw: &serde_json::Value) -> bool {
    raw.as_object()
        .and_then(|o| o.get("name"))
        .and_then(|v| v.as_str())
        == Some("MessageAbortedError")
}

pub(crate) fn extract_model_display(
    extra: &serde_json::Map<String, serde_json::Value>,
) -> Option<String> {
    if let Some(id) = extra.get("modelID").and_then(|v| v.as_str()) {
        return Some(id.to_string());
    }
    if let Some(model) = extra.get("model").and_then(|v| v.as_object()) {
        if let Some(id) = model.get("modelID").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
        if let Some(id) = model.get("id").and_then(|v| v.as_str()) {
            return Some(id.to_string());
        }
    }
    None
}
