use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};

use crate::action::ToastVariant;
use crate::app::App;

pub(crate) fn render_toast(f: &mut Frame, app: &App, screen: Rect) {
    let Some(toast) = &app.dialogs.toast else {
        return;
    };
    if screen.width < 8 || screen.height < 4 {
        return;
    }

    let max_width = screen.width.saturating_sub(6).clamp(10, 60);
    let inner_width = max_width.saturating_sub(6).max(1) as usize;
    let mut lines: Vec<Line> = Vec::new();
    if let Some(title) = &toast.title {
        lines.push(Line::from(Span::styled(
            title.clone(),
            Style::default()
                .fg(app.theme.theme.text)
                .bg(app.theme.theme.background_panel)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::default());
    }
    let wrap_opts = textwrap::Options::new(inner_width).break_words(false);
    for wrapped in textwrap::wrap(&toast.message, &wrap_opts) {
        lines.push(Line::from(Span::styled(
            wrapped.into_owned(),
            Style::default()
                .fg(app.theme.theme.text)
                .bg(app.theme.theme.background_panel),
        )));
    }
    if lines.is_empty() {
        return;
    }

    let content_rows = lines.len() as u16;
    let height = content_rows
        .saturating_add(2)
        .min(screen.height.saturating_sub(2));
    let width = max_width;
    let x = screen.x + screen.width.saturating_sub(width).saturating_sub(2);
    let y = screen.y + 2;
    let area = Rect::new(x, y, width, height);
    let border_color = match toast.variant {
        ToastVariant::Info => app.theme.theme.info,
        ToastVariant::Success => app.theme.theme.success,
        ToastVariant::Warning => app.theme.theme.warning,
        ToastVariant::Error => app.theme.theme.error,
    };
    let block = Block::default()
        .borders(Borders::LEFT | Borders::RIGHT)
        .padding(Padding::new(2, 2, 1, 1))
        .border_style(
            Style::default()
                .fg(border_color)
                .bg(app.theme.theme.background_panel),
        )
        .style(Style::default().bg(app.theme.theme.background_panel));
    f.render_widget(Clear, area);
    f.render_widget(
        Paragraph::new(lines)
            .block(block)
            .style(Style::default().bg(app.theme.theme.background_panel)),
        area,
    );
}
