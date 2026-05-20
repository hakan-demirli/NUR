// Auto-generated from tests/smoke.rs split.
#![allow(dead_code)]
#![allow(unused_imports)]

pub use crossterm::event::{KeyCode, KeyModifiers};
pub use raider_tui::action::{
    Action, HostAction, Lifecycle, PluginCommand, PluginDialogOption, Toast, ToastVariant,
    UserAction, ViewAction,
};
pub use raider_tui::dialog::{DialogKind, DialogPayload};
pub use raider_tui::event::Event;
pub use raider_tui::harness::Harness;
pub use raider_tui::provider::{ModelCatalog, ModelInfo, ModelRef, ProviderInfo};
pub use raider_tui::SidebarSection;
pub use raider_tui::ThemeMode;
pub use ratatui::style::Modifier;

pub fn key(c: char) -> Action {
    Action::User(UserAction::Key {
        code: KeyCode::Char(c),
        mods: KeyModifiers::NONE,
    })
}

pub fn ctrl(c: char) -> Action {
    Action::User(UserAction::Key {
        code: KeyCode::Char(c),
        mods: KeyModifiers::CONTROL,
    })
}

pub fn special(code: KeyCode) -> Action {
    Action::User(UserAction::Key {
        code,
        mods: KeyModifiers::NONE,
    })
}

pub fn type_text(h: &mut Harness, text: &str) {
    for c in text.chars() {
        h.dispatch(key(c));
    }
}

pub fn pin_dummy_model(h: &mut Harness) {
    use raider_tui::provider::{ModelCatalog, ModelInfo, ProviderInfo};
    h.app.models.set_catalog(ModelCatalog {
        providers: vec![ProviderInfo {
            id: "anthropic".into(),
            name: Some("Anthropic".into()),
            models: vec![ModelInfo {
                id: "claude".into(),
                name: Some("Claude".into()),
                variants: vec![],
                context_limit: 0,
            }],
        }],
    });
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude",
    ))));
    h.clear_events();
}

pub fn seed_catalog(h: &mut Harness) {
    h.app.models.set_catalog(ModelCatalog {
        providers: vec![
            ProviderInfo {
                id: "anthropic".into(),
                name: Some("Anthropic".into()),
                models: vec![
                    ModelInfo {
                        id: "claude-sonnet-4-5".into(),
                        name: Some("Claude Sonnet 4.5".into()),
                        variants: vec!["thinking".into(), "fast".into()],
                        context_limit: 0,
                    },
                    ModelInfo {
                        id: "claude-opus-4-7".into(),
                        name: Some("Claude Opus 4.7".into()),
                        variants: vec![],
                        context_limit: 0,
                    },
                ],
            },
            ProviderInfo {
                id: "openai".into(),
                name: Some("OpenAI".into()),
                models: vec![ModelInfo {
                    id: "gpt-5".into(),
                    name: Some("GPT-5".into()),
                    variants: vec![],
                    context_limit: 0,
                }],
            },
        ],
    });
}

pub fn open_sidebar_with(h: &mut Harness, sections: Vec<raider_tui::SidebarSection>) {
    h.app.sidebar.set_title("Greeting");
    h.app.sidebar.set_subtitle(Some("ses_abc".into()));
    h.app.sidebar.set_sections(sections);
    h.app.sidebar.set_visible(true);
}

pub fn find_row(snap: &str, needle: &str) -> u16 {
    snap.lines()
        .position(|l| l.contains(needle))
        .unwrap_or_else(|| panic!("no row contains {needle:?}; snap:\n{snap}")) as u16
}

pub fn find_glyph_x(buf: &ratatui::buffer::Buffer, y: u16, glyph: &str) -> Option<u16> {
    (0..buf.area.width).find(|&x| buf[(x, y)].symbol() == glyph)
}

pub fn many_sidebar_entries(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("entry {i}")).collect()
}

pub fn cell_is_thumb(h: &Harness, x: u16, y: u16) -> bool {
    let buf = h.terminal.backend().buffer();
    buf[(x, y)].symbol() == "█"
}
