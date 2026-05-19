use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::ui::theme::Theme;

use super::centered_box;

pub(crate) fn render_plugin_alert_dialog(
    f: &mut Frame,
    theme: &Theme,
    screen: Rect,
    title: &str,
    message: &str,
) {
    let body_lines = message.lines().count().max(1) as u16;
    let height = body_lines + 4;
    let (_rect, inner) = centered_box(f, screen, theme, title, (40, 74), height);

    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(inner);
    f.render_widget(
        Paragraph::new(message.to_string())
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(theme.text).bg(theme.background_menu)),
        layout[0],
    );
    f.render_widget(
        Paragraph::new("enter/esc dismiss")
            .alignment(Alignment::Center)
            .style(
                Style::default()
                    .fg(theme.text_muted)
                    .bg(theme.background_menu),
            ),
        layout[1],
    );
}
