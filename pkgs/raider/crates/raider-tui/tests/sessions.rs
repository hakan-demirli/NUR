// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn sessions_picker_warns_when_empty() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::Command("/sessions".into())));
    assert!(
        h.app.dialogs.dialog.is_none(),
        "empty list does not open a dialog"
    );
    let snap = h.snapshot();
    assert!(
        snap.contains("no session list yet"),
        "warning surfaced:\n{snap}"
    );
}

#[test]
fn sessions_picker_lists_host_supplied_entries() {
    use raider_tui::SessionEntry;
    let mut h = Harness::new(120, 30);
    h.app.sessions.set_sessions(vec![
        SessionEntry::new("s-1", "Refactor raider", "9:44 PM"),
        SessionEntry::new("s-2", "Greeting", "6:23 PM"),
    ]);
    h.app.sessions.set_current(Some("s-2".into()));

    h.dispatch(Action::View(ViewAction::Command("/sessions".into())));
    let dialog = h.app.dialogs.dialog.as_ref().expect("picker opens");
    let titles: Vec<String> = dialog
        .visible_options()
        .iter()
        .map(|o| o.title.clone())
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("Refactor raider")),
        "Refactor option visible: {titles:?}"
    );
    assert!(
        titles.iter().any(|t| t.contains("Greeting")),
        "Greeting option visible: {titles:?}"
    );
    assert_eq!(dialog.current_value, "s-2", "current session preselected");
}

#[test]
fn session_picker_enter_emits_switch_event() {
    use raider_tui::SessionEntry;
    let mut h = Harness::new(120, 30);
    h.app.sessions.set_sessions(vec![
        SessionEntry::new("s-1", "First", "9:44 PM"),
        SessionEntry::new("s-2", "Second", "6:23 PM"),
    ]);
    h.dispatch(Action::View(ViewAction::OpenSessionPicker));
    h.clear_events();
    h.dispatch(special(KeyCode::Enter));

    assert!(h.app.dialogs.dialog.is_none());
    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("s-1"));
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::SessionSwitched(id) if id == "s-1")));
}

#[test]
fn switching_back_restores_cached_transcript_immediately() {
    use raider_tui::{HostMessage, SessionEntry};

    let mut h = Harness::new(120, 30);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("s-1", "First", "9:44 PM"),
        SessionEntry::new("s-2", "Second", "6:23 PM"),
    ])));
    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "s-1".into(),
    ))));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "live worker output".into(),
        thoughts: false,
        message_id: None,
    }));

    h.dispatch(Action::View(ViewAction::SwitchSession("s-2".into())));
    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("s-2"));
    assert!(
        h.app.messages.messages.is_empty(),
        "uncached target should clear immediately instead of showing old transcript"
    );

    h.dispatch(Action::Host(HostAction::ReplaceMessages(vec![
        HostMessage::user("judge transcript"),
    ])));
    h.dispatch(Action::View(ViewAction::SwitchSession("s-1".into())));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("s-1"));
    assert_eq!(h.app.messages.messages.len(), 1);
    assert_eq!(h.app.messages.messages[0].content, "live worker output");
}

#[test]
fn new_session_adoption_preserves_optimistic_first_message() {
    let mut h = Harness::new(120, 30);
    pin_dummy_model(&mut h);

    h.dispatch(Action::User(UserAction::PasteText("sup".into())));
    h.dispatch(special(KeyCode::Enter));

    assert_eq!(h.app.sessions.sessions.current, None);
    assert!(
        h.app.messages.messages.iter().any(|m| m.content == "sup"),
        "submitted prompt should render optimistically before the host creates a session"
    );

    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "ses_new".into(),
    ))));

    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("ses_new"));
    assert!(
        h.app.messages.messages.iter().any(|m| m.content == "sup"),
        "adopting the new backend session must not clear the optimistic first prompt"
    );
}

#[test]
fn switching_back_preserves_render_cache_for_recent_messages() {
    use raider_tui::{HostMessage, HostMessagePart, SessionEntry};

    let mut h = Harness::new(120, 30);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("s-1", "First", "9:44 PM"),
        SessionEntry::new("s-2", "Second", "6:23 PM"),
    ])));
    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "s-1".into(),
    ))));

    let mut message = HostMessage::assistant("cached ordered markdown", "");
    message.parts = vec![HostMessagePart::Text("cached ordered markdown".into())];
    h.dispatch(Action::Host(HostAction::ReplaceMessages(vec![message])));
    assert!(
        !h.app.messages.messages[0].part_render_cache.is_empty(),
        "initial draw should populate ordered assistant part cache"
    );

    h.dispatch(Action::View(ViewAction::SwitchSession("s-2".into())));
    h.dispatch(Action::View(ViewAction::SwitchSession("s-1".into())));

    assert!(
        !h.app.messages.messages[0].part_render_cache.is_empty(),
        "switch away/back must preserve render cache for recent messages"
    );
}

#[test]
fn rendering_large_transcript_only_builds_recent_tail() {
    use raider_tui::HostMessage;

    let mut h = Harness::new(120, 30);
    let messages: Vec<_> = (0..150)
        .map(|i| HostMessage::user(format!("message {i}")))
        .collect();
    h.dispatch(Action::Host(HostAction::ReplaceMessages(messages)));

    let rendered_count = h
        .app
        .messages
        .messages
        .iter()
        .filter(|m| m.rendered_content_cache.is_some())
        .count();
    assert_eq!(
        rendered_count, 100,
        "rendering should cache only the recent transcript tail"
    );
    assert!(
        h.app.messages.messages[..50]
            .iter()
            .all(|m| m.rendered_content_cache.is_none()),
        "older messages should not be rendered/cached"
    );
    assert!(
        h.app.messages.messages[50..]
            .iter()
            .all(|m| m.rendered_content_cache.is_some()),
        "recent tail should be rendered/cached"
    );
}

#[test]
fn slash_sessions_with_arg_switches_directly() {
    use raider_tui::SessionEntry;
    let mut h = Harness::new(120, 30);
    h.app
        .sessions
        .set_sessions(vec![SessionEntry::new("s-42", "Title", "9:44 PM")]);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("/sessions s-42".into())));
    assert_eq!(h.app.sessions.sessions.current.as_deref(), Some("s-42"));
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::SessionSwitched(id) if id == "s-42")));
}

#[test]
fn unknown_session_id_emits_warning_no_event() {
    let mut h = Harness::new(120, 30);
    h.dispatch(Action::View(ViewAction::SwitchSession("nope".into())));
    assert!(h
        .events()
        .iter()
        .all(|e| !matches!(e, Event::SessionSwitched(_))));
    let snap = h.snapshot();
    assert!(snap.contains("unknown session"), "warning visible:\n{snap}");
}

#[test]
fn plugin_navigation_accepts_session_not_in_local_picker_snapshot() {
    let mut h = Harness::new(120, 30);
    h.dispatch(Action::View(ViewAction::PluginNavigateSession(
        "ses_judge".into(),
    )));
    assert_eq!(
        h.events(),
        &[Event::SessionSwitched("ses_judge".to_string())]
    );
    assert_eq!(
        h.app.sessions.sessions.current.as_deref(),
        Some("ses_judge")
    );
}

#[test]
fn host_upsert_session_inserts_new_id_at_head() {
    use raider_tui::session::SessionEntry;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses-old-a", "Old A", "yesterday"),
        SessionEntry::new("ses-old-b", "Old B", "two days ago"),
    ])));
    h.dispatch(Action::Host(HostAction::UpsertSession(SessionEntry::new(
        "ses-from-other-tui",
        "Hello from Tab 2",
        "just now",
    ))));
    let entries = &h.app.sessions.sessions.entries;
    assert_eq!(
        entries.len(),
        3,
        "new id must be inserted, not merged; got {entries:?}",
    );
    assert_eq!(
        entries[0].id, "ses-from-other-tui",
        "newest session must float to head (matches opencode's \
         time.updated DESC order); got {entries:?}",
    );
}

#[test]
fn host_upsert_session_replaces_existing_id_in_place() {
    use raider_tui::session::SessionEntry;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses-1", "Original title", "yesterday"),
        SessionEntry::new("ses-2", "Second session", "today"),
    ])));
    h.dispatch(Action::Host(HostAction::UpsertSession(SessionEntry::new(
        "ses-1",
        "Renamed title",
        "just now",
    ))));
    let entries = &h.app.sessions.sessions.entries;
    assert_eq!(entries.len(), 2, "in-place update must not grow the list");
    assert_eq!(entries[0].id, "ses-1");
    assert_eq!(
        entries[0].title, "Renamed title",
        "metadata must be the freshly-upserted shape",
    );
    assert_eq!(
        entries[0].updated_label, "just now",
        "updated_label must reflect the latest event",
    );
}

#[test]
fn host_remove_session_drops_entry_and_ignores_unknown_id() {
    use raider_tui::session::SessionEntry;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses-1", "Keep", "today"),
        SessionEntry::new("ses-2", "Goodbye", "today"),
    ])));
    h.dispatch(Action::Host(HostAction::RemoveSession("ses-2".into())));
    let entries = &h.app.sessions.sessions.entries;
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].id, "ses-1");
    h.dispatch(Action::Host(HostAction::RemoveSession("ses-2".into())));
    assert_eq!(h.app.sessions.sessions.entries.len(), 1);
    h.dispatch(Action::Host(HostAction::RemoveSession(
        "ses-never-seen".into(),
    )));
    assert_eq!(h.app.sessions.sessions.entries.len(), 1);
}

#[test]
fn newly_pinned_session_appends_to_end_of_pinned_list() {
    let mut h = Harness::new(80, 24);
    h.app.sessions.toggle_pin("s-a".to_string());
    h.app.sessions.toggle_pin("s-b".to_string());
    h.app.sessions.toggle_pin("s-c".to_string());

    assert_eq!(
        h.app.sessions.pinned_sessions,
        vec!["s-a".to_string(), "s-b".to_string(), "s-c".to_string()],
        "each new pin must be appended; old behavior inserted at index 0",
    );

    h.app.sessions.toggle_pin("s-a".to_string());
    h.app.sessions.toggle_pin("s-a".to_string());
    assert_eq!(
        h.app.sessions.pinned_sessions,
        vec!["s-b".to_string(), "s-c".to_string(), "s-a".to_string()],
        "unpin + re-pin must append to end",
    );
}

#[test]
fn slash_resume_and_continue_alias_to_session_picker() {
    use raider_tui::app::SlashCommand;
    for slash in ["/resume", "/continue"] {
        match SlashCommand::parse(slash) {
            SlashCommand::Action(action) => {
                assert_eq!(*action, Action::View(ViewAction::OpenSessionPicker))
            }
            other => panic!(
                "expected Action::View(ViewAction::OpenSessionPicker) for {slash}; got {other:?}"
            ),
        }
    }
}
