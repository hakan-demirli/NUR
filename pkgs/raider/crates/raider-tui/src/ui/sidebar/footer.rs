use ratatui::prelude::*;

use crate::ui::theme::Theme;

pub(crate) fn footer_path_line(theme: &Theme, panel_bg: Color, path: &str) -> Line<'static> {
    let muted = Style::default().fg(theme.text_muted).bg(panel_bg);
    let bright = Style::default().fg(theme.text).bg(panel_bg);

    let (path_part, branch_suffix) = match path.find(':') {
        Some(idx) => (&path[..idx], &path[idx..]),
        None => (path, ""),
    };

    let mut spans: Vec<Span<'static>> = Vec::new();
    match path_part.rfind('/') {
        Some(idx) => {
            let parent = &path_part[..=idx];
            let basename = &path_part[idx + 1..];
            if !parent.is_empty() {
                spans.push(Span::styled(parent.to_string(), muted));
            }
            if !basename.is_empty() {
                spans.push(Span::styled(basename.to_string(), bright));
            }
        }
        None => {
            spans.push(Span::styled(path_part.to_string(), bright));
        }
    }
    if !branch_suffix.is_empty() {
        spans.push(Span::styled(branch_suffix.to_string(), muted));
    }
    Line::from(spans)
}

pub(crate) fn status_bullet_line(theme: &Theme, panel_bg: Color, label: &str) -> Line<'static> {
    Line::from(vec![
        Span::styled("•", Style::default().fg(theme.success).bg(panel_bg)),
        Span::raw(" "),
        Span::styled(
            label.to_string(),
            Style::default().fg(theme.text_muted).bg(panel_bg),
        ),
    ])
}
