//! NOTE: `crate::path_format` exists separately and is used by other modules.
pub(crate) fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().to_string();
        if !home.is_empty() && path.starts_with(&home) {
            let rest = &path[home.len()..];
            if rest.is_empty() {
                return "~".to_string();
            }
            if rest.starts_with('/') {
                return format!("~{rest}");
            }
            return format!("~/{rest}");
        }
    }
    path.to_string()
}

pub(crate) fn truncate_path_right(path: &str, width: usize) -> String {
    let chars: Vec<char> = path.chars().collect();
    if chars.len() <= width {
        return path.to_string();
    }
    chars.iter().take(width).collect()
}
