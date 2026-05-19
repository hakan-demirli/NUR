use ratatui::prelude::*;
use ratatui::widgets::ListItem;

use crate::action::ToolCall;
use crate::model::{ToolHeaderKind, ToolHeaderSlot, ToolRenderCacheEntry};
use crate::ui::diff::render_diff_block_with_width;
use crate::ui::path::normalize_path;
use crate::ui::primitives::bar_gap1;
use crate::ui::spinner::{spinner_frame, tool_uses_running_spinner};
use crate::ui::syntax::{build_syntax_ctx, SyntaxResources};
use crate::ui::theme::Theme;

pub(crate) mod apply_patch;
pub(crate) mod cache;
pub(crate) mod error;

use apply_patch::render_apply_patch_blocks;
use error::push_tool_error_lines;

pub(crate) fn tool_is_inline(tool: &ToolCall) -> bool {
    use crate::action::ToolStatus;
    let running = matches!(tool.status, ToolStatus::Running | ToolStatus::Pending);
    let has_block_body = match tool.name.as_str() {
        "bash" => !tool.output.trim().is_empty(),
        "edit" => tool.diff.is_some() && !running,
        "write" => tool.diff.is_some() && !running,
        "todowrite" => !tool.todos.is_empty() && !running,
        "apply_patch" => !tool.patches.is_empty(),
        "question" => !tool.answers.is_empty(),
        "glob" | "read" | "grep" | "webfetch" | "websearch" | "skill" | "task" => false,
        _ => !tool.output.trim().is_empty(),
    };
    !has_block_body
}

fn title_is_bare(title: &str, tool_name: &str) -> bool {
    let t = title.trim();
    if t.is_empty() {
        return true;
    }
    if t.eq_ignore_ascii_case(tool_name) {
        return true;
    }
    let pretty = match tool_name {
        "bash" => "Bash",
        "read" => "Read",
        "write" => "Write",
        "edit" => "Edit",
        "glob" => "Glob",
        "grep" => "Grep",
        "webfetch" => "WebFetch",
        "websearch" => "WebSearch",
        "todowrite" => "TodoWrite",
        "apply_patch" => "Patch",
        "skill" => "Skill",
        "task" => "Task",
        "question" => "Question",
        _ => "",
    };
    if !pretty.is_empty() && t.eq_ignore_ascii_case(pretty) {
        return true;
    }
    false
}

pub(crate) fn inline_title_for(tool: &ToolCall, running: bool) -> String {
    let bare = title_is_bare(&tool.title, &tool.name);
    let path = tool
        .file_path
        .as_deref()
        .map(normalize_path)
        .unwrap_or_default();
    let cmd = tool
        .command
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .to_string();
    match tool.name.as_str() {
        "bash" => {
            if !cmd.is_empty() {
                cmd
            } else if !bare {
                tool.title.clone()
            } else {
                "Writing command...".to_string()
            }
        }
        "read" => {
            if !bare {
                tool.title.clone()
            } else if !path.is_empty() {
                format!("Read {path}")
            } else {
                "Reading file...".to_string()
            }
        }
        "write" => {
            if !path.is_empty() {
                format!("Write {path}")
            } else if !bare {
                tool.title.clone()
            } else {
                "Preparing write...".to_string()
            }
        }
        "edit" => {
            if !path.is_empty() {
                format!("Edit {path}")
            } else if !bare {
                tool.title.clone()
            } else {
                "Preparing edit...".to_string()
            }
        }
        "apply_patch" => {
            if tool.patches.is_empty() {
                "Preparing patch...".to_string()
            } else if !bare {
                tool.title.clone()
            } else {
                "Patch".to_string()
            }
        }
        "todowrite" => {
            if running || tool.todos.is_empty() {
                "Updating todos...".to_string()
            } else if !bare {
                tool.title.clone()
            } else {
                "Todos".to_string()
            }
        }
        "glob" => {
            if !bare {
                tool.title.clone()
            } else if running {
                "Finding files...".to_string()
            } else {
                "Glob".to_string()
            }
        }
        "grep" => {
            if !bare {
                tool.title.clone()
            } else if running {
                "Searching content...".to_string()
            } else {
                "Grep".to_string()
            }
        }
        "webfetch" => {
            if !bare {
                tool.title.clone()
            } else if running {
                "Fetching from the web...".to_string()
            } else {
                "WebFetch".to_string()
            }
        }
        "websearch" => {
            if !bare {
                tool.title.clone()
            } else if running {
                "Searching web...".to_string()
            } else {
                "WebSearch".to_string()
            }
        }
        "skill" => {
            if !bare {
                tool.title.clone()
            } else if running {
                "Loading skill...".to_string()
            } else {
                "Skill".to_string()
            }
        }
        "task" => {
            if !bare {
                tool.title.clone()
            } else if running {
                "Delegating...".to_string()
            } else {
                "Task".to_string()
            }
        }
        "question" => {
            if !bare {
                tool.title.clone()
            } else if running {
                "Asking questions...".to_string()
            } else {
                "Question".to_string()
            }
        }
        _ => tool.title.clone(),
    }
}

pub(crate) fn render_tool_call_v2(
    tool: &ToolCall,
    theme: &Theme,
    width: usize,
    ps: &syntect::parsing::SyntaxSet,
    ts: &syntect::highlighting::ThemeSet,
    synth_theme: &syntect::highlighting::Theme,
) -> Vec<ListItem<'static>> {
    if tool.name == "apply_patch" && !tool.patches.is_empty() {
        return render_apply_patch_blocks(tool, theme, width, ps, ts, synth_theme);
    }
    let has_block_body = !tool_is_inline(tool);
    let (bar_span, gap_span, row_bg) = if has_block_body {
        let bg = theme.background_panel;
        let (b, g) = bar_gap1(theme.background, bg);
        (b, g, bg)
    } else {
        let bg = theme.background;
        (
            Span::styled("   ", Style::default().bg(bg)),
            Span::styled("", Style::default().bg(bg)),
            bg,
        )
    };
    let lines = render_tool_call(
        tool,
        theme,
        bar_span.clone(),
        gap_span.clone(),
        width,
        SyntaxResources {
            ps,
            ts,
            synth_theme,
        },
    );

    let row_style = Style::default().bg(row_bg);
    let pad_item = || -> ListItem<'static> {
        ListItem::new(vec![Line::from(vec![bar_span.clone(), gap_span.clone()])]).style(row_style)
    };

    let mut out: Vec<ListItem<'static>> = Vec::with_capacity(lines.len() + 2);
    if has_block_body {
        out.push(pad_item());
    }
    out.extend(
        lines
            .into_iter()
            .map(|l| ListItem::new(vec![l]).style(row_style)),
    );
    if has_block_body {
        out.push(pad_item());
    }
    out
}

pub(crate) fn build_spinner_slot_for(
    tool: &ToolCall,
    theme: &Theme,
    _width: usize,
) -> Option<ToolHeaderSlot> {
    use crate::action::ToolStatus;
    let has_block_body = !tool_is_inline(tool);
    let (bar_str, bar_fg, bar_bg, gap_str, gap_bg, row_bg) = if has_block_body {
        let bg = theme.background_panel;
        (
            "┃".to_string(),
            theme.background,
            bg,
            " ".to_string(),
            bg,
            bg,
        )
    } else {
        let bg = theme.background;
        ("   ".to_string(), bg, bg, String::new(), bg, bg)
    };
    let fg = match tool.status {
        ToolStatus::Error => theme.error,
        ToolStatus::Completed => theme.text_muted,
        _ => theme.text,
    };
    let title = tool.title.clone();
    let kind = if has_block_body {
        ToolHeaderKind::Block
    } else {
        ToolHeaderKind::Inline
    };
    Some(ToolHeaderSlot {
        bar_fg,
        bar_bg,
        gap_str,
        gap_bg,
        bar_str,
        row_bg,
        body_fg: fg,
        title_fg: theme.text,
        title,
        kind,
    })
}

pub(crate) fn build_spinner_header_item(slot: &ToolHeaderSlot, spin: &str) -> ListItem<'static> {
    let bar = Span::styled(
        slot.bar_str.clone(),
        Style::default().fg(slot.bar_fg).bg(slot.bar_bg),
    );
    let gap = Span::styled(slot.gap_str.clone(), Style::default().bg(slot.gap_bg));
    let line = match slot.kind {
        ToolHeaderKind::Inline => {
            let body_style = Style::default().fg(slot.body_fg);
            Line::from(vec![
                bar,
                gap,
                Span::styled(format!("{spin} "), body_style),
                Span::styled(slot.title.clone(), body_style),
            ])
        }
        ToolHeaderKind::Block => {
            let title_style = Style::default().fg(slot.title_fg);
            Line::from(vec![
                bar,
                gap,
                Span::styled(format!("{spin} {}", slot.title), title_style),
            ])
        }
    };
    ListItem::new(vec![line]).style(Style::default().bg(slot.row_bg))
}

pub(crate) fn render_tool_call_v2_cached(
    tool: &ToolCall,
    theme: &Theme,
    width: usize,
    ps: &syntect::parsing::SyntaxSet,
    ts: &syntect::highlighting::ThemeSet,
    synth_theme: &syntect::highlighting::Theme,
    cache: &mut std::collections::HashMap<String, ToolRenderCacheEntry>,
) -> Vec<ListItem<'static>> {
    use crate::action::ToolStatus;
    let id_opt = tool.id.as_deref().filter(|s| !s.is_empty());
    let key = cache::compute_tool_cache_key(tool, width, theme.mode);
    let running = matches!(tool.status, ToolStatus::Running | ToolStatus::Pending);
    let animated = running && tool_uses_running_spinner(&tool.name);

    if let Some(id) = id_opt {
        if let Some(entry) = cache.get(id) {
            if entry.key == key {
                if let Some(slot) = entry.spinner_slot.as_ref() {
                    let mut items = entry.items.clone();
                    let header_idx = match slot.kind {
                        ToolHeaderKind::Block => 1,
                        ToolHeaderKind::Inline => 0,
                    };
                    if header_idx < items.len() {
                        items[header_idx] = build_spinner_header_item(slot, spinner_frame());
                    }
                    return items;
                }
                return entry.items.clone();
            }
        }
    }
    let items = render_tool_call_v2(tool, theme, width, ps, ts, synth_theme);
    let spinner_slot = if animated {
        build_spinner_slot_for(tool, theme, width)
    } else {
        None
    };
    if let Some(id) = id_opt {
        cache.insert(
            id.to_string(),
            ToolRenderCacheEntry {
                key,
                items: items.clone(),
                spinner_slot,
            },
        );
    }
    items
}

fn render_tool_call<'a>(
    tool: &ToolCall,
    theme: &Theme,
    bar: Span<'a>,
    gap: Span<'a>,
    width: usize,
    syntax_resources: SyntaxResources<'_>,
) -> Vec<Line<'a>> {
    use crate::action::ToolStatus;
    let fg = match tool.status {
        ToolStatus::Error => theme.error,
        ToolStatus::Completed => theme.text_muted,
        _ => theme.text,
    };
    let muted = Style::default().fg(theme.text_muted);
    let body_style = Style::default().fg(fg);
    let running = matches!(tool.status, ToolStatus::Running | ToolStatus::Pending);

    if matches!(tool.name.as_str(), "edit" | "write") && !running {
        if let Some(diff_text) = tool.diff.as_deref() {
            let title = match tool.name.as_str() {
                "edit" => format!(
                    "← Edit {}",
                    normalize_path(tool.file_path.as_deref().unwrap_or(""))
                ),
                "write" => format!(
                    "# Wrote {}",
                    normalize_path(tool.file_path.as_deref().unwrap_or(""))
                ),
                _ => "# Edit".to_string(),
            };
            let title_style = Style::default().fg(theme.text);
            let mut out: Vec<Line<'a>> = Vec::new();
            out.push(Line::from(vec![
                bar.clone(),
                gap.clone(),
                Span::styled(title, title_style),
            ]));
            out.push(Line::from(vec![bar.clone(), gap.clone()]));
            let edit_path = tool.file_path.as_deref().unwrap_or("");
            let syntax = build_syntax_ctx(
                syntax_resources.ps,
                syntax_resources.ts,
                theme,
                syntax_resources.synth_theme,
                edit_path,
            );
            out.extend(render_diff_block_with_width(
                diff_text,
                theme,
                bar.clone(),
                gap.clone(),
                width.saturating_sub(2) as u16,
                syntax,
            ));
            if let Some(err) = &tool.error {
                push_tool_error_lines(&mut out, err, theme, bar.clone(), gap.clone(), width);
            }
            return out;
        }
    }

    if tool.name == "question" && !tool.answers.is_empty() {
        let mut out: Vec<Line<'a>> = Vec::new();
        out.push(Line::from(vec![
            bar.clone(),
            gap.clone(),
            Span::styled("# Questions".to_string(), muted),
        ]));
        let count = tool.questions.len().max(tool.answers.len());
        for i in 0..count {
            if i > 0 {
                out.push(Line::from(vec![bar.clone(), gap.clone()]));
            }
            let question_text = tool.questions.get(i).map(|q| q.text.as_str()).unwrap_or("");
            let answer_text = match tool.answers.get(i) {
                None => "(no answer)".to_string(),
                Some(parts) => {
                    let cleaned: Vec<&str> = parts
                        .iter()
                        .filter(|s| !s.is_empty())
                        .map(|s| s.as_str())
                        .collect();
                    if cleaned.is_empty() {
                        "(no answer)".to_string()
                    } else {
                        cleaned.join(", ")
                    }
                }
            };
            out.push(Line::from(vec![
                bar.clone(),
                gap.clone(),
                Span::styled(format!("Q: {question_text}"), muted),
            ]));
            out.push(Line::from(vec![
                bar.clone(),
                gap.clone(),
                Span::styled(format!("A: {answer_text}"), body_style),
            ]));
        }
        if let Some(err) = &tool.error {
            push_tool_error_lines(&mut out, err, theme, bar.clone(), gap.clone(), width);
        }
        return out;
    }

    if tool.name == "todowrite" && !tool.todos.is_empty() && !running {
        let mut out: Vec<Line<'a>> = Vec::new();
        out.push(Line::from(vec![
            bar.clone(),
            gap.clone(),
            Span::styled("# Todos".to_string(), muted),
        ]));
        for todo in &tool.todos {
            let (glyph, todo_fg) = match todo.status.as_str() {
                "completed" => ("✓", theme.text_muted),
                "in_progress" => ("•", theme.warning),
                _ => (" ", theme.text_muted),
            };
            let todo_style = Style::default().fg(todo_fg);
            out.push(Line::from(vec![
                bar.clone(),
                gap.clone(),
                Span::styled(format!("[{glyph}] "), todo_style),
                Span::styled(todo.content.clone(), todo_style),
            ]));
        }
        if let Some(err) = &tool.error {
            push_tool_error_lines(&mut out, err, theme, bar.clone(), gap.clone(), width);
        }
        return out;
    }

    let mut tool_title_override: Option<String> = None;
    if tool.name == "apply_patch" && tool.patches.is_empty() {
        tool_title_override = Some("Preparing patch...".to_string());
    } else if tool.name == "todowrite" {
        tool_title_override = Some(if running {
            "Updating todos...".to_string()
        } else {
            "Todos".to_string()
        });
    } else if matches!(tool.name.as_str(), "edit" | "write") {
        let verb = if tool.name == "write" {
            "Write"
        } else {
            "Edit"
        };
        let path_display = tool
            .file_path
            .as_deref()
            .map(normalize_path)
            .unwrap_or_default();
        tool_title_override = Some(if path_display.is_empty() {
            verb.to_string()
        } else {
            format!("{verb} {path_display}")
        });
    }
    let effective_title = tool_title_override
        .clone()
        .unwrap_or_else(|| tool.title.clone());
    let spin = spinner_frame();

    let icon = match tool.name.as_str() {
        "bash" => "$",
        "glob" => "✱",
        "read" => "→",
        "grep" => "✱",
        "webfetch" => "%",
        "websearch" => "◈",
        "edit" | "write" => "←",
        "todowrite" => "⚙",
        "skill" => "→",
        "task" => "│",
        "question" => "→",
        _ => "⚙",
    };
    let leading: &str = if tool.name == "apply_patch" && tool.patches.is_empty() {
        "~"
    } else if running {
        if tool_uses_running_spinner(&tool.name) {
            spin
        } else {
            "~"
        }
    } else {
        icon
    };

    let always_inline = matches!(
        tool.name.as_str(),
        "glob" | "read" | "grep" | "webfetch" | "websearch" | "skill" | "task" | "question"
    );
    let suppress_output = tool.name == "todowrite" || always_inline;
    let has_output = !suppress_output && !tool.output.trim().is_empty();
    let has_command = !tool
        .command
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty();

    if !has_output && !has_command {
        let inline_label = inline_title_for(tool, running);
        let row = Line::from(vec![
            bar.clone(),
            gap.clone(),
            Span::styled(format!("{leading} "), body_style),
            Span::styled(inline_label, body_style),
        ]);
        let mut out = vec![row];
        if tool.name == "read" && !tool.loaded.is_empty() {
            for path in &tool.loaded {
                out.push(Line::from(vec![
                    bar.clone(),
                    gap.clone(),
                    Span::styled(format!("   ↳ Loaded {}", normalize_path(path)), muted),
                ]));
            }
        }
        if tool.name == "task" && running {
            if let Some(child) = tool.current_child.as_ref() {
                let label = format_child_tool_label(child);
                if !label.is_empty() {
                    out.push(Line::from(vec![
                        bar.clone(),
                        gap.clone(),
                        Span::styled(format!("   ↳ {label}"), muted),
                    ]));
                }
            } else if tool.child_tool_count > 0 {
                out.push(Line::from(vec![
                    bar.clone(),
                    gap.clone(),
                    Span::styled(format!("   ↳ {} toolcalls", tool.child_tool_count), muted),
                ]));
            }
        }
        if tool.name == "task" && !running && tool.child_tool_count > 0 {
            let mut summary = format!("   └ {} toolcalls", tool.child_tool_count);
            if let (Some(start), Some(end)) = (tool.started_at_ms, tool.completed_at_ms) {
                let dur = end.saturating_sub(start);
                summary.push_str(" · ");
                summary.push_str(&format_locale_duration_ms(dur));
            }
            out.push(Line::from(vec![
                bar.clone(),
                gap.clone(),
                Span::styled(summary, muted),
            ]));
        }
        if let Some(err) = &tool.error {
            push_tool_error_lines(&mut out, err, theme, bar.clone(), gap.clone(), width);
        }
        return out;
    }

    let mut out: Vec<Line<'a>> = Vec::new();

    let header_prefix = if running {
        format!("{spin} ")
    } else {
        "# ".to_string()
    };
    let title_style = Style::default().fg(theme.text);
    out.push(Line::from(vec![
        bar.clone(),
        gap.clone(),
        Span::styled(format!("{header_prefix}{effective_title}"), title_style),
    ]));

    let cmd_style = Style::default().fg(theme.text);
    if let Some(cmd) = &tool.command {
        if !cmd.trim().is_empty() {
            out.push(Line::from(vec![bar.clone(), gap.clone()]));
            out.push(Line::from(vec![
                bar.clone(),
                gap.clone(),
                Span::styled(format!("$ {cmd}"), cmd_style),
            ]));
        }
    }

    const MAX_EXPANDED_LINES: usize = 256;
    let max_lines = if tool.name == "bash" { 10 } else { 3 };
    let body_lines: Vec<&str> = tool.output.lines().collect();
    let overflow_collapsed = body_lines.len() > max_lines;
    let expanded_tail_start = body_lines.len().saturating_sub(MAX_EXPANDED_LINES);
    let expanded_overflow = body_lines.len() > MAX_EXPANDED_LINES;
    let shown: &[&str] = if overflow_collapsed && !tool.expanded {
        &body_lines[..max_lines]
    } else if tool.expanded && expanded_overflow {
        &body_lines[expanded_tail_start..]
    } else {
        &body_lines
    };
    if !shown.is_empty() {
        out.push(Line::from(vec![bar.clone(), gap.clone()]));
    }
    for line in shown {
        out.push(Line::from(vec![
            bar.clone(),
            gap.clone(),
            Span::styled(line.to_string(), body_style),
        ]));
    }
    if overflow_collapsed && !tool.expanded {
        out.push(Line::from(vec![
            bar.clone(),
            gap.clone(),
            Span::styled("…".to_string(), body_style),
        ]));
        out.push(Line::from(vec![bar.clone(), gap.clone()]));
        out.push(Line::from(vec![
            bar.clone(),
            gap.clone(),
            Span::styled("Click to expand".to_string(), muted),
        ]));
    } else if overflow_collapsed {
        out.push(Line::from(vec![bar.clone(), gap.clone()]));
        out.push(Line::from(vec![
            bar.clone(),
            gap.clone(),
            Span::styled("Click to collapse".to_string(), muted),
        ]));
    }

    if let Some(err) = &tool.error {
        push_tool_error_lines(&mut out, err, theme, bar.clone(), gap.clone(), width);
    }

    out
}

fn format_locale_duration_ms(input: u128) -> String {
    if input < 1_000 {
        return format!("{input}ms");
    }
    if input < 60_000 {
        let secs = (input as f64) / 1000.0;
        return format!("{secs:.1}s");
    }
    if input < 3_600_000 {
        let minutes = input / 60_000;
        let seconds = (input % 60_000) / 1_000;
        return format!("{minutes}m {seconds}s");
    }
    if input < 86_400_000 {
        let hours = input / 3_600_000;
        let minutes = (input % 3_600_000) / 60_000;
        return format!("{hours}h {minutes}m");
    }
    let hours = input / 3_600_000;
    let days = (input % 3_600_000) / 86_400_000;
    format!("{days}d {hours}h")
}

fn format_child_tool_label(child: &crate::action::ChildToolRef) -> String {
    fn verb(name: &str) -> &'static str {
        match name {
            "read" => "Read",
            "bash" => "Bash",
            "edit" => "Edit",
            "write" => "Write",
            "glob" => "Glob",
            "grep" => "Grep",
            "webfetch" => "Webfetch",
            "websearch" => "Websearch",
            "todowrite" => "Update todos",
            "task" => "Task",
            "skill" => "Skill",
            "question" => "Ask",
            "apply_patch" => "Apply patch",
            _ => "Run",
        }
    }
    let v = verb(&child.name);
    let target = match child.name.as_str() {
        "read" | "edit" | "write" => child.file_path.as_deref().map(normalize_path),
        "bash" => child
            .command
            .as_deref()
            .map(|c| c.split('\n').next().unwrap_or(c).to_string()),
        _ => None,
    };
    match target {
        Some(t) if !t.is_empty() => {
            let trimmed = if t.chars().count() > 60 {
                let mut s: String = t.chars().take(57).collect();
                s.push('…');
                s
            } else {
                t
            };
            format!("{v} {trimmed}")
        }
        _ => {
            if !child.title.is_empty() {
                child.title.clone()
            } else {
                v.to_string()
            }
        }
    }
}
