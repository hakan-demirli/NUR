// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn resize_action_resticks_scroll_to_bottom() {
    // BUG5 user-reported: resizing the terminal blanked the
    use raider_tui::HostMessage;
    let mut h = Harness::new(160, 40);
    h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
        "hello",
    ))));
    h.dispatch(Action::Host(HostAction::AppendMessage(
        HostMessage::assistant("world", ""),
    )));
    h.draw();
    h.app.scroll.scroll_stick_to_bottom = false;

    h.app.dispatch(Action::Lifecycle(Lifecycle::Resize {
        cols: 100,
        rows: 30,
    }));

    assert!(
        h.app.scroll.scroll_stick_to_bottom,
        "Action::Resize must re-pin scroll_stick_to_bottom so the \
         next draw clamps list_state to the (recomputed) bottom \
         (otherwise a shrink leaves list_state pointing past \
         total_visual_lines and ratatui's List paints a blank \
         viewport — user-reported BUG5)",
    );
}

#[test]
fn mouse_scroll_up_moves_viewport_up_and_disables_stick_to_bottom() {
    // BUG6 user-reported: raider's main loop only handled
    use raider_tui::HostMessage;
    let mut h = Harness::new(160, 40);
    for i in 0..10 {
        h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
            format!("msg {i}"),
        ))));
    }
    h.draw();
    let pre_total = h.app.scroll.total_visual_lines;
    assert!(
        pre_total > 2,
        "test setup: need >2 visual lines for scroll to have room; got {pre_total}",
    );
    assert!(
        h.app.scroll.scroll_stick_to_bottom,
        "test setup: post-draw should be stuck to bottom",
    );
    let pre_offset = h.app.scroll.list_state.offset();

    h.dispatch(Action::User(UserAction::MouseScroll { lines: 1 }));
    let post_offset = h.app.scroll.list_state.offset();
    assert!(
        post_offset < pre_offset,
        "MouseScroll {{ lines: 1 }} (wheel UP) must DECREASE the \
         list_state offset (move viewport up); pre={pre_offset} \
         post={post_offset}",
    );
    assert!(
        !h.app.scroll.scroll_stick_to_bottom,
        "wheel scroll must release the stick-to-bottom anchor — \
         otherwise the next draw snaps the viewport back to the \
         bottom and the user's scroll is invisible",
    );
}

#[test]
fn mouse_scroll_down_moves_viewport_down() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(160, 40);
    for i in 0..10 {
        h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
            format!("msg {i}"),
        ))));
    }
    h.draw();
    h.dispatch(Action::User(UserAction::MouseScroll { lines: 5 }));
    let mid_offset = h.app.scroll.list_state.offset();
    h.dispatch(Action::User(UserAction::MouseScroll { lines: -3 }));
    let post_offset = h.app.scroll.list_state.offset();
    assert!(
        post_offset > mid_offset,
        "MouseScroll {{ lines: -1 }} (wheel DOWN) must INCREASE the \
         list_state offset (move viewport down); mid={mid_offset} \
         post={post_offset}",
    );
}

#[test]
fn scrolling_back_to_bottom_re_engages_auto_stick() {
    use raider_tui::action::Action;
    use raider_tui::HostMessage;
    let mut h = Harness::new(80, 20);
    for i in 0..30 {
        h.dispatch(Action::Host(HostAction::AppendMessage(
            HostMessage::assistant(format!("assistant body {i}"), ""),
        )));
    }
    h.dispatch(special(KeyCode::PageUp));
    assert!(
        !h.app.scroll.scroll_stick_to_bottom,
        "scrolling up must disengage auto-stick",
    );
    for _ in 0..20 {
        h.dispatch(special(KeyCode::PageDown));
    }
    assert!(
        h.app.scroll.scroll_stick_to_bottom,
        "scrolling back to the visual bottom must re-engage auto-stick \
         so future streaming deltas auto-scroll; flag={}, total={}, sel={:?}",
        h.app.scroll.scroll_stick_to_bottom,
        h.app.scroll.total_visual_lines,
        h.app.scroll.list_state.selected(),
    );
    h.dispatch(Action::Host(HostAction::AssistantDelta {
        text: "fresh tail".into(),
        thoughts: false,
        message_id: None,
    }));
    let total = h.app.scroll.total_visual_lines;
    let viewport = h.app.scroll.last_messages_viewport_rows.max(1);
    let expected = total.saturating_sub(viewport);
    assert_eq!(
        h.app.scroll.list_state.offset(),
        expected,
        "after re-engaging, a streaming delta must pin the viewport \
         offset to the bottom; total_visual_lines={}, viewport={}, \
         offset={}",
        total,
        viewport,
        h.app.scroll.list_state.offset(),
    );
}

#[test]
fn streaming_deltas_do_not_yank_viewport_back_to_bottom_when_user_scrolled_up() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(80, 20);
    for i in 0..40 {
        h.dispatch(Action::Host(HostAction::AppendMessage(
            HostMessage::assistant(format!("body {i}"), ""),
        )));
    }
    h.draw();
    h.dispatch(Action::User(UserAction::MouseScroll { lines: 3 }));
    assert!(
        !h.app.scroll.scroll_stick_to_bottom,
        "test setup: scrolling up must release auto-stick",
    );
    let frozen_offset = h.app.scroll.list_state.offset();

    for i in 0..10 {
        h.dispatch(Action::Host(HostAction::AssistantDelta {
            text: format!("delta-{i} "),
            thoughts: false,
            message_id: None,
        }));
        h.draw();
        assert!(
            !h.app.scroll.scroll_stick_to_bottom,
            "delta {i}: auto-stick must stay disengaged once the user \
             has scrolled up; flag flipped back to true after a streaming \
             chunk",
        );
        assert_eq!(
            h.app.scroll.list_state.offset(),
            frozen_offset,
            "delta {i}: list_state offset must stay where the user left \
             it across streaming chunks; expected={frozen_offset}, got={}",
            h.app.scroll.list_state.offset(),
        );
    }
}

#[test]
fn host_append_message_does_not_yank_viewport_when_user_scrolled_up() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(80, 20);
    for i in 0..40 {
        h.dispatch(Action::Host(HostAction::AppendMessage(
            HostMessage::assistant(format!("seed {i}"), ""),
        )));
    }
    h.draw();
    h.dispatch(Action::User(UserAction::MouseScroll { lines: 3 }));
    let frozen_offset = h.app.scroll.list_state.offset();
    assert!(
        !h.app.scroll.scroll_stick_to_bottom,
        "test setup: scrolled up",
    );

    for i in 0..5 {
        h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
            format!("queued user {i}"),
        ))));
        h.draw();
        assert!(
            !h.app.scroll.scroll_stick_to_bottom,
            "host append #{i}: auto-stick must stay disengaged",
        );
        assert_eq!(
            h.app.scroll.list_state.offset(),
            frozen_offset,
            "host append #{i}: offset must stay frozen at {frozen_offset}",
        );
    }
}

#[test]
fn system_message_does_not_yank_viewport_when_user_scrolled_up() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(80, 20);
    for i in 0..40 {
        h.dispatch(Action::Host(HostAction::AppendMessage(
            HostMessage::assistant(format!("seed {i}"), ""),
        )));
    }
    h.draw();
    h.dispatch(Action::User(UserAction::MouseScroll { lines: 3 }));
    let frozen_offset = h.app.scroll.list_state.offset();

    h.dispatch(Action::Host(HostAction::SystemMessage(
        "something happened".into(),
    )));
    h.draw();
    assert!(
        !h.app.scroll.scroll_stick_to_bottom,
        "system message must not re-engage auto-stick",
    );
    assert_eq!(
        h.app.scroll.list_state.offset(),
        frozen_offset,
        "system message must not move viewport",
    );
}
