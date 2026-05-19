use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use crate::ui::theme::Theme;

pub const TIPS: &[&str] = &[
    "Type {highlight}@{/highlight} followed by a filename to fuzzy search and attach files",
    "Start a message with {highlight}!{/highlight} to run shell commands directly (e.g., {highlight}!ls -la{/highlight})",
    "Press {highlight}tab{/highlight} to cycle between Build and Plan agents",
    "Use {highlight}/undo{/highlight} to revert the last message and file changes",
    "Use {highlight}/redo{/highlight} to restore previously undone messages and file changes",
    "Run {highlight}/share{/highlight} to create a public link to your conversation at opencode.ai",
    "Drag and drop images or PDFs into the terminal to add them as context",
    "Use {highlight}/editor{/highlight} to compose messages in your external editor",
    "Run {highlight}/init{/highlight} to auto-generate project rules based on your codebase",
    "Use {highlight}/models{/highlight} to see and switch between available AI models",
    "Use {highlight}/themes{/highlight} to switch between built-in themes",
    "Use {highlight}/new{/highlight} to start a fresh conversation session",
    "Use {highlight}/sessions{/highlight} to list and continue previous conversations",
    "Run {highlight}/compact{/highlight} to summarize long sessions near context limits",
    "Use {highlight}/export{/highlight} to save the conversation as Markdown",
    "Press {highlight}ctrl+p{/highlight} to see all available actions and commands",
    "Run {highlight}/connect{/highlight} to add API keys for 75+ supported LLM providers",
    "Press {highlight}shift+enter{/highlight} to add newlines in your prompt",
    "Press {highlight}esc{/highlight} to stop the AI mid-response",
    "Switch to {highlight}Plan{/highlight} agent to get suggestions without making actual changes",
    "Use {highlight}@agent-name{/highlight} in prompts to invoke specialized subagents",
];

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TipPart<'a> {
    pub text: &'a str,
    pub highlight: bool,
}

pub fn parse_tip(tip: &str) -> Vec<TipPart<'_>> {
    const OPEN: &str = "{highlight}";
    const CLOSE: &str = "{/highlight}";
    let mut parts: Vec<TipPart<'_>> = Vec::new();
    let mut rest = tip;
    while let Some(open_idx) = rest.find(OPEN) {
        if open_idx > 0 {
            parts.push(TipPart {
                text: &rest[..open_idx],
                highlight: false,
            });
        }
        let after_open = &rest[open_idx + OPEN.len()..];
        match after_open.find(CLOSE) {
            Some(close_idx) => {
                parts.push(TipPart {
                    text: &after_open[..close_idx],
                    highlight: true,
                });
                rest = &after_open[close_idx + CLOSE.len()..];
            }
            None => {
                parts.push(TipPart {
                    text: after_open,
                    highlight: false,
                });
                return parts;
            }
        }
    }
    if !rest.is_empty() {
        parts.push(TipPart {
            text: rest,
            highlight: false,
        });
    }
    parts
}

pub fn current_tip(index: usize) -> Option<&'static str> {
    TIPS.get(index % TIPS.len()).copied()
}

pub fn render_tip(f: &mut Frame, area: Rect, tip: Option<&str>, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let Some(body) = tip else {
        return;
    };
    let bg = theme.background;

    let mut spans: Vec<Span<'static>> = Vec::new();
    spans.push(Span::styled(
        "●".to_string(),
        Style::default().fg(theme.warning).bg(bg),
    ));
    spans.push(Span::styled(
        " Tip ".to_string(),
        Style::default().fg(theme.text_muted).bg(bg),
    ));
    for part in parse_tip(body) {
        let fg = if part.highlight {
            theme.text
        } else {
            theme.text_muted
        };
        spans.push(Span::styled(
            part.text.to_string(),
            Style::default().fg(fg).bg(bg),
        ));
    }

    let total_w: usize = spans.iter().map(|s| s.content.chars().count()).sum();
    let max = area.width as usize;
    if total_w > max {
        let mut flat = String::new();
        for s in &spans {
            flat.push_str(&s.content);
        }
        let trimmed: String = flat.chars().take(max).collect();
        let p = Paragraph::new(Line::from(Span::styled(
            trimmed,
            Style::default().fg(theme.text_muted).bg(bg),
        )))
        .style(Style::default().bg(bg));
        f.render_widget(p, area);
        return;
    }

    let p = Paragraph::new(Line::from(spans)).style(Style::default().bg(bg));
    f.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_tip_handles_no_markers() {
        let parts = parse_tip("plain text");
        assert_eq!(
            parts,
            vec![TipPart {
                text: "plain text",
                highlight: false,
            }],
        );
    }

    #[test]
    fn parse_tip_extracts_single_highlight() {
        let parts = parse_tip("Press {highlight}tab{/highlight} to cycle");
        assert_eq!(
            parts,
            vec![
                TipPart {
                    text: "Press ",
                    highlight: false
                },
                TipPart {
                    text: "tab",
                    highlight: true
                },
                TipPart {
                    text: " to cycle",
                    highlight: false
                },
            ],
        );
    }

    #[test]
    fn parse_tip_handles_multiple_highlights() {
        let parts = parse_tip("{highlight}/models{/highlight} or {highlight}/themes{/highlight}");
        assert_eq!(parts.len(), 3);
        assert!(parts[0].highlight);
        assert_eq!(parts[0].text, "/models");
        assert!(!parts[1].highlight);
        assert_eq!(parts[1].text, " or ");
        assert!(parts[2].highlight);
        assert_eq!(parts[2].text, "/themes");
    }

    #[test]
    fn parse_tip_tolerates_unbalanced_marker() {
        let parts = parse_tip("Press {highlight}tab to cycle");
        let recon: String = parts.iter().map(|p| p.text).collect();
        assert_eq!(recon, "Press tab to cycle");
        assert!(
            parts.iter().all(|p| !p.highlight),
            "no part should be marked highlighted when the open marker \
             is unbalanced: {parts:?}",
        );
    }

    #[test]
    fn current_tip_wraps_via_modulo() {
        assert_eq!(current_tip(0), Some(TIPS[0]));
        assert_eq!(current_tip(TIPS.len()), Some(TIPS[0]));
        assert_eq!(current_tip(TIPS.len() + 5), Some(TIPS[5]));
    }

    #[test]
    fn shift_enter_newline_tip_is_in_pool() {
        let needle = "{highlight}shift+enter{/highlight}";
        assert!(
            TIPS.iter().any(|t| t.contains(needle)),
            "TIPS pool must include the shift+enter newline hint that \
             opencode's reference home screen surfaces",
        );
    }
}
