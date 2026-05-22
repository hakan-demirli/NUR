use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, Paragraph};

use crate::app::App;
use crate::ui::agent::agent_color;

pub(crate) fn render_completion(f: &mut Frame, app: &mut App, input_area: Rect) {
    if !app.input.completion.active || app.input.completion.candidates.is_empty() {
        return;
    }

    let theme = &app.theme.theme;
    let popup_bg = theme.background_element;
    let bar_color = agent_color(theme, &app.agents, &app.current_agent().name);

    let bar_x = input_area.x;
    let inner_x = bar_x + 3;
    let max_w = input_area.width.saturating_sub(4);
    if max_w == 0 {
        return;
    }

    let display_pad = 2usize;
    let widest_text = app
        .input
        .completion
        .candidates
        .iter()
        .map(|c| c.text.chars().count())
        .max()
        .unwrap_or(0);
    let widest_desc = app
        .input
        .completion
        .candidates
        .iter()
        .map(|c| c.description.chars().count())
        .max()
        .unwrap_or(0);
    let pad_target = widest_text + display_pad;
    let content_w = (pad_target + widest_desc).min(max_w as usize);

    let visible_rows = (app.input.completion.candidates.len() as u16).min(12);
    let total_h = visible_rows;
    let y_top = input_area.y.saturating_sub(total_h);
    if total_h == 0 {
        return;
    }

    let selected = app.input.completion.state.selected();
    let start = match selected {
        Some(sel) if sel as u16 >= visible_rows => sel as u16 - visible_rows + 1,
        _ => 0,
    } as usize;
    let end = (start + visible_rows as usize).min(app.input.completion.candidates.len());

    let bar_style = Style::default().fg(bar_color).bg(popup_bg);
    let body_style = Style::default().fg(theme.text).bg(popup_bg);
    let muted_style = Style::default().fg(theme.text_muted).bg(popup_bg);
    let accent_style = Style::default()
        .fg(theme.accent)
        .bg(popup_bg)
        .add_modifier(Modifier::BOLD);

    let clear_rect = Rect::new(bar_x, y_top, input_area.width, total_h);
    f.render_widget(Clear, clear_rect);
    f.render_widget(
        Block::default().style(Style::default().bg(popup_bg)),
        clear_rect,
    );

    let strip_x = bar_x + 1;
    let strip_w = input_area.width.saturating_sub(1);

    for (row_idx, cand_idx) in (start..end).enumerate() {
        let y = y_top + row_idx as u16;
        let c = &app.input.completion.candidates[cand_idx];
        let is_selected = selected == Some(cand_idx);

        let row_bg = if is_selected { theme.primary } else { popup_bg };
        let selected_fg = theme.selected_list_item_text;
        let row_body = Style::default()
            .fg(if is_selected { selected_fg } else { theme.text })
            .bg(row_bg);
        let row_muted = Style::default()
            .fg(if is_selected {
                selected_fg
            } else {
                theme.text_muted
            })
            .bg(row_bg);
        let row_accent = Style::default()
            .fg(if is_selected {
                selected_fg
            } else {
                theme.accent
            })
            .bg(row_bg)
            .add_modifier(Modifier::BOLD);

        f.buffer_mut()[(bar_x, y)]
            .set_symbol("┃")
            .set_style(bar_style);

        if strip_w > 0 {
            let strip = Rect::new(strip_x, y, strip_w, 1);
            f.render_widget(Block::default().style(Style::default().bg(row_bg)), strip);
        }

        let mut spans: Vec<Span> = Vec::new();
        let mut last = 0;
        let mut idxs = c.indices.clone();
        idxs.sort();
        for i in &idxs {
            let i = *i;
            if i >= c.text.len() {
                continue;
            }
            if i > last {
                spans.push(Span::styled(c.text[last..i].to_string(), row_body));
            }
            spans.push(Span::styled(
                c.text[i..=i].to_string(),
                if is_selected {
                    row_accent
                } else {
                    accent_style
                },
            ));
            last = i + 1;
        }
        if last < c.text.len() {
            spans.push(Span::styled(
                c.text[last..].to_string(),
                if is_selected { row_body } else { body_style },
            ));
        }
        let visible_len = c.text.chars().count();
        if pad_target > visible_len {
            spans.push(Span::styled(
                " ".repeat(pad_target - visible_len),
                Style::default().bg(row_bg),
            ));
        }
        if !c.description.is_empty() {
            spans.push(Span::styled(
                c.description.clone(),
                if is_selected { row_muted } else { muted_style },
            ));
        }

        let row_rect = Rect::new(inner_x, y, content_w as u16, 1);
        f.render_widget(
            Paragraph::new(Line::from(spans)).style(Style::default().bg(row_bg)),
            row_rect,
        );
    }
}
