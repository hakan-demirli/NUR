use ratatui::layout::Position;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Clear, List, ListItem, Paragraph};

use crate::app::App;
use crate::dialog::{DialogKind, DialogPayload};
use crate::ui::path::truncate_text_right_ellipsis;
use crate::ui::spinner::spinner_frame;

use super::plugin_alert::render_plugin_alert_dialog;

const PANEL_WIDTH: u16 = 60;

pub(crate) fn render_dialog(f: &mut Frame, app: &mut App, screen: Rect) {
    let busy_lookup_owned: std::collections::HashMap<String, bool> = app
        .sessions
        .sessions
        .entries
        .iter()
        .map(|e| (e.id.clone(), e.busy))
        .collect();
    let theme = app.theme.theme.clone();
    let theme = &theme;

    let Some(dialog) = app.dialogs.dialog.as_mut() else {
        return;
    };

    if let DialogPayload::PluginAlert { message } = &dialog.payload {
        render_plugin_alert_dialog(f, theme, screen, dialog.title.as_str(), message.as_str());
        return;
    }

    if matches!(&dialog.payload, DialogPayload::SessionRename { .. }) {
        render_prompt_dialog(
            f,
            theme,
            screen,
            PromptDialog {
                title: dialog.title.as_str(),
                value: dialog.filter.as_str(),
                cursor_position: dialog.filter_cursor_position,
                placeholder: "Enter text",
                footer_hint: None,
            },
        );
        return;
    }
    if let DialogPayload::PluginInstall { scope, .. } = &dialog.payload {
        let scope_hint = format!("tab toggles scope · current: {}", scope.label());
        render_prompt_dialog(
            f,
            theme,
            screen,
            PromptDialog {
                title: dialog.title.as_str(),
                value: dialog.filter.as_str(),
                cursor_position: dialog.filter_cursor_position,
                placeholder: "Path to .lua file",
                footer_hint: Some(scope_hint.as_str()),
            },
        );
        return;
    }

    let visible = dialog.visible_options();
    let footer_rows: u16 = if dialog.actions.is_empty() { 0 } else { 1 };
    let h = visible.len() as u16 + 6 + footer_rows;
    let rect = panel(f, screen, theme, h);
    let title_area = padded_row(rect, 1, 4);
    render_title_row(f, theme, title_area, dialog.title.as_str());

    let filter_area = padded_row(rect, 3, 4);
    let filter_line = if dialog.filter.is_empty() {
        Line::from(Span::styled(
            "Search",
            Style::default()
                .fg(theme.text_muted)
                .bg(theme.background_panel),
        ))
    } else {
        Line::from(Span::styled(
            dialog.filter.clone(),
            Style::default().fg(theme.text).bg(theme.background_panel),
        ))
    };
    f.render_widget(
        Paragraph::new(filter_line).style(Style::default().bg(theme.background_panel)),
        filter_area,
    );

    let filter_cursor_cols = dialog.filter[..dialog.filter_cursor_position]
        .chars()
        .count() as u16;
    let filter_cursor_x = filter_area.x.saturating_add(filter_cursor_cols).min(
        filter_area
            .x
            .saturating_add(filter_area.width.saturating_sub(1)),
    );
    f.set_cursor_position(Position::new(filter_cursor_x, filter_area.y));

    let kind = dialog.kind();
    let has_state_marker = !matches!(kind, DialogKind::CommandPalette | DialogKind::PluginSelect);
    let is_session_picker = matches!(kind, DialogKind::SessionPicker);
    let busy_lookup: std::collections::HashMap<&str, bool> = if is_session_picker {
        busy_lookup_owned
            .iter()
            .map(|(k, v)| (k.as_str(), *v))
            .collect()
    } else {
        std::collections::HashMap::new()
    };
    let spinner_glyph = spinner_frame();
    let selected_idx = dialog.list_state.selected();
    let has_visible_sections = visible.iter().any(|o| o.is_header);
    let row_width = rect.width.saturating_sub(2) as usize;
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            if opt.is_header {
                let header_style = Style::default()
                    .fg(theme.accent)
                    .bg(theme.background_panel)
                    .add_modifier(Modifier::BOLD);
                let spans = vec![
                    Span::styled("  ", Style::default().bg(theme.background_panel)),
                    Span::styled(opt.title.clone(), header_style),
                ];
                return ListItem::new(Line::from(spans))
                    .style(Style::default().bg(theme.background_panel));
            }
            let is_selected_row = selected_idx == Some(i);
            let row_bg = if is_selected_row {
                theme.primary
            } else {
                theme.background_panel
            };
            let is_current = !opt.value.is_empty() && opt.value == dialog.initial_value;
            let marker_text: &str = if has_state_marker && is_current {
                "● "
            } else if has_state_marker {
                "  "
            } else {
                ""
            };
            let marker_fg = if is_selected_row {
                theme.selected_list_item_text
            } else {
                theme.primary
            };
            let marker = if marker_text.is_empty() {
                Span::raw("")
            } else if has_state_marker && is_current {
                Span::styled(marker_text, Style::default().fg(marker_fg).bg(row_bg))
            } else {
                Span::styled(marker_text, Style::default().bg(row_bg))
            };
            let marker_width = marker_text.chars().count();
            let title_fg = if opt.disabled {
                theme.text_muted
            } else if is_selected_row {
                theme.selected_list_item_text
            } else {
                theme.text
            };
            let muted_fg = if is_selected_row {
                theme.selected_list_item_text
            } else {
                theme.text_muted
            };

            let has_spinner = is_session_picker
                && busy_lookup
                    .get(opt.value.as_str())
                    .copied()
                    .unwrap_or(false);
            let spinner_width = if has_spinner { 2 } else { 0 };

            let footer_text = opt.footer.as_deref().filter(|s| !s.is_empty());
            let footer_width = footer_text.map(|s| s.chars().count() + 5).unwrap_or(0);

            let auto_fit = footer_width > 0 || spinner_width > 0;

            let description = opt
                .description
                .as_ref()
                .filter(|s| !s.is_empty())
                .map(|s| format!("  {}", s));
            let category = opt
                .category
                .as_ref()
                .filter(|s| !s.is_empty())
                .filter(|_| !has_visible_sections)
                .map(|s| format!("  · {}", s));
            let disabled_hint = if opt.disabled { "  disabled" } else { "" };

            let (title_display, pad_width) = if auto_fit {
                let description_width = description
                    .as_deref()
                    .map(|s| s.chars().count())
                    .unwrap_or(0);
                let category_width = category.as_deref().map(|s| s.chars().count()).unwrap_or(0);
                let disabled_width = disabled_hint.chars().count();
                let reserved = marker_width
                    + description_width
                    + category_width
                    + disabled_width
                    + footer_width
                    + spinner_width;
                let title_budget = row_width.saturating_sub(reserved).max(1);
                let truncated = truncate_text_right_ellipsis(&opt.title, title_budget);
                let title_width = truncated.chars().count();
                let pad = row_width
                    .saturating_sub(marker_width)
                    .saturating_sub(title_width)
                    .saturating_sub(description_width)
                    .saturating_sub(category_width)
                    .saturating_sub(disabled_width)
                    .saturating_sub(footer_width)
                    .saturating_sub(spinner_width);
                (truncated, pad)
            } else {
                (opt.title.clone(), 0)
            };

            let mut spans: Vec<Span<'_>> = Vec::new();
            if !marker_text.is_empty() {
                spans.push(marker);
            }
            spans.push(Span::styled(
                title_display,
                Style::default().fg(title_fg).bg(row_bg),
            ));
            if let Some(desc) = description {
                spans.push(Span::styled(desc, Style::default().fg(muted_fg).bg(row_bg)));
            }
            if let Some(cat) = category {
                spans.push(Span::styled(cat, Style::default().fg(muted_fg).bg(row_bg)));
            }
            if !disabled_hint.is_empty() {
                spans.push(Span::styled(
                    disabled_hint.to_string(),
                    Style::default().fg(muted_fg).bg(row_bg),
                ));
            }
            if auto_fit && pad_width > 0 {
                spans.push(Span::styled(
                    " ".repeat(pad_width),
                    Style::default().bg(row_bg),
                ));
            }
            if let Some(footer) = footer_text {
                spans.push(Span::styled(
                    format!("  ·  {}", footer),
                    Style::default().fg(muted_fg).bg(row_bg),
                ));
            }
            if has_spinner {
                spans.push(Span::styled(" ", Style::default().bg(row_bg)));
                spans.push(Span::styled(
                    spinner_glyph.to_string(),
                    Style::default().fg(theme.accent).bg(row_bg),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .style(Style::default().bg(theme.background_panel))
        .highlight_style(
            Style::default()
                .bg(theme.primary)
                .fg(theme.selected_list_item_text)
                .add_modifier(Modifier::BOLD),
        );
    let list_area = Rect {
        x: rect.x.saturating_add(1),
        y: rect.y.saturating_add(5),
        width: rect.width.saturating_sub(2),
        height: rect.height.saturating_sub(6 + footer_rows),
    };
    f.render_stateful_widget(list, list_area, &mut dialog.list_state);

    if footer_rows > 0 {
        let footer_area = padded_row(rect, rect.height.saturating_sub(2), 4);
        let title_style = Style::default()
            .fg(theme.text)
            .bg(theme.background_panel)
            .add_modifier(Modifier::BOLD);
        let label_style = Style::default()
            .fg(theme.text_muted)
            .bg(theme.background_panel);
        let gap_style = Style::default().bg(theme.background_panel);
        let mut spans: Vec<Span<'_>> = Vec::new();
        for (i, action) in dialog.actions.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("   ", gap_style));
            }
            spans.push(Span::styled(action.label.clone(), title_style));
            spans.push(Span::styled(" ", gap_style));
            spans.push(Span::styled(action.key_hint.clone(), label_style));
        }
        f.render_widget(
            Paragraph::new(Line::from(spans))
                .alignment(Alignment::Center)
                .style(Style::default().bg(theme.background_panel)),
            footer_area,
        );
    }
}

fn panel(f: &mut Frame, screen: Rect, theme: &crate::ui::theme::Theme, height: u16) -> Rect {
    let max_width = screen.width.saturating_sub(2).max(1);
    let width = PANEL_WIDTH.min(max_width);
    let max_height = screen.height.saturating_sub(2).max(1);
    let height = height.clamp(5, max_height);
    let x = screen.x + screen.width.saturating_sub(width) / 2;
    let preferred_y = screen.y + screen.height / 4;
    let max_y = screen.y + screen.height.saturating_sub(height);
    let y = preferred_y.min(max_y);
    let rect = Rect::new(x, y, width, height);

    f.render_widget(Clear, rect);
    f.render_widget(
        Block::default().style(Style::default().bg(theme.background_panel)),
        rect,
    );
    rect
}

fn padded_row(rect: Rect, y_offset: u16, x_pad: u16) -> Rect {
    Rect {
        x: rect.x.saturating_add(x_pad),
        y: rect.y.saturating_add(y_offset),
        width: rect.width.saturating_sub(x_pad.saturating_mul(2)),
        height: 1,
    }
}

fn render_title_row(f: &mut Frame, theme: &crate::ui::theme::Theme, area: Rect, title: &str) {
    let title_area = Rect {
        width: area.width.saturating_sub(4),
        ..area
    };
    f.render_widget(
        Paragraph::new(title.to_string()).style(
            Style::default()
                .fg(theme.text)
                .bg(theme.background_panel)
                .add_modifier(Modifier::BOLD),
        ),
        title_area,
    );
    f.render_widget(
        Paragraph::new("esc").alignment(Alignment::Right).style(
            Style::default()
                .fg(theme.text_muted)
                .bg(theme.background_panel),
        ),
        area,
    );
}

struct PromptDialog<'a> {
    title: &'a str,
    value: &'a str,
    cursor_position: usize,
    placeholder: &'a str,
    footer_hint: Option<&'a str>,
}

fn render_prompt_dialog(
    f: &mut Frame,
    theme: &crate::ui::theme::Theme,
    screen: Rect,
    prompt: PromptDialog<'_>,
) {
    let PromptDialog {
        title,
        value,
        cursor_position,
        placeholder,
        footer_hint,
    } = prompt;
    let rect = panel(f, screen, theme, 8);
    render_title_row(f, theme, padded_row(rect, 1, 2), title);

    let input_area = Rect {
        x: rect.x.saturating_add(2),
        y: rect.y.saturating_add(3),
        width: rect.width.saturating_sub(4),
        height: 3,
    };
    let input_line = if value.is_empty() {
        Line::from(Span::styled(
            placeholder.to_string(),
            Style::default()
                .fg(theme.text_muted)
                .bg(theme.background_panel),
        ))
    } else {
        Line::from(Span::styled(
            value.to_string(),
            Style::default().fg(theme.text).bg(theme.background_panel),
        ))
    };
    f.render_widget(
        Paragraph::new(input_line).style(Style::default().bg(theme.background_panel)),
        input_area,
    );
    let cursor_cols = value[..cursor_position.min(value.len())].chars().count() as u16;
    let cursor_x = input_area.x.saturating_add(cursor_cols).min(
        input_area
            .x
            .saturating_add(input_area.width.saturating_sub(1)),
    );
    f.set_cursor_position(Position::new(cursor_x, input_area.y));

    let footer_area = padded_row(rect, rect.height.saturating_sub(2), 2);
    let mut footer_spans = vec![
        Span::styled(
            "enter",
            Style::default().fg(theme.text).bg(theme.background_panel),
        ),
        Span::styled(
            " submit",
            Style::default()
                .fg(theme.text_muted)
                .bg(theme.background_panel),
        ),
    ];
    if let Some(hint) = footer_hint {
        footer_spans.push(Span::styled(
            "    ",
            Style::default().bg(theme.background_panel),
        ));
        footer_spans.push(Span::styled(
            hint.to_string(),
            Style::default()
                .fg(theme.text_muted)
                .bg(theme.background_panel),
        ));
    }
    f.render_widget(
        Paragraph::new(Line::from(footer_spans)).style(Style::default().bg(theme.background_panel)),
        footer_area,
    );
}
