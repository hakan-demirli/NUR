use ratatui::prelude::*;

use crate::ui::theme::Theme;

pub(crate) fn lsp_entry_line(
    theme: &Theme,
    panel_bg: Color,
    entry: &crate::sidebar::LspEntry,
) -> Line<'static> {
    let dot = if entry.status == "connected" {
        theme.success
    } else {
        theme.error
    };
    let muted = Style::default().fg(theme.text_muted).bg(panel_bg);
    Line::from(vec![
        Span::styled("• ", Style::default().fg(dot).bg(panel_bg)),
        Span::styled(format!("{} {}", entry.id, entry.root), muted),
    ])
}
