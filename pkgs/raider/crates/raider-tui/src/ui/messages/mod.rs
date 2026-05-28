use ratatui::prelude::*;
use ratatui::widgets::{Block, List, ListItem};

use crate::action::HostMessagePart;
use crate::app::App;
use crate::model::{Message, PartRenderCacheEntry, PartRenderCacheKey, PartRenderKind, Sender};
use crate::ui::agent::agent_color;
use crate::ui::logo::render_logo;
use crate::ui::tool_calls::{render_tool_call_v2_cached, tool_is_inline};

pub(crate) mod compaction;
pub(crate) mod footer;

use compaction::compaction_divider_line;
use footer::assistant_footer_line;

pub(crate) fn reasoning_title(text: &str) -> Option<String> {
    let t = text.trim_start();
    let rest = t.strip_prefix("**")?;
    let end = rest.find("**")?;
    let title = &rest[..end];
    if title.is_empty() || title.contains('\n') || title.contains('*') {
        return None;
    }
    Some(title.trim().to_string())
}

#[derive(Clone, Copy)]
struct MarkdownResources<'a> {
    ps: &'a syntect::parsing::SyntaxSet,
    ts: &'a syntect::highlighting::ThemeSet,
    synth_theme: &'a syntect::highlighting::Theme,
}

#[derive(Clone, Copy)]
struct ThoughtRenderState {
    collapsed: bool,
    streaming: bool,
}

fn assistant_text_items(
    text: &str,
    width: usize,
    theme: &crate::ui::theme::Theme,
    resources: MarkdownResources<'_>,
) -> Vec<ListItem<'static>> {
    let text = text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    let style = Style::default().fg(theme.text).bg(theme.background);
    let lines = crate::ui::markdown::render_markdown_with_synth(
        text,
        width,
        resources.ps,
        resources.ts,
        theme,
        crate::ui::markdown::MarkdownRenderOptions {
            synth_theme: Some(resources.synth_theme),
            default_style: style,
            suppress_style: false,
        },
    );
    lines
        .into_iter()
        .map(|line| {
            let mut spans = vec![Span::styled("   ", Style::default().bg(theme.background))];
            spans.extend(line.spans);
            ListItem::new(vec![Line::from(spans)]).style(Style::default().bg(theme.background))
        })
        .collect()
}

fn cached_assistant_text_items(
    cache: &mut std::collections::HashMap<String, PartRenderCacheEntry>,
    cache_id_prefix: String,
    text: &str,
    width: usize,
    theme: &crate::ui::theme::Theme,
    resources: MarkdownResources<'_>,
) -> Vec<ListItem<'static>> {
    let segments = crate::stream::split_into_segments(text);
    let mut out: Vec<ListItem<'static>> = Vec::new();
    for (seg_idx, seg) in segments.iter().enumerate() {
        let cache_id = format!("{cache_id_prefix}:seg:{seg_idx}");
        let key = PartRenderCacheKey {
            width,
            theme_mode: theme.mode,
            kind: PartRenderKind::Text,
            collapsed: false,
            streaming: false,
            content_hash: crate::model::content_fingerprint(seg),
        };
        if let Some(entry) = cache.get(&cache_id) {
            if entry.key == key {
                out.extend(entry.items.iter().cloned());
                continue;
            }
        }
        let items = assistant_text_items(seg, width, theme, resources);
        cache.insert(
            cache_id,
            PartRenderCacheEntry {
                key,
                items: items.clone(),
            },
        );
        out.extend(items);
    }
    out
}

fn assistant_thought_items(
    text: &str,
    collapsed: bool,
    streaming: bool,
    width: usize,
    theme: &crate::ui::theme::Theme,
    resources: MarkdownResources<'_>,
) -> Vec<ListItem<'static>> {
    let content = text.replace("[REDACTED]", "");
    let content = content.trim();
    if content.is_empty() {
        return Vec::new();
    }
    let reasoning_bar = Span::styled(
        "┃",
        Style::default()
            .fg(theme.background_element)
            .bg(theme.background),
    );
    let reasoning_gap = Span::styled("  ", Style::default().bg(theme.background));
    let thought_style = Style::default()
        .fg(theme.text_muted)
        .bg(theme.background)
        .add_modifier(Modifier::ITALIC);
    let label_fg = mute_toward(
        theme.markdown_emph,
        theme.background,
        theme.thinking_opacity,
    );
    let label_style = Style::default()
        .fg(label_fg)
        .bg(theme.background)
        .add_modifier(Modifier::ITALIC | Modifier::BOLD);

    if collapsed {
        let fallback = if streaming { "Thinking" } else { "Thought" };
        let label = reasoning_title(content).unwrap_or_else(|| fallback.to_string());
        let label_span = Span::styled(label, label_style);
        let suffix_span = Span::styled(" (hidden — /thinking to show)", thought_style);
        return vec![ListItem::new(vec![Line::from(vec![
            reasoning_bar,
            reasoning_gap,
            label_span,
            suffix_span,
        ])])
        .style(Style::default().bg(theme.background))];
    }

    let label = if streaming { "Thinking:" } else { "Thought:" };
    let label_span = Span::styled(format!("{label} "), label_style);
    crate::ui::markdown::render_markdown_with_synth(
        content,
        width,
        resources.ps,
        resources.ts,
        theme,
        crate::ui::markdown::MarkdownRenderOptions {
            synth_theme: Some(resources.synth_theme),
            default_style: thought_style,
            suppress_style: false,
        },
    )
    .into_iter()
    .enumerate()
    .map(|(idx, line)| {
        let mut spans = vec![reasoning_bar.clone(), reasoning_gap.clone()];
        if idx == 0 {
            spans.push(label_span.clone());
        }
        spans.extend(line.spans);
        ListItem::new(vec![Line::from(spans)]).style(Style::default().bg(theme.background))
    })
    .collect()
}

fn mute_toward(fg: Color, bg: Color, alpha: f32) -> Color {
    let alpha = alpha.clamp(0.0, 1.0);
    let (fr, fg_, fb) = rgb_components(fg);
    let (br, bg_, bb) = rgb_components(bg);
    let mix = |f: u8, b: u8| -> u8 {
        (f as f32 * alpha + b as f32 * (1.0 - alpha))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    Color::Rgb(mix(fr, br), mix(fg_, bg_), mix(fb, bb))
}

fn rgb_components(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb(r, g, b) => (r, g, b),
        _ => (128, 128, 128),
    }
}

fn cached_assistant_thought_items(
    cache: &mut std::collections::HashMap<String, PartRenderCacheEntry>,
    cache_id: String,
    text: &str,
    state: ThoughtRenderState,
    width: usize,
    theme: &crate::ui::theme::Theme,
    resources: MarkdownResources<'_>,
) -> Vec<ListItem<'static>> {
    let key = PartRenderCacheKey {
        width,
        theme_mode: theme.mode,
        kind: PartRenderKind::Thought,
        collapsed: state.collapsed,
        streaming: state.streaming,
        content_hash: crate::model::content_fingerprint(text),
    };
    if let Some(entry) = cache.get(&cache_id) {
        if entry.key == key {
            return entry.items.clone();
        }
    }
    let items = assistant_thought_items(
        text,
        state.collapsed,
        state.streaming,
        width,
        theme,
        resources,
    );
    cache.insert(
        cache_id,
        PartRenderCacheEntry {
            key,
            items: items.clone(),
        },
    );
    items
}

pub(crate) fn render_messages(f: &mut Frame, app: &mut App, area: Rect) {
    if app.messages.is_empty() {
        app.scroll.list_state.select(None);
        app.scroll.total_visual_lines = 0;
        f.render_widget(
            Block::default().style(Style::default().bg(app.theme.theme.background)),
            area,
        );
        render_logo(f, area, &app.theme.theme);
        return;
    }

    let width = area.width.saturating_sub(3).max(1) as usize;
    let mut items: Vec<ListItem> = Vec::new();
    let mut tool_item_ranges: Vec<(String, usize, usize)> = Vec::new();
    let mut user_message_ranges: Vec<(String, usize, usize)> = Vec::new();
    let render_start_idx = app
        .messages
        .len()
        .saturating_sub(crate::app::RENDER_MESSAGE_TAIL_LIMIT);

    let ps = &app.theme.ps;
    let ts = &app.theme.ts;
    let theme = &app.theme.theme;
    let synth_theme = &app.theme.synth_theme;

    let current_agent_name = app.current_agent().name.clone();
    let agents = &app.agents;
    let show_timestamps = app.messages.show_timestamps;
    let catalog = &app.models.catalog;

    let queued_flags_full: Vec<bool> = app.messages.queued_flags().to_vec();
    let queued_flags: Vec<bool> = queued_flags_full
        .iter()
        .skip(render_start_idx)
        .copied()
        .collect();
    let last_assistant_index: Option<usize> = app.messages.last_assistant_index();

    let subagent_footer_should_show = app.sessions.sessions.current_is_child();
    let current_session_has_children = app
        .sessions
        .sessions
        .current
        .as_deref()
        .map(|cur| !app.sessions.sessions.children_of(cur).is_empty())
        .unwrap_or(false);

    for (tail_idx, msg) in app.messages.messages[render_start_idx..]
        .iter_mut()
        .enumerate()
    {
        let msg_idx = render_start_idx + tail_idx;
        let queued = queued_flags.get(tail_idx).copied().unwrap_or(false);

        if let Some(marker) = msg.compaction {
            items.push(ListItem::new(vec![Line::default()]));
            let title = if marker.auto {
                " Auto Compaction "
            } else {
                " Compaction "
            };
            items.push(ListItem::new(vec![compaction_divider_line(
                title, width, theme,
            )]));
            items.push(ListItem::new(vec![Line::default()]));
            continue;
        }

        let legacy_key = crate::model::LegacyCacheKey {
            version: msg.version(),
            width,
            theme_mode: theme.mode,
        };
        let cache_stale =
            msg.rendered_content_cache.is_none() || msg.legacy_cache_key != Some(legacy_key);
        let bg_color = match msg.sender {
            Sender::User => theme.background_panel,
            _ => theme.background,
        };
        let text_style = Style::default().fg(theme.text).bg(bg_color);
        let gap_style = Style::default().bg(bg_color);
        let block_style = Style::default().bg(bg_color);
        let user_bar_color = agent_color(
            theme,
            agents,
            msg.agent.as_deref().unwrap_or(&current_agent_name),
        );
        let user_bar_style = Style::default().fg(user_bar_color).bg(bg_color);
        let user_bar = Span::styled("┃", user_bar_style);
        let user_gap = Span::styled("  ", gap_style);

        let user_msg_start: Option<usize> = if msg.sender == Sender::User && msg.server_id.is_some()
        {
            Some(items.len())
        } else {
            None
        };

        if matches!(msg.sender, Sender::User | Sender::System) {
            let bar_for_open = match msg.sender {
                Sender::User => user_bar.clone(),
                _ => Span::styled("┃", Style::default().fg(theme.text_muted).bg(bg_color)),
            };
            items.push(
                ListItem::new(vec![Line::from(vec![bar_for_open, user_gap.clone()])])
                    .style(block_style),
            );
        }

        let mut emitted_any_part: bool = false;
        let mut margin_top = |items: &mut Vec<ListItem<'_>>| {
            if emitted_any_part {
                items.push(ListItem::new(vec![Line::default()]));
            }
            emitted_any_part = true;
        };

        if msg.sender == Sender::Assistant && !msg.parts.is_empty() {
            let parts = msg.parts.clone();
            let live_ids: std::collections::HashSet<&str> = msg
                .tool_calls
                .iter()
                .filter_map(|t| t.id.as_deref())
                .collect();
            msg.tool_render_cache
                .retain(|id, _| live_ids.contains(id.as_str()));

            let part_count = parts.len();
            msg.part_render_cache.retain(|key, _| {
                let body = key
                    .strip_prefix("ordered:text:")
                    .or_else(|| key.strip_prefix("ordered:thought:"));
                let Some(body) = body else {
                    return key.starts_with("legacy:thought:");
                };
                let part_idx_str = body.split(':').next().unwrap_or("");
                part_idx_str
                    .parse::<usize>()
                    .map(|i| i < part_count)
                    .unwrap_or(false)
            });

            let mut prev_was_inline_tool = false;
            for (part_idx, part) in parts.into_iter().enumerate() {
                match part {
                    HostMessagePart::Text(text) => {
                        let rendered = cached_assistant_text_items(
                            &mut msg.part_render_cache,
                            format!("ordered:text:{part_idx}"),
                            &text,
                            width,
                            theme,
                            MarkdownResources {
                                ps,
                                ts,
                                synth_theme,
                            },
                        );
                        if !rendered.is_empty() {
                            margin_top(&mut items);
                            items.extend(rendered);
                            prev_was_inline_tool = false;
                        }
                    }
                    HostMessagePart::Thought(text) => {
                        let rendered = cached_assistant_thought_items(
                            &mut msg.part_render_cache,
                            format!("ordered:thought:{part_idx}"),
                            &text,
                            ThoughtRenderState {
                                collapsed: msg.thoughts_collapsed,
                                streaming: msg.is_streaming,
                            },
                            width,
                            theme,
                            MarkdownResources {
                                ps,
                                ts,
                                synth_theme,
                            },
                        );
                        if !rendered.is_empty() {
                            margin_top(&mut items);
                            items.extend(rendered);
                            prev_was_inline_tool = false;
                        }
                    }
                    HostMessagePart::Tool(tool) => {
                        let inline = tool_is_inline(&tool.name);
                        if !(prev_was_inline_tool && inline) {
                            margin_top(&mut items);
                        }
                        let start_idx = items.len();
                        let lines = render_tool_call_v2_cached(
                            &tool,
                            theme,
                            width,
                            ps,
                            ts,
                            synth_theme,
                            &mut msg.tool_render_cache,
                        );
                        items.extend(lines);
                        let end_idx = items.len();
                        if let Some(id) = tool.id.as_deref() {
                            tool_item_ranges.push((id.to_string(), start_idx, end_idx));
                        }
                        prev_was_inline_tool = inline;
                    }
                }
            }
        } else {
            if msg.sender == Sender::Assistant
                && !msg.thoughts.is_empty()
                && !msg.thoughts_collapsed
            {
                let rendered = cached_assistant_thought_items(
                    &mut msg.part_render_cache,
                    "legacy:thought:expanded".to_string(),
                    &msg.thoughts,
                    ThoughtRenderState {
                        collapsed: false,
                        streaming: msg.is_streaming,
                    },
                    width,
                    theme,
                    MarkdownResources {
                        ps,
                        ts,
                        synth_theme,
                    },
                );
                if !rendered.is_empty() {
                    margin_top(&mut items);
                    items.extend(rendered);
                }
            } else if msg.sender == Sender::Assistant
                && !msg.thoughts.is_empty()
                && msg.thoughts_collapsed
            {
                let rendered = cached_assistant_thought_items(
                    &mut msg.part_render_cache,
                    "legacy:thought:collapsed".to_string(),
                    &msg.thoughts,
                    ThoughtRenderState {
                        collapsed: true,
                        streaming: msg.is_streaming,
                    },
                    width,
                    theme,
                    MarkdownResources {
                        ps,
                        ts,
                        synth_theme,
                    },
                );
                if !rendered.is_empty() {
                    margin_top(&mut items);
                    items.extend(rendered);
                }
            }

            if cache_stale {
                let render_style = match msg.sender {
                    Sender::Assistant => Style::default().fg(theme.text).bg(theme.background),
                    _ => text_style,
                };
                msg.rendered_content_cache = Some(crate::ui::markdown::render_markdown_with_synth(
                    &msg.content,
                    width,
                    ps,
                    ts,
                    theme,
                    crate::ui::markdown::MarkdownRenderOptions {
                        synth_theme: Some(synth_theme),
                        default_style: render_style,
                        suppress_style: false,
                    },
                ));
                msg.legacy_cache_key = Some(legacy_key);
            }
            if let Some(cache) = &msg.rendered_content_cache {
                if !cache.is_empty() && msg.sender == Sender::Assistant {
                    margin_top(&mut items);
                }
                for line in cache {
                    let (prefix_spans, row_bg): (Vec<Span<'_>>, Color) = match msg.sender {
                        Sender::Assistant => (
                            vec![Span::styled("   ", Style::default().bg(theme.background))],
                            theme.background,
                        ),
                        _ => (vec![user_bar.clone(), user_gap.clone()], bg_color),
                    };
                    let mut spans = prefix_spans;
                    spans.extend(line.spans.clone());
                    items.push(
                        ListItem::new(vec![Line::from(spans)]).style(Style::default().bg(row_bg)),
                    );
                }
            }

            if msg.sender == Sender::Assistant && !msg.tool_calls.is_empty() {
                let mut prev_was_inline_tool = false;
                let Message {
                    ref tool_calls,
                    ref mut tool_render_cache,
                    ..
                } = *msg;
                let live_ids: std::collections::HashSet<&str> =
                    tool_calls.iter().filter_map(|t| t.id.as_deref()).collect();
                tool_render_cache.retain(|id, _| live_ids.contains(id.as_str()));
                for tool in tool_calls.iter() {
                    let inline = tool_is_inline(&tool.name);
                    if !(prev_was_inline_tool && inline) {
                        margin_top(&mut items);
                    }
                    let start_idx = items.len();
                    let lines = render_tool_call_v2_cached(
                        tool,
                        theme,
                        width,
                        ps,
                        ts,
                        synth_theme,
                        tool_render_cache,
                    );
                    items.extend(lines);
                    let end_idx = items.len();
                    if let Some(id) = tool.id.as_deref() {
                        tool_item_ranges.push((id.to_string(), start_idx, end_idx));
                    }
                    prev_was_inline_tool = inline;
                }
            }
        }

        if msg.sender == Sender::Assistant && !msg.interrupted {
            if let Some(err) = msg.error.as_deref() {
                if !err.is_empty() {
                    margin_top(&mut items);
                    let panel_bg = theme.background_panel;
                    let err_bar = Span::styled("┃", Style::default().fg(theme.error).bg(panel_bg));
                    let err_gap = Span::styled("  ", Style::default().bg(panel_bg));
                    let err_text_style = Style::default().fg(theme.text_muted).bg(panel_bg);
                    let panel_block_style = Style::default().bg(panel_bg);

                    items.push(
                        ListItem::new(vec![Line::from(vec![err_bar.clone(), err_gap.clone()])])
                            .style(panel_block_style),
                    );

                    let inner_w = width.saturating_sub(3).max(1);
                    let wrap_opts = textwrap::Options::new(inner_w).break_words(true);
                    for paragraph in err.split('\n') {
                        let wrapped = if paragraph.is_empty() {
                            vec![std::borrow::Cow::Borrowed("")]
                        } else {
                            textwrap::wrap(paragraph, &wrap_opts)
                        };
                        for line in wrapped {
                            items.push(
                                ListItem::new(vec![Line::from(vec![
                                    err_bar.clone(),
                                    err_gap.clone(),
                                    Span::styled(line.into_owned(), err_text_style),
                                ])])
                                .style(panel_block_style),
                            );
                        }
                    }

                    items.push(
                        ListItem::new(vec![Line::from(vec![err_bar.clone(), err_gap.clone()])])
                            .style(panel_block_style),
                    );
                }
            }
        }

        let is_last_assistant = Some(msg_idx) == last_assistant_index;
        let user_system_footer = if msg.sender == Sender::User && queued {
            let badge_bg = agent_color(theme, agents, &current_agent_name);
            let badge_fg = theme.selected_list_item_text;
            Some(Line::from(vec![
                user_bar.clone(),
                user_gap.clone(),
                Span::styled(
                    " QUEUED ",
                    Style::default()
                        .fg(badge_fg)
                        .bg(badge_bg)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
        } else if msg.sender == Sender::User {
            if show_timestamps {
                Some(Line::from(vec![
                    user_bar.clone(),
                    user_gap.clone(),
                    Span::styled(
                        msg.timestamp.clone(),
                        text_style.add_modifier(Modifier::DIM),
                    ),
                ]))
            } else {
                None
            }
        } else if msg.sender == Sender::System {
            let footer_text = if show_timestamps {
                format!("{}, {}", msg.sender.label(), msg.timestamp)
            } else {
                msg.sender.label().to_string()
            };
            Some(Line::from(vec![
                Span::styled("┃", Style::default().fg(theme.text_muted).bg(bg_color)),
                user_gap.clone(),
                Span::styled(footer_text, text_style.add_modifier(Modifier::DIM)),
            ]))
        } else {
            None
        };
        match msg.sender {
            Sender::User => {
                if let Some(user_system_footer) = user_system_footer {
                    items.push(ListItem::new(vec![user_system_footer]).style(block_style));
                }
                items.push(
                    ListItem::new(vec![Line::from(vec![user_bar.clone(), user_gap.clone()])])
                        .style(block_style),
                );
            }
            Sender::Assistant => {}
            Sender::System => {
                if let Some(user_system_footer) = user_system_footer {
                    items.push(ListItem::new(vec![user_system_footer]).style(block_style));
                }
            }
        }

        if msg.sender == Sender::Assistant && (is_last_assistant || msg.interrupted) {
            margin_top(&mut items);
            items.push(ListItem::new(vec![assistant_footer_line(
                msg,
                show_timestamps,
                text_style,
                theme,
                agents,
                bg_color,
                catalog,
            )]));
        }

        if msg.sender == Sender::Assistant
            && msg.tool_calls.iter().any(|t| t.name == "task")
            && !subagent_footer_should_show
            && current_session_has_children
        {
            let key_style = Style::default().fg(theme.text).bg(theme.background);
            let muted = Style::default().fg(theme.text_muted).bg(theme.background);
            items.push(ListItem::new(vec![Line::default()]));
            items.push(
                ListItem::new(vec![Line::from(vec![
                    Span::styled("   ", Style::default().bg(theme.background)),
                    Span::styled("ctrl+x down", key_style.add_modifier(Modifier::BOLD)),
                    Span::styled(" view subagents", muted),
                ])])
                .style(Style::default().bg(theme.background)),
            );
        }

        if let (Some(start), Some(sid)) = (user_msg_start, msg.server_id.clone()) {
            let end = items.len();
            if end > start {
                user_message_ranges.push((sid, start, end));
            }
        }

        items.push(ListItem::new(vec![Line::default()]));
    }

    app.scroll.total_visual_lines = items.len();
    app.scroll.last_messages_viewport_rows = area.height as usize;
    app.messages.last_messages_rect = Some(area);
    if app.scroll.scroll_stick_to_bottom && !items.is_empty() {
        let viewport = area.height as usize;
        let target = items.len().saturating_sub(viewport.max(1));
        app.scroll.list_state = ratatui::widgets::ListState::default().with_offset(target);
    }

    let total_items = items.len();
    let list = List::new(items);
    f.render_stateful_widget(list, area, &mut app.scroll.list_state);

    let offset = app.scroll.list_state.offset();
    let viewport_rows = area.height as usize;
    let mut rects: Vec<(String, ratatui::layout::Rect)> = Vec::new();
    for (id, start, end) in &tool_item_ranges {
        let vis_start = (*start).max(offset);
        let vis_end = (*end).min(offset + viewport_rows);
        if vis_start >= vis_end {
            continue;
        }
        let y_off = (vis_start - offset) as u16;
        let h = (vis_end - vis_start) as u16;
        if h == 0 {
            continue;
        }
        let y = area.y.saturating_add(y_off);
        let h = h.min(area.height.saturating_sub(y_off));
        if h == 0 {
            continue;
        }
        rects.push((
            id.clone(),
            ratatui::layout::Rect {
                x: area.x,
                y,
                width: area.width,
                height: h,
            },
        ));
    }
    app.messages.tool_block_rects = rects;

    let mut user_rects: Vec<(String, ratatui::layout::Rect)> = Vec::new();
    for (id, start, end) in &user_message_ranges {
        let vis_start = (*start).max(offset);
        let vis_end = (*end).min(offset + viewport_rows);
        if vis_start >= vis_end {
            continue;
        }
        let y_off = (vis_start - offset) as u16;
        let h = (vis_end - vis_start) as u16;
        if h == 0 {
            continue;
        }
        let y = area.y.saturating_add(y_off);
        let h = h.min(area.height.saturating_sub(y_off));
        if h == 0 {
            continue;
        }
        user_rects.push((
            id.clone(),
            ratatui::layout::Rect {
                x: area.x,
                y,
                width: area.width,
                height: h,
            },
        ));
    }
    app.messages.user_message_rects = user_rects;
    let _ = total_items;
}

#[cfg(test)]
mod reasoning_title_tests {
    use super::reasoning_title;

    #[test]
    fn extracts_bolded_lead_line() {
        assert_eq!(
            reasoning_title("**Inspecting PR workflow**\n\nbody"),
            Some("Inspecting PR workflow".to_string()),
        );
    }

    #[test]
    fn returns_none_for_plain_text() {
        assert_eq!(reasoning_title("plain reasoning"), None);
    }

    #[test]
    fn returns_none_when_only_one_bold_marker() {
        assert_eq!(reasoning_title("**unterminated"), None);
    }

    #[test]
    fn returns_none_when_title_contains_newline() {
        assert_eq!(reasoning_title("**a\nb**"), None);
    }

    #[test]
    fn returns_none_for_empty_title() {
        assert_eq!(reasoning_title("****"), None);
    }

    #[test]
    fn tolerates_leading_whitespace() {
        assert_eq!(
            reasoning_title("   \n  **Title**\nrest"),
            Some("Title".to_string()),
        );
    }
}
