use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;

pub(crate) fn render_status_bar(
    f: &mut Frame,
    app: &App,
    inner_x: u16,
    footer_y: u16,
    inner_w: u16,
    prompt_bg: Color,
    bar_color: Color,
) {
    let theme = &app.theme.theme;
    let agent = app.current_agent();

    let mut left_spans: Vec<Span> = vec![Span::styled(
        agent.title.clone(),
        Style::default()
            .fg(bar_color)
            .bg(prompt_bg)
            .add_modifier(Modifier::BOLD),
    )];

    if let Some(m) = app.models.current_model.as_ref() {
        let (provider_display, model_name) = match app.models.catalog.find(m) {
            Some((p, mi)) => (p.name.clone(), mi.display_name().to_string()),
            None => (None, m.model_id.clone()),
        };
        left_spans.push(Span::styled(
            " · ",
            Style::default().fg(theme.text_muted).bg(prompt_bg),
        ));
        left_spans.push(Span::styled(
            model_name,
            Style::default().fg(theme.text).bg(prompt_bg),
        ));
        if let Some(provider_name) = provider_display {
            left_spans.push(Span::styled(" ", Style::default().bg(prompt_bg)));
            left_spans.push(Span::styled(
                provider_name,
                Style::default().fg(theme.text_muted).bg(prompt_bg),
            ));
        }
        if let Some(variant) = app.models.current_variant.as_deref() {
            left_spans.push(Span::styled(
                " · ",
                Style::default().fg(theme.text_muted).bg(prompt_bg),
            ));
            left_spans.push(Span::styled(
                variant.to_string(),
                Style::default()
                    .fg(theme.warning)
                    .bg(prompt_bg)
                    .add_modifier(Modifier::BOLD),
            ));
        }
    }

    let right_text = app.prompt.prompt_info.right.clone().unwrap_or_default();
    let right_w = right_text.chars().count() as u16;

    let left_rect = Rect::new(inner_x, footer_y, inner_w, 1);
    f.render_widget(
        Paragraph::new(Line::from(left_spans)).style(Style::default().bg(prompt_bg)),
        left_rect,
    );
    if right_w > 0 && right_w + 1 < inner_w {
        let right_x = inner_x + inner_w - right_w;
        let right_rect = Rect::new(right_x, footer_y, right_w, 1);
        let right_span = Span::styled(
            right_text,
            Style::default().fg(theme.text_muted).bg(prompt_bg),
        );
        f.render_widget(
            Paragraph::new(Line::from(right_span)).style(Style::default().bg(prompt_bg)),
            right_rect,
        );
    }
}
