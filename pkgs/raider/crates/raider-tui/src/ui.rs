pub mod diff;
pub mod logo;
pub mod markdown;
pub mod theme;
pub mod theme_detect;
pub mod tips;
pub mod wipe_spinner;

pub(crate) mod agent;
pub(crate) mod completion;
pub(crate) mod layout;
pub(crate) mod messages;
pub(crate) mod modal;
pub(crate) mod path;
pub(crate) mod primitives;
pub(crate) mod prompt;
pub(crate) mod sidebar;
pub(crate) mod spinner;
pub(crate) mod syntax;
pub(crate) mod toast;
pub(crate) mod tool_calls;

use ratatui::prelude::*;
use ratatui::widgets::Block;

use crate::app::App;

#[allow(unused_imports)]
pub(crate) use modal::required_modal_height;
#[allow(unused_imports)]
pub(crate) use spinner::{
    spinner_frame_for, tool_uses_running_spinner, SPINNER_FRAMES, SPINNER_INTERVAL_MS,
};
#[allow(unused_imports)]
pub(crate) use tool_calls::tool_is_inline;

use self::completion::render_completion;
use self::layout::compute_layout;
use self::messages::render_messages;
use self::modal::{render_dialog, render_prompt_modals};
use self::prompt::{
    render_connector, render_prompt, render_sub_tray, render_subagent_footer, wrap_input,
};
use self::sidebar::render_sidebar;
use self::tips::{current_tip, render_tip};
use self::toast::render_toast;

pub fn ui(f: &mut Frame, app: &mut App) {
    let screen = f.area();

    f.render_widget(
        Block::default().style(Style::default().bg(app.theme.theme.background)),
        screen,
    );

    let pad_left = 2u16;
    let pad_right = 2u16;
    let preliminary_main_width = {
        let sidebar_min_main_width: u16 = 40;
        let sidebar_visible = app.sidebar.sidebar.visible
            && screen.width
                >= app
                    .sidebar
                    .sidebar
                    .width
                    .saturating_add(sidebar_min_main_width);
        let main_full_w = if sidebar_visible {
            screen.width.saturating_sub(app.sidebar.sidebar.width)
        } else {
            screen.width
        };
        main_full_w.saturating_sub(4)
    };
    let text_width = preliminary_main_width
        .saturating_sub(1 + pad_left + pad_right)
        .max(1) as usize;
    app.last_text_width = text_width;
    let (wrapped, cursor_pos) = wrap_input(&app.input.input, text_width, app);
    let visible_text_rows = wrapped.len().clamp(1, 6);
    let prompt_box_height = 1 + visible_text_rows as u16 + 1 + 1;

    let modal_height_request = required_modal_height(app, preliminary_main_width);
    let rects = compute_layout(app, screen, prompt_box_height, modal_height_request);

    render_messages(f, app, rects.messages);
    if !rects.modal_active {
        if app.sessions.sessions.current_is_child() {
            let footer_area = Rect {
                x: rects.prompt.x,
                y: rects.prompt.y,
                width: rects.prompt.width,
                height: rects
                    .sub_tray
                    .y
                    .saturating_add(rects.sub_tray.height)
                    .saturating_sub(rects.prompt.y),
            };
            render_subagent_footer(f, app, footer_area);
        } else {
            render_prompt(f, app, rects.prompt, &wrapped, cursor_pos);
            render_connector(f, app, rects.connector);
            render_sub_tray(f, app, rects.sub_tray);
        }
    }
    if !rects.modal_active && rects.tip_visible && rects.tip_strip_height >= 2 {
        let tip_row = Rect {
            x: rects.tip.x + 1 + 2,
            y: rects.tip.y + 1,
            width: rects.tip.width.saturating_sub(1 + 2 + 2),
            height: 1,
        };
        let body = current_tip(app.prompt.prompt_placeholder_index);
        render_tip(f, tip_row, body, &app.theme.theme);
    }
    if let Some(area) = rects.sidebar {
        render_sidebar(f, app, area);
    } else {
        app.sidebar.last_sidebar_rect = None;
        app.sidebar.sidebar_header_rects.clear();
    }
    if rects.modal_active {
        render_prompt_modals(f, app, rects.modal);
    }
    render_dialog(f, app, screen);
    render_completion(f, app, rects.prompt);
    render_toast(f, app, screen);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_frames_match_opencode_braille_sequence() {
        assert_eq!(
            SPINNER_FRAMES,
            &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
        );
    }

    #[test]
    fn spinner_frame_for_advances_every_80ms() {
        assert_eq!(spinner_frame_for(0), SPINNER_FRAMES[0]);
        assert_eq!(spinner_frame_for(79), SPINNER_FRAMES[0]);
        assert_eq!(spinner_frame_for(80), SPINNER_FRAMES[1]);
        assert_eq!(spinner_frame_for(160), SPINNER_FRAMES[2]);
        assert_eq!(spinner_frame_for(800), SPINNER_FRAMES[0]);
        assert_eq!(spinner_frame_for(880), SPINNER_FRAMES[1]);
    }

    #[test]
    fn spinner_interval_matches_opencode() {
        assert_eq!(SPINNER_INTERVAL_MS, 80);
    }

    #[test]
    fn resolve_model_display_returns_name_when_catalog_has_match() {
        use crate::provider::{ModelCatalog, ModelInfo, ProviderInfo};
        let catalog = ModelCatalog {
            providers: vec![ProviderInfo {
                id: "anthropic".to_string(),
                name: Some("Anthropic".to_string()),
                models: vec![ModelInfo {
                    id: "claude-opus-4-7".to_string(),
                    name: Some("Claude Opus 4.7".to_string()),
                    variants: vec![],
                    context_limit: 0,
                }],
            }],
        };
        assert_eq!(
            agent::resolve_model_display(&catalog, Some("anthropic"), "claude-opus-4-7"),
            Some("Claude Opus 4.7".to_string()),
        );
        assert_eq!(
            agent::resolve_model_display(&catalog, None, "claude-opus-4-7"),
            Some("Claude Opus 4.7".to_string()),
        );
    }

    #[test]
    fn resolve_model_display_returns_none_when_unknown() {
        use crate::provider::ModelCatalog;
        let catalog = ModelCatalog::default();
        assert_eq!(
            agent::resolve_model_display(&catalog, Some("anthropic"), "claude-opus-4-7"),
            None,
        );
    }

    #[test]
    fn resolve_model_display_scans_all_providers() {
        use crate::provider::{ModelCatalog, ModelInfo, ProviderInfo};
        let catalog = ModelCatalog {
            providers: vec![
                ProviderInfo {
                    id: "google".to_string(),
                    name: Some("Google".to_string()),
                    models: vec![ModelInfo {
                        id: "gemini-flash-latest".to_string(),
                        name: Some("Gemini Flash Latest".to_string()),
                        variants: vec![],
                        context_limit: 0,
                    }],
                },
                ProviderInfo {
                    id: "anthropic".to_string(),
                    name: Some("Anthropic".to_string()),
                    models: vec![ModelInfo {
                        id: "claude-opus-4-7".to_string(),
                        name: Some("Claude Opus 4.7".to_string()),
                        variants: vec![],
                        context_limit: 0,
                    }],
                },
            ],
        };
        assert_eq!(
            agent::resolve_model_display(&catalog, None, "claude-opus-4-7"),
            Some("Claude Opus 4.7".to_string()),
        );
    }

    #[test]
    fn resolve_model_display_prefers_provider_scoped_match() {
        use crate::provider::{ModelCatalog, ModelInfo, ProviderInfo};
        let catalog = ModelCatalog {
            providers: vec![
                ProviderInfo {
                    id: "302ai".to_string(),
                    name: Some("302.AI".to_string()),
                    models: vec![ModelInfo {
                        id: "claude-opus-4-7".to_string(),
                        name: Some("claude-opus-4-7".to_string()),
                        variants: vec![],
                        context_limit: 0,
                    }],
                },
                ProviderInfo {
                    id: "anthropic".to_string(),
                    name: Some("Anthropic".to_string()),
                    models: vec![ModelInfo {
                        id: "claude-opus-4-7".to_string(),
                        name: Some("Claude Opus 4.7".to_string()),
                        variants: vec![],
                        context_limit: 0,
                    }],
                },
            ],
        };
        assert_eq!(
            agent::resolve_model_display(&catalog, Some("anthropic"), "claude-opus-4-7"),
            Some("Claude Opus 4.7".to_string()),
        );
        assert_eq!(
            agent::resolve_model_display(&catalog, None, "claude-opus-4-7"),
            Some("Claude Opus 4.7".to_string()),
        );
    }

    #[test]
    fn titlecase_single_word_uppercases_first_char() {
        assert_eq!(agent::titlecase("build"), "Build");
    }

    #[test]
    fn titlecase_hyphenated_uppercases_each_token() {
        assert_eq!(agent::titlecase("code-reviewer"), "Code-Reviewer");
    }

    #[test]
    fn titlecase_underscored_uppercases_each_token() {
        assert_eq!(agent::titlecase("my_special_agent"), "My_Special_Agent");
    }

    #[test]
    fn titlecase_space_separated_uppercases_each_token() {
        assert_eq!(agent::titlecase("foo bar"), "Foo Bar");
    }

    #[test]
    fn titlecase_empty_string_returns_empty() {
        assert_eq!(agent::titlecase(""), "");
    }

    #[test]
    fn titlecase_single_char_uppercases() {
        assert_eq!(agent::titlecase("a"), "A");
    }

    #[test]
    fn tool_uses_running_spinner_only_for_bash_read_task_question() {
        assert!(tool_uses_running_spinner("bash"));
        assert!(tool_uses_running_spinner("read"));
        assert!(tool_uses_running_spinner("task"));
        assert!(tool_uses_running_spinner("question"));
        for other in [
            "glob",
            "grep",
            "webfetch",
            "websearch",
            "edit",
            "write",
            "skill",
            "todowrite",
            "apply_patch",
            "unknown_tool",
        ] {
            assert!(
                !tool_uses_running_spinner(other),
                "tool {other:?} must NOT use the running spinner",
            );
        }
    }
}
