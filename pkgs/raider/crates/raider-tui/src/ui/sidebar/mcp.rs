use ratatui::prelude::*;

use crate::ui::theme::Theme;

pub(crate) fn mcp_entry_line(
    theme: &Theme,
    panel_bg: Color,
    entry: &crate::sidebar::McpEntry,
) -> Line<'static> {
    let dot_color = match entry.status.as_str() {
        "connected" => theme.success,
        "failed" | "needs_client_registration" => theme.error,
        "needs_auth" => theme.warning,
        "disabled" => theme.text_muted,
        _ => theme.text_muted,
    };
    let label = match entry.status.as_str() {
        "connected" => "Connected".to_string(),
        "failed" => {
            if entry.error.is_empty() {
                "Failed".to_string()
            } else {
                entry.error.clone()
            }
        }
        "disabled" => "Disabled".to_string(),
        "needs_auth" => "Needs auth".to_string(),
        "needs_client_registration" => "Needs client ID".to_string(),
        other => other.to_string(),
    };
    let muted = Style::default().fg(theme.text_muted).bg(panel_bg);
    let text = Style::default().fg(theme.text).bg(panel_bg);
    Line::from(vec![
        Span::styled("• ", Style::default().fg(dot_color).bg(panel_bg)),
        Span::styled(format!("{} ", entry.name), text),
        Span::styled(label, muted),
    ])
}
