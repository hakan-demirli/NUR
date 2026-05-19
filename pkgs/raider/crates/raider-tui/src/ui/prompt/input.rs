use ratatui::layout::Position;
use ratatui::prelude::*;
use ratatui::style::Modifier;
use ratatui::widgets::{Block, Paragraph};
use unicode_width::UnicodeWidthStr;

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
    let mut out: Vec<String> = Vec::new();
    let mut cursor_pos: Option<(usize, usize)> = None;
    let mut current_byte_idx = 0;

    let parts: Vec<&str> = input.split('\n').collect();
    let parts_count = parts.len();
    let opts = textwrap::Options::new(width.max(1)).break_words(true);

    let cursor = app.input.cursor_position;

    for (i, part) in parts.iter().enumerate() {
        let part_start = current_byte_idx;
        let part_len = part.len();

        let mut lines_for_part = Vec::new();
        if part.is_empty() {
            lines_for_part.push(String::new());
        } else {
            let s = format!("{}\u{200B}", part);
            let wrapped = textwrap::wrap(&s, &opts);
            let last_idx = wrapped.len().saturating_sub(1);
            for (w_i, w) in wrapped.iter().enumerate() {
                let mut s = w.to_string();
                if w_i == last_idx && s.ends_with('\u{200B}') {
                    s.pop();
                }
                lines_for_part.push(s);
            }
        }

        let mut local = 0;
        for (li, line_str) in lines_for_part.iter().enumerate() {
            let line_bytes = line_str.len();
            let g_start = part_start + local;
            let g_end = g_start + line_bytes;
            let is_last_visual = li == lines_for_part.len() - 1;

            if cursor_pos.is_none() {
                if cursor >= g_start && cursor < g_end {
                    let off = cursor - g_start;
                    let cx = UnicodeWidthStr::width(&line_str[..off]);
                    cursor_pos = Some((cx, out.len()));
                } else if cursor == g_end && is_last_visual {
                    let cx = UnicodeWidthStr::width(line_str.as_str());
                    cursor_pos = Some((cx, out.len()));
                }
            }
            out.push(line_str.clone());
            local += line_bytes;
        }

        current_byte_idx += part_len;
        if i < parts_count - 1 {
            current_byte_idx += 1;
        }
    }

    if cursor_pos.is_none() && cursor == current_byte_idx {
        if out.is_empty() {
            cursor_pos = Some((0, 0));
            out.push(String::new());
        } else {
            let last = out.len() - 1;
            cursor_pos = Some((UnicodeWidthStr::width(out[last].as_str()), last));
        }
    }

    (out, cursor_pos)
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
    } else if cursor_row + 1 > inner_h {
        (cursor_row + 1).saturating_sub(inner_h)
    } else {
        0
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
