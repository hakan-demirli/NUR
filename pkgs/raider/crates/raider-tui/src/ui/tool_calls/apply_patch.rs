use ratatui::prelude::*;
use ratatui::widgets::ListItem;

use crate::action::ToolCall;
use crate::ui::diff::render_diff_block_with_width;
use crate::ui::path::normalize_path;
use crate::ui::primitives::bar_gap1;
use crate::ui::syntax::build_syntax_ctx;
use crate::ui::theme::Theme;

use super::error::push_tool_error_lines;

pub(crate) fn render_apply_patch_blocks(
    tool: &ToolCall,
    theme: &Theme,
    width: usize,
    ps: &syntect::parsing::SyntaxSet,
    ts: &syntect::highlighting::ThemeSet,
    synth_theme: &syntect::highlighting::Theme,
) -> Vec<ListItem<'static>> {
    use crate::action::PatchKind;
    let bg = theme.background_panel;
    let (bar, gap) = bar_gap1(theme.background, bg);
    let muted = Style::default().fg(theme.text_muted).bg(bg);

    let mut items: Vec<ListItem<'static>> = Vec::new();
    for (idx, patch) in tool.patches.iter().enumerate() {
        if idx > 0 {
            items.push(ListItem::new(vec![Line::default()]));
        }
        let normalized_path = normalize_path(&patch.path);
        let normalized_new = patch
            .new_path
            .as_deref()
            .map(normalize_path)
            .unwrap_or_default();
        let title_spans: Vec<Span<'static>> = match patch.kind {
            PatchKind::Created => vec![
                bar.clone(),
                gap.clone(),
                Span::styled(
                    "# Created ".to_string(),
                    Style::default().fg(theme.diff_added).bg(bg),
                ),
                Span::styled(normalized_path, muted),
            ],
            PatchKind::Deleted => vec![
                bar.clone(),
                gap.clone(),
                Span::styled(
                    "# Deleted ".to_string(),
                    Style::default().fg(theme.diff_removed).bg(bg),
                ),
                Span::styled(normalized_path, muted),
            ],
            PatchKind::Moved => vec![
                bar.clone(),
                gap.clone(),
                Span::styled(
                    format!("# Moved {normalized_path} → {normalized_new}"),
                    muted,
                ),
            ],
            PatchKind::Patched => vec![
                bar.clone(),
                gap.clone(),
                Span::styled(format!("← Patched {normalized_path}"), muted),
            ],
        };
        items.push(ListItem::new(vec![Line::from(title_spans)]).style(Style::default().bg(bg)));
        if let Some(diff_text) = patch.diff.as_deref() {
            let syntax = build_syntax_ctx(ps, ts, theme, synth_theme, &patch.path);
            for line in render_diff_block_with_width(
                diff_text,
                theme,
                bar.clone(),
                gap.clone(),
                width.saturating_sub(2) as u16,
                syntax,
            ) {
                items.push(ListItem::new(vec![line]).style(Style::default().bg(bg)));
            }
        }
    }
    if let Some(err) = &tool.error {
        items.push(ListItem::new(vec![Line::default()]));
        let mut error_lines = Vec::new();
        push_tool_error_lines(
            &mut error_lines,
            err,
            theme,
            bar.clone(),
            gap.clone(),
            width,
        );
        for line in error_lines {
            items.push(ListItem::new(vec![line]).style(Style::default().bg(bg)));
        }
    }
    items
}
