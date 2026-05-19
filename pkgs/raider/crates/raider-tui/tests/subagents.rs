mod common;
use common::*;

use raider_tui::session::SessionEntry;
use raider_tui::{HostMessage, ToolCall, ToolStatus};

fn seed_parent_with_children(h: &mut Harness, parent: &str, children: &[&str]) {
    let mut entries = vec![SessionEntry::new(parent, parent, "now")];
    for c in children {
        entries.push(SessionEntry::new(*c, *c, "now").with_parent(parent));
    }
    h.app.sessions.set_sessions(entries);
    h.app.sessions.set_current(Some(parent.to_string()));
}

#[test]
fn session_entry_with_parent_records_parent_id() {
    let e = SessionEntry::new("c", "Child", "now").with_parent("p");
    assert_eq!(e.parent_id.as_deref(), Some("p"));
}

#[test]
fn children_of_returns_only_direct_children_sorted() {
    let mut h = Harness::new(80, 24);
    h.app.sessions.set_sessions(vec![
        SessionEntry::new("p", "Parent", "now"),
        SessionEntry::new("c-b", "Child B", "now").with_parent("p"),
        SessionEntry::new("c-a", "Child A", "now").with_parent("p"),
        SessionEntry::new("other", "Other", "now"),
    ]);
    let kids = h.app.sessions.sessions.children_of("p");
    assert_eq!(
        kids.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        vec!["c-a", "c-b"]
    );
}

#[test]
fn current_is_child_is_true_only_when_parent_id_present() {
    let mut h = Harness::new(80, 24);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    assert!(!h.app.sessions.sessions.current_is_child());
    h.app.sessions.set_current(Some("c1".into()));
    assert!(h.app.sessions.sessions.current_is_child());
}

#[test]
fn subagent_enter_first_child_navigates_into_first_child_no_abort() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2"]);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SubagentEnterFirstChild));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("c1"));
    let events = h.events();
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::SubagentNavigate(id) if id == "c1")),
        "must emit SubagentNavigate (no-abort), got: {events:#?}",
    );
    assert!(
        !events
            .iter()
            .any(|e| matches!(e, Event::SessionSwitched(_))),
        "must NOT emit SessionSwitched (would abort parent run), got: {events:#?}",
    );
}

#[test]
fn subagent_enter_first_child_is_noop_when_no_children() {
    let mut h = Harness::new(100, 24);
    h.app
        .sessions
        .set_sessions(vec![SessionEntry::new("p", "Parent", "now")]);
    h.app.sessions.set_current(Some("p".into()));
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SubagentEnterFirstChild));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("p"));
    assert!(h
        .events()
        .iter()
        .all(|e| !matches!(e, Event::SubagentNavigate(_) | Event::SessionSwitched(_))));
}

#[test]
fn subagent_go_to_parent_returns_to_parent_no_abort() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    h.app.sessions.set_current(Some("c1".into()));
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SubagentGoToParent));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("p"));
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::SubagentNavigate(id) if id == "p")));
}

#[test]
fn subagent_go_to_parent_is_noop_when_already_at_root() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SubagentGoToParent));

    assert!(h
        .events()
        .iter()
        .all(|e| !matches!(e, Event::SubagentNavigate(_))));
}

#[test]
fn subagent_cycle_sibling_forward_wraps_around() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2", "c3"]);
    h.app.sessions.set_current(Some("c2".into()));
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SubagentCycleSibling(1)));
    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("c3"));

    h.dispatch(Action::View(ViewAction::SubagentCycleSibling(1)));
    assert_eq!(
        h.app.sessions.sessions.current.as_deref(),
        Some("c1"),
        "must wrap from last back to first",
    );
}

#[test]
fn subagent_cycle_sibling_reverse_wraps_around() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2", "c3"]);
    h.app.sessions.set_current(Some("c1".into()));
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SubagentCycleSibling(-1)));
    assert_eq!(
        h.app.sessions.sessions.current.as_deref(),
        Some("c3"),
        "must wrap from first to last",
    );
}

#[test]
fn subagent_cycle_sibling_noop_with_single_child() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["only"]);
    h.app.sessions.set_current(Some("only".into()));
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SubagentCycleSibling(1)));
    h.dispatch(Action::View(ViewAction::SubagentCycleSibling(-1)));
    assert!(h
        .events()
        .iter()
        .all(|e| !matches!(e, Event::SubagentNavigate(_))));
}

#[test]
fn leader_chord_ctrl_x_then_down_enters_first_child() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2"]);
    h.clear_events();
    h.dispatch(ctrl('x'));
    h.dispatch(special(KeyCode::Down));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("c1"));
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::SubagentNavigate(id) if id == "c1")));
}

#[test]
fn leader_chord_ctrl_x_then_unbound_key_is_silently_consumed() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    h.clear_events();
    h.dispatch(ctrl('x'));
    h.dispatch(key('q'));

    assert!(
        h.app.input.input.is_empty(),
        "leader follow-up must not insert into prompt; got {:?}",
        h.app.input.input,
    );
    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("p"));
}

#[test]
fn bare_up_in_child_session_navigates_to_parent() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    h.app.sessions.set_current(Some("c1".into()));
    h.clear_events();
    h.dispatch(special(KeyCode::Up));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("p"));
}

#[test]
fn bare_right_in_child_session_cycles_to_next_sibling() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2"]);
    h.app.sessions.set_current(Some("c1".into()));
    h.clear_events();
    h.dispatch(special(KeyCode::Right));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("c2"));
}

#[test]
fn bare_left_in_child_session_cycles_to_previous_sibling() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2"]);
    h.app.sessions.set_current(Some("c2".into()));
    h.clear_events();
    h.dispatch(special(KeyCode::Left));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("c1"));
}

#[test]
fn arrow_keys_do_not_navigate_when_input_is_nonempty() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2"]);
    h.app.sessions.set_current(Some("c1".into()));
    h.app.input.input = "typing".into();
    h.app.input.cursor_position = h.app.input.input.len();
    h.clear_events();
    h.dispatch(special(KeyCode::Right));
    h.dispatch(special(KeyCode::Left));

    assert_eq!(
        h.app.sessions.sessions.current.as_deref(),
        Some("c1"),
        "arrow keys must not switch session when input is non-empty",
    );
}

#[test]
fn submit_input_is_swallowed_in_child_session() {
    let mut h = Harness::new(100, 24);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    h.app.sessions.set_current(Some("c1".into()));
    h.app.input.input = "hello".into();
    h.app.input.cursor_position = h.app.input.input.len();
    h.clear_events();
    h.app.submit_input();
    let events = h.events();
    assert!(
        events.iter().all(|e| !matches!(e, Event::UserMessage(_))),
        "child sessions must not accept prompt submissions: {events:#?}",
    );
    assert_eq!(
        h.app.input.input, "hello",
        "buffer should remain untouched since submission was swallowed",
    );
}

#[test]
fn subagent_footer_renders_when_active_session_is_child() {
    let mut h = Harness::new(120, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2"]);
    h.app.sessions.set_current(Some("c1".into()));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Parent"),
        "footer label 'Parent' must render:\n{snap}"
    );
    assert!(
        snap.contains("Prev"),
        "footer label 'Prev' must render:\n{snap}"
    );
    assert!(
        snap.contains("Next"),
        "footer label 'Next' must render:\n{snap}"
    );
}

#[test]
fn subagent_footer_shows_index_of_total_when_multiple_children() {
    let mut h = Harness::new(120, 24);
    seed_parent_with_children(&mut h, "p", &["c1", "c2", "c3"]);
    h.app.sessions.set_current(Some("c2".into()));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("(2 of 3)"),
        "footer must show 1-based position; got:\n{snap}",
    );
}

#[test]
fn subagent_footer_label_extracts_agent_from_title() {
    let mut h = Harness::new(120, 24);
    h.app.sessions.set_sessions(vec![
        SessionEntry::new("p", "Parent", "now"),
        SessionEntry::new("c1", "find auth helpers (@explore subagent)", "now").with_parent("p"),
    ]);
    h.app.sessions.set_current(Some("c1".into()));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Explore"),
        "label must be derived from '@<agent> subagent)' marker; got:\n{snap}",
    );
}

#[test]
fn view_subagents_hint_renders_when_assistant_used_task_tool() {
    let mut h = Harness::new(140, 30);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    let task = ToolCall {
        id: Some("part-task".into()),
        name: "task".into(),
        status: ToolStatus::Running,
        title: "Explore Task — find auth helpers".into(),
        ..Default::default()
    };
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Delegating...", "").with_tool(task),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("ctrl+x down"),
        "view-subagents key hint must render:\n{snap}",
    );
    assert!(
        snap.contains("view subagents"),
        "view-subagents label must render:\n{snap}",
    );
}

#[test]
fn view_subagents_hint_hidden_when_no_child_sessions_yet() {
    let mut h = Harness::new(140, 30);
    h.app
        .sessions
        .set_sessions(vec![SessionEntry::new("p", "Parent", "now")]);
    h.app.sessions.set_current(Some("p".into()));
    let task = ToolCall {
        id: Some("part-task".into()),
        name: "task".into(),
        status: ToolStatus::Running,
        title: "Explore Task".into(),
        ..Default::default()
    };
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Delegating...", "").with_tool(task),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("view subagents"),
        "hint must NOT render when no child session entry exists yet; got:\n{snap}",
    );
}

#[test]
fn view_subagents_hint_hidden_in_child_session() {
    let mut h = Harness::new(140, 30);
    seed_parent_with_children(&mut h, "p", &["c1"]);
    h.app.sessions.set_current(Some("c1".into()));
    let task = ToolCall {
        id: Some("part-task".into()),
        name: "task".into(),
        status: ToolStatus::Running,
        title: "Explore Task".into(),
        ..Default::default()
    };
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Delegating...", "").with_tool(task),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("view subagents"),
        "hint must be suppressed inside a child session (footer is already visible):\n{snap}",
    );
}

#[test]
fn switch_session_emits_legacy_switched_event_still() {
    let mut h = Harness::new(100, 24);
    h.app.sessions.set_sessions(vec![
        SessionEntry::new("a", "A", "now"),
        SessionEntry::new("b", "B", "now"),
    ]);
    h.app.sessions.set_current(Some("a".into()));
    h.clear_events();
    h.dispatch(Action::View(ViewAction::SwitchSession("b".into())));
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::SessionSwitched(id) if id == "b")));
    assert!(h
        .events()
        .iter()
        .all(|e| !matches!(e, Event::SubagentNavigate(_))));
}
