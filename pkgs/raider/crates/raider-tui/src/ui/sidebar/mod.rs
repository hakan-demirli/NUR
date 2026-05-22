use std::sync::Arc;

use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use crate::app::sidebar_state::{SidebarCacheKey, SidebarRender};
use crate::app::App;

pub(crate) mod files;
pub(crate) mod footer;
pub(crate) mod lsp;
pub(crate) mod mcp;
pub(crate) mod scroll;
pub(crate) mod section_header;
pub(crate) mod todos;

use files::file_change_line;
use footer::{footer_path_line, status_bullet_line};
use lsp::lsp_entry_line;
use mcp::mcp_entry_line;
use scroll::draw_scrollbar;
use section_header::section_header;
use todos::todo_entry_lines;

pub(crate) fn render_sidebar(f: &mut Frame, app: &mut App, area: Rect) {
    app.sidebar.last_sidebar_rect = Some(area);
    let theme = app.theme.theme.clone();
    let panel_bg = theme.background_panel;

    f.render_widget(Block::default().style(Style::default().bg(panel_bg)), area);

    let pad_x = 2u16;
    let inner = Rect::new(
        area.x + pad_x,
        area.y + 1,
        area.width.saturating_sub(pad_x * 2),
        area.height.saturating_sub(2),
    );
    if inner.width == 0 || inner.height == 0 {
        return;
    }

    let sidebar_footer = app.sidebar.sidebar.footer.clone();
    let sidebar_footer_path = app.sidebar.sidebar.footer_path.clone();

    let render = build_or_reuse_render(app, &theme, inner.width);
    let lines = &render.lines;
    let header_line_indices = &render.header_line_indices;

    let footer_height: u16 = if sidebar_footer_path.is_some() { 2 } else { 1 };
    let gap: u16 = 1;
    let footer_y = inner.y + inner.height.saturating_sub(footer_height);
    let footer_area = Rect::new(inner.x, footer_y, inner.width, footer_height);

    let body_height = inner
        .height
        .saturating_sub(footer_height)
        .saturating_sub(gap);
    let body_area = Rect::new(inner.x, inner.y, inner.width, body_height);

    let total_lines = lines.len();
    app.sidebar.total_sidebar_content_lines = total_lines;
    app.sidebar.sidebar_body_height = body_height;
    let max_offset = total_lines.saturating_sub(body_height as usize);
    let needs_scrollbar = max_offset > 0 && body_area.width > 0 && body_area.height > 0;
    if app.sidebar.sidebar.scroll_offset > max_offset {
        app.sidebar.sidebar.scroll_offset = max_offset;
    }
    let offset = app.sidebar.sidebar.scroll_offset;

    let body_content_width = if needs_scrollbar {
        body_area.width.saturating_sub(1)
    } else {
        body_area.width
    };
    let body_content_area = Rect::new(
        body_area.x,
        body_area.y,
        body_content_width,
        body_area.height,
    );

    if body_content_area.height > 0 && body_content_area.width > 0 {
        let visible: Vec<Line> = lines
            .iter()
            .skip(offset)
            .take(body_content_area.height as usize)
            .cloned()
            .collect();
        let body_text = Paragraph::new(visible).style(Style::default().bg(panel_bg));
        f.render_widget(body_text, body_content_area);
    }

    let mut header_rects: Vec<(u32, ratatui::layout::Rect)> = Vec::new();
    for (slot, line_idx) in header_line_indices.iter() {
        if *line_idx < offset {
            continue;
        }
        let local = *line_idx - offset;
        if local >= body_content_area.height as usize {
            continue;
        }
        let y = body_content_area.y + local as u16;
        header_rects.push((
            *slot,
            ratatui::layout::Rect {
                x: body_content_area.x,
                y,
                width: body_content_area.width,
                height: 1,
            },
        ));
    }
    app.sidebar.sidebar_header_rects = header_rects;

    if needs_scrollbar {
        draw_scrollbar(
            f,
            body_area,
            total_lines,
            offset,
            max_offset,
            &theme,
            panel_bg,
        );
    } else {
        app.sidebar.sidebar.scroll_offset = 0;
    }

    let mut footer_lines: Vec<Line> = Vec::with_capacity(footer_height as usize);
    if let Some(path) = sidebar_footer_path.as_deref() {
        footer_lines.push(footer_path_line(&theme, panel_bg, path));
    }
    footer_lines.push(status_bullet_line(&theme, panel_bg, &sidebar_footer));
    f.render_widget(
        Paragraph::new(footer_lines).style(Style::default().bg(panel_bg)),
        footer_area,
    );
}

fn build_or_reuse_render(
    app: &mut App,
    theme: &crate::ui::theme::Theme,
    inner_width: u16,
) -> Arc<SidebarRender> {
    let key = SidebarCacheKey {
        version: app.sidebar.version(),
        width: inner_width,
        theme_mode: theme.mode,
    };
    if let Some((cached_key, render)) = app.sidebar.render_cache.as_ref() {
        if *cached_key == key {
            return render.clone();
        }
    }
    let render = Arc::new(build_sidebar_render(app, theme, inner_width));
    app.sidebar.render_cache = Some((key, render.clone()));
    render
}

fn build_sidebar_render(
    app: &App,
    theme: &crate::ui::theme::Theme,
    inner_width: u16,
) -> SidebarRender {
    let panel_bg = theme.background_panel;
    let body_width = inner_width.saturating_sub(1) as usize;
    let title_style = Style::default()
        .fg(theme.text)
        .bg(panel_bg)
        .add_modifier(Modifier::BOLD);
    let header_style = title_style;
    let muted = Style::default().fg(theme.text_muted).bg(panel_bg);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut header_line_indices: Vec<(u32, usize)> = Vec::new();

    push_wrapped_text_lines(
        &mut lines,
        &app.sidebar.sidebar.title,
        title_style,
        body_width,
    );
    if let Some(subtitle) = app.sidebar.sidebar.subtitle.as_deref() {
        push_wrapped_text_lines(&mut lines, subtitle, muted, body_width);
    }

    for section in &app.sidebar.sidebar.sections {
        lines.push(Line::from(Span::raw("")));
        match &section.body {
            crate::sidebar::SidebarBody::Lines(text_lines) => {
                lines.push(Line::from(Span::styled(
                    section.title.clone(),
                    header_style,
                )));
                for line in text_lines {
                    push_wrapped_text_lines(&mut lines, line, muted, body_width);
                }
            }
            crate::sidebar::SidebarBody::Files {
                ref entries,
                collapsed,
            } => {
                let collapsed = *collapsed;
                header_line_indices.push((section.order, lines.len()));
                lines.push(section_header(
                    &section.title,
                    entries.len(),
                    collapsed,
                    theme,
                    panel_bg,
                ));
                if entries.len() <= 2 || !collapsed {
                    for entry in entries.iter() {
                        lines.push(file_change_line(theme, panel_bg, entry, body_width));
                    }
                }
            }
            crate::sidebar::SidebarBody::Todos {
                ref entries,
                collapsed,
            } => {
                let collapsed = *collapsed;
                header_line_indices.push((section.order, lines.len()));
                lines.push(section_header(
                    &section.title,
                    entries.len(),
                    collapsed,
                    theme,
                    panel_bg,
                ));
                if entries.len() <= 2 || !collapsed {
                    for entry in entries.iter() {
                        lines.extend(todo_entry_lines(theme, panel_bg, entry, body_width));
                    }
                }
            }
            crate::sidebar::SidebarBody::Mcps {
                ref entries,
                collapsed,
            } => {
                let collapsed = *collapsed;
                header_line_indices.push((section.order, lines.len()));
                lines.push(section_header(
                    &section.title,
                    entries.len(),
                    collapsed,
                    theme,
                    panel_bg,
                ));
                if entries.len() <= 2 || !collapsed {
                    for entry in entries.iter() {
                        lines.push(mcp_entry_line(theme, panel_bg, entry));
                    }
                }
            }
            crate::sidebar::SidebarBody::Lsps {
                ref entries,
                ref placeholder,
                collapsed,
            } => {
                let collapsed = *collapsed;
                header_line_indices.push((section.order, lines.len()));
                lines.push(section_header(
                    &section.title,
                    entries.len(),
                    collapsed,
                    theme,
                    panel_bg,
                ));
                if entries.is_empty() {
                    lines.push(Line::from(Span::styled(placeholder.clone(), muted)));
                } else if entries.len() <= 2 || !collapsed {
                    for entry in entries.iter() {
                        lines.push(lsp_entry_line(theme, panel_bg, entry));
                    }
                }
            }
        }
    }

    SidebarRender {
        lines,
        header_line_indices,
    }
}

fn push_wrapped_text_lines(out: &mut Vec<Line<'static>>, text: &str, style: Style, width: usize) {
    let options = textwrap::Options::new(width.max(1)).break_words(false);
    let wrapped = textwrap::wrap(text, options);
    if wrapped.is_empty() {
        out.push(Line::from(Span::styled(String::new(), style)));
    } else {
        out.extend(
            wrapped
                .into_iter()
                .map(|line| Line::from(Span::styled(line.into_owned(), style))),
        );
    }
}
