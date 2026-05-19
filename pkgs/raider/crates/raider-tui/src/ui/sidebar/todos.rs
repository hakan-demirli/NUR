use ratatui::prelude::*;
use unicode_width::UnicodeWidthStr;

use crate::ui::theme::Theme;

pub(crate) fn todo_entry_lines(
    theme: &Theme,
    panel_bg: Color,
    entry: &crate::sidebar::TodoEntry,
    width: usize,
) -> Vec<Line<'static>> {
    let width = width.max(1);
    let (glyph, fg) = match entry.status.as_str() {
        "completed" => ("✓", theme.text_muted),
        "in_progress" => ("•", theme.warning),
        _ => (" ", theme.text_muted),
    };
    let style = Style::default().fg(fg).bg(panel_bg);
    let prefix = format!("[{glyph}] ");
    let prefix_width = UnicodeWidthStr::width(prefix.as_str()).min(width);
    let content_width = width.saturating_sub(prefix_width).max(1);
    let wrapped = wrap_words(&entry.content, content_width);
    let mut out = Vec::with_capacity(wrapped.len().max(1));
    for (idx, line) in wrapped.into_iter().enumerate() {
        let prefix_text = if idx == 0 {
            prefix.clone()
        } else {
            " ".repeat(prefix_width)
        };
        out.push(Line::from(vec![
            Span::styled(prefix_text, style),
            Span::styled(line, style),
        ]));
    }
    if out.is_empty() {
        out.push(Line::from(Span::styled(prefix, style)));
    }
    out
}

fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let options = textwrap::Options::new(width.max(1)).break_words(false);
    let wrapped = textwrap::wrap(text, options);
    if wrapped.is_empty() {
        vec![String::new()]
    } else {
        wrapped.into_iter().map(|s| s.into_owned()).collect()
    }
}
