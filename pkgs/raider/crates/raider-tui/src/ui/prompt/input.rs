use ratatui::layout::Position;
use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::widgets::{Block, Paragraph};

use crate::app::input::{wrap_for_display, WrapResult};
use crate::app::App;
use crate::ui::agent::agent_color;

use super::status_bar::render_status_bar;

fn build_input_spans<'a>(
    row: &'a str,
    placeholders: &[(&str, Style)],
    default_style: Style,
) -> Vec<Span<'a>> {
    if placeholders.is_empty() || row.is_empty() {
        return vec![Span::styled(row.to_string(), default_style)];
    }
    let mut out: Vec<Span<'a>> = Vec::new();
    let bytes = row.as_bytes();
    let mut i = 0usize;
    let mut plain_start = 0usize;
    while i < bytes.len() {
        let mut hit: Option<(usize, Style)> = None;
        for (ph, style) in placeholders {
            let pb = ph.as_bytes();
            if pb.is_empty() || i + pb.len() > bytes.len() {
                continue;
            }
            if &bytes[i..i + pb.len()] == pb {
                hit = Some((pb.len(), *style));
                break;
            }
        }
        match hit {
            Some((len, style)) => {
                if i > plain_start {
                    out.push(Span::styled(row[plain_start..i].to_string(), default_style));
                }
                out.push(Span::styled(row[i..i + len].to_string(), style));
                i += len;
                plain_start = i;
            }
            None => {
                i += 1;
            }
        }
    }
    if plain_start < bytes.len() {
        out.push(Span::styled(row[plain_start..].to_string(), default_style));
    }
    if out.is_empty() {
        out.push(Span::styled(String::new(), default_style));
    }
    out
}

pub(crate) fn wrap_input(
    input: &str,
    width: usize,
    app: &App,
) -> (Vec<String>, Option<(usize, usize)>) {
    let WrapResult { rows, cursor, .. } = wrap_for_display(input, app.input.cursor_position, width);
    (rows, cursor)
}

pub(crate) fn render_prompt(
    f: &mut Frame,
    app: &App,
    area: Rect,
    wrapped: &[String],
    cursor_visual: Option<(usize, usize)>,
) {
    let theme = &app.theme.theme;
    let prompt_bg = theme.background_element;

    f.render_widget(Block::default().style(Style::default().bg(prompt_bg)), area);

    if area.width < 3 {
        return;
    }

    let agent = app.current_agent();
    let bar_color = agent_color(theme, app.agents.as_slice(), &agent.name);
    let bar_style = Style::default().fg(bar_color).bg(prompt_bg);
    let frame_buf = f.buffer_mut();
    for y in area.y..area.y + area.height {
        frame_buf[(area.x, y)].set_symbol("┃").set_style(bar_style);
    }

    let pad_left = 2u16;
    let pad_right = 2u16;
    let inner_x = area.x + 1 + pad_left;
    let inner_y = area.y + 1;
    let inner_w = area.width.saturating_sub(1 + pad_left + pad_right).max(1);
    let inner_h = area.height.saturating_sub(3).max(1);

    let text_style = Style::default().fg(theme.text).bg(prompt_bg);

    if app.input.input.is_empty() {
        let home_hints_visible = app.sessions.sessions.current.is_none() && app.messages.is_empty();
        if home_hints_visible {
            if let Some(example) = app.prompt.current_placeholder() {
                let hint = format!("Ask anything... \"{example}\"");
                let max = inner_w as usize;
                let trimmed: String = hint.chars().take(max).collect();
                let line = Line::from(Span::styled(
                    trimmed,
                    Style::default().fg(theme.text_muted).bg(prompt_bg),
                ));
                let p = Paragraph::new(line).style(Style::default().bg(prompt_bg));
                let target = Rect::new(inner_x, inner_y, inner_w, 1);
                f.render_widget(p, target);
            }
        }
    }

    let total = wrapped.len() as u16;
    let cursor_row = cursor_visual.map(|(_, cy)| cy as u16).unwrap_or(0);
    let scroll = if total <= inner_h {
        0
    } else {
        let max_scroll = total - inner_h;
        let needed = cursor_row.saturating_sub(inner_h.saturating_sub(1));
        needed.min(max_scroll)
    };

    if !app.input.input.is_empty() {
        use crate::app::input::PromptPartKind;
        let paste_style = Style::default()
            .fg(theme.background)
            .bg(theme.warning)
            .add_modifier(Modifier::BOLD);
        let file_style = Style::default()
            .fg(theme.warning)
            .bg(prompt_bg)
            .add_modifier(Modifier::BOLD);
        let fallback_style = Style::default()
            .fg(theme.text_muted)
            .bg(prompt_bg)
            .add_modifier(Modifier::ITALIC);

        let placeholders: Vec<(&str, Style)> = app
            .input
            .parts
            .iter()
            .filter(|p| !p.placeholder.is_empty())
            .map(|p| {
                let style = match &p.kind {
                    PromptPartKind::Text(_) => paste_style,
                    PromptPartKind::File { .. } => file_style,
                };
                (p.placeholder.as_str(), style)
            })
            .collect();
        let _ = fallback_style;
        let scroll_usize = scroll as usize;
        for (i, row) in wrapped.iter().skip(scroll_usize).enumerate() {
            if i as u16 >= inner_h {
                break;
            }
            let spans = build_input_spans(row, &placeholders, text_style);
            let line = Line::from(spans);
            let p = Paragraph::new(line).style(Style::default().bg(prompt_bg));
            let target = Rect::new(inner_x, inner_y + i as u16, inner_w, 1);
            f.render_widget(p, target);
        }
    }

    if let Some((cx, cy)) = cursor_visual {
        let cy_u16 = cy as u16;
        if cy_u16 >= scroll && cy_u16 < scroll + inner_h {
            let x = inner_x + cx as u16;
            let y = inner_y + (cy_u16 - scroll);
            if x < area.x + area.width - 1 && y < area.y + area.height - 1 {
                f.set_cursor_position(Position::new(x, y));
            }
        }
    }

    let footer_y = area.y + area.height - 1;
    render_status_bar(f, app, inner_x, footer_y, inner_w, prompt_bg, bar_color);
}
