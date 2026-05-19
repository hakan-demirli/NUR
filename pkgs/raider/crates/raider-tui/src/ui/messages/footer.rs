use ratatui::prelude::*;

use crate::app::builtin::Agent;
use crate::ui::agent::{agent_color, agent_color_by_index, resolve_model_display, titlecase};
use crate::ui::theme::Theme;

pub(crate) fn assistant_footer_line<'a>(
    msg: &crate::model::Message,
    show_timestamps: bool,
    text_style: Style,
    theme: &Theme,
    agents: &[Agent],
    bg_color: Color,
    catalog: &crate::provider::ModelCatalog,
) -> Line<'a> {
    let mut spans: Vec<Span<'a>> = vec![Span::styled("   ", Style::default().bg(bg_color))];

    let marker_color = msg
        .agent
        .as_deref()
        .map(|a| agent_color(theme, agents, a))
        .unwrap_or_else(|| agent_color_by_index(theme, 0));
    spans.push(Span::styled(
        "▣  ",
        Style::default().fg(marker_color).bg(bg_color),
    ));

    let label = match msg.agent.as_deref() {
        Some(a) => titlecase(a),
        None => msg.sender.label().to_string(),
    };
    spans.push(Span::styled(label, text_style.add_modifier(Modifier::DIM)));

    if let Some(model_id) = msg.model.as_deref() {
        let display = resolve_model_display(catalog, msg.provider_id.as_deref(), model_id)
            .unwrap_or_else(|| model_id.to_string());
        spans.push(Span::styled(
            " · ",
            Style::default().fg(theme.text_muted).bg(bg_color),
        ));
        spans.push(Span::styled(
            display,
            text_style.add_modifier(Modifier::DIM),
        ));
    }

    if let Some(d) = msg.duration {
        spans.push(Span::styled(
            " · ",
            Style::default().fg(theme.text_muted).bg(bg_color),
        ));
        spans.push(Span::styled(
            crate::model::format_duration(d),
            text_style.add_modifier(Modifier::DIM),
        ));
    }

    if show_timestamps && !msg.timestamp.is_empty() {
        spans.push(Span::styled(
            format!(", {}", msg.timestamp),
            Style::default().fg(theme.text_muted).bg(bg_color),
        ));
    }

    Line::from(spans)
}
