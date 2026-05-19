use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::app::App;

use crate::ui::primitives::bar_gap;

pub(crate) fn render_subagent_footer(f: &mut Frame, app: &App, area: Rect) {
    if area.width < 8 || area.height == 0 {
        return;
    }
    let theme = &app.theme.theme;
    let panel_bg = theme.background_panel;

    f.render_widget(Block::default().style(Style::default().bg(panel_bg)), area);

    let pad_x = 2u16;
    let inset_left = 1u16;
    let inset_right = 1u16;
    let inner_x = area.x + inset_left + pad_x;
    if area.width <= inset_left + inset_right + pad_x * 2 {
        return;
    }
    let inner_w = area.width - inset_left - inset_right - pad_x * 2;

    let (bar, _gap) = bar_gap(theme.border, panel_bg);
    let bar_only_rect = Rect::new(area.x + inset_left, area.y, 1, area.height);
    f.render_widget(
        Paragraph::new(vec![Line::from(vec![bar.clone()]); area.height as usize])
            .style(Style::default().bg(panel_bg)),
        bar_only_rect,
    );

    let info = subagent_info(app);

    let mut left_spans: Vec<Span<'_>> = Vec::new();
    left_spans.push(Span::styled(
        info.label.clone(),
        Style::default()
            .fg(theme.text)
            .bg(panel_bg)
            .add_modifier(Modifier::BOLD),
    ));
    if info.total > 0 {
        left_spans.push(Span::styled(
            format!(" ({} of {})", info.index, info.total),
            Style::default().fg(theme.text_muted).bg(panel_bg),
        ));
    }
    if let Some(usage) = app.prompt.prompt_info.usage.as_deref() {
        if !usage.is_empty() {
            left_spans.push(Span::styled(
                format!(" · {usage}"),
                Style::default().fg(theme.text_muted).bg(panel_bg),
            ));
        }
    }

    let key_style = Style::default()
        .fg(theme.text)
        .bg(panel_bg)
        .add_modifier(Modifier::BOLD);
    let muted = Style::default().fg(theme.text_muted).bg(panel_bg);
    let right_spans: Vec<Span<'_>> = vec![
        Span::styled("Parent ", muted),
        Span::styled("up", key_style),
        Span::styled("  Prev ", muted),
        Span::styled("left", key_style),
        Span::styled("  Next ", muted),
        Span::styled("right", key_style),
    ];

    let right_w: u16 = right_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();

    let top_y = area.y + area.height.saturating_sub(2);

    let max_left_w = inner_w.saturating_sub(right_w + 2);
    if max_left_w > 0 {
        let left_total_w: usize = left_spans.iter().map(|s| s.content.chars().count()).sum();
        let left_w = left_total_w.min(max_left_w as usize);
        let lrect = Rect::new(inner_x, top_y, left_w as u16, 1);
        f.render_widget(
            Paragraph::new(Line::from(left_spans)).style(Style::default().bg(panel_bg)),
            lrect,
        );
    }

    if right_w < inner_w {
        let rx = inner_x + inner_w - right_w;
        let rrect = Rect::new(rx, top_y, right_w, 1);
        f.render_widget(
            Paragraph::new(Line::from(right_spans)).style(Style::default().bg(panel_bg)),
            rrect,
        );
    }
}

struct SubagentInfo {
    label: String,
    index: usize,
    total: usize,
}

fn subagent_info(app: &App) -> SubagentInfo {
    let Some(current) = app.sessions.sessions.current.as_deref() else {
        return SubagentInfo {
            label: "Subagent".into(),
            index: 0,
            total: 0,
        };
    };
    let cur_entry = app.sessions.sessions.get(current);
    let label = derive_label_from_title(cur_entry.map(|e| e.title.as_str()).unwrap_or(""));
    let Some(parent_id) = cur_entry.and_then(|e| e.parent_id.clone()) else {
        return SubagentInfo {
            label,
            index: 0,
            total: 0,
        };
    };
    let siblings = app.sessions.sessions.children_of(&parent_id);
    let total = siblings.len();
    let index = app
        .sessions
        .sessions
        .child_index(&parent_id, current)
        .map(|i| i + 1)
        .unwrap_or(0);
    SubagentInfo {
        label,
        index,
        total,
    }
}

fn derive_label_from_title(title: &str) -> String {
    if let Some(start) = title.rfind("(@") {
        let rest = &title[start + 2..];
        if let Some(end) = rest.find(" subagent)") {
            let agent = &rest[..end];
            if !agent.is_empty() {
                let mut chars = agent.chars();
                let first = chars
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default();
                return format!("{}{}", first, chars.as_str());
            }
        }
    }
    "Subagent".to_string()
}

#[cfg(test)]
mod tests {
    use super::derive_label_from_title;

    #[test]
    fn label_extracts_agent_from_opencode_style_title() {
        assert_eq!(
            derive_label_from_title("find auth helpers (@explore subagent)"),
            "Explore",
        );
    }

    #[test]
    fn label_falls_back_when_no_marker() {
        assert_eq!(derive_label_from_title("Plain title"), "Subagent");
    }

    #[test]
    fn label_falls_back_for_empty_string() {
        assert_eq!(derive_label_from_title(""), "Subagent");
    }
}
