pub(crate) fn tail_bytes(text: &str, max_bytes: usize) -> String {
    if text.len() <= max_bytes {
        return text.to_string();
    }
    let start = text.len() - max_bytes;
    let mut boundary = start;
    while boundary < text.len() && !text.is_char_boundary(boundary) {
        boundary += 1;
    }
    let mut out = String::with_capacity(max_bytes + 5);
    out.push_str("...\n");
    out.push_str(&text[boundary..]);
    out
}

pub(super) fn format_input_primitives(
    input: Option<&serde_json::Map<String, serde_json::Value>>,
    omit: &[&str],
) -> String {
    let Some(input) = input else {
        return String::new();
    };
    let mut parts: Vec<String> = Vec::new();
    for (k, v) in input {
        if omit.contains(&k.as_str()) {
            continue;
        }
        let formatted = match v {
            serde_json::Value::String(s) => Some(format!("{k}={s}")),
            serde_json::Value::Number(n) => Some(format!("{k}={n}")),
            serde_json::Value::Bool(b) => Some(format!("{k}={b}")),
            _ => None,
        };
        if let Some(f) = formatted {
            parts.push(f);
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("[{}]", parts.join(", "))
    }
}

pub(super) fn normalize_workspace_path(input: &str) -> String {
    if input.is_empty() {
        return String::new();
    }
    let Ok(cwd) = std::env::current_dir() else {
        return input.to_string();
    };
    let p = std::path::Path::new(input);
    if !p.is_absolute() {
        return input.to_string();
    }
    match p.strip_prefix(&cwd) {
        Ok(rel) => {
            let s = rel.display().to_string();
            if s.is_empty() {
                ".".to_string()
            } else {
                s
            }
        }
        Err(_) => input.to_string(),
    }
}

pub(super) fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(first) => first.to_uppercase().collect::<String>() + c.as_str(),
    }
}
