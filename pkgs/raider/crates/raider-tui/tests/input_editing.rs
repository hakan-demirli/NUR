// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn typed_keys_go_directly_into_input() {
    let mut h = Harness::new(80, 24);
    pin_dummy_model(&mut h);
    h.dispatch(key('h'));
    h.dispatch(key('i'));
    assert_eq!(
        h.app.input.input, "hi",
        "no modal split; chars land in input"
    );
    h.dispatch(special(KeyCode::Enter));
    assert_eq!(h.events(), &[Event::UserMessage("hi".to_string())]);
}

#[test]
fn ctrl_c_on_empty_input_quits() {
    let mut h = Harness::new(80, 24);
    h.dispatch(ctrl('c'));
    assert!(h.app.should_quit());
    assert!(h.events().contains(&Event::Quit));
}

#[test]
fn ctrl_c_clears_nonempty_input_without_quit() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::User(UserAction::PasteText("dont-send".into())));
    h.dispatch(ctrl('c'));
    assert!(!h.app.should_quit());
    assert!(h.events().is_empty());
    assert!(h.app.input.input.is_empty());
}

#[test]
fn esc_on_empty_input_emits_interrupt() {
    let mut h = Harness::new(80, 24);
    h.dispatch(special(KeyCode::Esc));
    assert!(
        h.events().contains(&Event::Interrupt),
        "Esc on empty input should ask the host to interrupt: {:?}",
        h.events()
    );
    assert!(!h.app.should_quit());
}

#[test]
fn esc_with_text_clears_input_without_event() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::User(UserAction::PasteText("draft".into())));
    h.dispatch(special(KeyCode::Esc));
    assert!(h.app.input.input.is_empty());
    assert!(h.events().is_empty(), "no event for input-clear");
}

#[test]
fn ctrl_u_clears_the_entire_input_line() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("hello world".into())));
    assert_eq!(h.app.input.input, "hello world");
    h.dispatch(ctrl('u'));
    assert_eq!(h.app.input.input, "");
    assert_eq!(h.app.input.cursor_position, 0);
}

#[test]
fn ctrl_w_deletes_word_before_cursor() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("foo bar baz".into())));
    h.dispatch(ctrl('w'));
    assert_eq!(h.app.input.input, "foo bar ");
    h.dispatch(ctrl('w'));
    assert_eq!(h.app.input.input, "foo ");
    h.dispatch(ctrl('w'));
    assert_eq!(h.app.input.input, "");
    h.dispatch(ctrl('w'));
    assert_eq!(h.app.input.input, "");
}

#[test]
fn ctrl_left_jumps_to_previous_word_boundary() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("foo bar baz".into())));
    assert_eq!(h.app.input.cursor_position, 11);
    h.dispatch(Action::User(UserAction::Key {
        code: KeyCode::Left,
        mods: KeyModifiers::CONTROL,
    }));
    assert_eq!(
        h.app.input.cursor_position, 8,
        "first jump lands at start of `baz`"
    );
    h.dispatch(Action::User(UserAction::Key {
        code: KeyCode::Left,
        mods: KeyModifiers::CONTROL,
    }));
    assert_eq!(
        h.app.input.cursor_position, 4,
        "second jump lands at start of `bar`"
    );
    h.dispatch(Action::User(UserAction::Key {
        code: KeyCode::Left,
        mods: KeyModifiers::CONTROL,
    }));
    assert_eq!(
        h.app.input.cursor_position, 0,
        "third jump lands at start of `foo`"
    );
}

#[test]
fn ctrl_right_jumps_to_next_word_boundary() {
    use crossterm::event::{KeyCode, KeyModifiers};
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("foo bar baz".into())));
    h.dispatch(ctrl('a'));
    h.dispatch(Action::User(UserAction::Key {
        code: KeyCode::Right,
        mods: KeyModifiers::CONTROL,
    }));
    assert_eq!(
        h.app.input.cursor_position, 4,
        "first jump lands at start of `bar`"
    );
    h.dispatch(Action::User(UserAction::Key {
        code: KeyCode::Right,
        mods: KeyModifiers::CONTROL,
    }));
    assert_eq!(
        h.app.input.cursor_position, 8,
        "second jump lands at start of `baz`"
    );
}

#[test]
fn ctrl_a_moves_cursor_to_start_of_line() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("foo bar".into())));
    assert_eq!(h.app.input.cursor_position, 7);
    h.dispatch(ctrl('a'));
    assert_eq!(h.app.input.cursor_position, 0);
}

#[test]
fn ctrl_e_moves_cursor_to_end_of_line() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("foo bar".into())));
    h.dispatch(ctrl('a'));
    assert_eq!(h.app.input.cursor_position, 0);
    h.dispatch(ctrl('e'));
    assert_eq!(h.app.input.cursor_position, 7);
}

#[test]
fn ctrl_k_deletes_to_end_of_line() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("foo bar baz".into())));
    h.dispatch(ctrl('a'));
    for _ in 0..4 {
        h.dispatch(special(KeyCode::Right));
    }
    assert_eq!(h.app.input.cursor_position, 4);
    h.dispatch(ctrl('k'));
    assert_eq!(h.app.input.input, "foo ");
}

#[test]
fn clear_input_action_resets_buffer_and_cursor() {
    let mut h = Harness::new(100, 24);
    h.dispatch(Action::User(UserAction::PasteText("hello world".into())));
    assert!(
        !h.app.input.input.is_empty(),
        "precondition: input populated"
    );

    h.dispatch(Action::User(UserAction::ClearInput));

    assert!(
        h.app.input.input.is_empty(),
        "input must be empty after ClearInput"
    );
    assert_eq!(
        h.app.input.cursor_position, 0,
        "cursor must return to start"
    );
}

fn alt_enter() -> Action {
    Action::User(UserAction::Key {
        code: KeyCode::Enter,
        mods: KeyModifiers::ALT,
    })
}

#[test]
fn prompt_textarea_keeps_last_line_visible_when_buffer_exceeds_six_rows() {
    let mut h = Harness::new(80, 30);
    pin_dummy_model(&mut h);
    for i in 0..10 {
        for c in format!("line{i}").chars() {
            h.dispatch(key(c));
        }
        if i < 9 {
            h.dispatch(alt_enter());
        }
    }
    let snap = h.snapshot();
    assert!(
        snap.contains("line9"),
        "last typed line must render after scrolling past 6 rows; snap:\n{snap}",
    );
    assert!(
        !snap.contains("line0"),
        "first line should be scrolled out of view; snap:\n{snap}",
    );
}

#[test]
fn prompt_textarea_cursor_stays_on_screen_after_overflow() {
    let mut h = Harness::new(80, 30);
    pin_dummy_model(&mut h);
    for i in 0..10 {
        for c in format!("row{i}").chars() {
            h.dispatch(key(c));
        }
        if i < 9 {
            h.dispatch(alt_enter());
        }
    }
    let pos = h
        .terminal
        .get_cursor_position()
        .expect("terminal must report a cursor position");
    let buf_area = h.terminal.backend().buffer().area;
    assert!(
        pos.x < buf_area.width && pos.y < buf_area.height,
        "cursor must be inside the terminal viewport: {pos:?} vs {buf_area:?}",
    );
}

#[test]
fn prompt_textarea_scrolls_back_when_cursor_moves_up() {
    let mut h = Harness::new(80, 30);
    pin_dummy_model(&mut h);
    for i in 0..10 {
        for c in format!("line{i}").chars() {
            h.dispatch(key(c));
        }
        if i < 9 {
            h.dispatch(alt_enter());
        }
    }
    h.app.input.cursor_position = 0;
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("line0"),
        "first line must come back into view when cursor moves to top; snap:\n{snap}",
    );
    assert!(
        !snap.contains("line9"),
        "last line should be scrolled out of view when cursor is at top; snap:\n{snap}",
    );
}

#[test]
fn interrupt_action_emits_interrupt_event_without_clearing_input() {
    let mut h = Harness::new(100, 24);
    h.dispatch(Action::User(UserAction::PasteText("still here".into())));
    h.clear_events();

    h.dispatch(Action::User(UserAction::Interrupt));

    assert!(
        h.events().iter().any(|e| matches!(e, Event::Interrupt)),
        "Interrupt must emit Event::Interrupt"
    );
    assert_eq!(
        h.app.input.input, "still here",
        "Interrupt must NOT clear the prompt buffer"
    );
}

#[test]
fn pasting_short_text_inserts_inline_without_collapse() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText("hello world".into())));
    assert_eq!(h.app.input.input, "hello world");
    assert!(
        h.app.input.parts.is_empty(),
        "short paste must not produce a prompt part",
    );
}

#[test]
fn pasting_three_or_more_lines_collapses_to_pasted_placeholder() {
    let mut h = Harness::new(120, 24);
    let payload = "line A\nline B\nline C\nline D".to_string();
    h.dispatch(Action::User(UserAction::PasteText(payload.clone())));
    assert!(
        h.app.input.input.contains("[Pasted ~4 lines]"),
        "input must contain the [Pasted ~N lines] placeholder; got: {:?}",
        h.app.input.input,
    );
    assert_eq!(
        h.app.input.parts.len(),
        1,
        "exactly one prompt part recorded",
    );
    let part = &h.app.input.parts[0];
    assert!(!part.is_empty());
    assert_eq!(
        &h.app.input.input[part.source_start..part.source_end],
        part.placeholder.as_str(),
        "part byte range must equal placeholder text",
    );
}

#[test]
fn pasting_long_single_line_over_150_chars_collapses() {
    let mut h = Harness::new(120, 24);
    let payload = "x".repeat(200);
    h.dispatch(Action::User(UserAction::PasteText(payload)));
    assert!(
        h.app.input.input.contains("[Pasted ~1 lines]"),
        "long single-line paste must collapse too; got: {:?}",
        h.app.input.input,
    );
}

#[test]
fn submitting_collapsed_paste_expands_text_back_for_llm() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    let payload = "line A\nline B\nline C\nline D";
    h.dispatch(Action::User(UserAction::PasteText(payload.into())));
    h.clear_events();
    h.dispatch(Action::User(UserAction::SubmitInput));
    let user_msg = h
        .events()
        .iter()
        .find_map(|e| match e {
            Event::UserMessage(text) => Some(text.clone()),
            _ => None,
        })
        .expect("UserMessage event must be emitted");
    assert!(
        user_msg.contains("line A") && user_msg.contains("line D"),
        "expanded submit text must contain the full original lines; got: {user_msg:?}",
    );
    assert!(
        !user_msg.contains("[Pasted"),
        "expanded submit text must NOT carry the placeholder; got: {user_msg:?}",
    );
}

#[test]
fn editing_inside_a_paste_placeholder_drops_the_part() {
    let mut h = Harness::new(120, 24);
    let payload = "line A\nline B\nline C\nline D";
    h.dispatch(Action::User(UserAction::PasteText(payload.into())));
    assert_eq!(h.app.input.parts.len(), 1);
    h.dispatch(special(KeyCode::Backspace));
    h.dispatch(special(KeyCode::Backspace));
    assert!(
        h.app.input.parts.is_empty(),
        "backspacing into a placeholder must drop the part; parts={:?}",
        h.app.input.parts,
    );
}

#[test]
fn clearing_input_clears_parts() {
    let mut h = Harness::new(120, 24);
    let payload = "line A\nline B\nline C";
    h.dispatch(Action::User(UserAction::PasteText(payload.into())));
    assert_eq!(h.app.input.parts.len(), 1);
    h.dispatch(Action::User(UserAction::ClearInput));
    assert!(h.app.input.parts.is_empty(), "ClearInput must drop parts");
    assert!(h.app.input.input.is_empty());
}

#[test]
fn pasting_image_path_attaches_as_file_part_with_image_placeholder() {
    use std::io::Write;
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "raider-paste-test-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut f = std::fs::File::create(&tmp).expect("create tmp png");
    f.write_all(b"\x89PNG\r\n\x1a\nfake-bytes").expect("write");
    drop(f);

    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText(
        tmp.to_string_lossy().to_string(),
    )));

    assert!(
        h.app.input.input.contains("[Image 1]"),
        "input must contain [Image 1] placeholder for an image paste; got: {:?}",
        h.app.input.input,
    );
    assert_eq!(h.app.input.parts.len(), 1);
    match &h.app.input.parts[0].kind {
        raider_tui::PromptPartKind::File {
            mime,
            filename,
            base64,
            ..
        } => {
            assert_eq!(mime, "image/png");
            assert!(filename.ends_with(".png"));
            assert!(!base64.is_empty());
        }
        other => panic!("expected File part, got {other:?}"),
    }

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn submitting_image_paste_emits_user_message_with_files_event() {
    use std::io::Write;
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "raider-paste-img-{}-{}.png",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut f = std::fs::File::create(&tmp).expect("create tmp png");
    f.write_all(b"\x89PNG\r\n\x1a\nfake").expect("write");
    drop(f);

    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText(
        tmp.to_string_lossy().to_string(),
    )));
    h.dispatch(Action::User(UserAction::SubmitInput));

    let multipart = h.events().iter().find_map(|e| match e {
        Event::UserMessageWithFiles { text, files } => Some((text.clone(), files.clone())),
        _ => None,
    });
    let (text, files) = multipart.expect("must emit UserMessageWithFiles for image paste");
    assert!(
        text.contains("[Image 1]"),
        "text must keep the [Image N] placeholder; got: {text:?}",
    );
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].mime, "image/png");
    assert!(!files[0].base64.is_empty());

    assert!(
        h.events()
            .iter()
            .all(|e| !matches!(e, Event::UserMessage(_))),
        "must not emit plain UserMessage when files are attached",
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn pasting_file_url_scheme_is_unwrapped_to_filepath() {
    use std::io::Write;
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "raider-paste-url-{}-{}.pdf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut f = std::fs::File::create(&tmp).expect("create tmp pdf");
    f.write_all(b"%PDF-1.4\nfake").expect("write");
    drop(f);

    let url = format!("file://{}", tmp.to_string_lossy());
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText(url)));

    assert!(
        h.app.input.input.contains("[PDF 1]"),
        "PDF file:// URL must be detected and attached as [PDF 1]; got: {:?}",
        h.app.input.input,
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn pasting_quoted_filepath_is_unwrapped() {
    use std::io::Write;
    let mut tmp = std::env::temp_dir();
    tmp.push(format!(
        "raider-paste-quote-{}-{}.jpg",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    let mut f = std::fs::File::create(&tmp).expect("create tmp jpg");
    f.write_all(b"fake-jpg").expect("write");
    drop(f);

    let quoted = format!("\"{}\"", tmp.to_string_lossy());
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText(quoted)));
    assert!(
        h.app.input.input.contains("[Image 1]"),
        "quoted JPG path must be unwrapped + attached; got: {:?}",
        h.app.input.input,
    );

    let _ = std::fs::remove_file(&tmp);
}

#[test]
fn pasting_url_does_not_trigger_file_attachment() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::User(UserAction::PasteText(
        "https://example.com/img.png".into(),
    )));
    assert_eq!(
        h.app.input.input, "https://example.com/img.png",
        "URL paste must NOT be attached as a file; got: {:?}",
        h.app.input.input,
    );
    assert!(h.app.input.parts.is_empty());
}

fn prompt_cursor_x_after_typing(text: &str) -> u16 {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    for c in text.chars() {
        h.dispatch(key(c));
    }
    h.draw();
    let pos = h
        .terminal
        .get_cursor_position()
        .expect("cursor must be reported");
    pos.x
}

#[test]
fn cursor_advances_two_cells_per_cjk_character() {
    let base = prompt_cursor_x_after_typing("a");
    let after = prompt_cursor_x_after_typing("a漢字");
    assert_eq!(
        after - base,
        4,
        "CJK must advance 2 cells per glyph (a={base}, a漢字={after}); \
         before the unicode-width fix this regressed to 2 (codepoint count).",
    );
}

#[test]
fn cursor_advances_two_cells_per_wide_emoji() {
    let base = prompt_cursor_x_after_typing("a");
    let after = prompt_cursor_x_after_typing("a😀");
    assert_eq!(
        after - base,
        2,
        "wide emoji must advance 2 cells (a={base}, a😀={after}); \
         pre-fix would be 1 cell (codepoint count).",
    );
}

#[test]
fn cursor_advances_one_cell_per_ascii_character() {
    let base = prompt_cursor_x_after_typing("a");
    let after = prompt_cursor_x_after_typing("abc");
    assert_eq!(
        after - base,
        2,
        "ascii must advance 1 cell per char (a={base}, abc={after})",
    );
}

#[test]
fn pasted_long_text_placeholder_renders_with_yellow_chip_styling() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    let long_paste: String = (0..50)
        .map(|i| format!("line-{i:02} aaaaaaaaaaaa"))
        .collect::<Vec<_>>()
        .join("\n");
    h.dispatch(Action::User(UserAction::PasteText(long_paste)));
    h.draw();

    let snap = h.snapshot();
    let placeholder_y = snap
        .lines()
        .position(|l| l.contains("[Pasted"))
        .unwrap_or_else(|| panic!("no `[Pasted` placeholder on screen:\n{snap}"))
        as u16;

    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;
    let mut found_any = false;
    for x in 0..buf.area.width {
        let cell = &buf[(x, placeholder_y)];
        if cell.symbol() == "[" || cell.symbol() == "P" {
            let bg = cell.style().bg.unwrap_or(ratatui::style::Color::Reset);
            let fg = cell.style().fg.unwrap_or(ratatui::style::Color::Reset);
            if bg == theme.warning {
                found_any = true;
                assert_eq!(
                    fg, theme.background,
                    "paste-chip fg must be theme.background (opencode \
                     extmark.paste); cell={cell:?}",
                );
                assert!(
                    cell.style()
                        .add_modifier
                        .contains(ratatui::style::Modifier::BOLD),
                    "paste-chip must be bold (opencode extmark.paste); cell={cell:?}",
                );
            }
        }
    }
    assert!(
        found_any,
        "no `[` or `P` cell with bg=theme.warning found on placeholder row; \
         the paste-chip styling regressed. snap:\n{snap}",
    );
}

#[test]
fn paste_then_submit_on_resumed_session_emits_user_message() {
    use raider_tui::action::{HostAction, HostMessage};
    use raider_tui::SessionEntry;
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);

    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses_old", "Earlier conversation", "9:44 PM"),
    ])));
    h.dispatch(Action::View(ViewAction::SwitchSession("ses_old".into())));
    h.dispatch(Action::Host(HostAction::ReplaceMessages(vec![
        HostMessage::user("ping").with_server_id("m_prev_user"),
        HostMessage::assistant("pong", "").with_server_id("m_prev_assistant"),
    ])));
    h.clear_events();

    let payload: String = (0..50)
        .map(|i| format!("line-{i:02} content-aaaaaaaaaaaa"))
        .collect::<Vec<_>>()
        .join("\n");
    h.dispatch(Action::User(UserAction::PasteText(payload.clone())));
    assert_eq!(
        h.app.input.parts.len(),
        1,
        "long paste must register exactly one PromptPart",
    );

    h.dispatch(Action::User(UserAction::SubmitInput));

    assert!(
        h.app.input.input.is_empty(),
        "input must clear after submit; got: {:?}",
        h.app.input.input,
    );
    assert!(
        h.app.input.parts.is_empty(),
        "parts must clear after submit"
    );

    let user_msg = h
        .events()
        .iter()
        .find_map(|e| match e {
            Event::UserMessage(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!(
                "no UserMessage event emitted after paste+submit on resumed session; \
                 events={:?}",
                h.events()
            )
        });
    assert!(
        user_msg.contains("line-00") && user_msg.contains("line-49"),
        "submitted text must contain the full pasted blob; got first 80 chars: {:?}",
        &user_msg[..80.min(user_msg.len())],
    );
    assert!(
        !user_msg.contains("[Pasted"),
        "submitted text must NOT carry the placeholder; got first 80 chars: {:?}",
        &user_msg[..80.min(user_msg.len())],
    );
}

#[test]
fn pasted_short_absolute_path_does_not_route_to_slash_handler() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);

    h.dispatch(Action::User(UserAction::PasteText(
        "/home/emre/Desktop/dotfiles".into(),
    )));
    assert_eq!(h.app.input.input, "/home/emre/Desktop/dotfiles");

    h.clear_events();
    h.dispatch(Action::User(UserAction::SubmitInput));

    assert_eq!(
        h.events(),
        &[Event::UserMessage(
            "/home/emre/Desktop/dotfiles".to_string()
        )],
        "pasted short absolute path must be sent as a user message",
    );
}

#[test]
fn leading_spaces_before_pasted_absolute_path_do_not_route_to_slash_handler() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);

    type_text(&mut h, "                 ");
    h.dispatch(Action::User(UserAction::PasteText(
        "/home/emre/Desktop/dotfiles".into(),
    )));

    h.clear_events();
    h.dispatch(Action::User(UserAction::SubmitInput));

    assert_eq!(
        h.events(),
        &[Event::UserMessage(
            "/home/emre/Desktop/dotfiles".to_string()
        )],
        "leading spaces are trimmed, but pasted slash text must remain a user message",
    );
}

#[test]
fn pasted_absolute_path_does_not_mis_route_to_slash_handler() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);

    let pasted_path = format!("/home/emre/Desktop/raider/{}", "README.md ".repeat(20));
    assert!(pasted_path.len() > 150);
    assert!(pasted_path.starts_with('/'));

    h.dispatch(Action::User(UserAction::PasteText(pasted_path.clone())));
    assert_eq!(
        h.app.input.parts.len(),
        1,
        "long single-line `/` paste must collapse into a placeholder part",
    );
    assert!(
        h.app.input.input.starts_with("[Pasted"),
        "displayed input must be the placeholder, not the raw path; got {:?}",
        h.app.input.input,
    );

    h.clear_events();
    h.dispatch(Action::User(UserAction::SubmitInput));

    let events = h.events();
    let mis_routed = events.iter().any(|e| matches!(e, Event::Command { .. }));
    assert!(
        !mis_routed,
        "pasted `/`-prefixed text must NOT be routed to the slash handler; \
         events={events:?}",
    );

    let user_msg = events
        .iter()
        .find_map(|e| match e {
            Event::UserMessage(text) => Some(text.clone()),
            _ => None,
        })
        .unwrap_or_else(|| {
            panic!("no UserMessage emitted for pasted `/`-prefixed text; events={events:?}")
        });
    assert!(
        user_msg.contains("/home/emre/Desktop/raider"),
        "submitted text must contain the full pasted path; got first 80: {:?}",
        &user_msg[..80.min(user_msg.len())],
    );
}

#[test]
fn typed_slash_command_still_routes_to_run_command() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    for c in "/new".chars() {
        h.dispatch(key(c));
    }
    h.clear_events();
    h.dispatch(Action::User(UserAction::SubmitInput));
    let routed_as_user_message = h
        .events()
        .iter()
        .any(|e| matches!(e, Event::UserMessage(text) if text.starts_with('/')));
    assert!(
        !routed_as_user_message,
        "typed `/new` must NOT be sent as a UserMessage; events={:?}",
        h.events(),
    );
}
