// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn default_theme_is_opencode() {
    let h = Harness::new(80, 24);
    assert_eq!(h.app.theme.theme.name, "opencode");
    assert_eq!(h.app.theme.theme.mode, ThemeMode::Dark);
}

#[test]
fn registry_contains_dracula_and_friends() {
    let h = Harness::new(80, 24);
    let names = h.app.theme.theme_registry.names();
    for required in [
        "dracula",
        "opencode",
        "tokyonight",
        "gruvbox",
        "nord",
        "github",
    ] {
        assert!(
            names.iter().any(|n| n == required),
            "registry missing {required}; have: {names:?}"
        );
    }
}

#[test]
fn slash_themes_opens_picker_with_current_highlighted() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::Command("/themes".into())));
    let dialog = h.app.dialogs.dialog.as_ref().expect("theme picker opens");
    assert_eq!(dialog.current_value, "opencode");
    let snap = h.snapshot();
    assert!(snap.contains("Themes"), "dialog title rendered:\n{snap}");
}

#[test]
fn slash_theme_with_arg_switches_directly() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::Command("/theme dracula".into())));
    assert_eq!(h.app.theme.theme.name, "dracula");
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::ThemeChanged(n) if n == "dracula")),);
}

#[test]
fn theme_picker_previews_on_navigation_and_commits_on_enter() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenThemePicker));
    let starting = h.app.theme.theme.name.clone();
    h.clear_events();

    h.dispatch(special(KeyCode::Down));
    h.dispatch(special(KeyCode::Down));
    let previewed = h.app.theme.theme.name.clone();
    assert_ne!(previewed, starting);

    h.dispatch(special(KeyCode::Enter));
    assert!(h.app.dialogs.dialog.is_none());
    assert_eq!(h.app.theme.theme.name, previewed);
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::ThemeChanged(n) if n == &previewed)),);
}

#[test]
fn theme_picker_reverts_on_escape() {
    let mut h = Harness::new(100, 30);
    let original = h.app.theme.theme.name.clone();
    h.dispatch(Action::View(ViewAction::OpenThemePicker));
    h.dispatch(special(KeyCode::Down));
    h.dispatch(special(KeyCode::Down));
    assert_ne!(h.app.theme.theme.name, original);

    h.dispatch(special(KeyCode::Esc));
    assert!(h.app.dialogs.dialog.is_none());
    assert_eq!(h.app.theme.theme.name, original);
    assert!(!h
        .events()
        .iter()
        .any(|e| matches!(e, Event::ThemeChanged(_))));
}

#[test]
fn theme_picker_filter_narrows_options() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenThemePicker));
    for c in "drac".chars() {
        h.dispatch(key(c));
    }
    let dialog = h.app.dialogs.dialog.as_ref().expect("still open");
    assert_eq!(dialog.current_value, "dracula");
}

#[test]
fn slash_dark_and_light_switch_mode() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::Command("/light".into())));
    assert_eq!(h.app.theme.theme.mode, ThemeMode::Light);
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::ThemeModeChanged(ThemeMode::Light))),);

    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("/dark".into())));
    assert_eq!(h.app.theme.theme.mode, ThemeMode::Dark);
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::ThemeModeChanged(ThemeMode::Dark))),);
}

#[test]
fn dracula_dark_colors_are_real_dracula() {
    use ratatui::style::Color;
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::SetTheme("dracula".into())));
    assert_eq!(h.app.theme.theme.primary, Color::Rgb(0xbd, 0x93, 0xf9));
    assert_eq!(h.app.theme.theme.background, Color::Rgb(0x28, 0x2a, 0x36));
}

#[test]
fn unknown_theme_emits_system_message_and_keeps_old_theme() {
    let mut h = Harness::new(80, 24);
    let before = h.app.theme.theme.name.clone();
    h.dispatch(Action::View(ViewAction::Command(
        "/theme nonsense-xyz".into(),
    )));
    assert_eq!(h.app.theme.theme.name, before);
    let snap = h.snapshot();
    assert!(snap.contains("unknown theme"), "snap:\n{snap}");
}

#[test]
fn system_theme_is_bundled() {
    let h = Harness::new(80, 24);
    let names = h.app.theme.theme_registry.names();
    assert!(
        names.iter().any(|n| n == "system"),
        "`system` theme must be bundled (opencode parity): {names:?}"
    );
}

#[test]
fn system_theme_uses_terminal_defaults_for_bg_and_text() {
    use ratatui::style::Color;
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::SetTheme("system".into())));
    assert_eq!(
        h.app.theme.theme.background,
        Color::Reset,
        "background uses terminal default"
    );
    assert_eq!(
        h.app.theme.theme.text,
        Color::Reset,
        "text uses terminal default"
    );
    assert_eq!(h.app.theme.theme.primary, Color::Cyan);
    assert_eq!(h.app.theme.theme.accent, Color::Cyan);
}

#[test]
fn toggle_theme_mode_flips_dark_light_round_trip() {
    use raider_tui::ui::theme::Mode;
    let mut h = Harness::new(100, 24);
    let start = h.app.theme.theme.mode;
    h.clear_events();

    h.dispatch(Action::View(ViewAction::ToggleThemeMode));
    let after_one = h.app.theme.theme.mode;
    assert_ne!(after_one, start, "first toggle must flip the mode");
    assert!(
        matches!(after_one, Mode::Dark | Mode::Light),
        "still a valid mode after toggle"
    );
    let mode_changed = h
        .events()
        .iter()
        .any(|e| matches!(e, Event::ThemeModeChanged(_)));
    assert!(
        mode_changed,
        "ToggleThemeMode must emit Event::ThemeModeChanged"
    );

    h.dispatch(Action::View(ViewAction::ToggleThemeMode));
    let after_two = h.app.theme.theme.mode;
    assert_eq!(after_two, start, "second toggle must round-trip");
}
