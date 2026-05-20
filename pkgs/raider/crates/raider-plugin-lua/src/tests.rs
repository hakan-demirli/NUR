use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use mlua::Lua;
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::{Action, HostAction, PluginKind, PluginStatus, ToastVariant, ViewAction};

use crate::bindings::install_api;
use crate::dispatch::handle_event;
use crate::registry::PluginRegistry;
use crate::runtime::RuntimeState;
use crate::{PluginEvent, PluginId};

fn test_runtime() -> (
    Lua,
    Arc<Mutex<RuntimeState>>,
    UnboundedSender<Action>,
    tokio::sync::mpsc::UnboundedReceiver<Action>,
) {
    let lua = Lua::new();
    let state = Arc::new(Mutex::new(RuntimeState {
        next_callback_id: 1,
        ..RuntimeState::default()
    }));
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    install_api(
        &lua,
        Arc::clone(&state),
        tx.clone(),
        Some("/workspace".to_string()),
        Some("ses_worker".to_string()),
    )
    .expect("install api");
    (lua, state, tx, rx)
}

#[test]
fn official_command_register_and_toast_shape_work() {
    let (lua, state, tx, mut rx) = test_runtime();
    lua.load(
        r#"
        assert(api.route.current.name == "session")
        assert(api.route.current.params.sessionID == "ses_worker")
        api.command.register(function()
          return {
            {
              value = "office.judge",
              title = "Judge controls",
              description = "Open the judge daemon control menu",
              category = "Judge",
              slash = { name = "judge" },
              onSelect = function()
                api.ui.toast({ variant = "success", message = "judge on" })
              end,
            },
          }
        end)
        "#,
    )
    .exec()
    .expect("plugin source executes");

    let action = rx.try_recv().expect("registered command action");
    let Action::Host(HostAction::RegisterPluginCommands(commands)) = action else {
        panic!("unexpected action: {action:?}");
    };
    assert_eq!(commands[0].name, "office.judge");
    assert_eq!(commands[0].slash_name.as_deref(), Some("judge"));

    handle_event(
        &lua,
        state,
        &tx,
        PluginEvent::Command {
            name: "office.judge".to_string(),
            args: String::new(),
        },
    )
    .expect("command callback");
    let action = rx.try_recv().expect("toast action");
    let Action::View(ViewAction::ShowToast(toast)) = action else {
        panic!("unexpected action: {action:?}");
    };
    assert_eq!(toast.message, "judge on");
    assert_eq!(toast.variant, ToastVariant::Success);
}

#[test]
fn dialog_select_on_select_receives_option_table() {
    let (lua, state, tx, mut rx) = test_runtime();
    lua.load(
        r#"
        picked = nil
        api.ui.dialog.replace(function()
          return api.ui.DialogSelect({
            title = "Judge",
            options = {
              { title = "On", value = "judge.on", description = "Start supervision", category = "Judge" },
            },
            onSelect = function(option)
              picked = option.title .. ":" .. option.value .. ":" .. option.category
            end,
          })
        end)
        "#,
    )
    .exec()
    .expect("dialog source executes");

    let action = rx.try_recv().expect("open select action");
    let Action::Host(HostAction::OpenPluginSelect { callback_id, .. }) = action else {
        panic!("unexpected action: {action:?}");
    };
    handle_event(
        &lua,
        state,
        &tx,
        PluginEvent::DialogSelected {
            callback_id,
            value: "judge.on".to_string(),
        },
    )
    .expect("select callback");
    let picked: String = lua.globals().get("picked").expect("picked global");
    assert_eq!(picked, "On:judge.on:Judge");
}

#[test]
fn route_navigate_session_emits_plugin_navigation_action() {
    let (lua, _state, _tx, mut rx) = test_runtime();
    lua.load(r#"api.route.navigate("session", { sessionID = "ses_judge" })"#)
        .exec()
        .expect("navigate source executes");
    let action = rx.try_recv().expect("navigate action");
    assert_eq!(
        action,
        Action::View(ViewAction::PluginNavigateSession("ses_judge".to_string()))
    );
}

fn temp_plugin_file(label: &str, source: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "raider-plugin-lua-tests-{}-{}",
        std::process::id(),
        label
    ));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    let path = dir.join(format!("{label}.lua"));
    std::fs::write(&path, source).expect("write plugin source");
    path
}

#[test]
fn registry_load_path_executes_source_and_marks_active() {
    let (lua, state, _tx, mut rx) = test_runtime();
    let path = temp_plugin_file(
        "load_active",
        r#"
        -- @id judge.daemon
        -- @title Judge Daemon
        api.command.register({
          { value = "judge.go", title = "Judge go", run = function() end },
        })
        "#,
    );
    let mut registry = PluginRegistry::new();
    let id = registry
        .load_path(&lua, &state, path.clone(), PluginKind::Configured)
        .expect("load_path succeeds");

    assert_eq!(id.as_str(), "judge.daemon");
    let snapshot = registry.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert_eq!(snapshot[0].title, "Judge Daemon");
    assert_eq!(snapshot[0].kind, PluginKind::Configured);
    assert_eq!(snapshot[0].status, PluginStatus::Active);

    let state_locked = state.lock().expect("lock state");
    assert!(state_locked.commands.contains_key("judge.go"));
    assert_eq!(
        state_locked
            .command_owners
            .get("judge.go")
            .map(|id| id.as_str()),
        Some("judge.daemon")
    );
    drop(state_locked);

    let mut saw_register = false;
    while let Ok(action) = rx.try_recv() {
        if matches!(action, Action::Host(HostAction::RegisterPluginCommands(_))) {
            saw_register = true;
        }
    }
    assert!(
        saw_register,
        "registry load must emit RegisterPluginCommands"
    );
}

#[test]
fn registry_deactivate_drops_owned_commands_and_returns_their_names() {
    let (lua, state, _tx, _rx) = test_runtime();
    let path = temp_plugin_file(
        "deactivate",
        r#"
        -- @id judge
        api.command.register({
          { value = "judge.alpha", title = "alpha", run = function() end },
          { value = "judge.beta",  title = "beta",  run = function() end },
        })
        "#,
    );
    let mut registry = PluginRegistry::new();
    let id = registry
        .load_path(&lua, &state, path, PluginKind::Configured)
        .expect("load");

    let dropped = registry.deactivate(&state, &id).expect("deactivate");
    let mut dropped = dropped;
    dropped.sort();
    assert_eq!(
        dropped,
        vec!["judge.alpha".to_string(), "judge.beta".to_string()]
    );

    let snapshot = registry.snapshot();
    assert_eq!(snapshot[0].status, PluginStatus::Inactive);

    let state_locked = state.lock().expect("lock state");
    assert!(!state_locked.commands.contains_key("judge.alpha"));
    assert!(!state_locked.commands.contains_key("judge.beta"));
    assert!(state_locked.command_owners.is_empty());
}

#[test]
fn registry_activate_after_deactivate_re_registers_commands() {
    let (lua, state, _tx, _rx) = test_runtime();
    let path = temp_plugin_file(
        "reactivate",
        r#"
        -- @id judge
        api.command.register({
          { value = "judge.go", title = "Judge", run = function() end },
        })
        "#,
    );
    let mut registry = PluginRegistry::new();
    let id = registry
        .load_path(&lua, &state, path, PluginKind::Configured)
        .expect("load");

    registry.deactivate(&state, &id).expect("deactivate");
    assert!(state.lock().unwrap().commands.is_empty());

    registry.activate(&lua, &state, &id).expect("activate");
    let state_locked = state.lock().unwrap();
    assert!(state_locked.commands.contains_key("judge.go"));
    assert_eq!(
        state_locked
            .command_owners
            .get("judge.go")
            .map(|id| id.as_str()),
        Some("judge")
    );
}

#[test]
fn registry_reload_re_reads_source_and_picks_up_changes() {
    let (lua, state, _tx, _rx) = test_runtime();
    let path = temp_plugin_file(
        "reload",
        r#"
        -- @id judge
        api.command.register({
          { value = "judge.v1", title = "v1", run = function() end },
        })
        "#,
    );
    let mut registry = PluginRegistry::new();
    let id = registry
        .load_path(&lua, &state, path.clone(), PluginKind::Configured)
        .expect("load");
    assert!(state.lock().unwrap().commands.contains_key("judge.v1"));

    std::fs::write(
        &path,
        r#"
        -- @id judge
        -- @title Reloaded Judge
        api.command.register({
          { value = "judge.v2", title = "v2", run = function() end },
        })
        "#,
    )
    .expect("rewrite source");

    let dropped = registry.reload(&lua, &state, &id).expect("reload");
    assert_eq!(dropped, vec!["judge.v1".to_string()]);
    let state_locked = state.lock().unwrap();
    assert!(state_locked.commands.contains_key("judge.v2"));
    assert!(!state_locked.commands.contains_key("judge.v1"));
    drop(state_locked);

    let snapshot = registry.snapshot();
    assert_eq!(snapshot[0].title, "Reloaded Judge");
}

#[test]
fn registry_load_path_returns_error_for_bad_source_without_killing_runtime() {
    let (lua, state, _tx, _rx) = test_runtime();
    let path = temp_plugin_file("syntax_error", "this is not lua code !!");
    let mut registry = PluginRegistry::new();
    let err = registry
        .load_path(&lua, &state, path, PluginKind::Configured)
        .expect_err("syntax error must propagate");
    assert!(err.contains("error"), "expected syntax error, got: {err}");

    let snapshot = registry.snapshot();
    assert_eq!(snapshot.len(), 1);
    assert!(matches!(snapshot[0].status, PluginStatus::Error(_)));
}

#[test]
fn registry_synthesises_id_from_path_when_manifest_missing() {
    let (lua, state, _tx, _rx) = test_runtime();
    let path = temp_plugin_file("synthesized", "api.command.register({})");
    let mut registry = PluginRegistry::new();
    let id = registry
        .load_path(&lua, &state, path, PluginKind::Configured)
        .expect("load");
    assert_eq!(id.as_str(), "synthesized");
}

#[test]
fn dispatched_command_callback_inherits_plugin_owner_for_further_registers() {
    let (lua, state, tx, mut rx) = test_runtime();
    let path = temp_plugin_file(
        "owner_inheritance",
        r#"
        -- @id alpha
        api.command.register({
          { value = "alpha.bootstrap", title = "bootstrap", run = function()
              api.command.register({
                { value = "alpha.dynamic", title = "dynamic", run = function() end },
              })
          end },
        })
        "#,
    );
    let mut registry = PluginRegistry::new();
    let id = registry
        .load_path(&lua, &state, path, PluginKind::Configured)
        .expect("load");
    while rx.try_recv().is_ok() {}

    handle_event(
        &lua,
        Arc::clone(&state),
        &tx,
        PluginEvent::Command {
            name: "alpha.bootstrap".to_string(),
            args: String::new(),
        },
    )
    .expect("dispatch bootstrap");

    let owner = state
        .lock()
        .unwrap()
        .command_owners
        .get("alpha.dynamic")
        .cloned();
    assert_eq!(owner.as_ref().map(PluginId::as_str), Some("alpha"));

    let mut dropped = registry.deactivate(&state, &id).expect("deactivate");
    dropped.sort();
    assert_eq!(
        dropped,
        vec!["alpha.bootstrap".to_string(), "alpha.dynamic".to_string()]
    );
}

#[test]
fn api_plugins_activate_emits_view_action_toggle() {
    let (lua, _state, _tx, mut rx) = test_runtime();
    lua.load(r#"api.plugins.activate("judge")"#)
        .exec()
        .expect("call api.plugins.activate");

    let action = rx.try_recv().expect("expected action");
    assert_eq!(
        action,
        Action::View(ViewAction::TogglePlugin("judge".to_string()))
    );
}

#[test]
fn api_plugins_reload_emits_view_action_reload() {
    let (lua, _state, _tx, mut rx) = test_runtime();
    lua.load(r#"api.plugins.reload("judge")"#)
        .exec()
        .expect("call api.plugins.reload");
    let action = rx.try_recv().expect("expected action");
    assert_eq!(
        action,
        Action::View(ViewAction::ReloadPlugin("judge".to_string()))
    );
}

#[test]
fn api_plugins_add_emits_view_action_add_path() {
    let (lua, _state, _tx, mut rx) = test_runtime();
    lua.load(r#"api.plugins.add("/tmp/extra.lua")"#)
        .exec()
        .expect("call api.plugins.add");
    let action = rx.try_recv().expect("expected action");
    assert_eq!(
        action,
        Action::View(ViewAction::AddPluginPath("/tmp/extra.lua".to_string()))
    );
}

#[test]
fn api_plugins_open_emits_view_action_open_manager() {
    let (lua, _state, _tx, mut rx) = test_runtime();
    lua.load(r#"api.plugins.open()"#)
        .exec()
        .expect("call api.plugins.open");
    let action = rx.try_recv().expect("expected action");
    assert_eq!(action, Action::View(ViewAction::OpenPluginManager));
}
