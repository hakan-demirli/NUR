use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::{App, PermissionStage};
use crate::prompt::PermissionPrompt;

use super::frame;

pub(crate) fn required_permission_height(app: &App, prompt: &PermissionPrompt, _width: u16) -> u16 {
    let footer = 1u16;
    let padding = 2u16;
    let body = match app.permissions.permission_stage {
        PermissionStage::Permission => {
            let mut n: u16 = 2;
            if !prompt.view.detail.is_empty() {
                n = n.saturating_add(1 + prompt.view.detail.len() as u16);
            }
            n
        }
        PermissionStage::Always => {
            let pat_rows = if prompt.always.len() == 1 && prompt.always[0] == "*" {
                0
            } else {
                prompt.always.len() as u16
            };
            1u16.saturating_add(1).saturating_add(pat_rows)
        }
        PermissionStage::Reject => 4,
    };
    body.saturating_add(padding).saturating_add(footer)
}

pub(crate) fn render_permission_modal(
    f: &mut Frame,
    app: &App,
    area: Rect,
    prompt: &PermissionPrompt,
) {
    let theme = &app.theme.theme;
    let bar_color = match app.permissions.permission_stage {
        PermissionStage::Reject => theme.error,
        _ => theme.warning,
    };
    let inner = frame(f, area, theme, bar_color);
    let bg = theme.background_panel;

    let mut lines: Vec<Line> = Vec::new();
    match app.permissions.permission_stage {
        PermissionStage::Permission => {
            lines.push(Line::from(vec![
                Span::styled("△  ", Style::default().fg(theme.warning).bg(bg)),
                Span::styled(
                    "Permission required",
                    Style::default().fg(theme.text).bg(bg),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  ", Style::default().bg(bg)),
                Span::styled(
                    format!("{} ", prompt.view.icon),
                    Style::default().fg(theme.text_muted).bg(bg),
                ),
                Span::styled(
                    prompt.view.title.clone(),
                    Style::default().fg(theme.text).bg(bg),
                ),
            ]));
            if !prompt.view.detail.is_empty() {
                lines.push(Line::from(""));
                for d in &prompt.view.detail {
                    let style = if d == "Patterns" {
                        Style::default().fg(theme.text_muted).bg(bg)
                    } else {
                        Style::default().fg(theme.text).bg(bg)
                    };
                    lines.push(Line::from(vec![
                        Span::styled(" ", Style::default().bg(bg)),
                        Span::styled(d.clone(), style),
                    ]));
                }
            }
        }
        PermissionStage::Always => {
            lines.push(Line::from(vec![
                Span::styled("△  ", Style::default().fg(theme.warning).bg(bg)),
                Span::styled("Always allow", Style::default().fg(theme.text).bg(bg)),
            ]));
            if prompt.always.len() == 1 && prompt.always[0] == "*" {
                lines.push(Line::from(vec![Span::styled(
                    format!(
                        " This will allow {} until OpenCode is restarted.",
                        prompt.permission
                    ),
                    Style::default().fg(theme.text_muted).bg(bg),
                )]));
            } else {
                lines.push(Line::from(vec![Span::styled(
                    " This will allow the following patterns until OpenCode is restarted",
                    Style::default().fg(theme.text_muted).bg(bg),
                )]));
                for p in &prompt.always {
                    lines.push(Line::from(vec![Span::styled(
                        format!("  - {p}"),
                        Style::default().fg(theme.text).bg(bg),
                    )]));
                }
            }
        }
        PermissionStage::Reject => {
            lines.push(Line::from(vec![
                Span::styled("△  ", Style::default().fg(theme.error).bg(bg)),
                Span::styled("Reject permission", Style::default().fg(theme.text).bg(bg)),
            ]));
            lines.push(Line::from(vec![Span::styled(
                " Tell OpenCode what to do differently",
                Style::default().fg(theme.text_muted).bg(bg),
            )]));
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(" > ", Style::default().fg(theme.accent).bg(bg)),
                Span::styled(
                    app.permissions.permission_reject_buffer.clone(),
                    Style::default().fg(theme.text).bg(bg),
                ),
                Span::styled("_", Style::default().fg(theme.primary).bg(bg)),
            ]));
        }
    }

    let body_height = inner.height.saturating_sub(1).max(1);
    let body_area = Rect {
        x: inner.x,
        y: inner.y,
        width: inner.width,
        height: body_height,
    };
    f.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(bg))
            .wrap(Wrap { trim: false }),
        body_area,
    );

    let footer_y = inner.y + body_height;
    let footer_area = Rect {
        x: inner.x,
        y: footer_y,
        width: inner.width,
        height: 1,
    };
    let footer_spans: Vec<Span> = match app.permissions.permission_stage {
        PermissionStage::Permission => vec![
            Span::styled("1 ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("Allow once  ", Style::default().fg(theme.text_muted).bg(bg)),
            Span::styled("2 ", Style::default().fg(theme.text).bg(bg)),
            Span::styled(
                "Allow always  ",
                Style::default().fg(theme.text_muted).bg(bg),
            ),
            Span::styled("3 ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("Reject  ", Style::default().fg(theme.text_muted).bg(bg)),
            Span::styled("·  enter ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("confirm  ", Style::default().fg(theme.text_muted).bg(bg)),
            Span::styled("esc ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("reject", Style::default().fg(theme.text_muted).bg(bg)),
        ],
        PermissionStage::Always => vec![
            Span::styled("enter ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("confirm  ", Style::default().fg(theme.text_muted).bg(bg)),
            Span::styled("esc ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("cancel", Style::default().fg(theme.text_muted).bg(bg)),
        ],
        PermissionStage::Reject => vec![
            Span::styled("enter ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("confirm  ", Style::default().fg(theme.text_muted).bg(bg)),
            Span::styled("esc ", Style::default().fg(theme.text).bg(bg)),
            Span::styled("cancel", Style::default().fg(theme.text_muted).bg(bg)),
        ],
    };
    f.render_widget(
        Paragraph::new(Line::from(footer_spans)).style(Style::default().bg(bg)),
        footer_area,
    );
}
