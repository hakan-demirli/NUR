// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn switching_session_clears_pending_permission_and_question_modals() {
    use raider_tui::session::SessionEntry;
    use raider_tui::{PermissionPrompt, PermissionView, QuestionPrompt};
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses-a", "Old", "today"),
        SessionEntry::new("ses-b", "New", "today"),
    ])));
    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "ses-a".into(),
    ))));
    h.dispatch(Action::Host(HostAction::PermissionAsked(
        PermissionPrompt {
            id: "perm-1".into(),
            session_id: "ses-a".into(),
            permission: "external_directory".into(),
            patterns: vec!["/tmp/x".into()],
            metadata: serde_json::Map::new(),
            always: vec![],
            view: PermissionView {
                icon: "←".into(),
                title: "Access /tmp/x".into(),
                detail: vec![],
            },
        },
    )));
    h.dispatch(Action::Host(HostAction::QuestionAsked(
        QuestionPrompt::new(
            "q-1",
            "ses-a",
            vec![raider_tui::QuestionInfo {
                question: "Continue?".into(),
                header: "Confirm".into(),
                options: vec![raider_tui::QuestionOption {
                    label: "yes".into(),
                    description: "".into(),
                }],
                multiple: false,
                custom_allowed: false,
            }],
        ),
    )));
    assert!(
        h.app.permissions.permission_active.is_some(),
        "permission modal must be active before switch",
    );
    h.dispatch(Action::View(ViewAction::SwitchSession("ses-b".into())));
    assert!(
        h.app.permissions.permission_active.is_none(),
        "permission modal must be cleared by session switch",
    );
    assert!(
        h.app.permissions.permission_queue.is_empty(),
        "permission queue must be drained by session switch",
    );
    assert!(
        h.app.questions.question_active.is_none(),
        "question modal must be cleared by session switch",
    );
    assert!(
        h.app.questions.question_queue.is_empty(),
        "question queue must be drained by session switch",
    );
}

#[test]
fn permission_modal_renders_title_and_buttons() {
    use raider_tui::{PermissionPrompt, PermissionView};
    let mut h = Harness::new(120, 20);
    let prompt = PermissionPrompt {
        id: "perm_1".into(),
        session_id: "ses_x".into(),
        permission: "external_directory".into(),
        patterns: vec!["/home/emre/.config/opencode-plugins/claude-auth/*".into()],
        metadata: serde_json::Map::new(),
        always: vec!["/home/emre/.config/opencode-plugins/claude-auth/*".into()],
        view: PermissionView {
            icon: "←".into(),
            title: "Access external directory ~/.config/opencode-plugins/claude-auth".into(),
            detail: vec![
                "Patterns".into(),
                "- /home/emre/.config/opencode-plugins/claude-auth/*".into(),
            ],
        },
    };
    h.dispatch(Action::Host(HostAction::PermissionAsked(prompt)));
    let snap = h.snapshot();
    assert!(
        snap.contains("Permission required"),
        "modal must render the `Permission required` header; snap:\n{snap}",
    );
    assert!(
        snap.contains("Access external directory"),
        "modal must render the per-permission title; snap:\n{snap}",
    );
    assert!(
        snap.contains("Allow once"),
        "modal must render the `Allow once` button label; snap:\n{snap}",
    );
    assert!(
        snap.contains("Allow always"),
        "modal must render the `Allow always` button label; snap:\n{snap}",
    );
    assert!(
        snap.contains("Reject"),
        "modal must render the `Reject` button label; snap:\n{snap}",
    );
}

#[test]
fn permission_modal_enter_emits_allow_once_event() {
    use raider_tui::event::PermissionReplyChoice;
    use raider_tui::{PermissionPrompt, PermissionView};
    let mut h = Harness::new(120, 20);
    h.dispatch(Action::Host(HostAction::PermissionAsked(
        PermissionPrompt {
            id: "perm_42".into(),
            session_id: "ses_y".into(),
            permission: "read".into(),
            patterns: Vec::new(),
            metadata: serde_json::Map::new(),
            always: Vec::new(),
            view: PermissionView {
                icon: "→".into(),
                title: "Read foo.rs".into(),
                detail: Vec::new(),
            },
        },
    )));
    h.dispatch(special(KeyCode::Enter));
    let evs: Vec<Event> = h.events().to_vec();
    assert!(
        evs.iter().any(|e| matches!(
            e,
            Event::PermissionReply { request_id, reply, message: None }
                if request_id == "perm_42" && *reply == PermissionReplyChoice::Once
        )),
        "Enter must emit PermissionReply(Once); got {evs:?}",
    );
    let snap = h.snapshot();
    assert!(
        !snap.contains("Permission required"),
        "modal must clear itself after a reply; snap:\n{snap}",
    );
}

#[test]
fn permission_modal_esc_emits_reject() {
    use raider_tui::event::PermissionReplyChoice;
    use raider_tui::{PermissionPrompt, PermissionView};
    let mut h = Harness::new(120, 20);
    h.dispatch(Action::Host(HostAction::PermissionAsked(
        PermissionPrompt {
            id: "perm_e".into(),
            session_id: "ses_y".into(),
            permission: "bash".into(),
            patterns: Vec::new(),
            metadata: serde_json::Map::new(),
            always: Vec::new(),
            view: PermissionView {
                icon: "#".into(),
                title: "Shell command".into(),
                detail: Vec::new(),
            },
        },
    )));
    h.dispatch(special(KeyCode::Esc));
    let evs: Vec<Event> = h.events().to_vec();
    assert!(
        evs.iter().any(|e| matches!(
            e,
            Event::PermissionReply { request_id, reply, .. }
                if request_id == "perm_e" && *reply == PermissionReplyChoice::Reject
        )),
        "Esc must emit PermissionReply(Reject); got {evs:?}",
    );
}

#[test]
fn question_modal_renders_question_and_options() {
    use raider_tui::{QuestionInfo, QuestionOption, QuestionPrompt};
    let prompt = QuestionPrompt::new(
        "q_1",
        "ses_x",
        vec![QuestionInfo {
            question: "What color do you prefer?".into(),
            header: "color".into(),
            options: vec![
                QuestionOption {
                    label: "Red".into(),
                    description: "Warm".into(),
                },
                QuestionOption {
                    label: "Blue".into(),
                    description: "Cool".into(),
                },
            ],
            multiple: false,
            custom_allowed: true,
        }],
    );
    let mut h = Harness::new(120, 20);
    h.dispatch(Action::Host(HostAction::QuestionAsked(prompt)));
    let snap = h.snapshot();
    assert!(
        snap.contains("What color do you prefer?"),
        "modal must render the question text; snap:\n{snap}",
    );
    assert!(
        snap.contains("Red"),
        "modal must render option Red; snap:\n{snap}"
    );
    assert!(
        snap.contains("Blue"),
        "modal must render option Blue; snap:\n{snap}",
    );
    assert!(
        snap.contains("Type your own answer"),
        "modal must render the custom-answer entry; snap:\n{snap}",
    );
}

#[test]
fn question_modal_enter_on_single_option_submits_answer() {
    use raider_tui::{QuestionInfo, QuestionOption, QuestionPrompt};
    let mut h = Harness::new(120, 20);
    h.dispatch(Action::Host(HostAction::QuestionAsked(
        QuestionPrompt::new(
            "q_red",
            "ses_x",
            vec![QuestionInfo {
                question: "Pick one".into(),
                header: "pick".into(),
                options: vec![
                    QuestionOption {
                        label: "Red".into(),
                        description: "".into(),
                    },
                    QuestionOption {
                        label: "Blue".into(),
                        description: "".into(),
                    },
                ],
                multiple: false,
                custom_allowed: false,
            }],
        ),
    )));
    h.dispatch(special(KeyCode::Enter));
    let evs: Vec<Event> = h.events().to_vec();
    let reply = evs
        .iter()
        .find_map(|e| match e {
            Event::QuestionReply {
                request_id,
                answers,
            } if request_id == "q_red" => Some(answers.clone()),
            _ => None,
        })
        .expect("must emit QuestionReply for q_red");
    assert_eq!(
        reply,
        vec![vec!["Red".to_string()]],
        "Enter on first option must submit `[[Red]]`",
    );
}

#[test]
fn question_modal_esc_emits_reject() {
    use raider_tui::{QuestionInfo, QuestionOption, QuestionPrompt};
    let mut h = Harness::new(120, 20);
    h.dispatch(Action::Host(HostAction::QuestionAsked(
        QuestionPrompt::new(
            "q_dismiss",
            "ses_x",
            vec![QuestionInfo {
                question: "Whatever".into(),
                header: "w".into(),
                options: vec![QuestionOption {
                    label: "Foo".into(),
                    description: "".into(),
                }],
                multiple: false,
                custom_allowed: false,
            }],
        ),
    )));
    h.dispatch(special(KeyCode::Esc));
    let evs: Vec<Event> = h.events().to_vec();
    assert!(
        evs.iter().any(|e| matches!(
            e,
            Event::QuestionReject { request_id } if request_id == "q_dismiss"
        )),
        "Esc must emit QuestionReject; got {evs:?}",
    );
}

#[test]
fn question_modal_renders_all_options_at_live_terminal_size() {
    use raider_tui::{QuestionInfo, QuestionOption, QuestionPrompt};
    let prompt = QuestionPrompt::new(
        "q_live",
        "ses_x",
        vec![QuestionInfo {
            question: "Which beverage do you prefer right now?".into(),
            header: "Pick one".into(),
            options: vec![
                QuestionOption {
                    label: "Coffee".into(),
                    description: "Hot, caffeinated, classic".into(),
                },
                QuestionOption {
                    label: "Tea".into(),
                    description: "Lighter caffeine, many varieties".into(),
                },
                QuestionOption {
                    label: "Water".into(),
                    description: "Plain, hydrating, neutral".into(),
                },
                QuestionOption {
                    label: "Energy drink".into(),
                    description: "Heavy caffeine, sweet".into(),
                },
            ],
            multiple: false,
            custom_allowed: true,
        }],
    );
    let mut h = Harness::new(180, 30);
    h.dispatch(Action::Host(HostAction::QuestionAsked(prompt)));
    let snap = h.snapshot();
    for label in &[
        "Coffee",
        "Tea",
        "Water",
        "Energy drink",
        "Type your own answer",
    ] {
        assert!(
            snap.contains(label),
            "option {label:?} must render in the question modal; snap:\n{snap}",
        );
    }
    for desc in &[
        "Hot, caffeinated, classic",
        "Lighter caffeine, many varieties",
        "Plain, hydrating, neutral",
        "Heavy caffeine, sweet",
    ] {
        assert!(
            snap.contains(desc),
            "description {desc:?} must render under its option; snap:\n{snap}",
        );
    }
}

#[test]
fn permission_modal_renders_external_directory_patterns() {
    use raider_tui::{PermissionPrompt, PermissionView};
    let prompt = PermissionPrompt {
        id: "perm_ed".into(),
        session_id: "ses_x".into(),
        permission: "external_directory".into(),
        patterns: vec![
            "/home/emre/.config/opencode-plugins/claude-auth/*".into(),
            "/home/emre/.config/opencode-plugins/claude-auth/**".into(),
        ],
        metadata: serde_json::Map::new(),
        always: vec!["/home/emre/.config/opencode-plugins/claude-auth/*".into()],
        view: PermissionView {
            icon: "←".into(),
            title: "Access external directory ~/.config/opencode-plugins/claude-auth".into(),
            detail: vec![
                "Patterns".into(),
                "- /home/emre/.config/opencode-plugins/claude-auth/*".into(),
                "- /home/emre/.config/opencode-plugins/claude-auth/**".into(),
            ],
        },
    };
    let mut h = Harness::new(180, 30);
    h.dispatch(Action::Host(HostAction::PermissionAsked(prompt)));
    let snap = h.snapshot();
    assert!(
        snap.contains("Patterns"),
        "modal must render the Patterns header; snap:\n{snap}",
    );
    assert!(
        snap.contains("/home/emre/.config/opencode-plugins/claude-auth/*"),
        "modal must render every pattern row; snap:\n{snap}",
    );
    assert!(
        snap.contains("/home/emre/.config/opencode-plugins/claude-auth/**"),
        "modal must render every pattern row; snap:\n{snap}",
    );
}

#[test]
fn question_modal_long_question_does_not_clip_type_your_own_answer_row() {
    use raider_tui::{QuestionInfo, QuestionOption, QuestionPrompt};
    let prompt = QuestionPrompt::new(
        "q_wrap",
        "ses_x",
        vec![QuestionInfo {
            question:
                "This is a deliberately long question that absolutely must wrap onto several visual rows when rendered into a narrow modal so that we exercise the wrap-aware height calculation path"
                    .into(),
            header: "wrap".into(),
            options: vec![
                QuestionOption { label: "Alpha".into(),   description: String::new() },
                QuestionOption { label: "Bravo".into(),   description: String::new() },
                QuestionOption { label: "Charlie".into(), description: String::new() },
                QuestionOption { label: "Delta".into(),   description: String::new() },
            ],
            multiple: false,
            custom_allowed: true,
        }],
    );
    let mut h = Harness::new(60, 30);
    h.dispatch(Action::Host(HostAction::QuestionAsked(prompt)));
    let snap = h.snapshot();
    for label in &["Alpha", "Bravo", "Charlie", "Delta", "Type your own answer"] {
        assert!(
            snap.contains(label),
            "row {label:?} must render even when the question text wraps; snap:\n{snap}",
        );
    }
}

#[test]
fn question_modal_long_option_labels_do_not_clip_custom_row() {
    use raider_tui::{QuestionInfo, QuestionOption, QuestionPrompt};
    let prompt = QuestionPrompt::new(
        "q_wide_opts",
        "ses_x",
        vec![QuestionInfo {
            question: "Pick".into(),
            header: "p".into(),
            options: vec![
                QuestionOption {
                    label: "Option one with a verbose description that will surely soft-wrap inside the modal"
                        .into(),
                    description: String::new(),
                },
                QuestionOption {
                    label: "Option two with another sufficiently long label that pushes the layout further down"
                        .into(),
                    description: String::new(),
                },
                QuestionOption {
                    label: "Option three rounding things out with yet more text that the wrapper must account for"
                        .into(),
                    description: String::new(),
                },
                QuestionOption { label: "Option four short".into(), description: String::new() },
            ],
            multiple: false,
            custom_allowed: true,
        }],
    );
    let mut h = Harness::new(60, 30);
    h.dispatch(Action::Host(HostAction::QuestionAsked(prompt)));
    let snap = h.snapshot();
    assert!(
        snap.contains("Type your own answer"),
        "custom-answer row must remain visible even when preset option labels wrap; snap:\n{snap}",
    );
}

#[test]
fn question_modal_dismissed_on_server_replied_event() {
    use raider_tui::{QuestionInfo, QuestionPrompt};
    let mut h = Harness::new(120, 20);
    h.dispatch(Action::Host(HostAction::QuestionAsked(
        QuestionPrompt::new(
            "q_remote",
            "ses_x",
            vec![QuestionInfo {
                question: "Remote".into(),
                header: "r".into(),
                options: Vec::new(),
                multiple: false,
                custom_allowed: true,
            }],
        ),
    )));
    assert!(h.snapshot().contains("Remote"));
    h.dispatch(Action::Host(HostAction::QuestionDismissed(
        "q_remote".into(),
    )));
    assert!(
        !h.snapshot().contains("Remote"),
        "modal must dismiss when the matching dismissed action arrives",
    );
}

fn seed_fork_session(h: &mut Harness) {
    use raider_tui::session::SessionEntry;
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses-fork", "Forkable", "today"),
    ])));
    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "ses-fork".into(),
    ))));
}

#[test]
fn command_palette_lists_session_fork_entry() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    h.dispatch(Action::View(ViewAction::OpenCommandPalette));
    let snap = h.snapshot();
    assert!(
        snap.contains("Fork session"),
        "command palette must list 'Fork session'; snap:\n{snap}",
    );
}

#[test]
fn slash_fork_opens_fork_picker_with_full_session_anchor() {
    use raider_tui::dialog::DialogKind;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    type_text(&mut h, "/fork");
    h.dispatch(Action::User(UserAction::SubmitInput));
    assert_eq!(
        h.app.dialog_kind(),
        Some(DialogKind::ForkPicker),
        "/fork must open the ForkPicker dialog",
    );
    let snap = h.snapshot();
    assert!(
        snap.contains("Full session"),
        "fork picker must include the 'Full session' anchor option; snap:\n{snap}",
    );
}

#[test]
fn fork_picker_shows_user_messages_newest_first() {
    use raider_tui::dialog::DialogKind;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("oldest user prompt").with_server_id("m-1"),
    )));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::assistant("ok", "").with_server_id("m-2"),
    )));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("newest user prompt").with_server_id("m-3"),
    )));
    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    assert_eq!(h.app.dialog_kind(), Some(DialogKind::ForkPicker));
    let visible = h
        .app
        .dialogs
        .dialog
        .as_ref()
        .expect("dialog open")
        .visible_options();
    let labels: Vec<&str> = visible.iter().map(|o| o.title.as_str()).collect();
    assert_eq!(
        labels,
        vec!["Full session", "newest user prompt", "oldest user prompt"],
        "Full session first, then user messages newest-first (matches opencode order)",
    );
}

#[test]
fn confirming_fork_picker_full_session_emits_fork_event_without_message_id() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    h.clear_events();
    h.dispatch(special(KeyCode::Enter));
    let fork_event = h.events().iter().find_map(|e| match e {
        Event::ForkSession { message_id } => Some(message_id.clone()),
        _ => None,
    });
    assert_eq!(
        fork_event,
        Some(None),
        "Enter on 'Full session' must emit ForkSession {{ message_id: None }}; events: {:?}",
        h.events(),
    );
}

#[test]
fn confirming_fork_picker_user_message_emits_fork_with_message_id() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("first user prompt").with_server_id("msg-1"),
    )));
    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    h.dispatch(special(KeyCode::Down));
    h.clear_events();
    h.dispatch(special(KeyCode::Enter));
    let fork_event = h.events().iter().find_map(|e| match e {
        Event::ForkSession { message_id } => Some(message_id.clone()),
        _ => None,
    });
    assert_eq!(
        fork_event,
        Some(Some("msg-1".to_string())),
        "selecting a user-message row must emit ForkSession with that server_id; events: {:?}",
        h.events(),
    );
}

#[test]
fn fork_picker_includes_locally_submitted_user_message_after_bind() {
    use raider_tui::dialog::DialogKind;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);

    type_text(&mut h, "draft me a plan");
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::BindLastUserMessage {
        server_id: "msg-plan-1".into(),
        agent: Some("plan".into()),
    }));

    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    assert_eq!(h.app.dialog_kind(), Some(DialogKind::ForkPicker));
    let visible = h
        .app
        .dialogs
        .dialog
        .as_ref()
        .expect("dialog open")
        .visible_options();
    let labels: Vec<&str> = visible.iter().map(|o| o.title.as_str()).collect();
    assert!(
        labels.contains(&"draft me a plan"),
        "fork picker must include locally-submitted user message after bind; got: {labels:?}",
    );

    let bound = h
        .app
        .messages
        .iter()
        .find(|m| m.server_id.as_deref() == Some("msg-plan-1"))
        .expect("bound message present");
    assert_eq!(bound.agent.as_deref(), Some("plan"));
}

#[test]
fn bind_last_user_message_targets_oldest_untagged_user_in_fifo_order() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);

    type_text(&mut h, "first plan");
    h.dispatch(Action::User(UserAction::SubmitInput));
    type_text(&mut h, "second plan");
    h.dispatch(Action::User(UserAction::SubmitInput));

    h.dispatch(Action::Host(HostAction::BindLastUserMessage {
        server_id: "msg-A".into(),
        agent: Some("plan".into()),
    }));
    h.dispatch(Action::Host(HostAction::BindLastUserMessage {
        server_id: "msg-B".into(),
        agent: Some("plan".into()),
    }));

    let users: Vec<(Option<&str>, &str)> = h
        .app
        .messages
        .iter()
        .filter(|m| m.sender == raider_tui::Sender::User)
        .map(|m| (m.server_id.as_deref(), m.content.as_str()))
        .collect();
    assert_eq!(
        users,
        vec![
            (Some("msg-A"), "first plan"),
            (Some("msg-B"), "second plan"),
        ],
        "binds must consume oldest-untagged-first to match the host's FIFO prompt queue",
    );
}

#[test]
fn opening_fork_picker_without_active_session_pushes_system_message_not_dialog() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    assert!(
        h.app.dialog_kind().is_none(),
        "no dialog should be opened without an active session",
    );
    let snap = h.snapshot();
    assert!(
        snap.contains("No active session"),
        "must surface guidance when /fork is run without an active session; snap:\n{snap}",
    );
}

#[test]
fn fork_picker_includes_compaction_row_with_recognizable_label() {
    use raider_tui::dialog::DialogKind;
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("pre-compaction prompt").with_server_id("msg-pre"),
    )));
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-1".into(),
        marker: CompactionMarker { auto: false },
    }));

    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    assert_eq!(h.app.dialog_kind(), Some(DialogKind::ForkPicker));
    let visible = h
        .app
        .dialogs
        .dialog
        .as_ref()
        .expect("dialog open")
        .visible_options();
    let labels: Vec<&str> = visible.iter().map(|o| o.title.as_str()).collect();
    assert!(
        labels.contains(&"Compaction"),
        "fork picker must surface a `Compaction` row so users can fork to undo /compact; \
         got: {labels:?}",
    );
    assert!(
        !labels.contains(&"(empty message)"),
        "compaction rows must NOT be mislabeled as `(empty message)`; got: {labels:?}",
    );
}

#[test]
fn fork_picker_compaction_row_uses_auto_label_when_auto_true() {
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-auto-comp".into(),
        marker: CompactionMarker { auto: true },
    }));
    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    let visible = h
        .app
        .dialogs
        .dialog
        .as_ref()
        .expect("dialog open")
        .visible_options();
    let labels: Vec<&str> = visible.iter().map(|o| o.title.as_str()).collect();
    assert!(
        labels.contains(&"Auto Compaction"),
        "auto compaction rows must be labeled `Auto Compaction` to match the in-chat divider; \
         got: {labels:?}",
    );
}

#[test]
fn confirming_fork_picker_compaction_row_emits_fork_with_compaction_message_id() {
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);
    h.dispatch(Action::Host(HostAction::MarkCompaction {
        message_id: "msg-comp-fork".into(),
        marker: CompactionMarker { auto: false },
    }));
    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    h.dispatch(special(KeyCode::Down));
    h.clear_events();
    h.dispatch(special(KeyCode::Enter));
    let fork_event = h.events().iter().find_map(|e| match e {
        Event::ForkSession { message_id } => Some(message_id.clone()),
        _ => None,
    });
    assert_eq!(
        fork_event,
        Some(Some("msg-comp-fork".to_string())),
        "selecting the compaction row must emit ForkSession with the compaction's \
         opencode message id; events: {:?}",
        h.events(),
    );
}

#[test]
fn fork_picker_compaction_row_survives_bulk_refresh_path() {
    use raider_tui::dialog::DialogKind;
    use raider_tui::model::CompactionMarker;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    seed_fork_session(&mut h);

    let mut compaction_msg = raider_tui::action::HostMessage::user(String::new());
    compaction_msg.server_id = Some("msg-comp-bulk".into());
    compaction_msg.compaction = Some(CompactionMarker { auto: false });
    h.dispatch(Action::Host(HostAction::ReplaceMessages(vec![
        compaction_msg,
    ])));

    h.dispatch(Action::View(ViewAction::OpenForkPicker));
    assert_eq!(h.app.dialog_kind(), Some(DialogKind::ForkPicker));
    let visible = h
        .app
        .dialogs
        .dialog
        .as_ref()
        .expect("dialog open")
        .visible_options();
    let labels: Vec<&str> = visible.iter().map(|o| o.title.as_str()).collect();
    assert!(
        labels.contains(&"Compaction"),
        "compaction rows from the bulk-refresh path (session reload) must also be \
         labeled `Compaction`, not `(empty message)`; got: {labels:?}",
    );
}
