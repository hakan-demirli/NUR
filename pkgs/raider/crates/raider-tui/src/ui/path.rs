//! NOTE: `crate::path_format` exists separately and is used by other modules.
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub(crate) fn normalize_path(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy().to_string();
        if !home.is_empty()
            && path.starts_with(&home)
            && (path.len() == home.len() || path[home.len()..].starts_with('/'))
        {
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
    if path.width() <= width {
        return path.to_string();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in path.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > width {
            break;
        }
        out.push(ch);
        used += w;
    }
    out
}

pub(crate) fn truncate_text_right_ellipsis(s: &str, width: usize) -> String {
    if s.width() <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let ellipsis = '…';
    let ellipsis_w = ellipsis.width().unwrap_or(1);
    if width <= ellipsis_w {
        return ellipsis.to_string();
    }
    let budget = width - ellipsis_w;
    let mut out = String::new();
    let mut used = 0usize;
    for ch in s.chars() {
        let w = ch.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push(ellipsis);
    out
}

pub(crate) fn truncate_to_width(s: &str, max_cols: usize) -> String {
    truncate_text_right_ellipsis(s, max_cols)
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
