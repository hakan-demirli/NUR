use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::app::App;
use crate::ui::agent::agent_color;

pub(crate) fn render_connector(f: &mut Frame, app: &App, area: Rect) {
    if area.width < 2 {
        return;
    }
    let theme = &app.theme.theme;
    let bar_color = agent_color(theme, &app.agents, &app.current_agent().name);
    let left_cap = Style::default().fg(bar_color).bg(theme.background);
    let strip_style = Style::default()
        .fg(theme.background_element)
        .bg(theme.background);

    let strip_glyph = if matches!(theme.background_element, Color::Reset) {
        " "
    } else {
        "▀"
    };
    let mut spans = Vec::with_capacity(area.width as usize);
    spans.push(Span::styled("╹", left_cap));
    let strip_width = area.width.saturating_sub(1) as usize;
    spans.push(Span::styled(strip_glyph.repeat(strip_width), strip_style));

    let p = Paragraph::new(Line::from(spans)).style(Style::default().bg(theme.background));
    f.render_widget(p, area);
}
