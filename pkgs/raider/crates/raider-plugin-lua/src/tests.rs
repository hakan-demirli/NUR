use std::sync::{Arc, Mutex};

use mlua::Lua;
use tokio::sync::mpsc::UnboundedSender;

use raider_tui::{Action, HostAction, ToastVariant, ViewAction};

use crate::bindings::install_api;
use crate::dispatch::handle_event;
use crate::runtime::RuntimeState;
use crate::PluginEvent;

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
