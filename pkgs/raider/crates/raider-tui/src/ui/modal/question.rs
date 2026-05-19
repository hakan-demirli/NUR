use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::App;
use crate::prompt::QuestionPrompt;

use super::frame;

fn wrapped_rows(text: &str, width: u16) -> u16 {
    let width = width.max(1) as usize;
    if text.is_empty() {
        return 1;
    }
    let rows = textwrap::wrap(text, width).len();
    (rows as u16).max(1)
}

pub(crate) fn required_question_height(_app: &App, prompt: &QuestionPrompt, width: u16) -> u16 {
    let footer = 1u16;
    let padding = 2u16;
    let inner_width = width.saturating_sub(4).max(1);
    if prompt.on_confirm() {
        let tabs = if prompt.is_single() { 0 } else { 2 };
        let body = tabs + 1 + (prompt.questions.len() as u16);
        return body.saturating_add(padding).saturating_add(footer);
    }
    let info = match prompt.current() {
        Some(i) => i,
        None => return 0,
    };
    let tabs = if prompt.is_single() { 0 } else { 2 };
    let suffix = if info.multiple {
        " (select all that apply)"
    } else {
        ""
    };
    let question_text = format!("{}{}", info.question, suffix);
    let question_rows = wrapped_rows(&question_text, inner_width).saturating_add(1);
    let mut option_rows: u16 = 0;
    for (i, opt) in info.options.iter().enumerate() {
        let label_render = if info.multiple {
            format!(
                " {}. [{}] {}",
                i + 1,
                if false { "✓" } else { " " },
                opt.label
            )
        } else {
            format!(" {}. {}", i + 1, opt.label)
        };
        option_rows = option_rows.saturating_add(wrapped_rows(&label_render, inner_width));
        if !opt.description.is_empty() {
            let desc_render = format!("    {}", opt.description);
            option_rows = option_rows.saturating_add(wrapped_rows(&desc_render, inner_width));
        }
    }
    if info.custom_allowed {
        let custom_label = if info.multiple {
            format!(" {}. [ ] Type your own answer", info.options.len() + 1)
        } else {
            format!(" {}. Type your own answer", info.options.len() + 1)
        };
        option_rows = option_rows.saturating_add(wrapped_rows(&custom_label, inner_width));
        if prompt.editing {
            let edit_render = format!("    > {}_", prompt.edit_buffer);
            option_rows = option_rows.saturating_add(wrapped_rows(&edit_render, inner_width));
        } else {
            let stored = prompt.custom.get(prompt.tab).cloned().unwrap_or_default();
            if !stored.is_empty() {
                let stored_render = format!("    {}", stored);
                option_rows = option_rows.saturating_add(wrapped_rows(&stored_render, inner_width));
            }
        }
    }
    let body = tabs + question_rows + option_rows;
    body.saturating_add(padding).saturating_add(footer)
}

pub(crate) fn render_question_modal(f: &mut Frame, app: &App, area: Rect, prompt: &QuestionPrompt) {
    let theme = &app.theme.theme;
    let bar_color = theme.accent;
    let inner = frame(f, area, theme, bar_color);
    let bg = theme.background_panel;

    let mut lines: Vec<Line> = Vec::new();

    if !prompt.is_single() {
        let mut spans: Vec<Span> = Vec::new();
        for (i, q) in prompt.questions.iter().enumerate() {
            let is_active = i == prompt.tab;
            let is_answered = prompt
                .answers
                .get(i)
                .map(|a| !a.is_empty())
                .unwrap_or(false);
            let style = if is_active {
                Style::default().fg(theme.background).bg(theme.accent)
            } else if is_answered {
                Style::default().fg(theme.text).bg(bg)
            } else {
                Style::default().fg(theme.text_muted).bg(bg)
            };
            spans.push(Span::styled(format!(" {} ", q.header), style));
            spans.push(Span::styled(" ", Style::default().bg(bg)));
        }
        let confirm_style = if prompt.on_confirm() {
            Style::default().fg(theme.background).bg(theme.accent)
        } else {
            Style::default().fg(theme.text_muted).bg(bg)
        };
        spans.push(Span::styled(" Confirm ", confirm_style));
        lines.push(Line::from(spans));
        lines.push(Line::from(""));
    }

    if prompt.on_confirm() {
        lines.push(Line::from(Span::styled(
            "Review",
            Style::default().fg(theme.text).bg(bg),
        )));
        for (i, q) in prompt.questions.iter().enumerate() {
            let value = prompt
                .answers
                .get(i)
                .map(|a| a.join(", "))
                .unwrap_or_default();
            let value_style = if value.is_empty() {
                Style::default().fg(theme.error).bg(bg)
            } else {
                Style::default().fg(theme.text).bg(bg)
            };
            let rendered = if value.is_empty() {
                "(not answered)".to_string()
            } else {
                value
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!(" {}: ", q.header),
                    Style::default().fg(theme.text_muted).bg(bg),
                ),
                Span::styled(rendered, value_style),
            ]));
        }
    } else if let Some(info) = prompt.current() {
        let suffix = if info.multiple {
            " (select all that apply)".to_string()
        } else {
            String::new()
        };
        lines.push(Line::from(vec![Span::styled(
            format!("{}{}", info.question, suffix),
            Style::default().fg(theme.text).bg(bg),
        )]));
        lines.push(Line::from(""));
        for (i, opt) in info.options.iter().enumerate() {
            let active = i == prompt.selected;
            let picked = prompt.option_picked(&opt.label);
            let row_bg = if active { theme.background_element } else { bg };
            let prefix_style = if active {
                Style::default().fg(theme.secondary).bg(row_bg)
            } else {
                Style::default().fg(theme.text_muted).bg(row_bg)
            };
            let label_style = if active {
                Style::default().fg(theme.secondary).bg(row_bg)
            } else if picked {
                Style::default().fg(theme.success).bg(row_bg)
            } else {
                Style::default().fg(theme.text).bg(row_bg)
            };
            let label = if info.multiple {
                format!("[{}] {}", if picked { "✓" } else { " " }, opt.label)
            } else {
                opt.label.clone()
            };
            let mut spans: Vec<Span> = vec![
                Span::styled(format!(" {}. ", i + 1), prefix_style),
                Span::styled(label, label_style),
            ];
            if !info.multiple && picked {
                spans.push(Span::styled(
                    " ✓",
                    Style::default().fg(theme.success).bg(row_bg),
                ));
            }
            lines.push(Line::from(spans));
            if !opt.description.is_empty() {
                let desc_style = Style::default().fg(theme.text_muted).bg(row_bg);
                lines.push(Line::from(vec![
                    Span::styled("    ", Style::default().bg(row_bg)),
                    Span::styled(opt.description.clone(), desc_style),
                ]));
            }
        }
        if info.custom_allowed {
            let total = info.options.len();
            let active = prompt.selected == total;
            let picked = prompt.custom_picked();
            let row_bg = if active { theme.background_element } else { bg };
            let prefix_style = if active {
                Style::default().fg(theme.secondary).bg(row_bg)
            } else {
                Style::default().fg(theme.text_muted).bg(row_bg)
            };
            let label_style = if active {
                Style::default().fg(theme.secondary).bg(row_bg)
            } else if picked {
                Style::default().fg(theme.success).bg(row_bg)
            } else {
                Style::default().fg(theme.text).bg(row_bg)
            };
            let label = if info.multiple {
                format!("[{}] Type your own answer", if picked { "✓" } else { " " })
            } else {
                "Type your own answer".to_string()
            };
            lines.push(Line::from(vec![
                Span::styled(format!(" {}. ", total + 1), prefix_style),
                Span::styled(label, label_style),
            ]));
            if prompt.editing {
                lines.push(Line::from(vec![
                    Span::styled("    > ", Style::default().fg(theme.accent).bg(bg)),
                    Span::styled(
                        prompt.edit_buffer.clone(),
                        Style::default().fg(theme.text).bg(bg),
                    ),
                    Span::styled("_", Style::default().fg(theme.primary).bg(bg)),
                ]));
            } else {
                let stored = prompt.custom.get(prompt.tab).cloned().unwrap_or_default();
                if !stored.is_empty() {
                    lines.push(Line::from(vec![
                        Span::styled("    ", Style::default().bg(bg)),
                        Span::styled(stored, Style::default().fg(theme.text_muted).bg(bg)),
                    ]));
                }
            }
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
    let mut footer: Vec<Span> = Vec::new();
    let single = prompt.is_single();
    let on_confirm = prompt.on_confirm();
    let multi = prompt.current().map(|i| i.multiple).unwrap_or(false);
    if !single {
        footer.push(Span::styled("⇆ ", Style::default().fg(theme.text).bg(bg)));
        footer.push(Span::styled(
            "tab  ",
            Style::default().fg(theme.text_muted).bg(bg),
        ));
    }
    if !on_confirm {
        footer.push(Span::styled("↑↓ ", Style::default().fg(theme.text).bg(bg)));
        footer.push(Span::styled(
            "select  ",
            Style::default().fg(theme.text_muted).bg(bg),
        ));
    }
    let enter_label = if on_confirm {
        "submit"
    } else if multi {
        "toggle"
    } else if single {
        "submit"
    } else {
        "confirm"
    };
    footer.push(Span::styled(
        "enter ",
        Style::default().fg(theme.text).bg(bg),
    ));
    footer.push(Span::styled(
        format!("{enter_label}  "),
        Style::default().fg(theme.text_muted).bg(bg),
    ));
    footer.push(Span::styled("esc ", Style::default().fg(theme.text).bg(bg)));
    footer.push(Span::styled(
        "dismiss",
        Style::default().fg(theme.text_muted).bg(bg),
    ));
    f.render_widget(
        Paragraph::new(Line::from(footer)).style(Style::default().bg(bg)),
        footer_area,
    );
}
