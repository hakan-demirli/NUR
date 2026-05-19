use ratatui::prelude::*;

use crate::ui::theme::Theme;

pub(crate) fn compaction_divider_line<'a>(title: &'a str, width: usize, theme: &Theme) -> Line<'a> {
    let border_style = Style::default()
        .fg(theme.border_active)
        .bg(theme.background);
    let title_style = Style::default().fg(theme.text).bg(theme.background);
    let title_cols = title.chars().count();
    let total = width.max(title_cols);
    let pad = total - title_cols;
    let left = pad / 2;
    let right = pad - left;
    let left_rule: String = std::iter::repeat_n('─', left).collect();
    let right_rule: String = std::iter::repeat_n('─', right).collect();
    Line::from(vec![
        Span::styled(left_rule, border_style),
        Span::styled(title.to_string(), title_style),
        Span::styled(right_rule, border_style),
    ])
}
