use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear};

use crate::app::App;
use crate::ui::theme::Theme;

pub(crate) mod dialog;
pub(crate) mod permission;
pub(crate) mod plugin_alert;
pub(crate) mod question;

pub(crate) use dialog::render_dialog;
pub(crate) use permission::{render_permission_modal, required_permission_height};
pub(crate) use question::{render_question_modal, required_question_height};

pub(crate) fn frame(f: &mut Frame, area: Rect, theme: &Theme, bar_color: Color) -> Rect {
    f.render_widget(Clear, area);
    let bg = theme.background_panel;
    f.render_widget(Block::default().style(Style::default().bg(bg)), area);

    let bar_style = Style::default().fg(bar_color).bg(bg);
    let buf = f.buffer_mut();
    for row in area.y..area.y + area.height {
        buf[(area.x, row)].set_symbol("┃").set_style(bar_style);
    }

    Rect {
        x: area.x + 2,
        y: area.y + 1,
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(2),
    }
}

pub(crate) fn centered_box(
    f: &mut Frame,
    screen: Rect,
    theme: &Theme,
    title: &str,
    width_clamp: (u16, u16),
    height: u16,
) -> (Rect, Rect) {
    use ratatui::widgets::Block;
    let w = screen
        .width
        .saturating_sub(8)
        .clamp(width_clamp.0, width_clamp.1);
    let h = height.clamp(5, screen.height.saturating_sub(4));
    let x = screen.x + (screen.width.saturating_sub(w)) / 2;
    let y = screen.y + screen.height.saturating_sub(h) / 3;
    let rect = Rect::new(x, y, w, h);

    f.render_widget(Clear, rect);

    let block = Block::bordered()
        .title(format!(" {} ", title))
        .title_alignment(Alignment::Center)
        .style(
            Style::default()
                .bg(theme.background_menu)
                .fg(theme.border_active),
        );
    let inner = block.inner(rect);
    f.render_widget(block, rect);
    (rect, inner)
}

pub(crate) fn render_prompt_modals(f: &mut Frame, app: &App, area: Rect) {
    if let Some(p) = app.permissions.permission_active.as_ref() {
        render_permission_modal(f, app, area, p);
        return;
    }
    if let Some(q) = app.questions.question_active.as_ref() {
        render_question_modal(f, app, area, q);
    }
}

pub(crate) fn required_modal_height(app: &App, width: u16) -> u16 {
    if let Some(p) = app.permissions.permission_active.as_ref() {
        return required_permission_height(app, p, width);
    }
    if let Some(q) = app.questions.question_active.as_ref() {
        return required_question_height(app, q, width);
    }
    0
}
