use ratatui::prelude::*;

use crate::ui::path::truncate_path_right;
use crate::ui::theme::Theme;

/// Regression context (user-reported BUG10): an earlier rev used left-truncation
pub(crate) fn file_change_line(
    theme: &Theme,
    panel_bg: Color,
    entry: &crate::sidebar::FileChange,
    width: usize,
) -> Line<'static> {
    let muted = Style::default().fg(theme.text_muted).bg(panel_bg);
    let added_style = Style::default().fg(theme.diff_added).bg(panel_bg);
    let removed_style = Style::default().fg(theme.diff_removed).bg(panel_bg);

    let mut stats = String::new();
    if entry.additions > 0 {
        stats.push_str(&format!("+{}", entry.additions));
    }
    if entry.deletions > 0 {
        if !stats.is_empty() {
            stats.push(' ');
        }
        stats.push_str(&format!("-{}", entry.deletions));
    }
    let stats_width = stats.chars().count();

    let separator = if stats_width > 0 { 1 } else { 0 };
    let max_path_width = width.saturating_sub(stats_width + separator);

    let path_display = truncate_path_right(&entry.file, max_path_width);
    let path_width = path_display.chars().count();

    let pad = width
        .saturating_sub(path_width)
        .saturating_sub(stats_width)
        .max(separator);

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(path_display, muted));
    if stats_width > 0 {
        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(panel_bg)));
        if entry.additions > 0 {
            spans.push(Span::styled(format!("+{}", entry.additions), added_style));
        }
        if entry.deletions > 0 {
            if entry.additions > 0 {
                spans.push(Span::styled(" ", Style::default().bg(panel_bg)));
            }
            spans.push(Span::styled(format!("-{}", entry.deletions), removed_style));
        }
    }
    Line::from(spans)
}
