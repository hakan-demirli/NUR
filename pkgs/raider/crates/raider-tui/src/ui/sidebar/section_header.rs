use ratatui::prelude::*;

use crate::ui::theme::Theme;

pub(crate) fn section_header(
    title: &str,
    entries_len: usize,
    collapsed: bool,
    theme: &Theme,
    panel_bg: Color,
) -> Line<'static> {
    let header_style = Style::default()
        .fg(theme.text)
        .bg(panel_bg)
        .add_modifier(Modifier::BOLD);
    let body = Style::default().fg(theme.text).bg(panel_bg);

    let mut spans: Vec<Span<'static>> = Vec::new();
    if entries_len > 2 {
        let glyph = if collapsed { "▶" } else { "▼" };
        spans.push(Span::styled(format!("{glyph} "), body));
    }
    spans.push(Span::styled(title.to_string(), header_style));
    Line::from(spans)
}
