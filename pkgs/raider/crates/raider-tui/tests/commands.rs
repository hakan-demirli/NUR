// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn typing_then_submit_emits_user_message_event() {
    let mut h = Harness::new(100, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("hello world".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));

    assert_eq!(h.events(), &[Event::UserMessage("hello world".to_string())],);

    let snap = h.snapshot();
    assert!(snap.contains("hello world"), "user text:\n{snap}");
    assert!(
        !snap.lines().any(|l| l.trim_end().ends_with("┃  User")),
        "opencode user messages do not render a default `User` footer:\n{snap}"
    );
}

#[test]
fn unknown_slash_command_is_forwarded_as_event() {
    let mut h = Harness::new(80, 24);
    type_text(&mut h, "/wibble foo");
    h.dispatch(Action::User(UserAction::SubmitInput));

    assert_eq!(
        h.events(),
        &[Event::Command {
            name: "wibble".to_string(),
            args: "foo".to_string()
        }],
    );
}

#[test]
fn registered_plugin_slash_emits_plugin_command_event() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::Host(HostAction::RegisterPluginCommands(vec![
        PluginCommand {
            name: "office.judge".into(),
            title: "Judge".into(),
            description: Some("Review the current answer".into()),
            category: Some("office".into()),
            slash_name: Some("judge".into()),
            slash_aliases: vec!["j".into()],
        },
    ])));
    h.clear_events();

    type_text(&mut h, "/judge now");
    h.dispatch(Action::User(UserAction::SubmitInput));

    assert_eq!(
        h.events(),
        &[Event::PluginCommand {
            name: "office.judge".to_string(),
            args: "now".to_string()
        }],
    );
}

#[test]
fn plugin_select_emits_selected_value_and_skips_disabled_options() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::Host(HostAction::OpenPluginSelect {
        callback_id: 7,
        title: "Pick one".into(),
        placeholder: None,
        options: vec![
            PluginDialogOption {
                title: "Unavailable".into(),
                value: "no".into(),
                description: None,
                category: None,
                disabled: true,
            },
            PluginDialogOption {
                title: "Available".into(),
                value: "yes".into(),
                description: None,
                category: None,
                disabled: false,
            },
        ],
    }));

    h.dispatch(special(KeyCode::Enter));

    assert_eq!(
        h.events(),
        &[Event::PluginDialogSelected {
            callback_id: 7,
            value: "yes".to_string()
        }],
    );
}

#[test]
fn plugin_select_escape_emits_dismissed_event() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::Host(HostAction::OpenPluginSelect {
        callback_id: 9,
        title: "Pick one".into(),
        placeholder: None,
        options: vec![PluginDialogOption {
            title: "Available".into(),
            value: "yes".into(),
            description: None,
            category: None,
            disabled: false,
        }],
    }));

    h.dispatch(special(KeyCode::Esc));

    assert_eq!(
        h.events(),
        &[Event::PluginDialogDismissed { callback_id: 9 }],
    );
}

#[test]
fn slash_exit_quits() {
    let mut h = Harness::new(80, 24);
    type_text(&mut h, "/exit");
    h.dispatch(Action::User(UserAction::SubmitInput));
    assert!(h.app.should_quit());
    assert!(h.events().contains(&Event::Quit));
}

#[test]
fn slash_help_opens_help_dialog_not_command_palette() {
    let mut h = Harness::new(100, 24);
    h.dispatch(Action::View(ViewAction::Command("/help".into())));

    let snap = h.snapshot();
    assert!(snap.contains("Help"), "help dialog title missing:\n{snap}");
    assert!(
        snap.contains("Press Ctrl+P to see all available actions"),
        "help dialog body should mirror opencode DialogHelp; snap:\n{snap}",
    );
    assert!(
        !snap.contains("Switch model"),
        "/help must not open the command palette directly; snap:\n{snap}",
    );
}

#[test]
fn clear_command_wipes_transcript_and_forwards_event() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::User(UserAction::PasteText("a".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "b".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));

    assert!(!h.app.messages.is_empty());

    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("/clear".into())));

    assert!(h.app.messages.is_empty());
    assert_eq!(
        h.events(),
        &[Event::Command {
            name: "clear".to_string(),
            args: String::new()
        }],
    );
}

#[test]
fn system_message_renders() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::Host(HostAction::SystemMessage(
        "connecting…".to_string(),
    )));
    let snap = h.snapshot();
    assert!(snap.contains("connecting"), "snap:\n{snap}");
    assert!(h.events().is_empty());
}

#[test]
fn empty_submit_is_a_noop() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::User(UserAction::SubmitInput));
    assert!(h.events().is_empty());
    assert!(h.app.messages.is_empty());
}

#[test]
fn ctrl_p_opens_command_palette() {
    let mut h = Harness::new(100, 30);
    h.dispatch(ctrl('p'));
    let dialog = h.app.dialogs.dialog.as_ref().expect("palette open");
    assert!(dialog
        .visible_options()
        .iter()
        .any(|o| o.title.contains("Switch theme")),);
}

#[test]
fn command_palette_lists_toggle_sidebar() {
    let mut h = Harness::new(120, 30);
    h.dispatch(ctrl('p'));
    let palette = h.app.dialogs.dialog.as_ref().expect("palette open");
    assert!(
        palette
            .visible_options()
            .iter()
            .any(|o| o.title.contains("Toggle sidebar")),
        "command palette must include the sidebar toggle"
    );
}

#[test]
fn timestamps_hidden_by_default() {
    assert!(!Harness::new(80, 24).app.messages.show_timestamps);
}

#[test]
fn slash_timestamps_toggles_visibility() {
    let mut h = Harness::new(100, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("hi".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));

    let snap_before = h.snapshot();
    assert!(
        !snap_before.contains("User, "),
        "default state hides user timestamp metadata:\n{snap_before}"
    );
    assert!(
        !snap_before
            .lines()
            .any(|l| l.trim_end().ends_with("┃  User")),
        "default state also hides the legacy `User` footer:\n{snap_before}"
    );

    h.dispatch(Action::View(ViewAction::Command("/timestamps".into())));
    h.draw();
    assert!(h.app.messages.show_timestamps);
    let snap_after = h.snapshot();
    assert!(
        snap_after.lines().any(|l| l.contains("┃  00:00")),
        "after toggle only the timestamp appears in the user metadata row:\n{snap_after}"
    );
    assert!(
        !snap_after.contains("User, ")
            && !snap_after
                .lines()
                .any(|l| l.trim_end().ends_with("┃  User")),
        "timestamp metadata must not include the `User` label:\n{snap_after}"
    );

    h.dispatch(Action::View(ViewAction::Command(
        "/toggle-timestamps".into(),
    )));
    assert!(!h.app.messages.show_timestamps, "alias also toggles");
}

#[test]
fn slash_export_emits_event_with_markdown_and_filename() {
    let mut h = Harness::new(100, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("ping".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "pong".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));
    h.clear_events();

    h.dispatch(Action::View(ViewAction::Command("/export".into())));

    let export = h
        .events()
        .iter()
        .find_map(|e| match e {
            Event::Export {
                suggested_filename,
                markdown,
            } => Some((suggested_filename.clone(), markdown.clone())),
            _ => None,
        })
        .expect("/export emits Event::Export");
    assert!(
        export.0.starts_with("session-") && export.0.ends_with(".md"),
        "filename matches `session-*.md`: {:?}",
        export.0
    );
    assert!(
        export.1.contains("# Session"),
        "markdown starts with the session header:\n{}",
        export.1
    );
    assert!(
        export.1.contains("ping") && export.1.contains("pong"),
        "transcript carries both turns:\n{}",
        export.1
    );
    assert!(
        !h.snapshot().contains("Session exported to"),
        "export confirmation is a live-binary toast after the file write, \
         not a transcript SystemMessage"
    );
}

#[test]
fn command_palette_lists_new_session_commands() {
    let mut h = Harness::new(120, 30);
    h.dispatch(ctrl('p'));
    let palette = h.app.dialogs.dialog.as_ref().expect("palette open");
    let titles: Vec<String> = palette
        .visible_options()
        .iter()
        .map(|o| o.title.clone())
        .collect();
    for want in [
        "Toggle timestamps",
        "Export session transcript",
        "Switch session",
    ] {
        assert!(
            titles.iter().any(|t| t.contains(want)),
            "{want} missing from palette: {titles:?}"
        );
    }
}

#[test]
fn command_palette_filter_left_right_edit_cursor() {
    let mut h = Harness::new(120, 30);
    h.dispatch(ctrl('p'));
    for c in "abc".chars() {
        h.dispatch(key(c));
    }
    {
        let dialog = h.app.dialogs.dialog.as_ref().expect("palette open");
        assert_eq!(dialog.filter, "abc");
        assert_eq!(dialog.filter_cursor_position, 3);
    }

    h.dispatch(special(KeyCode::Left));
    h.dispatch(special(KeyCode::Left));
    {
        let dialog = h.app.dialogs.dialog.as_ref().expect("palette open");
        assert_eq!(dialog.filter_cursor_position, 1);
    }

    h.dispatch(key('X'));
    {
        let dialog = h.app.dialogs.dialog.as_ref().expect("palette open");
        assert_eq!(dialog.filter, "aXbc");
        assert_eq!(dialog.filter_cursor_position, 2);
    }

    h.dispatch(special(KeyCode::Right));
    h.dispatch(special(KeyCode::Backspace));
    let dialog = h.app.dialogs.dialog.as_ref().expect("palette open");
    assert_eq!(dialog.filter, "aXc");
    assert_eq!(dialog.filter_cursor_position, 2);
}

#[test]
fn slash_rename_emits_command_event_with_title_args() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command(
        "rename fresh title".into(),
    )));
    let title = h.events().iter().find_map(|ev| match ev {
        Event::Command { name, args } if name == "rename" => Some(args.clone()),
        _ => None,
    });
    assert_eq!(
        title.as_deref(),
        Some("fresh title"),
        "/rename must surface its args to the host; events: {:?}",
        h.events(),
    );
}

#[test]
fn slash_rename_with_no_args_emits_usage_hint_no_event() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("rename".into())));
    let any = h
        .events()
        .iter()
        .any(|ev| matches!(ev, Event::Command { name, .. } if name == "rename"));
    assert!(!any, "no-arg /rename must NOT fire the host event");
    let snap = h.snapshot();
    assert!(
        snap.to_lowercase().contains("usage"),
        "no-arg /rename must surface a usage hint; snap:\n{snap}",
    );
}

#[test]
fn slash_thinking_toggles_collapse_on_existing_and_future_messages() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::View(ViewAction::Command("thinking".into())));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::assistant("hello", "deep reasoning").with_server_id("a1"),
    )));
    assert!(
        !h.app.messages.messages[0].thoughts_collapsed,
        "after a single toggle the store must be in show-mode and \
         newly-appended messages must inherit that visible state",
    );
    h.dispatch(Action::View(ViewAction::Command("thinking".into())));
    assert!(
        h.app.messages.messages[0].thoughts_collapsed,
        "/thinking must collapse existing assistant reasoning",
    );
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::assistant("again", "more thoughts").with_server_id("a2"),
    )));
    assert!(
        h.app.messages.messages[1].thoughts_collapsed,
        "new assistant messages must adopt the pinned hidden state",
    );
    h.dispatch(Action::View(ViewAction::Command("thinking".into())));
    assert!(
        !h.app.messages.messages[0].thoughts_collapsed,
        "third /thinking must re-expand",
    );
    assert!(!h.app.messages.messages[1].thoughts_collapsed);
}

#[test]
fn slash_undo_emits_event_with_last_user_message_id() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("first user prompt").with_server_id("msg-1"),
    )));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::assistant("ok", "").with_server_id("msg-2"),
    )));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        raider_tui::action::HostMessage::user("second user prompt").with_server_id("msg-3"),
    )));
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("undo".into())));
    let undo = h.events().iter().find_map(|ev| match ev {
        Event::Undo { message_id } => Some(message_id.clone()),
        _ => None,
    });
    assert_eq!(
        undo.as_deref(),
        Some("msg-3"),
        "/undo must emit the LAST user message id; events: {:?}",
        h.events(),
    );
}

#[test]
fn slash_undo_without_user_message_emits_system_message_and_no_event() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("undo".into())));
    let undo_ev = h.events().iter().any(|ev| matches!(ev, Event::Undo { .. }));
    assert!(!undo_ev, "no user message ⇒ no Event::Undo");
    let snap = h.snapshot();
    assert!(
        snap.contains("Nothing to undo"),
        "guidance must surface in transcript; snap:\n{snap}",
    );
}

#[test]
fn slash_redo_emits_redo_event() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.clear_events();
    h.dispatch(Action::View(ViewAction::Command("redo".into())));
    let redo = h.events().iter().any(|ev| matches!(ev, Event::Redo));
    assert!(
        redo,
        "/redo must emit Event::Redo; events: {:?}",
        h.events(),
    );
}

#[test]
fn copy_last_assistant_message_emits_clipboard_event() {
    let mut h = Harness::new(100, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("ping".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "pong  \n".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));
    h.clear_events();

    h.dispatch(Action::View(ViewAction::CopyLastAssistantMessage));

    let copied = h
        .events()
        .iter()
        .find_map(|e| match e {
            Event::CopyToClipboard { text, .. } => Some(text.clone()),
            _ => None,
        })
        .expect("CopyLastAssistantMessage must emit Event::CopyToClipboard");
    assert_eq!(copied, "pong", "trimmed text content of last assistant");
}

#[test]
fn copy_session_transcript_emits_clipboard_event_with_toast_messages() {
    let mut h = Harness::new(100, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("ping".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "pong".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));
    h.clear_events();

    h.dispatch(Action::View(ViewAction::CopySessionTranscript));

    let (text, success, error) = h
        .events()
        .iter()
        .find_map(|e| match e {
            Event::CopyToClipboard {
                text,
                success_message,
                error_message,
            } => Some((text.clone(), success_message.clone(), error_message.clone())),
            _ => None,
        })
        .expect("CopySessionTranscript must emit Event::CopyToClipboard");
    assert!(text.contains("# Session") && text.contains("ping") && text.contains("pong"));
    assert_eq!(success, "Session transcript copied to clipboard!");
    assert_eq!(error, "Failed to copy session transcript");
}

#[test]
fn toast_feedback_renders_top_right_without_adding_transcript_message() {
    let mut h = Harness::new(100, 24);
    h.dispatch(Action::View(ViewAction::ShowToast(Toast::new(
        "Message copied to clipboard!",
        ToastVariant::Success,
    ))));

    let snap = h.snapshot();
    assert!(
        snap.contains("Message copied to clipboard!"),
        "toast message must render in the frame; snap:\n{snap}",
    );
    assert!(
        h.app.messages.is_empty(),
        "toast feedback must not create a chat transcript System message"
    );
}

#[test]
fn copy_last_assistant_message_without_any_assistant_shows_error_toast() {
    let mut h = Harness::new(100, 24);
    h.clear_events();

    h.dispatch(Action::View(ViewAction::CopyLastAssistantMessage));

    let has_clipboard = h
        .events()
        .iter()
        .any(|e| matches!(e, Event::CopyToClipboard { .. }));
    assert!(
        !has_clipboard,
        "must NOT emit a clipboard event when there's nothing to copy"
    );
    let snap = h.snapshot();
    assert!(
        snap.contains("No assistant messages found"),
        "must surface the empty-state toast; snap:\n{snap}",
    );
    assert!(
        h.app.messages.is_empty(),
        "copy error toast must not append a transcript System message"
    );
}

#[test]
fn copy_last_assistant_message_with_blank_content_shows_error_toast() {
    let mut h = Harness::new(100, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("ping".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::AssistantDone));
    h.clear_events();

    h.dispatch(Action::View(ViewAction::CopyLastAssistantMessage));

    let has_clipboard = h
        .events()
        .iter()
        .any(|e| matches!(e, Event::CopyToClipboard { .. }));
    assert!(
        !has_clipboard,
        "must NOT emit a clipboard event for blank content"
    );
    let snap = h.snapshot();
    assert!(
        snap.contains("No text content found"),
        "must surface the blank-content toast; snap:\n{snap}",
    );
}

#[test]
fn open_docs_emits_open_url_event() {
    let mut h = Harness::new(100, 24);
    h.clear_events();

    h.dispatch(Action::View(ViewAction::OpenDocs));

    let url = h
        .events()
        .iter()
        .find_map(|e| match e {
            Event::OpenUrl(u) => Some(u.clone()),
            _ => None,
        })
        .expect("OpenDocs must emit Event::OpenUrl");
    assert_eq!(url, "https://opencode.ai/docs");
}

#[test]
fn slash_summarize_aliases_to_compact_command() {
    use raider_tui::app::SlashCommand;
    match SlashCommand::parse("/summarize") {
        SlashCommand::Action(action) => match *action {
            Action::View(ViewAction::Command(name)) => assert_eq!(name, "compact"),
            other => {
                panic!("expected Action::View(ViewAction::Command(\"compact\")); got {other:?}")
            }
        },
        other => panic!("expected Action::View(ViewAction::Command(\"compact\")); got {other:?}"),
    }
}

#[test]
fn slash_toggle_thinking_aliases_to_thinking_command() {
    use raider_tui::app::SlashCommand;
    match SlashCommand::parse("/toggle-thinking") {
        SlashCommand::Action(action) => match *action {
            Action::View(ViewAction::Command(name)) => assert_eq!(name, "thinking"),
            other => {
                panic!("expected Action::View(ViewAction::Command(\"thinking\")); got {other:?}")
            }
        },
        other => panic!("expected Action::View(ViewAction::Command(\"thinking\")); got {other:?}"),
    }
}
