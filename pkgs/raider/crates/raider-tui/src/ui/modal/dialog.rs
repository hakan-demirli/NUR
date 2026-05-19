use ratatui::layout::Position;
use ratatui::prelude::*;
use ratatui::widgets::{List, ListItem, Paragraph};

use crate::app::App;
use crate::dialog::{DialogKind, DialogPayload};
use crate::ui::spinner::spinner_frame;

use super::{centered_box, plugin_alert::render_plugin_alert_dialog};

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

    let visible = dialog.visible_options();
    let footer_rows: u16 = if dialog.actions.is_empty() { 0 } else { 1 };
    let h = visible.len() as u16 + 4 + footer_rows;
    let (_rect, inner) = centered_box(f, screen, theme, dialog.title.as_str(), (40, 70), h);

    let filter_line = Line::from(vec![
        Span::styled("> ", Style::default().fg(theme.accent)),
        Span::styled(
            dialog.filter.clone(),
            Style::default().fg(theme.text).bg(theme.background_menu),
        ),
    ]);
    let constraints: Vec<Constraint> = if footer_rows > 0 {
        vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(footer_rows),
        ]
    } else {
        vec![
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
        ]
    };
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(inner);
    f.render_widget(
        Paragraph::new(filter_line).style(Style::default().bg(theme.background_menu)),
        layout[0],
    );
    let filter_cursor_cols = dialog.filter[..dialog.filter_cursor_position]
        .chars()
        .count() as u16;
    let filter_cursor_x = layout[0]
        .x
        .saturating_add(2)
        .saturating_add(filter_cursor_cols)
        .min(
            layout[0]
                .x
                .saturating_add(layout[0].width.saturating_sub(1)),
        );
    f.set_cursor_position(Position::new(filter_cursor_x, layout[0].y));
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            "─".repeat(inner.width as usize),
            Style::default()
                .fg(theme.border_subtle)
                .bg(theme.background_menu),
        ))),
        layout[1],
    );

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
    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .map(|(i, opt)| {
            if opt.is_header {
                let header_style = Style::default()
                    .fg(theme.accent)
                    .bg(theme.background_menu)
                    .add_modifier(Modifier::BOLD);
                let spans = vec![
                    Span::styled("  ", Style::default().bg(theme.background_menu)),
                    Span::styled(opt.title.clone(), header_style),
                ];
                return ListItem::new(Line::from(spans))
                    .style(Style::default().bg(theme.background_menu));
            }
            let is_selected_row = selected_idx == Some(i);
            let is_current = !opt.value.is_empty() && opt.value == dialog.initial_value;
            let marker = if has_state_marker && is_current {
                let fg = if is_selected_row {
                    theme.selected_list_item_text
                } else {
                    theme.primary
                };
                Span::styled("● ", Style::default().fg(fg))
            } else if has_state_marker {
                Span::raw("  ")
            } else {
                Span::raw("")
            };
            let title_fg = if opt.disabled {
                theme.text_muted
            } else if is_selected_row {
                theme.selected_list_item_text
            } else {
                theme.text
            };
            let trailing: Vec<Span<'_>> = if is_session_picker
                && busy_lookup
                    .get(opt.value.as_str())
                    .copied()
                    .unwrap_or(false)
            {
                let bg = if is_selected_row {
                    theme.primary
                } else {
                    theme.background_menu
                };
                vec![
                    Span::raw(" "),
                    Span::styled(
                        spinner_glyph.to_string(),
                        Style::default().fg(theme.accent).bg(bg),
                    ),
                ]
            } else {
                Vec::new()
            };
            let mut spans = vec![
                marker,
                Span::styled(opt.title.clone(), Style::default().fg(title_fg)),
            ];
            if let Some(description) = opt.description.as_ref().filter(|s| !s.is_empty()) {
                spans.push(Span::styled(
                    format!("  {}", description),
                    Style::default().fg(theme.text_muted),
                ));
            }
            if let Some(category) = opt.category.as_ref().filter(|s| !s.is_empty()) {
                if !has_visible_sections {
                    spans.push(Span::styled(
                        format!("  · {}", category),
                        Style::default().fg(theme.text_muted),
                    ));
                }
            }
            if opt.disabled {
                spans.push(Span::styled(
                    "  disabled",
                    Style::default().fg(theme.text_muted),
                ));
            }
            spans.extend(trailing);
            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .style(Style::default().bg(theme.background_menu))
        .highlight_style(
            Style::default()
                .bg(theme.primary)
                .fg(theme.selected_list_item_text)
                .add_modifier(Modifier::BOLD),
        );
    f.render_stateful_widget(list, layout[2], &mut dialog.list_state);

    if footer_rows > 0 {
        let footer_area = layout[3];
        let title_style = Style::default()
            .fg(theme.text)
            .bg(theme.background_menu)
            .add_modifier(Modifier::BOLD);
        let label_style = Style::default()
            .fg(theme.text_muted)
            .bg(theme.background_menu);
        let gap_style = Style::default().bg(theme.background_menu);
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
                .style(Style::default().bg(theme.background_menu)),
            footer_area,
        );
    }
}
