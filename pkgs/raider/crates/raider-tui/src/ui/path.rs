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

pub(crate) fn truncate_text_right_ellipsis(s: &str, width: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".to_string();
    }
    let mut out: String = chars.iter().take(width - 1).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_text_right_ellipsis_shorter_than_width_is_unchanged() {
        assert_eq!(truncate_text_right_ellipsis("hello", 10), "hello");
        assert_eq!(truncate_text_right_ellipsis("hello", 5), "hello");
    }

    #[test]
    fn truncate_text_right_ellipsis_adds_ellipsis_when_too_long() {
        assert_eq!(truncate_text_right_ellipsis("hello world", 5), "hell…");
        assert_eq!(truncate_text_right_ellipsis("hello world", 8), "hello w…");
    }

    #[test]
    fn truncate_text_right_ellipsis_handles_tiny_widths() {
        assert_eq!(truncate_text_right_ellipsis("hello", 0), "");
        assert_eq!(truncate_text_right_ellipsis("hello", 1), "…");
    }
}
