mod common;
use common::*;

use raider_tui::action::{PluginInfo, PluginKind, PluginStatus};
use raider_tui::dialog::PluginInstallScope;

fn seed_plugins(h: &mut Harness, plugins: Vec<PluginInfo>) {
    h.dispatch(Action::Host(HostAction::SetPluginList(plugins)));
    h.clear_events();
}

fn open_command_palette(h: &mut Harness) {
    h.dispatch(Action::View(ViewAction::OpenCommandPalette));
}

#[test]
fn ctrl_p_typing_plugins_finds_the_plugin_manager_entry() {
    let mut h = Harness::new(100, 30);
    h.dispatch(ctrl('p'));
    type_text(&mut h, "plugins");

    let snap = h.snapshot();
    assert!(
        snap.contains("Plugins"),
        "command palette did not surface a Plugins entry after typing 'plugins':\n{snap}"
    );
    assert!(
        snap.contains("Install plugin"),
        "command palette did not surface 'Install plugin':\n{snap}"
    );
}

#[test]
fn open_plugin_manager_action_opens_dialog_with_empty_state() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenPluginManager));

    assert_eq!(h.app.dialog_kind(), Some(DialogKind::PluginManager));
    let snap = h.snapshot();
    assert!(snap.contains("Plugins"), "title not shown:\n{snap}");
    assert!(
        snap.contains("No plugins loaded"),
        "empty-state hint not shown:\n{snap}"
    );
}

#[test]
fn plugin_manager_groups_active_inactive_and_error_with_headers() {
    let mut h = Harness::new(100, 30);
    seed_plugins(
        &mut h,
        vec![
            PluginInfo {
                id: "judge".into(),
                title: "Judge daemon".into(),
                description: Some("supervisor".into()),
                version: Some("0.3.1".into()),
                kind: PluginKind::Configured,
                source: "/tmp/judge.lua".into(),
                status: PluginStatus::Active,
            },
            PluginInfo {
                id: "inactive.one".into(),
                title: "Disabled plugin".into(),
                description: None,
                version: None,
                kind: PluginKind::Discovered,
                source: "/tmp/inactive.lua".into(),
                status: PluginStatus::Inactive,
            },
            PluginInfo {
                id: "broken".into(),
                title: "Broken".into(),
                description: None,
                version: None,
                kind: PluginKind::Installed,
                source: "/tmp/broken.lua".into(),
                status: PluginStatus::Error("syntax error".into()),
            },
        ],
    );
    h.dispatch(Action::View(ViewAction::OpenPluginManager));

    let snap = h.snapshot();
    assert!(snap.contains("Active"), "missing Active header:\n{snap}");
    assert!(
        snap.contains("Inactive"),
        "missing Inactive header:\n{snap}"
    );
    assert!(snap.contains("Errors"), "missing Errors header:\n{snap}");
    assert!(snap.contains("Judge daemon"), "missing active row:\n{snap}");
    assert!(
        snap.contains("Disabled plugin"),
        "missing inactive row:\n{snap}"
    );
    assert!(snap.contains("Broken"), "missing error row:\n{snap}");
}

#[test]
fn plugin_manager_space_emits_toggle_plugin_event() {
    let mut h = Harness::new(100, 30);
    seed_plugins(
        &mut h,
        vec![PluginInfo {
            id: "judge".into(),
            title: "Judge daemon".into(),
            description: None,
            version: None,
            kind: PluginKind::Configured,
            source: "/tmp/judge.lua".into(),
            status: PluginStatus::Active,
        }],
    );
    h.dispatch(Action::View(ViewAction::OpenPluginManager));

    h.dispatch(special(KeyCode::Char(' ')));

    assert_eq!(
        h.events(),
        &[Event::TogglePlugin("judge".to_string())],
        "space must emit a TogglePlugin event for the highlighted plugin"
    );
    assert_eq!(h.app.dialog_kind(), Some(DialogKind::PluginManager));
}

#[test]
fn plugin_manager_ctrl_r_emits_reload_plugin_event() {
    let mut h = Harness::new(100, 30);
    seed_plugins(
        &mut h,
        vec![PluginInfo {
            id: "judge".into(),
            title: "Judge daemon".into(),
            description: None,
            version: None,
            kind: PluginKind::Configured,
            source: "/tmp/judge.lua".into(),
            status: PluginStatus::Active,
        }],
    );
    h.dispatch(Action::View(ViewAction::OpenPluginManager));

    h.dispatch(ctrl('r'));

    assert_eq!(
        h.events(),
        &[Event::ReloadPlugin("judge".to_string())],
        "ctrl+r must emit a ReloadPlugin event"
    );
}

#[test]
fn plugin_manager_shift_i_opens_install_prompt() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenPluginManager));

    h.dispatch(Action::User(UserAction::Key {
        code: KeyCode::Char('I'),
        mods: KeyModifiers::NONE,
    }));

    assert_eq!(h.app.dialog_kind(), Some(DialogKind::PluginInstall));
    let snap = h.snapshot();
    assert!(
        snap.contains("Install plugin"),
        "install prompt title missing:\n{snap}"
    );
    assert!(
        snap.contains("Path to .lua file"),
        "install prompt placeholder missing:\n{snap}"
    );
    assert!(
        snap.contains("global"),
        "install prompt scope hint missing:\n{snap}"
    );
}

#[test]
fn install_prompt_tab_toggles_scope_global_local() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenPluginInstallPrompt));

    let snap = h.snapshot();
    assert!(
        snap.contains("current: global"),
        "initial scope wrong:\n{snap}"
    );

    h.dispatch(special(KeyCode::Tab));
    let snap = h.snapshot();
    assert!(
        snap.contains("current: local"),
        "tab did not flip to local:\n{snap}"
    );

    h.dispatch(special(KeyCode::Tab));
    let snap = h.snapshot();
    assert!(
        snap.contains("current: global"),
        "second tab did not flip back to global:\n{snap}"
    );
}

#[test]
fn install_prompt_submit_emits_install_event_with_scope() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenPluginInstallPrompt));
    h.dispatch(special(KeyCode::Tab));

    type_text(&mut h, "/tmp/plug.lua");
    h.dispatch(special(KeyCode::Enter));

    assert_eq!(
        h.events(),
        &[Event::InstallPluginPath {
            path: "/tmp/plug.lua".into(),
            scope: PluginInstallScope::Local,
        }],
        "install enter must emit InstallPluginPath with current scope"
    );
}

#[test]
fn install_prompt_rejects_empty_path() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenPluginInstallPrompt));
    h.dispatch(special(KeyCode::Enter));

    assert!(
        h.events().is_empty(),
        "empty install path must not produce an event: {:?}",
        h.events()
    );
}

#[test]
fn set_plugin_list_refreshes_open_manager_dialog() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::OpenPluginManager));
    let initial = h.snapshot();
    assert!(initial.contains("No plugins loaded"));

    h.dispatch(Action::Host(HostAction::SetPluginList(vec![PluginInfo {
        id: "judge".into(),
        title: "Judge".into(),
        description: None,
        version: None,
        kind: PluginKind::Configured,
        source: "/tmp/judge.lua".into(),
        status: PluginStatus::Active,
    }])));
    h.draw();

    let after = h.snapshot();
    assert!(
        !after.contains("No plugins loaded"),
        "empty state should be gone after SetPluginList:\n{after}"
    );
    assert!(
        after.contains("Judge"),
        "new plugin should be listed:\n{after}"
    );
}

#[test]
fn unregister_plugin_commands_removes_palette_entries() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::Host(HostAction::RegisterPluginCommands(vec![
        PluginCommand {
            name: "judge.go".into(),
            title: "Judge: go".into(),
            description: None,
            category: None,
            slash_name: Some("judge".into()),
            slash_aliases: vec![],
        },
    ])));

    open_command_palette(&mut h);
    type_text(&mut h, "judge");
    let with_command = h.snapshot();
    assert!(
        with_command.contains("Judge: go"),
        "plugin command must show up in palette:\n{with_command}"
    );
    h.dispatch(special(KeyCode::Esc));

    h.dispatch(Action::Host(HostAction::UnregisterPluginCommands(vec![
        "judge.go".into(),
    ])));

    open_command_palette(&mut h);
    type_text(&mut h, "judge.go");
    let without_command = h.snapshot();
    assert!(
        !without_command.contains("Judge: go"),
        "plugin command must disappear after unregister:\n{without_command}"
    );
}

#[test]
fn add_plugin_path_view_action_emits_install_event() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::AddPluginPath(
        "/tmp/extra.lua".into(),
    )));

    assert_eq!(
        h.events(),
        &[Event::InstallPluginPath {
            path: "/tmp/extra.lua".into(),
            scope: PluginInstallScope::Global,
        }],
        "AddPluginPath must emit an InstallPluginPath event defaulting to global scope"
    );
}

#[test]
fn toggle_plugin_view_action_emits_toggle_event() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::TogglePlugin("judge".into())));

    assert_eq!(h.events(), &[Event::TogglePlugin("judge".into())]);
}

#[test]
fn reload_plugin_view_action_emits_reload_event() {
    let mut h = Harness::new(100, 30);
    h.dispatch(Action::View(ViewAction::ReloadPlugin("judge".into())));

    assert_eq!(h.events(), &[Event::ReloadPlugin("judge".into())]);
}
