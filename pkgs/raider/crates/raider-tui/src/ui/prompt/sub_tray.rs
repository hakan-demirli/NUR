use ratatui::prelude::*;
use ratatui::widgets::{Block, Paragraph};

use unicode_width::UnicodeWidthStr;

use crate::app::App;
use crate::session::SessionStatus;
use crate::ui::agent::agent_color;
use crate::ui::path::truncate_path_right;
use crate::ui::wipe_spinner::wipe_frame_now;

pub(crate) fn render_sub_tray(f: &mut Frame, app: &App, area: Rect) {
    if area.width < 4 {
        return;
    }
    let theme = &app.theme.theme;

    f.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );

    let inset_left = 3u16;
    let inset_right = 2u16;
    if area.width <= inset_left + inset_right {
        return;
    }
    let inner_x = area.x + inset_left;
    let inner_w = area.width - inset_left - inset_right;
    let y = area.y;

    let current_status = app.sessions.current_status();
    let retry = match current_status {
        Some(SessionStatus::Retry {
            attempt,
            message,
            next,
        }) => Some((*attempt, message.as_deref(), *next)),
        _ => None,
    };
    let working = retry.is_some() || app.sessions.current_busy() || app.prompt.prompt_info.busy;

    let mut left_spans: Vec<Span> = Vec::new();
    if working {
        let frame = wipe_frame_now();
        let agent_tint = agent_color(theme, &app.agents, &app.current_agent().name);
        left_spans.push(Span::styled(
            frame,
            Style::default().fg(agent_tint).bg(theme.background),
        ));
        if let Some((attempt, message, next)) = retry {
            left_spans.push(Span::styled(" ", Style::default().bg(theme.background)));
            left_spans.push(Span::styled(
                retry_status_text(message, attempt, next, now_ms()),
                Style::default().fg(theme.error).bg(theme.background),
            ));
        } else {
            left_spans.push(Span::styled("  ", Style::default().bg(theme.background)));
            push_esc_interrupt(&mut left_spans, theme, theme.background);
        }
    }

    let mut right_spans: Vec<Span> = Vec::new();
    if retry.is_some() {
        push_esc_interrupt(&mut right_spans, theme, theme.background);
    } else if let Some(usage) = app.prompt.prompt_info.usage.as_deref() {
        right_spans.push(Span::styled(
            usage.to_string(),
            Style::default().fg(theme.text_muted).bg(theme.background),
        ));
    } else {
        right_spans.push(Span::styled(
            "tab",
            Style::default()
                .fg(theme.text)
                .bg(theme.background)
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled(
            " agents",
            Style::default().fg(theme.text_muted).bg(theme.background),
        ));
    }
    if retry.is_none() {
        right_spans.push(Span::styled("  ", Style::default().bg(theme.background)));
        right_spans.push(Span::styled(
            "ctrl+p",
            Style::default()
                .fg(theme.text)
                .bg(theme.background)
                .add_modifier(Modifier::BOLD),
        ));
        right_spans.push(Span::styled(
            " commands",
            Style::default().fg(theme.text_muted).bg(theme.background),
        ));
    }

    let right_w: u16 = right_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();
    let left_w: u16 = left_spans
        .iter()
        .map(|s| s.content.chars().count() as u16)
        .sum();

    if !left_spans.is_empty() {
        let max_left = inner_w.saturating_sub(right_w.saturating_add(1));
        let lw = left_w.min(max_left.max(1));
        let lrect = Rect::new(inner_x, y, lw, 1);
        f.render_widget(
            Paragraph::new(Line::from(left_spans)).style(Style::default().bg(theme.background)),
            lrect,
        );
    }
    if right_w + 1 < inner_w {
        let rx = inner_x + inner_w - right_w;
        let rrect = Rect::new(rx, y, right_w, 1);
        f.render_widget(
            Paragraph::new(Line::from(right_spans)).style(Style::default().bg(theme.background)),
            rrect,
        );
    }
}

fn push_esc_interrupt(spans: &mut Vec<Span<'static>>, theme: &crate::ui::theme::Theme, bg: Color) {
    spans.push(Span::styled(
        "esc",
        Style::default()
            .fg(theme.text)
            .bg(bg)
            .add_modifier(Modifier::BOLD),
    ));
    spans.push(Span::styled(
        " interrupt",
        Style::default().fg(theme.text_muted).bg(bg),
    ));
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

pub(crate) fn retry_status_text(
    message: Option<&str>,
    attempt: Option<u32>,
    next: Option<i64>,
    now_ms: i64,
) -> String {
    let raw = message.unwrap_or("Retrying request");
    let lower = raw.to_ascii_lowercase();
    let mut base = if lower.contains("exceeded your current quota") && lower.contains("gemini") {
        "gemini is way too hot right now".to_string()
    } else if raw.width() > 80 {
        let mut out = truncate_path_right(raw, 80);
        out.push_str("...");
        out
    } else {
        raw.to_string()
    };
    if raw.width() > 120 {
        base.push_str(" (click to expand)");
    }

    let seconds = next
        .map(|next| (((next - now_ms) as f64) / 1000.0).round() as i64)
        .unwrap_or(0);
    let duration = format_retry_duration(seconds);
    let attempt = attempt.unwrap_or(0);
    if duration.is_empty() {
        format!("{base} [retrying attempt #{attempt}]")
    } else {
        format!("{base} [retrying in {duration} attempt #{attempt}]")
    }
}

fn format_retry_duration(secs: i64) -> String {
    if secs <= 0 {
        return String::new();
    }
    if secs < 60 {
        return format!("{secs}s");
    }
    if secs < 3_600 {
        let mins = secs / 60;
        let remaining = secs % 60;
        return if remaining > 0 {
            format!("{mins}m {remaining}s")
        } else {
            format!("{mins}m")
        };
    }
    if secs < 86_400 {
        let hours = secs / 3_600;
        let remaining = (secs % 3_600) / 60;
        return if remaining > 0 {
            format!("{hours}h {remaining}m")
        } else {
            format!("{hours}h")
        };
    }
    if secs < 604_800 {
        let days = secs / 86_400;
        return if days == 1 {
            "~1 day".to_string()
        } else {
            format!("~{days} days")
        };
    }
    let weeks = secs / 604_800;
    if weeks == 1 {
        "~1 week".to_string()
    } else {
        format!("~{weeks} weeks")
    }
}
