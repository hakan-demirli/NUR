// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn default_agents_are_build_then_plan() {
    let h = Harness::new(80, 24);
    let names: Vec<&str> = h.app.agents.iter().map(|a| a.name.as_str()).collect();
    assert_eq!(names, vec!["build", "plan"]);
    assert_eq!(h.app.current_agent().name, "build");
}

#[test]
fn tab_cycles_agent_forward() {
    let mut h = Harness::new(80, 24);
    h.dispatch(special(KeyCode::Tab));
    assert_eq!(h.app.current_agent().name, "plan");
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::AgentChanged(n) if n == "plan")));

    h.dispatch(special(KeyCode::Tab));
    assert_eq!(
        h.app.current_agent().name,
        "build",
        "Tab wraps around to first agent"
    );
}

#[test]
fn shift_tab_cycles_agent_backward() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::User(UserAction::Key {
        code: KeyCode::Tab,
        mods: KeyModifiers::SHIFT,
    }));
    assert_eq!(h.app.current_agent().name, "plan");

    h.dispatch(special(KeyCode::BackTab));
    assert_eq!(h.app.current_agent().name, "build");
}

#[test]
fn tab_does_not_consume_input_text() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::User(UserAction::PasteText("hello".into())));
    h.dispatch(special(KeyCode::Tab));
    assert_eq!(
        h.app.input.input, "hello",
        "Tab must not mangle the input buffer"
    );
    assert_eq!(h.app.current_agent().name, "plan");
}

#[test]
fn host_can_replace_agents() {
    use raider_tui::Agent;
    let mut h = Harness::new(80, 24);
    h.app.set_agents(vec![
        Agent::new("build", "Build"),
        Agent::new("plan", "Plan"),
        Agent::new("review", "Review"),
    ]);
    h.dispatch(special(KeyCode::Tab));
    h.dispatch(special(KeyCode::Tab));
    assert_eq!(h.app.current_agent().name, "review");
}

#[test]
fn slash_agents_opens_picker_with_current_highlighted() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::Command("/agents".into())));

    let dialog = h.app.dialogs.dialog.as_ref().expect("/agents opens picker");
    assert_eq!(dialog.title, "Select agent");
    let titles: Vec<String> = dialog
        .visible_options()
        .iter()
        .map(|o| o.title.clone())
        .collect();
    assert_eq!(
        titles,
        vec!["build".to_string(), "plan".to_string()],
        "opencode parity: identifier as picker text"
    );
    assert_eq!(dialog.current_value, "build");
}

#[test]
fn agent_picker_enter_pins_selection_and_emits_event() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::OpenAgentPicker));
    h.clear_events();

    h.dispatch(special(KeyCode::Down));
    h.dispatch(special(KeyCode::Enter));

    assert!(h.app.dialogs.dialog.is_none(), "picker closes on enter");
    assert_eq!(h.app.current_agent().name, "plan");
    assert!(
        h.events()
            .iter()
            .any(|e| matches!(e, Event::AgentChanged(n) if n == "plan")),
        "AgentChanged emitted on picker confirm; got {:?}",
        h.events()
    );
}

#[test]
fn slash_agents_with_arg_switches_directly() {
    let mut h = Harness::new(80, 24);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("/agents plan".into())));

    assert!(
        h.app.dialogs.dialog.is_none(),
        "no picker opens for /agents <name>"
    );
    assert_eq!(h.app.current_agent().name, "plan");
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::AgentChanged(n) if n == "plan")));
}

#[test]
fn unknown_agent_emits_system_message_and_keeps_current() {
    let mut h = Harness::new(80, 24);
    let before = h.app.current_agent().name.clone();
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("/agents nope-xyz".into())));

    assert_eq!(h.app.current_agent().name, before);
    assert!(
        !h.events()
            .iter()
            .any(|e| matches!(e, Event::AgentChanged(_))),
        "no AgentChanged for unknown name"
    );
    let snap = h.snapshot();
    assert!(snap.contains("unknown agent"), "warning visible:\n{snap}");
}

#[test]
fn slash_autocomplete_lists_exit() {
    let mut h = Harness::new(80, 24);
    h.dispatch(key('/'));
    let titles: Vec<&str> = h
        .app
        .input
        .completion
        .candidates
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert!(
        titles.contains(&"/exit"),
        "/exit must appear in autocomplete; got {titles:?}"
    );
    assert!(
        titles.contains(&"/themes"),
        "/themes must appear in autocomplete; got {titles:?}"
    );
    assert!(
        titles.contains(&"/agents"),
        "/agents must appear in autocomplete; got {titles:?}"
    );
}

#[test]
fn slash_autocomplete_auto_highlights_first_row() {
    let mut h = Harness::new(80, 24);
    h.dispatch(key('/'));
    assert!(
        h.app.input.completion.active,
        "popup should be active after `/`"
    );
    assert!(
        !h.app.input.completion.candidates.is_empty(),
        "popup should have candidates"
    );
    assert_eq!(
        h.app.input.completion.state.selected(),
        Some(0),
        "first candidate must be auto-highlighted on open"
    );
}

#[test]
fn tab_in_completion_selects_first_item_not_cycle_agent() {
    let mut h = Harness::new(80, 24);
    h.dispatch(key('/'));
    assert!(
        h.app.input.completion.active,
        "popup is active after typing '/'"
    );

    let agent_before = h.app.current_agent().name.clone();
    h.dispatch(special(KeyCode::Tab));
    assert_eq!(
        h.app.current_agent().name,
        agent_before,
        "Tab must not cycle the agent while completion is open"
    );
    assert!(
        h.app.input.input.starts_with('/'),
        "first match got inserted into input: {:?}",
        h.app.input.input
    );
}

#[test]
fn slash_autocomplete_is_sorted_alphabetically() {
    let mut h = Harness::new(80, 24);
    h.dispatch(key('/'));
    let titles: Vec<&str> = h
        .app
        .input
        .completion
        .candidates
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    assert!(!titles.is_empty(), "popup has candidates");
    let mut sorted = titles.clone();
    sorted.sort_by_key(|s| s.to_ascii_lowercase());
    assert_eq!(titles, sorted, "candidates must be alphabetical");
}

#[test]
fn slash_autocomplete_carries_command_titles_as_descriptions() {
    let mut h = Harness::new(80, 24);
    h.dispatch(key('/'));
    let exit = h
        .app
        .input
        .completion
        .candidates
        .iter()
        .find(|c| c.text == "/exit")
        .expect("/exit present");
    assert_eq!(
        exit.description, "Exit the app",
        "description column shows the opencode-parity command title \
         (`app.tsx:697`)"
    );

    let models = h
        .app
        .input
        .completion
        .candidates
        .iter()
        .find(|c| c.text == "/models")
        .expect("/models present");
    assert_eq!(models.description, "Switch model");
}

#[test]
fn slash_autocomplete_lists_every_builtin_without_truncation() {
    let mut h = Harness::new(80, 24);
    h.dispatch(key('/'));
    let want = [
        "/sessions",
        "/new",
        "/models",
        "/agents",
        "/mcps",
        "/variants",
        "/connect",
        "/status",
        "/themes",
        "/help",
        "/exit",
        "/share",
        "/unshare",
        "/rename",
        "/compact",
        "/undo",
        "/redo",
        "/copy",
        "/export",
        "/timestamps",
        "/thinking",
        "/editor",
        "/init",
        "/review",
    ];
    let got: Vec<&str> = h
        .app
        .input
        .completion
        .candidates
        .iter()
        .map(|c| c.text.as_str())
        .collect();
    for w in want {
        assert!(
            got.contains(&w),
            "{w} missing from autocomplete; got {got:?}"
        );
    }
    for forbidden in [
        "/agent-next",
        "/model-next",
        "/model-prev",
        "/model-fav-next",
        "/variant-next",
        "/dark",
        "/light",
        "/clear",
        "/sidebar",
    ] {
        assert!(
            !got.contains(&forbidden),
            "{forbidden} must not appear in autocomplete (opencode has no equivalent); got {got:?}"
        );
    }
}
