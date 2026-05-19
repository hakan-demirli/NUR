use ratatui::prelude::*;

use crate::ui::theme::Theme;

pub(crate) fn push_tool_error_lines<'a>(
    out: &mut Vec<Line<'a>>,
    err: &str,
    theme: &Theme,
    bar: Span<'a>,
    gap: Span<'a>,
    width: usize,
) {
    let wrap_width = width.saturating_sub(4).max(1);
    let opts = textwrap::Options::new(wrap_width).break_words(true);
    let error_style = Style::default().fg(theme.error);
    for paragraph in err.split('\n') {
        if paragraph.is_empty() {
            out.push(Line::from(vec![bar.clone(), gap.clone()]));
            continue;
        }
        for wrapped in textwrap::wrap(paragraph, &opts) {
            out.push(Line::from(vec![
                bar.clone(),
                gap.clone(),
                Span::styled(wrapped.into_owned(), error_style),
            ]));
        }
    }
}
