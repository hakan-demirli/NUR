// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn assistant_streaming_appears_in_transcript() {
    let mut h = Harness::new(100, 24);
    h.dispatch(Action::User(UserAction::PasteText("ping".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.clear_events();

    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "pon".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "g!".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));

    assert!(h.events().is_empty(), "streaming actions emit no events");
    let snap = h.snapshot();
    assert!(snap.contains("pong!"), "concatenated text:\n{snap}");
}

#[test]
fn assistant_ordered_parts_preserve_text_reasoning_text_sequence() {
    use raider_tui::{HostMessage, HostMessagePart};

    let mut h = Harness::new(160, 44);
    h.app.messages.set_thinking_hidden_from_persisted(false);

    let mut msg = HostMessage::assistant(
        "legacy flattened text should not determine render order",
        "**Legacy thought**\n\nlegacy flattened thought should not merge parts",
    );
    msg.parts = vec![
        HostMessagePart::Text("First assistant text before any thinking.".into()),
        HostMessagePart::Thought(
            "**Considering pipeline design**\n\nFirst reasoning body stays in its own block.".into(),
        ),
        HostMessagePart::Thought(
            "**Planning Implementation Steps**\n\nSecond reasoning body must not be glued to the first.".into(),
        ),
        HostMessagePart::Text("Final assistant text after the thoughts.".into()),
    ];

    h.dispatch(Action::Host(HostAction::AppendMessage(msg)));
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let pos = |needle: &str| -> usize {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("missing {needle:?} in snapshot:\n{snap}"))
    };

    let first_text = pos("First assistant text before any thinking.");
    let first_thought = pos("Thought: Considering pipeline design");
    let first_body = pos("First reasoning body stays in its own block.");
    let second_thought = pos("Thought: Planning Implementation Steps");
    let second_body = pos("Second reasoning body must not be glued to the first.");
    let final_text = pos("Final assistant text after the thoughts.");

    assert!(
        first_text < first_thought
            && first_thought < first_body
            && first_body < second_thought
            && second_thought < second_body
            && second_body < final_text,
        "assistant parts must render in opencode content[] order; snap:\n{snap}",
    );
    assert!(
        !snap.contains("first. Planning Implementation Steps"),
        "separate reasoning parts must not be concatenated into one broken Thought block:\n{snap}",
    );
    assert!(
        !snap.contains("bubbles.This") && !snap.contains("thinking.Final"),
        "separate assistant text parts must not be glued together without a part boundary:\n{snap}",
    );
}

#[test]
fn user_message_submitted_while_busy_renders_queued_badge() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("first".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "thinking...".into(),
        thoughts: false,
    }));
    h.dispatch(Action::User(UserAction::PasteText("second".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("QUEUED"),
        "second user message must render the QUEUED badge while \
         the first assistant turn is in-flight:\n{snap}"
    );
    let queued_count = snap.matches("QUEUED").count();
    assert_eq!(
        queued_count, 1,
        "exactly one user message should be queued (the second one); \
         got {queued_count} QUEUED badges in:\n{snap}",
    );
}

#[test]
fn queued_badge_clears_when_blocking_assistant_finalises() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("first".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "answer1".into(),
        thoughts: false,
    }));
    h.dispatch(Action::User(UserAction::PasteText("second".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.draw();
    assert!(
        h.snapshot().contains("QUEUED"),
        "preconditions: second user should be QUEUED",
    );
    h.dispatch(Action::Host(HostAction::UpdateLastAssistantMeta {
        agent: Some("build".into()),
        model: Some("big-pickle".into()),
        provider_id: Some("opencode".into()),
        duration: Some(std::time::Duration::from_millis(9_700)),
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));
    let assistants: Vec<_> = h
        .app
        .messages
        .iter()
        .filter(|m| m.sender == raider_tui::Sender::Assistant)
        .collect();
    assert_eq!(assistants.len(), 2, "two optimistic assistant placeholders");
    assert!(
        !assistants[0].is_streaming,
        "the completed, older assistant placeholder must stop streaming",
    );
    assert!(
        assistants[1].is_streaming,
        "the queued prompt's placeholder must remain streaming for its future deltas",
    );
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("QUEUED"),
        "no user message should remain queued once the blocking \
         assistant placeholder is finalised:\n{snap}"
    );
}

#[test]
fn three_back_to_back_submits_all_unqueue_as_assistants_complete() {
    let mut h = Harness::new(140, 40);
    pin_dummy_model(&mut h);
    for i in 0..3 {
        h.dispatch(Action::User(UserAction::PasteText(format!("user_{i}"))));
        h.dispatch(Action::User(UserAction::SubmitInput));
    }

    h.draw();
    let after_submit = h.snapshot();
    let initial_queued = after_submit.matches("QUEUED").count();
    assert_eq!(
        initial_queued, 2,
        "with 3 back-to-back submits, 2 should be queued (the first one is \
         active); got {initial_queued} QUEUED badges in:\n{after_submit}",
    );

    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "answer_0".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));
    h.draw();
    let after_first = h.snapshot();
    let q1 = after_first.matches("QUEUED").count();
    assert_eq!(
        q1, 1,
        "after first assistant completes, exactly 1 user should remain queued; \
         got {q1} in:\n{after_first}",
    );

    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "answer_1".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::AssistantDone));
    h.draw();
    let after_second = h.snapshot();
    let q2 = after_second.matches("QUEUED").count();
    assert_eq!(
        q2, 0,
        "after second assistant completes, NO user should remain queued; \
         got {q2} (user-reported bug: bottom message stuck QUEUED forever):\n{after_second}",
    );
}

#[test]
fn queued_prompt_deltas_attach_above_queued_user_message() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    h.dispatch(Action::User(UserAction::PasteText("first prompt".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.dispatch(Action::User(UserAction::PasteText("second prompt".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));

    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "FIRST_ANSWER".into(),
        thoughts: false,
    }));
    h.draw();
    let snap = h.snapshot();
    let first_user = snap.find("first prompt").expect("first user rendered");
    let first_answer = snap.find("FIRST_ANSWER").expect("first answer rendered");
    let second_user = snap.find("second prompt").expect("second user rendered");
    assert!(
        first_user < first_answer && first_answer < second_user,
        "first assistant answer must render between the first user and queued second user:\n{snap}",
    );

    h.dispatch(Action::Host(HostAction::AssistantDone));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "SECOND_ANSWER".into(),
        thoughts: false,
    }));
    h.draw();
    let snap = h.snapshot();
    let second_user = snap.find("second prompt").expect("second user rendered");
    let second_answer = snap.find("SECOND_ANSWER").expect("second answer rendered");
    assert!(
        second_user < second_answer,
        "after the first turn finalises, the next deltas must fill the second placeholder:\n{snap}",
    );
}

#[test]
fn assistant_footer_renders_orphan_outside_message_box() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("yo", "")
            .with_agent("build")
            .with_model("claude")
            .with_duration(std::time::Duration::from_millis(900)),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let footer_idx = lines
        .iter()
        .position(|l| l.contains("▣"))
        .unwrap_or_else(|| panic!("no footer in snapshot:\n{snap}"));
    let footer_line = lines[footer_idx];
    assert!(
        !footer_line.contains('┃'),
        "assistant footer must not start with `┃` (it's rendered outside \
         the message box). Footer line: {footer_line:?}",
    );
    let gap_idx = footer_idx
        .checked_sub(1)
        .expect("footer row must not be the first row");
    let gap_line = lines[gap_idx];
    assert!(
        gap_line.trim().is_empty(),
        "row above orphan footer must be blank (marginTop=1 gap); \
         got: {gap_line:?}",
    );
    assert!(
        !gap_line.contains('┃'),
        "marginTop gap row above footer must not carry `┃` (it is \
         outside the assistant box); got: {gap_line:?}",
    );
    // 4) Post-BUG8 (opencode parity for `AssistantMessage`): there
    //    content row itself (`yo`). The pre-BUG8 raider added a
    let content_idx = gap_idx
        .checked_sub(1)
        .expect("expected assistant content row directly above the gap row");
    let content_line = lines[content_idx];
    assert!(
        content_line.contains("yo"),
        "expected assistant content (`yo`) directly above the marginTop=1 \
         gap row (no closing `┃ ` separator — opencode parity); \
         got: {content_line:?}",
    );
    assert!(
        !content_line.contains('┃'),
        "assistant content row must NOT carry a `┃` bar (opencode \
         renders AssistantText as `paddingLeft={{3}}` with no border); \
         got: {content_line:?}",
    );
}

#[test]
fn assistant_text_rows_have_no_bar_glyph() {
    // BUG8 user-reported: every raider message row carried a `┃`
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("the assistant text body here", ""),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let content_line = lines
        .iter()
        .find(|l| l.contains("the assistant text body"))
        .unwrap_or_else(|| panic!("missing assistant text body row:\n{snap}"));
    assert!(
        !content_line.contains('┃'),
        "assistant text row must NOT carry a `┃` bar (opencode renders \
         `AssistantText` with NO border); got: {content_line:?}",
    );
}

#[test]
fn user_message_text_row_carries_agent_tinted_bar() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::user("hi there").with_agent("plan"),
    )));
    h.draw();
    let snap = h.snapshot();
    let user_row = snap
        .lines()
        .find(|l| l.contains("hi there"))
        .unwrap_or_else(|| panic!("missing user row:\n{snap}"));
    assert!(
        user_row.contains('┃'),
        "user message row MUST carry a `┃` bar (opencode `UserMessage` \
         uses `border={{[\"left\"]}}`); got: {user_row:?}",
    );

    let buf = h.terminal.backend().buffer();
    let y = snap.lines().position(|l| l.contains("hi there")).unwrap() as u16;
    let mut bar_x = None;
    for x in 0..buf.area.width {
        if buf[(x, y)].symbol() == "┃" {
            bar_x = Some(x);
            break;
        }
    }
    let x = bar_x.expect("`┃` cell must exist on the user row");
    let cell = &buf[(x, y)];
    let fg = cell.style().fg.unwrap_or(ratatui::style::Color::Reset);
    assert_eq!(
        fg, h.app.theme.theme.warning,
        "user-message bar must be tinted with the Plan agent's palette slot \
         (warning, opencode palette index 3); cell={cell:?}",
    );
    assert_ne!(
        fg, h.app.theme.theme.secondary,
        "regression guard: plan bar must NOT collide with build's slot \
         (theme.secondary, index 0)",
    );
}

#[test]
fn assistant_reasoning_carries_subtle_bar_in_background_element() {
    // an accent bar; BUG8 fix routes them through
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.app.messages.toggle_thinking();
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("answer body", "chain of thought goes here"),
    )));
    h.draw();
    let snap = h.snapshot();
    let reasoning_y = snap
        .lines()
        .position(|l| l.contains("Thought:"))
        .unwrap_or_else(|| panic!("missing `Thought:` row:\n{snap}")) as u16;
    let snap_lines: Vec<&str> = snap.lines().collect();
    let first_row = snap_lines[reasoning_y as usize];
    assert!(
        first_row.contains("Thought:") && first_row.contains("chain of thought"),
        "P0-003: first reasoning row must inline `Thought:` with the body \
         on the SAME row (opencode renders `_Thought:_` markdown italics \
         inline). got row={first_row:?}; full snap=\n{snap}",
    );
    let buf = h.terminal.backend().buffer();
    let mut bar_x = None;
    for x in 0..buf.area.width {
        if buf[(x, reasoning_y)].symbol() == "┃" {
            bar_x = Some(x);
            break;
        }
    }
    let x = bar_x.expect("reasoning row must carry a `┃` bar");
    let cell = &buf[(x, reasoning_y)];
    assert_eq!(
        cell.style().fg.unwrap_or(ratatui::style::Color::Reset),
        h.app.theme.theme.background_element,
        "reasoning-bar fg must be theme.background_element (subtle gray); \
         cell={cell:?}",
    );
}

#[test]
fn assistant_reasoning_thinking_header_inlines_with_body_on_same_row() {
    let mut h = Harness::new(140, 30);
    h.app.messages.toggle_thinking();
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "weighing options now".into(),
        thoughts: true,
    }));
    h.draw();
    let snap = h.snapshot();
    let thinking_row = snap
        .lines()
        .find(|l| l.contains("Thinking:"))
        .unwrap_or_else(|| panic!("no `Thinking:` row in snapshot:\n{snap}"));
    assert!(
        thinking_row.contains("Thinking:") && thinking_row.contains("weighing options"),
        "P0-003: `Thinking:` and the body's opening words must share \
         one row. got={thinking_row:?}; full snap=\n{snap}",
    );
}

#[test]
fn assistant_reasoning_label_flips_to_thought_when_stream_finishes() {
    let mut h = Harness::new(140, 30);
    h.app.messages.toggle_thinking();
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "examining tradeoffs".into(),
        thoughts: true,
    }));
    let streaming = h.snapshot();
    assert!(
        streaming.contains("Thinking: examining tradeoffs"),
        "streaming reasoning must show Thinking; snap:\n{streaming}",
    );

    h.dispatch(Action::Host(HostAction::AssistantDone));
    let done = h.snapshot();
    assert!(
        done.contains("Thought: examining tradeoffs"),
        "completed reasoning must show Thought; snap:\n{done}",
    );
    assert!(
        !done.contains("Thinking: examining tradeoffs"),
        "completed reasoning must no longer show Thinking; snap:\n{done}",
    );
}

#[test]
fn no_reversed_highlight_band_at_bottom_of_transcript() {
    use raider_tui::HostMessage;
    use ratatui::style::Modifier;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
        "hi",
    ))));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("yo", "")
            .with_agent("build")
            .with_model("claude")
            .with_duration(std::time::Duration::from_millis(900)),
    )));
    h.draw();
    let buf = h.terminal.backend().buffer();
    let mut reversed_cells = 0;
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            if buf[(x, y)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED)
            {
                reversed_cells += 1;
            }
        }
    }
    assert_eq!(
        reversed_cells, 0,
        "no buffer cell should carry Modifier::REVERSED \
         (the List's select+highlight combo paints a bright band \
         glued to the bottom of the transcript)",
    );
}

#[test]
fn no_panel_background_band_below_assistant_message() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
        "hi",
    ))));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("yo", "thinking briefly")
            .with_agent("build")
            .with_model("claude-opus")
            .with_duration(std::time::Duration::from_millis(1_200)),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let footer_row = lines
        .iter()
        .position(|l| l.contains("▣"))
        .unwrap_or_else(|| panic!("no assistant footer row in snapshot:\n{snap}"));
    let target_row = (footer_row as u16) + 1;
    let theme = &h.app.theme.theme;
    if target_row < h.terminal.size().unwrap().height {
        let bgs = h.row_backgrounds(target_row);
        let third = bgs.len() / 3;
        let panel_cells = bgs.iter().filter(|c| **c == theme.background_panel).count();
        assert!(
            panel_cells < third,
            "row below assistant footer must not paint a wide \
             `background_panel` band (panel_cells={panel_cells} of {} cells, \
             snapshot row: {:?})",
            bgs.len(),
            lines.get(target_row as usize).unwrap_or(&""),
        );
    }
}

#[test]
fn assistant_footer_renders_marker_agent_model_duration() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Hi there!", "")
            .with_agent("build")
            .with_model("Claude Opus 4.7")
            .with_duration(std::time::Duration::from_millis(1_600)),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("▣"), "marker glyph must appear:\n{snap}");
    assert!(snap.contains("Build"), "capitalised agent label:\n{snap}");
    assert!(snap.contains("Claude Opus 4.7"), "model name:\n{snap}");
    assert!(
        snap.contains("1.6s"),
        "duration formatted as `1.6s`:\n{snap}"
    );
}

#[test]
fn assistant_footer_omits_missing_metadata_segments() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("streaming…", ""),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("▣"), "marker still present:\n{snap}");
    assert!(snap.contains("Raider"), "fallback sender label:\n{snap}");
    assert!(
        !snap.contains("▣  · "),
        "no orphan ` · ` segment when metadata is absent:\n{snap}"
    );
}

#[test]
fn submit_pre_populates_streaming_placeholder_metadata() {
    let mut h = Harness::new(140, 30);
    pin_dummy_model(&mut h);
    for c in "hi".chars() {
        h.dispatch(key(c));
    }
    h.dispatch(Action::User(UserAction::SubmitInput));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("▣"),
        "marker glyph must render on the streaming placeholder:\n{snap}"
    );
    assert!(
        snap.contains("Build"),
        "agent label must render on the streaming placeholder:\n{snap}"
    );
    assert!(
        snap.contains("Claude"),
        "model display must render on the streaming placeholder:\n{snap}"
    );
}

#[test]
fn host_update_last_assistant_meta_patches_streaming_message() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
        "hi",
    ))));
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "yo".into(),
        thoughts: false,
    }));
    h.dispatch(Action::Host(HostAction::UpdateLastAssistantMeta {
        agent: Some("build".into()),
        model: Some("claude-opus-4-7".into()),
        provider_id: Some("anthropic".into()),
        duration: Some(std::time::Duration::from_millis(3_900)),
    }));
    h.draw();
    let snap = h.snapshot();
    assert!(snap.contains("▣"), "marker:\n{snap}");
    assert!(snap.contains("Build"), "agent:\n{snap}");
    assert!(snap.contains("3.9s"), "duration after meta patch:\n{snap}");
}

#[test]
fn assistant_footer_only_renders_on_last_assistant_message() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("First reply", "")
            .with_agent("build")
            .with_model("Claude Opus 4.7")
            .with_duration(std::time::Duration::from_millis(1_600)),
    )));
    h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
        "follow-up",
    ))));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Second reply", "")
            .with_agent("plan")
            .with_model("Claude Sonnet 4.5")
            .with_duration(std::time::Duration::from_millis(2_400)),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("1.6s"),
        "older assistant footer must not render (duration leaked):\n{snap}"
    );
    assert!(
        !snap.contains("Claude Opus 4.7"),
        "older assistant footer must not render (model leaked):\n{snap}"
    );
    assert!(
        snap.contains("2.4s"),
        "last assistant footer must render its duration:\n{snap}"
    );
    assert!(
        snap.contains("Claude Sonnet 4.5"),
        "last assistant footer must render its model:\n{snap}"
    );
    let marker_count = snap.matches("▣").count();
    assert_eq!(
        marker_count, 1,
        "▣ marker should appear once (only on last assistant); \
         got {marker_count} occurrences:\n{snap}"
    );
}

#[test]
fn format_duration_matches_opencode_conventions() {
    use raider_tui::model::format_duration;
    use std::time::Duration;
    assert_eq!(format_duration(Duration::from_millis(400)), "0.4s");
    assert_eq!(format_duration(Duration::from_millis(1_600)), "1.6s");
    assert_eq!(format_duration(Duration::from_millis(12_400)), "12.4s");
    assert_eq!(format_duration(Duration::from_secs(63)), "1m3s");
    assert_eq!(format_duration(Duration::from_secs(125)), "2m5s");
}

#[test]
fn assistant_error_renders_red_bordered_panel() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("partial answer", "")
            .with_agent("build")
            .with_model("claude")
            .with_error("Rate limit exceeded"),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Rate limit exceeded"),
        "P1-018: assistant error text must appear in the rendered \
         transcript; snap=\n{snap}",
    );
    let row = find_row(&snap, "Rate limit exceeded");
    let buf = h.terminal.backend().buffer();
    let theme = &h.app.theme.theme;
    let bar_x = find_glyph_x(buf, row, "┃").expect("error row must carry a `┃` bar prefix");
    let bar_cell = &buf[(bar_x, row)];
    assert_eq!(
        bar_cell.style().fg.unwrap_or(ratatui::style::Color::Reset),
        theme.error,
        "P1-018: error-panel bar fg must be theme.error; cell={bar_cell:?}",
    );
    assert_eq!(
        bar_cell.style().bg.unwrap_or(ratatui::style::Color::Reset),
        theme.background_panel,
        "P1-018: error-panel bar bg must be theme.background_panel; \
         cell={bar_cell:?}",
    );
}

#[test]
fn assistant_error_block_has_margin_top_gap_before_it() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("here is an answer", "")
            .with_agent("build")
            .with_model("claude")
            .with_error("Network unreachable"),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let err_row = lines
        .iter()
        .position(|l| l.contains("Network unreachable"))
        .unwrap_or_else(|| panic!("missing error row:\n{snap}"));
    let mut open_pad = err_row;
    while open_pad > 0 {
        let candidate = lines[open_pad - 1];
        if !candidate.contains('┃') {
            break;
        }
        let after_bar: String = candidate
            .chars()
            .skip_while(|c| *c != '┃')
            .skip(1)
            .collect();
        if after_bar.trim().is_empty() {
            open_pad -= 1;
        } else {
            break;
        }
    }
    assert!(
        open_pad >= 1,
        "panel must have at least one row above it for the margin-top gap",
    );
    let gap_row = lines[open_pad - 1];
    assert!(
        gap_row.trim().is_empty(),
        "P1-018: there must be a blank gap row (marginTop=1) directly \
         above the assistant error panel; gap_row={gap_row:?}; snap=\n{snap}",
    );
}

#[test]
fn assistant_error_followed_by_orphan_footer() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("body", "")
            .with_agent("build")
            .with_model("claude")
            .with_duration(std::time::Duration::from_millis(800))
            .with_error("Provider returned 500"),
    )));
    h.draw();
    let snap = h.snapshot();
    let lines: Vec<&str> = snap.lines().collect();
    let err_idx = lines
        .iter()
        .position(|l| l.contains("Provider returned 500"))
        .unwrap_or_else(|| panic!("missing error row:\n{snap}"));
    let footer_idx = lines
        .iter()
        .position(|l| l.contains("▣"))
        .unwrap_or_else(|| panic!("missing orphan footer row:\n{snap}"));
    assert!(
        footer_idx > err_idx,
        "P1-018: orphan footer (▣ …) must render BELOW the error \
         panel; error at row {err_idx}, footer at row {footer_idx}; \
         snap=\n{snap}",
    );
}

#[test]
fn live_session_error_attaches_to_assistant_and_renders_red_panel() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Thinking…", ""),
    )));
    h.dispatch(Action::Host(HostAction::SetLastAssistantError(
        "messages.2: tool_use ids were found without tool_result blocks.".to_string(),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("messages.2: tool_use"),
        "live error string must surface in the assistant panel:\n{snap}",
    );
    assert!(
        !snap.contains("session error:"),
        "live error must not use the legacy `session error:` prefix:\n{snap}",
    );
}

#[test]
fn thinking_hidden_default_is_true_on_first_run() {
    let h = Harness::new(80, 24);
    assert!(
        h.app.messages.thinking_hidden,
        "first-run default must be hide (opencode `useThinkingMode(\"thinking_mode\", \"hide\")`)",
    );
}

#[test]
fn collapsed_reasoning_header_uses_bolded_title() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("body text", "**Inspecting PR workflow**\n\ndetail"),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Inspecting PR workflow"),
        "collapsed reasoning header must show the bolded title; snap:\n{snap}",
    );
    assert!(
        !snap.contains("Thought (hidden"),
        "must NOT fall back to generic `Thought` when a title is present; snap:\n{snap}",
    );
}

#[test]
fn collapsed_reasoning_header_falls_back_to_thought_without_title() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("body", "plain reasoning without bold marker"),
    )));
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Thought (hidden"),
        "fallback label must still render when no `**Title**` lead is present; snap:\n{snap}",
    );
}

#[test]
fn set_last_assistant_error_is_idempotent_does_not_duplicate() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(140, 30);
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("Thinking…", ""),
    )));
    h.dispatch(Action::Host(HostAction::SetLastAssistantError(
        "boom".to_string(),
    )));
    h.dispatch(Action::Host(HostAction::SetLastAssistantError(
        "boom".to_string(),
    )));
    h.draw();
    let snap = h.snapshot();
    let occurrences = snap.matches("boom").count();
    assert_eq!(
        occurrences, 1,
        "applying the same error twice must not duplicate; snap:\n{snap}",
    );
}
