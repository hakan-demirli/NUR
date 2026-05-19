use ratatui::prelude::*;

use super::theme::Theme;

#[allow(dead_code)]
pub(crate) fn bar_gap<'a>(fg: Color, bg: Color) -> (Span<'a>, Span<'a>) {
    (
        Span::styled("┃", Style::default().fg(fg).bg(bg)),
        Span::styled("  ", Style::default().bg(bg)),
    )
}

pub(crate) fn bar_gap1<'a>(fg: Color, bg: Color) -> (Span<'a>, Span<'a>) {
    (
        Span::styled("┃", Style::default().fg(fg).bg(bg)),
        Span::styled(" ", Style::default().bg(bg)),
    )
}

#[allow(dead_code)]
pub(crate) fn hint_row<'a>(
    items: &[(&str, &str)],
    theme: &Theme,
    bg: Color,
    separator: &str,
) -> Vec<Span<'a>> {
    let key_style = Style::default()
        .fg(theme.text)
        .bg(bg)
        .add_modifier(Modifier::BOLD);
    let label_style = Style::default().fg(theme.text_muted).bg(bg);
    let sep_style = Style::default().bg(bg);
    let mut out: Vec<Span<'a>> = Vec::with_capacity(items.len() * 3);
    for (i, (key, label)) in items.iter().enumerate() {
        if i > 0 {
            out.push(Span::styled(separator.to_string(), sep_style));
        }
        out.push(Span::styled(format!("{key} "), key_style));
        out.push(Span::styled(label.to_string(), label_style));
    }
    out
}
