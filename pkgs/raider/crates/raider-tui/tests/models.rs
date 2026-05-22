// Auto-generated from tests/smoke.rs split.

mod common;
use common::*;

#[test]
fn no_screen_wide_status_bar_is_rendered() {
    let mut h = Harness::new(120, 24);
    h.app.sidebar.set_visible(false);
    h.draw();
    let snap = h.snapshot();
    let last_nonblank = snap
        .lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("");
    assert!(
        !last_nonblank.contains("theme:"),
        "no status bar should carry `theme:` label: {last_nonblank:?}"
    );
    assert!(
        !last_nonblank.contains("raider v"),
        "version label must live in the host-supplied build label, \
         not a status bar: {last_nonblank:?}"
    );
}

#[test]
fn prompt_footer_omits_model_segment_when_no_model_is_pinned() {
    let mut h = Harness::new(120, 24);
    h.draw();
    let snap = h.snapshot();
    let footer_line = snap
        .lines()
        .find(|l| l.contains("┃") && l.contains("Build"))
        .unwrap_or_else(|| panic!("prompt footer missing:\n{snap}"));
    assert!(
        !footer_line.contains("·"),
        "no model-separator before a model is pinned: {footer_line:?}"
    );
}

#[test]
fn slash_model_with_arg_pins_model_and_emits_event() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.clear_events();

    h.dispatch(Action::View(ViewAction::Command(
        "/model anthropic/claude-sonnet-4-5".into(),
    )));

    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("anthropic", "claude-sonnet-4-5"))
    );
    assert!(h.events().iter().any(|e| matches!(
        e,
        Event::ModelChanged { model, .. }
            if model.provider_id == "anthropic" && model.model_id == "claude-sonnet-4-5"
    )));

    let snap = h.snapshot();
    assert!(
        snap.contains("Claude Sonnet 4.5"),
        "model name shows in status bar / footer:\n{snap}"
    );
    assert!(snap.contains("Anthropic"), "provider name shows:\n{snap}");
}

#[test]
fn slash_models_opens_model_picker() {
    let mut h = Harness::new(120, 30);
    seed_catalog(&mut h);

    h.dispatch(Action::View(ViewAction::Command("/models".into())));
    let dialog = h.app.dialogs.dialog.as_ref().expect("model picker opens");
    let titles: Vec<String> = dialog
        .visible_options()
        .iter()
        .map(|o| o.title.clone())
        .collect();
    assert!(
        titles.iter().any(|t| t.contains("Claude Sonnet 4.5")),
        "options include catalog models: {titles:?}"
    );
    assert!(
        titles.iter().any(|t| t.contains("GPT-5")),
        "options include cross-provider models: {titles:?}"
    );
}

#[test]
fn model_picker_enter_pins_selection() {
    let mut h = Harness::new(120, 30);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::OpenModelPicker));
    h.clear_events();
    h.dispatch(special(KeyCode::Enter));

    assert!(h.app.dialogs.dialog.is_none(), "picker closes on enter");
    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("anthropic", "claude-sonnet-4-5"))
    );
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::ModelChanged { .. })));
}

#[test]
fn unknown_model_emits_system_message_and_keeps_old() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::Command(
        "/model anthropic/claude-sonnet-4-5".into(),
    )));
    let pinned = h.app.models.current_model.clone();
    h.dispatch(Action::View(ViewAction::Command("/model nope/nada".into())));

    assert_eq!(
        h.app.models.current_model, pinned,
        "current model unchanged"
    );
    let snap = h.snapshot();
    assert!(snap.contains("unknown model"), "warning visible:\n{snap}");
}

#[test]
fn model_cycle_recent_round_trips() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);

    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-sonnet-4-5",
    ))));
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-opus-4-7",
    ))));
    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("anthropic", "claude-opus-4-7"))
    );

    h.clear_events();
    h.dispatch(Action::View(ViewAction::CycleModelRecent(1)));
    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("anthropic", "claude-sonnet-4-5")),
        "cycle forward goes to next recent"
    );
    h.dispatch(Action::View(ViewAction::CycleModelRecent(1)));
    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("anthropic", "claude-opus-4-7")),
        "cycle wraps around"
    );
}

#[test]
fn variant_picker_lists_current_models_variants() {
    let mut h = Harness::new(120, 30);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-sonnet-4-5",
    ))));
    h.dispatch(Action::View(ViewAction::OpenVariantPicker));

    let dialog = h.app.dialogs.dialog.as_ref().expect("variant picker opens");
    let titles: Vec<String> = dialog
        .visible_options()
        .iter()
        .map(|o| o.title.clone())
        .collect();
    assert!(titles.iter().any(|t| t == "thinking"), "{titles:?}");
    assert!(titles.iter().any(|t| t == "fast"), "{titles:?}");
    assert!(titles.iter().any(|t| t == "(default)"), "{titles:?}");
}

#[test]
fn slash_variant_pins_and_emits_event() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-sonnet-4-5",
    ))));
    h.clear_events();

    h.dispatch(Action::View(ViewAction::Command(
        "/variant thinking".into(),
    )));
    assert_eq!(h.app.models.current_variant, Some("thinking".to_string()));
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::VariantChanged(Some(v)) if v == "thinking")));

    let snap = h.snapshot();
    let footer_line = snap
        .lines()
        .find(|l| l.contains("┃") && l.contains("Claude Sonnet 4.5"))
        .unwrap_or_else(|| panic!("prompt footer missing:\n{snap}"));
    assert!(
        footer_line.contains("Claude Sonnet 4.5 Anthropic · thinking"),
        "variant rendered as separate opencode segment, line was: {footer_line:?}"
    );
    assert!(
        !footer_line.contains("Claude Sonnet 4.5/thinking"),
        "variant must not be folded into model label: {footer_line:?}"
    );
}

#[test]
fn prompt_footer_renders_variant_after_provider_in_warning_style() {
    let mut h = Harness::new(140, 24);
    h.app.models.set_catalog(ModelCatalog {
        providers: vec![ProviderInfo {
            id: "openai".into(),
            name: Some("OpenAI".into()),
            models: vec![ModelInfo {
                id: "gpt-5.5".into(),
                name: Some("GPT-5.5".into()),
                variants: vec!["xhigh".into()],
                context_limit: 0,
            }],
        }],
    });
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "openai", "gpt-5.5",
    ))));
    h.dispatch(Action::View(ViewAction::SetVariant(Some("xhigh".into()))));

    let snap = h.snapshot();
    let (row, footer_line) = snap
        .lines()
        .enumerate()
        .find(|(_, l)| l.contains("┃") && l.contains("Build · GPT-5.5"))
        .unwrap_or_else(|| panic!("prompt footer missing:\n{snap}"));
    assert!(
        footer_line.contains("Build · GPT-5.5 OpenAI · xhigh"),
        "expected variant after provider as its own segment, line was: {footer_line:?}"
    );
    assert!(
        !footer_line.contains("GPT-5.5/xhigh OpenAI"),
        "variant must not be rendered as model suffix, line was: {footer_line:?}"
    );

    let variant_x = footer_line
        .split("xhigh")
        .next()
        .expect("variant prefix")
        .chars()
        .count() as u16;
    let cell = h.terminal.backend().buffer()[(variant_x, row as u16)].clone();
    assert_eq!(cell.style().fg, Some(h.app.theme.theme.warning));
    assert!(
        cell.style().add_modifier.contains(Modifier::BOLD),
        "variant should be bold like opencode"
    );
}

#[test]
fn cycle_variant_steps_then_clears_to_default() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-sonnet-4-5",
    ))));

    h.dispatch(Action::View(ViewAction::CycleVariant));
    assert_eq!(h.app.models.current_variant, Some("thinking".to_string()));
    h.dispatch(Action::View(ViewAction::CycleVariant));
    assert_eq!(h.app.models.current_variant, Some("fast".to_string()));
    h.dispatch(Action::View(ViewAction::CycleVariant));
    assert_eq!(h.app.models.current_variant, None, "wraps to default");
    h.dispatch(Action::View(ViewAction::CycleVariant));
    assert_eq!(h.app.models.current_variant, Some("thinking".to_string()));
}

#[test]
fn changing_model_clears_variant() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-sonnet-4-5",
    ))));
    h.dispatch(Action::View(ViewAction::SetVariant(Some(
        "thinking".into(),
    ))));
    assert_eq!(h.app.models.current_variant, Some("thinking".to_string()));

    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "openai", "gpt-5",
    ))));
    assert_eq!(
        h.app.models.current_variant, None,
        "switching models drops the variant"
    );
}

#[test]
fn setting_invalid_catalog_clears_current_model() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "openai", "gpt-5",
    ))));
    assert!(h.app.models.current_model.is_some());

    h.app.models.set_catalog(ModelCatalog {
        providers: vec![ProviderInfo {
            id: "anthropic".into(),
            name: Some("Anthropic".into()),
            models: vec![ModelInfo {
                id: "claude-opus-4-7".into(),
                name: Some("Claude Opus 4.7".into()),
                variants: vec![],
                context_limit: 0,
            }],
        }],
    });
    assert!(h.app.models.current_model.is_none());
}

#[test]
fn command_palette_lists_switch_model() {
    let mut h = Harness::new(120, 30);
    h.dispatch(ctrl('p'));
    let palette = h.app.dialogs.dialog.as_ref().expect("palette open");
    assert!(
        palette
            .visible_options()
            .iter()
            .any(|o| o.title.contains("Switch model")),
        "command palette must include the model switcher"
    );
}

#[test]
fn open_model_picker_with_empty_catalog_emits_system_message() {
    let mut h = Harness::new(80, 24);
    h.dispatch(Action::View(ViewAction::OpenModelPicker));
    assert!(
        h.app.dialogs.dialog.is_none(),
        "no dialog when catalog empty"
    );
    let snap = h.snapshot();
    assert!(
        snap.contains("Model catalog hasn't arrived"),
        "system message visible:\n{snap}"
    );
}

#[test]
fn prompt_footer_shows_agent_model_and_provider() {
    let mut h = Harness::new(140, 24);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-opus-4-7",
    ))));
    let snap = h.snapshot();
    let footer_line = snap
        .lines()
        .find(|l| l.contains("┃") && l.contains("Build · Claude Opus 4.7"))
        .unwrap_or_else(|| panic!("prompt footer missing:\n{snap}"));
    assert!(
        footer_line.contains("Build · Claude Opus 4.7 Anthropic"),
        "exact opencode parity ordering, line was: {footer_line:?}"
    );
    assert!(
        !footer_line.contains("ctrl+p"),
        "keymap hints live in the sub-tray, not the prompt footer: {footer_line:?}"
    );
}

#[test]
fn prompt_footer_right_slot_renders_host_supplied_cwd_branch() {
    let mut h = Harness::new(140, 24);
    h.app
        .prompt
        .set_footer_right(Some("~/Desktop/dotfiles:main".into()));
    h.draw();
    let snap = h.snapshot();
    let footer_line = snap
        .lines()
        .find(|l| l.contains("┃") && l.contains("Build"))
        .unwrap_or_else(|| panic!("prompt footer missing:\n{snap}"));
    assert!(
        footer_line.contains("~/Desktop/dotfiles:main"),
        "cwd:branch rendered on the right side: {footer_line:?}"
    );
}

#[test]
fn sub_tray_carries_keymap_hints_below_connector() {
    let mut h = Harness::new(120, 24);
    h.draw();
    let snap = h.snapshot();
    let sub_tray = snap
        .lines()
        .find(|l| l.contains("ctrl+p"))
        .unwrap_or_else(|| panic!("sub-tray missing:\n{snap}"));
    assert!(
        !sub_tray.contains("┃"),
        "sub-tray must be outside the prompt box: {sub_tray:?}"
    );
    assert!(sub_tray.contains("tab agents"), "{sub_tray:?}");
    assert!(sub_tray.contains("ctrl+p commands"), "{sub_tray:?}");
}

#[test]
fn busy_state_swaps_sub_tray_left_for_esc_interrupt() {
    let mut h = Harness::new(120, 24);
    h.app.prompt.set_busy(true);
    h.draw();
    let snap = h.snapshot();
    assert!(
        snap.contains("esc interrupt"),
        "busy state replaces left hint with `esc interrupt`:\n{snap}"
    );
}

#[test]
fn sub_tray_renders_host_supplied_usage_without_build_label() {
    let mut h = Harness::new(140, 24);
    h.app.prompt.set_usage(Some("146.6K (15%) · $6.43".into()));
    h.app
        .prompt
        .set_build_label(Some("OpenCode dev-build".into()));
    h.draw();
    let snap = h.snapshot();
    let sub_tray = snap
        .lines()
        .find(|l| l.contains("ctrl+p"))
        .unwrap_or_else(|| panic!("sub-tray missing:\n{snap}"));
    assert!(
        sub_tray.contains("146.6K (15%) · $6.43"),
        "usage cluster visible: {sub_tray:?}"
    );
    assert!(
        !sub_tray.contains("• OpenCode dev-build"),
        "sub-tray must NOT carry the `• OpenCode …` build label \
         (opencode parity — the version belongs to the sidebar \
         footer only); sub_tray={sub_tray:?}",
    );
    assert!(
        !sub_tray.contains('•'),
        "sub-tray must NOT carry the `•` bullet at all; \
         sub_tray={sub_tray:?}",
    );
}

#[test]
fn prompt_footer_shows_provider_name_after_model() {
    let mut h = Harness::new(140, 24);
    seed_catalog(&mut h);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-opus-4-7",
    ))));
    let snap = h.snapshot();
    let footer_line = snap
        .lines()
        .find(|l| l.contains("┃") && l.contains("Build · Claude Opus 4.7"))
        .unwrap_or_else(|| panic!("prompt footer missing:\n{snap}"));
    assert!(
        footer_line.contains("Build · Claude Opus 4.7 Anthropic"),
        "expected `Build · Claude Opus 4.7 Anthropic` triplet, line was: {footer_line:?}"
    );
}

#[test]
fn prompt_footer_omits_provider_when_unknown() {
    let mut h = Harness::new(140, 24);
    h.dispatch(Action::View(ViewAction::SetModel(ModelRef::new(
        "anthropic",
        "claude-opus-4-7",
    ))));
    let snap = h.snapshot();
    let footer_line = snap
        .lines()
        .find(|l| l.contains("┃") && l.contains("Build · claude-opus-4-7"))
        .unwrap_or_else(|| panic!("prompt footer missing:\n{snap}"));
    let trimmed = footer_line.trim_end();
    assert!(
        !trimmed.ends_with("anthropic"),
        "provider slug must NOT appear after the model id: {trimmed:?}"
    );
    assert!(
        !trimmed.contains("claude-opus-4-7 anthropic"),
        "no `<model> <provider-slug>` sequence: {trimmed:?}"
    );
}

#[test]
fn sub_tray_does_not_show_tab_agents_shortcut_when_usage_is_present() {
    let mut h = Harness::new(140, 24);
    h.app.prompt.set_usage(Some("293.2K (29%) · $27.65".into()));
    h.draw();
    let snap = h.snapshot();
    let sub_tray = snap
        .lines()
        .find(|l| l.contains("ctrl+p"))
        .unwrap_or_else(|| panic!("sub-tray missing:\n{snap}"));
    assert!(
        sub_tray.contains("293.2K (29%) · $27.65"),
        "usage cluster takes the slot: {sub_tray:?}"
    );
    assert!(
        !sub_tray.contains("tab agents"),
        "`tab agents` must be hidden when usage is present (opencode parity): {sub_tray:?}"
    );
    assert!(
        sub_tray.contains("ctrl+p commands"),
        "`ctrl+p commands` always renders: {sub_tray:?}"
    );
}

#[test]
fn connector_tray_has_left_cap_only_and_extends_to_right_edge() {
    let mut h = Harness::new(120, 24);
    h.draw();
    let snap = h.snapshot();
    let tray_y = h
        .prompt_tray_row()
        .unwrap_or_else(|| panic!("connector tray missing:\n{snap}"));
    let buf = h.terminal.backend().buffer();
    let row_w = buf.area.width;

    let cap_positions: Vec<u16> = (0..row_w)
        .filter(|&x| buf[(x, tray_y)].symbol() == "╹")
        .collect();
    assert_eq!(
        cap_positions.len(),
        1,
        "tray row must have exactly one `╹` cap (left only), found {:?}:\n{snap}",
        cap_positions,
    );
    let lx = cap_positions[0];

    let left_fg = buf[(lx, tray_y)]
        .style()
        .fg
        .unwrap_or(ratatui::style::Color::Reset);
    assert_eq!(
        left_fg, h.app.theme.theme.secondary,
        "left cap must be tinted with the active agent's palette slot \
         (build → secondary): left={left_fg:?}"
    );

    for x in (lx + 1)..row_w {
        let sym = buf[(x, tray_y)].symbol();
        assert!(
            sym == "▀" || sym == " ",
            "tray row cell at x={x} expected `▀` or space, got {sym:?}:\n{snap}",
        );
    }
}

#[test]
fn completion_popup_has_no_border() {
    let mut h = Harness::new(120, 24);
    h.dispatch(key('/'));
    h.dispatch(key('e'));
    h.dispatch(key('x'));
    let snap = h.snapshot();
    assert!(
        !snap.contains('┌') && !snap.contains('┐'),
        "popup must not have top corners:\n{snap}"
    );
    assert!(
        !snap.contains('└') && !snap.contains('┘'),
        "popup must not have bottom corners:\n{snap}"
    );
    assert!(
        !snap.contains('─'),
        "popup must not have horizontal border:\n{snap}"
    );
    assert!(snap.contains("/exit"), "popup still shows /exit:\n{snap}");
}

#[test]
fn submit_without_model_warns_and_does_not_emit_user_message() {
    let mut h = Harness::new(100, 24);
    h.dispatch(Action::User(UserAction::PasteText("hello".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));

    assert!(
        h.events()
            .iter()
            .all(|e| !matches!(e, Event::UserMessage(_))),
        "must NOT emit UserMessage without a model: {:?}",
        h.events()
    );
    let snap = h.snapshot();
    assert!(
        snap.contains("Pick a model"),
        "warning surfaced in transcript:\n{snap}"
    );
    assert_eq!(
        h.app.input.input, "hello",
        "input is preserved so the user can pick a model and resubmit"
    );
}

#[test]
fn submit_without_model_but_with_catalog_opens_model_picker() {
    let mut h = Harness::new(120, 30);
    seed_catalog(&mut h);
    assert!(h.app.models.current_model.is_none());

    h.dispatch(Action::User(UserAction::PasteText("hi".into())));
    h.dispatch(Action::User(UserAction::SubmitInput));

    assert!(
        h.app.dialogs.dialog.is_some(),
        "picker opens to guide the user past the warning"
    );
}

#[test]
fn slash_commands_still_run_without_a_model() {
    let mut h = Harness::new(80, 24);
    type_text(&mut h, "/clear");
    h.dispatch(Action::User(UserAction::SubmitInput));
    assert!(h
        .events()
        .iter()
        .any(|e| matches!(e, Event::Command { name, .. } if name == "clear")),);
    let snap = h.snapshot();
    assert!(
        !snap.contains("Pick a model"),
        "guard must not block slash commands:\n{snap}"
    );
}

#[test]
fn host_set_current_model_does_not_clobber_existing_valid_choice() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.app
        .models
        .set_current_model(Some(ModelRef::new("anthropic", "claude-opus-4-7")));
    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("anthropic", "claude-opus-4-7")),
        "test setup",
    );
    h.clear_events();

    h.dispatch(Action::Host(HostAction::SetCurrentModel(Some(
        ModelRef::new("openai", "gpt-5"),
    ))));

    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("anthropic", "claude-opus-4-7")),
        "TUI must keep its persisted choice over the host's bootstrap suggestion",
    );
}

#[test]
fn host_set_current_model_emits_model_changed_event_to_resync_host() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.app
        .models
        .set_current_model(Some(ModelRef::new("anthropic", "claude-opus-4-7")));
    h.clear_events();

    h.dispatch(Action::Host(HostAction::SetCurrentModel(Some(
        ModelRef::new("openai", "gpt-5"),
    ))));

    let echoed = h.events().iter().find_map(|e| match e {
        Event::ModelChanged { model, .. } => Some(model.clone()),
        _ => None,
    });
    assert_eq!(
        echoed,
        Some(ModelRef::new("anthropic", "claude-opus-4-7")),
        "must echo our kept model so host's model_tx is in sync; events: {:?}",
        h.events(),
    );
}

#[test]
fn host_set_current_model_is_accepted_when_no_persisted_choice() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    assert!(h.app.models.current_model.is_none(), "test setup");
    h.clear_events();

    h.dispatch(Action::Host(HostAction::SetCurrentModel(Some(
        ModelRef::new("openai", "gpt-5"),
    ))));

    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("openai", "gpt-5")),
        "fresh-install path: host suggestion must be adopted",
    );
    assert!(
        !h.events()
            .iter()
            .any(|e| matches!(e, Event::ModelChanged { .. })),
        "no echo when we accepted the host's suggestion as-is; events: {:?}",
        h.events(),
    );
}

#[test]
fn host_set_current_model_replaces_invalid_persisted_choice() {
    let mut h = Harness::new(120, 24);
    seed_catalog(&mut h);
    h.app
        .models
        .set_current_model(Some(ModelRef::new("ghost-co", "missing-model")));
    h.app.models.set_catalog(h.app.models.catalog.clone());
    assert!(h.app.models.current_model.is_none(), "test setup");
    h.clear_events();

    h.dispatch(Action::Host(HostAction::SetCurrentModel(Some(
        ModelRef::new("openai", "gpt-5"),
    ))));

    assert_eq!(
        h.app.models.current_model,
        Some(ModelRef::new("openai", "gpt-5")),
    );
}
