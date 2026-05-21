// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn host_mark_compaction_renders_centered_divider() {
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-1".into(),
        marker: CompactionMarker { auto: false },
    }));
    let snap = h.snapshot();
    assert!(
        snap.contains("Compaction"),
        "compaction divider must surface the literal `Compaction` title; snap:\n{snap}",
    );
    assert!(
        snap.contains('─'),
        "compaction divider must render box-drawing rule chars; snap:\n{snap}",
    );
    let user_lines: Vec<&str> = snap
        .lines()
        .filter(|l| l.contains("User") && !l.contains("Compaction"))
        .collect();
    assert!(
        user_lines.is_empty(),
        "compaction message must NOT also emit a `User` bubble label; \
         offending lines: {user_lines:?}\nsnap:\n{snap}",
    );
}

#[test]
fn host_mark_compaction_is_idempotent_per_message_id() {
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-dup".into(),
        marker: CompactionMarker { auto: false },
    }));
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-dup".into(),
        marker: CompactionMarker { auto: false },
    }));
    let snap = h.snapshot();
    let occurrences = snap.matches("Compaction").count();
    assert_eq!(
        occurrences, 1,
        "duplicate `HostMarkCompaction` with the same id must coalesce \
         onto a single divider; got {occurrences} `Compaction` occurrences \
         in snap:\n{snap}",
    );
}

#[test]
fn host_set_vcs_branch_recomposes_footer_path_for_both_surfaces() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::SetWorkspaceCwd(Some(
        "~/Desktop/raider".into(),
    ))));
    h.dispatch(Action::Host(HostAction::SetVcsBranch(Some("main".into()))));
    assert_eq!(
        h.app.sidebar.sidebar.footer_path.as_deref(),
        Some("~/Desktop/raider:main"),
    );
    assert_eq!(
        h.app.prompt.prompt_info.right.as_deref(),
        Some("~/Desktop/raider:main"),
        "prompt footer right must stay in lockstep with sidebar footer path",
    );
    h.dispatch(Action::Host(HostAction::SetVcsBranch(Some(
        "feat/foo".into(),
    ))));
    assert_eq!(
        h.app.sidebar.sidebar.footer_path.as_deref(),
        Some("~/Desktop/raider:feat/foo"),
    );
    h.dispatch(Action::Host(HostAction::SetVcsBranch(None)));
    assert_eq!(
        h.app.sidebar.sidebar.footer_path.as_deref(),
        Some("~/Desktop/raider"),
        "detached HEAD must drop the `:branch` suffix",
    );
}

#[test]
fn host_set_session_busy_flips_entry_flag() {
    use raider_tui::session::SessionEntry;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses-a", "Idle", "today"),
        SessionEntry::new("ses-b", "Working", "today"),
    ])));
    h.dispatch(Action::Host(HostAction::SetSessionBusy {
        session_id: "ses-b".into(),
        busy: true,
    }));
    let ses_a = h
        .app
        .sessions
        .sessions
        .entries
        .iter()
        .find(|e| e.id == "ses-a")
        .unwrap();
    let ses_b = h
        .app
        .sessions
        .sessions
        .entries
        .iter()
        .find(|e| e.id == "ses-b")
        .unwrap();
    assert!(!ses_a.busy, "untouched session must stay idle");
    assert!(ses_b.busy, "matching id must flip busy=true");
    h.dispatch(Action::Host(HostAction::SetSessionBusy {
        session_id: "ses-b".into(),
        busy: false,
    }));
    let ses_b = h
        .app
        .sessions
        .sessions
        .entries
        .iter()
        .find(|e| e.id == "ses-b")
        .unwrap();
    assert!(!ses_b.busy, "busy=false must clear the flag");
    h.dispatch(Action::Host(HostAction::SetSessionBusy {
        session_id: "ses-never".into(),
        busy: true,
    }));
}

#[test]
fn host_remove_message_drops_matching_server_id() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("keep me").with_server_id("msg-keep"),
    )));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("drop me").with_server_id("msg-drop"),
    )));
    assert_eq!(h.app.messages.len(), 2);
    h.dispatch(Action::Host(HostAction::RemoveMessage("msg-drop".into())));
    assert_eq!(h.app.messages.len(), 1);
    assert_eq!(
        h.app.messages.messages[0].server_id.as_deref(),
        Some("msg-keep")
    );
    h.dispatch(Action::Host(HostAction::RemoveMessage("msg-never".into())));
    assert_eq!(h.app.messages.len(), 1);
}

#[test]
fn host_mark_compaction_auto_flag_changes_title() {
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-auto".into(),
        marker: CompactionMarker { auto: true },
    }));
    let snap = h.snapshot();
    assert!(
        snap.contains("Auto Compaction"),
        "auto compaction divider must show `Auto Compaction` title; snap:\n{snap}",
    );
}

#[test]
fn host_mark_compaction_stamps_server_id_on_synthetic_message() {
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-stamped".into(),
        marker: CompactionMarker { auto: false },
    }));
    let comp = h
        .app
        .messages
        .iter()
        .find(|m| m.compaction.is_some())
        .expect("compaction message must be inserted into the store");
    assert_eq!(
        comp.server_id.as_deref(),
        Some("msg-comp-stamped"),
        "mark_compaction must stamp the synthetic message's server_id with the \
         opencode message id so the fork picker can use it as an anchor",
    );
    assert!(
        comp.content.is_empty(),
        "compaction synthetic row stays content-empty; the renderer short-circuits \
         it to a divider via the compaction marker, not via content",
    );
}

#[test]
fn host_mark_compaction_idempotent_with_server_id_stamp() {
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-dup-2".into(),
        marker: CompactionMarker { auto: false },
    }));
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-dup-2".into(),
        marker: CompactionMarker { auto: false },
    }));
    let comp_rows: Vec<_> = h
        .app
        .messages
        .iter()
        .filter(|m| m.compaction.is_some())
        .collect();
    assert_eq!(
        comp_rows.len(),
        1,
        "duplicate MarkCompaction with the same id must still produce a single row \
         even after the server_id stamp",
    );
    assert_eq!(
        comp_rows[0].server_id.as_deref(),
        Some("msg-comp-dup-2"),
        "the surviving (deduped) row must carry the server_id stamp",
    );
}
