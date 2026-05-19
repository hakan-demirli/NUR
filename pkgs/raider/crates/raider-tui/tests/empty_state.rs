// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn renders_empty_state_shows_default_agent() {
    let mut h = Harness::new(80, 24);
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Build"),
        "status bar / prompt footer should show default agent 'Build':\n{snap}"
    );
    assert!(h.events().is_empty(), "no events on startup");
}

#[test]
fn default_prompt_placeholders_match_opencode_home_route() {
    let h = Harness::new(80, 24);
    let expected: Vec<&str> = vec![
        "Fix a TODO in the codebase",
        "What is the tech stack of this project?",
        "Fix broken tests",
    ];
    let got: Vec<&str> = h
        .app
        .prompt
        .prompt_placeholders
        .iter()
        .map(String::as_str)
        .collect();
    assert_eq!(got, expected);
}

#[test]
fn current_placeholder_resolves_via_modulo() {
    let mut h = Harness::new(80, 24);
    h.app.prompt.prompt_placeholder_index = 4;
    assert_eq!(
        h.app.prompt.current_placeholder(),
        Some("What is the tech stack of this project?")
    );
}

#[test]
fn cycle_prompt_placeholder_wraps() {
    let mut h = Harness::new(80, 24);
    h.app.prompt.prompt_placeholder_index = 0;
    h.app.prompt.cycle_placeholder();
    assert_eq!(h.app.prompt.prompt_placeholder_index, 1);
    h.app.prompt.cycle_placeholder();
    h.app.prompt.cycle_placeholder();
    assert_eq!(h.app.prompt.prompt_placeholder_index, 0);
}

#[test]
fn set_prompt_placeholders_replaces_pool() {
    let mut h = Harness::new(80, 24);
    h.app
        .prompt
        .set_placeholders(vec!["custom hint".to_string()]);
    assert_eq!(
        h.app.prompt.prompt_placeholders,
        vec!["custom hint".to_string()]
    );
    assert_eq!(h.app.prompt.current_placeholder(), Some("custom hint"));
}

#[test]
fn empty_placeholder_pool_disables_hint() {
    let mut h = Harness::new(80, 24);
    h.app.prompt.set_placeholders(vec![]);
    assert!(h.app.prompt.current_placeholder().is_none());
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("Ask anything") && !snap.contains("Send a message"),
        "empty placeholder pool should render no prompt hint:\n{snap}"
    );
}

#[test]
fn empty_state_renders_tip_strip_with_bullet_label() {
    let mut h = Harness::new(120, 30);
    h.app.prompt.prompt_placeholder_index = 0;
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Tip"),
        "empty-state must render the `Tip` label:\n{snap}"
    );
    assert!(
        snap.contains("@"),
        "first tip (`Type @ followed by ...`) must be rendered:\n{snap}"
    );
    assert!(
        snap.contains("●"),
        "tip strip must render the bullet glyph (●):\n{snap}"
    );
}

#[test]
fn tip_strip_hidden_when_messages_exist() {
    use raider_tui::HostMessage;
    let mut h = Harness::new(120, 30);
    h.app.prompt.prompt_placeholder_index = 0;
    h.dispatch(Action::Host(HostAction::AppendMessage(HostMessage::user(
        "hi",
    ))));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("● Tip"),
        "tip strip must be hidden once messages exist:\n{snap}"
    );
}

#[test]
fn session_route_hides_home_tip_even_with_empty_transcript() {
    let mut h = Harness::new(120, 30);
    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "ses-existing".into(),
    ))));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("● Tip"),
        "opencode session route must not render home tips, even before messages load:\n{snap}"
    );
}

#[test]
fn busy_state_renders_wipe_spinner_and_esc_hint() {
    let mut h = Harness::new(120, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::Host(HostAction::SetBusy(true)));
    h.draw();
    let snap = h.snapshot();
    let wipe_cells = snap.chars().filter(|c| *c == '■' || *c == '⬝').count();
    assert!(
        wipe_cells >= raider_tui::ui::wipe_spinner::WIPE_WIDTH,
        "busy sub-tray must contain ≥ {} wipe cells (■ or ⬝); got \
         {wipe_cells} in:\n{snap}",
        raider_tui::ui::wipe_spinner::WIPE_WIDTH,
    );
    let braille = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    assert!(
        !braille.iter().any(|f| snap.contains(f)),
        "wipe slot must NOT contain any braille frame (those are \
         reserved for tool-call rendering):\n{snap}"
    );
    assert!(
        snap.contains("esc"),
        "busy sub-tray must surface the `esc interrupt` hint:\n{snap}"
    );
    assert!(
        snap.contains("interrupt"),
        "busy sub-tray must surface the `esc interrupt` hint:\n{snap}"
    );
}

#[test]
fn retry_session_status_renders_wipe_spinner_rate_message_and_interrupt_hint() {
    use raider_tui::{SessionEntry, SessionStatus};

    let mut h = Harness::new(220, 24);
    pin_dummy_model(&mut h);
    h.dispatch(Action::Host(HostAction::SetSessions(vec![
        SessionEntry::new("ses-rate", "Rate limited", "today"),
    ])));
    h.dispatch(Action::Host(HostAction::SetUsage(Some(
        "221.5K (22%) · $11.17".into(),
    ))));

    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    h.dispatch(Action::Host(HostAction::SetSessionStatus {
        session_id: "ses-rate".into(),
        status: SessionStatus::Retry {
            attempt: Some(1),
            message: Some(
                "This request would exceed your account's rate limit. Please try again later."
                    .into(),
            ),
            next: Some(now_ms + (2 * 60 * 60 + 23 * 60) * 1000),
        },
    }));
    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "ses-rate".into(),
    ))));
    h.dispatch(Action::Host(HostAction::SetBusy(false)));

    let snap = h.snapshot();
    let wipe_cells = snap.chars().filter(|c| *c == '■' || *c == '⬝').count();
    assert!(
        wipe_cells >= raider_tui::ui::wipe_spinner::WIPE_WIDTH,
        "retry sub-tray must show opencode's knight-rider wipe spinner; snap:\n{snap}",
    );
    assert!(
        snap.contains("This request would exceed your account's rate limit"),
        "retry sub-tray must surface the rate-limit message; snap:\n{snap}",
    );
    assert!(
        snap.contains("retrying in") && snap.contains("attempt #1"),
        "retry sub-tray must include retry countdown and attempt number; snap:\n{snap}",
    );
    assert!(
        snap.contains("esc") && snap.contains("interrupt"),
        "retry sub-tray must show the interrupt hint; snap:\n{snap}",
    );
    assert!(
        !snap.contains("221.5K (22%)") && !snap.contains("ctrl+p commands"),
        "retry status must suppress usage/commands on the right, matching opencode; snap:\n{snap}",
    );
}

#[test]
fn wipe_spinner_eventually_shows_active_cells_across_full_cycle() {
    use raider_tui::ui::wipe_spinner::{render_frame, CYCLE_FRAMES, WIPE_WIDTH};
    let mut active_cell_seen = [false; WIPE_WIDTH];
    for frame in 0..CYCLE_FRAMES {
        for (i, c) in render_frame(frame).chars().enumerate() {
            if c == '■' {
                active_cell_seen[i] = true;
            }
        }
    }
    for (i, seen) in active_cell_seen.iter().enumerate() {
        assert!(
            *seen,
            "cell {i} was never active across {CYCLE_FRAMES} frames"
        );
    }
}

#[test]
fn idle_state_omits_spinner_and_esc_hint() {
    let mut h = Harness::new(120, 24);
    h.dispatch(Action::Host(HostAction::SetBusy(false)));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("esc interrupt"),
        "idle sub-tray must NOT show the `esc interrupt` hint:\n{snap}"
    );
}

#[test]
fn ask_anything_wrapper_appears_in_empty_textarea() {
    let mut h = Harness::new(120, 24);
    h.app.prompt.prompt_placeholder_index = 0;
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("Ask anything..."),
        "empty-state prompt must surface opencode's `Ask anything...` \
         wrapper around the rotating example:\n{snap}"
    );
    assert!(
        snap.contains("Fix a TODO in the codebase"),
        "the chosen example must be rendered inside the wrapper:\n{snap}"
    );
}

#[test]
fn session_route_omits_ask_anything_placeholder() {
    let mut h = Harness::new(120, 24);
    h.app.prompt.prompt_placeholder_index = 0;
    h.dispatch(Action::Host(HostAction::SetCurrentSession(Some(
        "ses-existing".into(),
    ))));
    h.draw();
    let snap = h.snapshot();
    assert!(
        !snap.contains("Ask anything..."),
        "opencode session route omits home-route prompt placeholders:\n{snap}"
    );
    assert!(
        !snap.contains("Fix a TODO in the codebase"),
        "home placeholder example must not leak into an existing session:\n{snap}"
    );
}
