//! [`super`] under `#[cfg(test)]` so the assertions remain unchanged.
use super::*;
use raider_opencode::events::MessagePartUpdatedProps;
use raider_opencode::events::{ServerEvent, SessionIdleProps};
use raider_opencode::types::common::{MessageId, PartId, SessionId};
use raider_opencode::types::message::{MessagePart, MessageRole, TextPart};
use raider_tui::action::ToolStatus;
use raider_tui::provider::ModelRef;
use raider_tui::{Action, HostAction};

#[test]
fn diff_emits_only_new_suffix() {
    let mut m = PartMirror::new();
    let id = MessageId::new("m");
    let pid = PartId::new("p");
    assert_eq!(
        m.diff_text(id.clone(), pid.clone(), "hello"),
        Some("hello".to_string())
    );
    assert_eq!(
        m.diff_text(id.clone(), pid.clone(), "hello world"),
        Some(" world".to_string())
    );
    assert_eq!(m.diff_text(id, pid, "hello world"), None);
}

#[test]
fn diff_resets_on_non_prefix_change() {
    let mut m = PartMirror::new();
    let id = MessageId::new("m");
    let pid = PartId::new("p");
    m.diff_text(id.clone(), pid.clone(), "abc");
    let delta = m.diff_text(id, pid, "xyz");
    assert_eq!(delta, Some("xyz".to_string()));
}

#[test]
fn message_part_updated_for_other_session_is_ignored() {
    let mut mirror = PartMirror::new();
    let active = SessionId::new("active");
    let ev = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: SessionId::new("other"),
        message_id: Some(MessageId::new("m")),
        part: MessagePart::Text(TextPart {
            id: PartId::new("p"),
            text: "hi".into(),

            message_id: None,
            extra: serde_json::Map::new(),
        }),
        part_id: None,
    });
    let t = translate(ev, Some(&active), &mut mirror);
    assert!(t.actions.is_empty());
}

#[test]
fn message_part_updated_text_emits_delta_and_busy() {
    let mut mirror = PartMirror::new();
    let active = SessionId::new("s");
    let ev = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: SessionId::new("s"),
        message_id: Some(MessageId::new("m")),
        part: MessagePart::Text(TextPart {
            id: PartId::new("p"),
            text: "hi".into(),

            message_id: None,
            extra: serde_json::Map::new(),
        }),
        part_id: None,
    });
    let t = translate(ev, Some(&active), &mut mirror);
    assert!(matches!(
        t.actions.as_slice(),
        [Action::Host(HostAction::AssistantDelta { text, thoughts: false }), Action::Host(HostAction::SetBusy(true))] if text == "hi"
    ));
}

#[test]
fn session_idle_for_active_emits_done() {
    let mut mirror = PartMirror::new();
    let active = SessionId::new("s");
    let ev = ServerEvent::SessionIdle(SessionIdleProps {
        session_id: SessionId::new("s"),
    });
    let t = translate(ev, Some(&active), &mut mirror);
    assert!(
        t.actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::AssistantDone))),
        "active session.idle must finish the assistant: {:?}",
        t.actions,
    );
    assert!(
        t.actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetBusy(false)))),
        "active session.idle must clear prompt busy: {:?}",
        t.actions,
    );
    assert!(
        t.actions.iter().any(|a| matches!(
            a,
            Action::Host(HostAction::SetSessionStatus {
                session_id,
                status: raider_tui::SessionStatus::Idle,
            }) if session_id == "s"
        )),
        "session.idle must clear the per-session status used by the retry footer: {:?}",
        t.actions,
    );
}

#[test]
fn provider_refresh_pushes_catalog_and_default_model() {
    use raider_opencode::types::provider::{
        ModelInfo as WireModelInfo, ProviderInfo as WireProviderInfo, ProviderList,
    };
    use std::collections::HashMap;

    let mut anthropic_models = HashMap::new();
    anthropic_models.insert(
        "claude-sonnet-4-5".to_string(),
        WireModelInfo {
            id: "claude-sonnet-4-5".into(),
            provider_id: "anthropic".into(),
            name: "Claude Sonnet 4.5".into(),
            status: Some("active".into()),
            cost: None,
            variants: HashMap::new(),
            limit: None,
            extra: serde_json::Map::new(),
        },
    );
    anthropic_models.insert(
        "claude-haiku-old".into(),
        WireModelInfo {
            id: "claude-haiku-old".into(),
            provider_id: "anthropic".into(),
            name: "Old Haiku".into(),
            status: Some("deprecated".into()),
            cost: None,
            variants: HashMap::new(),
            limit: None,
            extra: serde_json::Map::new(),
        },
    );

    let mut opencode_models = HashMap::new();
    opencode_models.insert(
        "claude-opus".into(),
        WireModelInfo {
            id: "claude-opus".into(),
            provider_id: "opencode".into(),
            name: "Claude Opus 4.7".into(),
            status: Some("active".into()),
            cost: None,
            variants: HashMap::new(),
            limit: None,
            extra: serde_json::Map::new(),
        },
    );

    let mut default = HashMap::new();
    default.insert("opencode".into(), "claude-opus".into());

    let list = ProviderList {
        all: vec![
            WireProviderInfo {
                id: "anthropic".into(),
                name: "Anthropic".into(),
                source: None,
                models: anthropic_models,
                extra: serde_json::Map::new(),
            },
            WireProviderInfo {
                id: "opencode".into(),
                name: "OpenCode Zen".into(),
                source: None,
                models: opencode_models,
                extra: serde_json::Map::new(),
            },
        ],
        default,
        connected: vec!["opencode".into()],
    };

    let actions = provider_refresh_actions(&list, None);
    assert_eq!(actions.len(), 2);

    match &actions[0] {
        Action::Host(HostAction::SetCatalog(c)) => {
            assert_eq!(c.providers[0].id, "opencode");
            assert_eq!(c.providers[1].id, "anthropic");
            let anth = &c.providers[1];
            assert_eq!(anth.models.len(), 1);
            assert_eq!(anth.models[0].id, "claude-sonnet-4-5");
        }
        other => panic!("expected HostSetCatalog, got {other:?}"),
    }

    match &actions[1] {
        Action::Host(HostAction::SetCurrentModel(Some(m))) => {
            assert_eq!(m.provider_id, "opencode");
            assert_eq!(m.model_id, "claude-opus");
        }
        other => {
            panic!("expected HostSetCurrentModel(Some(opencode/claude-opus)), got {other:?}")
        }
    }
}

#[test]
fn provider_refresh_preserves_existing_current_model() {
    use raider_opencode::types::provider::{
        ModelInfo as WireModelInfo, ProviderInfo as WireProviderInfo, ProviderList,
    };
    use std::collections::HashMap;

    let mut models = HashMap::new();
    models.insert(
        "m1".into(),
        WireModelInfo {
            id: "m1".into(),
            provider_id: "p1".into(),
            name: "M1".into(),
            status: Some("active".into()),
            cost: None,
            variants: HashMap::new(),
            limit: None,
            extra: serde_json::Map::new(),
        },
    );
    let list = ProviderList {
        all: vec![WireProviderInfo {
            id: "p1".into(),
            name: "P1".into(),
            source: None,
            models,
            extra: serde_json::Map::new(),
        }],
        default: HashMap::new(),
        connected: vec![],
    };
    let current = ModelRef::new("p1", "m1");
    let actions = provider_refresh_actions(&list, Some(&current));
    assert_eq!(actions.len(), 1);
    assert!(matches!(
        actions[0],
        Action::Host(HostAction::SetCatalog(_))
    ));
}

#[test]
fn extract_agent_returns_string_when_present() {
    let mut extra = serde_json::Map::new();
    extra.insert(
        "agent".to_string(),
        serde_json::Value::String("build".to_string()),
    );
    assert_eq!(super::extract_agent(&extra), Some("build".to_string()));
}

#[test]
fn extract_agent_returns_none_when_missing_or_wrong_type() {
    let empty = serde_json::Map::new();
    assert_eq!(super::extract_agent(&empty), None);
    let mut wrong = serde_json::Map::new();
    wrong.insert("agent".to_string(), serde_json::json!(123));
    assert_eq!(super::extract_agent(&wrong), None);
}

#[test]
fn extract_model_display_prefers_assistant_shape() {
    let mut extra = serde_json::Map::new();
    extra.insert(
        "modelID".to_string(),
        serde_json::Value::String("claude-opus-4-7".to_string()),
    );
    assert_eq!(
        super::extract_model_display(&extra),
        Some("claude-opus-4-7".to_string())
    );
}

#[test]
fn extract_model_display_falls_back_to_user_shape() {
    let mut extra = serde_json::Map::new();
    extra.insert(
        "model".to_string(),
        serde_json::json!({
            "providerID": "opencode",
            "modelID": "big-pickle",
        }),
    );
    assert_eq!(
        super::extract_model_display(&extra),
        Some("big-pickle".to_string())
    );
}

#[test]
fn extract_model_display_accepts_legacy_id_key() {
    let mut extra = serde_json::Map::new();
    extra.insert(
        "model".to_string(),
        serde_json::json!({
            "providerID": "opencode",
            "id": "qwen3.6-plus-free",
        }),
    );
    assert_eq!(
        super::extract_model_display(&extra),
        Some("qwen3.6-plus-free".to_string())
    );
}

#[test]
fn extract_provider_pulls_top_level_or_nested_field() {
    let mut top = serde_json::Map::new();
    top.insert(
        "providerID".to_string(),
        serde_json::Value::String("anthropic".to_string()),
    );
    assert_eq!(super::extract_provider(&top), Some("anthropic".to_string()));

    let mut nested = serde_json::Map::new();
    nested.insert(
        "model".to_string(),
        serde_json::json!({"providerID": "opencode", "modelID": "big-pickle"}),
    );
    assert_eq!(
        super::extract_provider(&nested),
        Some("opencode".to_string())
    );

    let empty = serde_json::Map::new();
    assert_eq!(super::extract_provider(&empty), None);
}

#[test]
fn extract_model_display_returns_none_when_no_field() {
    let empty = serde_json::Map::new();
    assert_eq!(super::extract_model_display(&empty), None);
}

#[test]
fn message_duration_computed_from_created_and_completed() {
    use raider_opencode::types::message::{Message as WireMsg, MessageTime, MessageWithParts};
    let info = WireMsg {
        id: raider_opencode::MessageId::new("msg_x"),
        session_id: None,
        role: raider_opencode::types::message::MessageRole::Assistant,
        time: MessageTime {
            created: Some(1_000),
            completed: Some(2_500),
        },
        extra: {
            let mut e = serde_json::Map::new();
            e.insert("agent".into(), serde_json::json!("build"));
            e.insert("modelID".into(), serde_json::json!("big-pickle"));
            e
        },
    };
    let wrap = MessageWithParts {
        info,
        parts: vec![],
    };
    let host = message_to_host(&wrap);
    assert_eq!(host.agent.as_deref(), Some("build"));
    assert_eq!(host.model.as_deref(), Some("big-pickle"));
    assert_eq!(host.duration, Some(std::time::Duration::from_millis(1_500)));
}

#[test]
fn message_to_host_preserves_ordered_assistant_parts() {
    use raider_opencode::types::message::{
        Message as WireMsg, MessageTime, MessageWithParts, ReasoningPart,
    };
    let info = WireMsg {
        id: raider_opencode::MessageId::new("msg_ordered"),
        session_id: None,
        role: raider_opencode::types::message::MessageRole::Assistant,
        time: MessageTime::default(),
        extra: serde_json::Map::new(),
    };
    let wrap = MessageWithParts {
        info,
        parts: vec![
            MessagePart::Text(TextPart {
                id: PartId::new("txt-a"),
                text: "first text".into(),
                message_id: None,
                extra: serde_json::Map::new(),
            }),
            MessagePart::Reasoning(ReasoningPart {
                id: PartId::new("rsn-a"),
                text: "**Thought A**\n\nbody a".into(),
                message_id: None,
                extra: serde_json::Map::new(),
            }),
            MessagePart::Text(TextPart {
                id: PartId::new("txt-b"),
                text: "second text".into(),
                message_id: None,
                extra: serde_json::Map::new(),
            }),
        ],
    };
    let host = message_to_host(&wrap);
    assert_eq!(host.content, "first textsecond text");
    assert_eq!(host.thoughts, "**Thought A**\n\nbody a");
    assert!(matches!(
        host.parts.as_slice(),
        [
            raider_tui::HostMessagePart::Text(a),
            raider_tui::HostMessagePart::Thought(b),
            raider_tui::HostMessagePart::Text(c),
        ] if a == "first text" && b == "**Thought A**\n\nbody a" && c == "second text"
    ));
}

#[test]
fn format_tokens_compact_matches_opencode_humanisation() {
    assert_eq!(super::format_tokens_compact(0), "0");
    assert_eq!(super::format_tokens_compact(999), "999");
    assert_eq!(super::format_tokens_compact(1_500), "1.5K");
    assert_eq!(super::format_tokens_compact(16_590), "16.6K");
    assert_eq!(super::format_tokens_compact(12_000), "12K");
    assert_eq!(super::format_tokens_compact(1_234_567), "1.2M");
    assert_eq!(super::format_tokens_compact(2_000_000), "2M");
}

#[test]
fn sidebar_actions_emit_usage_from_last_assistant_message_tokens() {
    use raider_opencode::types::message::{
        Message as WireMsg, MessageRole, MessageTime, MessageWithParts,
    };
    use raider_opencode::types::session::{Session, SessionTime};
    let mut session_extra = serde_json::Map::new();
    session_extra.insert("cost".to_string(), serde_json::json!(0.03));
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: session_extra,
    };
    let mut msg_extra = serde_json::Map::new();
    msg_extra.insert(
        "tokens".into(),
        serde_json::json!({"input": 16_500u64, "output": 90u64}),
    );
    let msg = MessageWithParts {
        info: WireMsg {
            id: raider_opencode::MessageId::new("msg_assist"),
            session_id: None,
            role: MessageRole::Assistant,
            time: MessageTime::default(),
            extra: msg_extra,
        },
        parts: vec![],
    };

    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[msg],
        &[],
        &[],
        &Default::default(),
        &[],
        true,
    );
    let usage = actions.iter().find_map(|a| match a {
        Action::Host(HostAction::SetUsage(Some(s))) => Some(s.clone()),
        _ => None,
    });
    let usage = usage.expect("HostSetUsage(Some(_)) must be emitted");
    assert!(
        usage.contains("16.6K"),
        "usage must humanise the LAST assistant message's tokens; got: {usage}"
    );
    assert!(
        usage.contains("$0.03"),
        "usage must include cost; got: {usage}"
    );
}

#[test]
fn sidebar_actions_emit_usage_none_for_empty_metadata() {
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let actions =
        super::sidebar_actions_for_session(&s, None, &[], &[], &[], &Default::default(), &[], true);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetUsage(None)))),
        "empty metadata must emit HostSetUsage(None) to clear stale state",
    );
}

#[test]
fn format_thousands_inserts_separators_at_every_three_digits() {
    assert_eq!(super::format_thousands(0), "0");
    assert_eq!(super::format_thousands(7), "7");
    assert_eq!(super::format_thousands(999), "999");
    assert_eq!(super::format_thousands(1_000), "1,000");
    assert_eq!(super::format_thousands(16_590), "16,590");
    assert_eq!(super::format_thousands(1_234_567), "1,234,567");
}

#[test]
fn sidebar_actions_use_title_when_present() {
    use raider_opencode::types::session::{Session, SessionTime};
    let mut extra = serde_json::Map::new();
    extra.insert("cost".to_string(), serde_json::json!(0.03));
    extra.insert(
        "tokens".to_string(),
        serde_json::json!({
            "input": 100u64,
            "output": 50u64,
            "reasoning": 0u64,
            "cache": {"read": 0u64, "write": 0u64},
        }),
    );
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "Greeting".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra,
    };
    let actions =
        super::sidebar_actions_for_session(&s, None, &[], &[], &[], &Default::default(), &[], true);
    assert!(actions
        .iter()
        .any(|a| matches!(a, Action::Host(HostAction::SetSidebarTitle(t)) if t == "Greeting")));
    assert!(actions.iter().any(
        |a| matches!(a, Action::Host(HostAction::SetSidebarSubtitle(Some(t))) if t == "ses_abc"),
    ));
    assert!(actions
        .iter()
        .any(|a| matches!(a, Action::Host(HostAction::SetSidebarVisible(true)))));
}

#[test]
fn sidebar_actions_fall_back_to_id_when_title_empty() {
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_xyz"),
        title: String::new(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let actions =
        super::sidebar_actions_for_session(&s, None, &[], &[], &[], &Default::default(), &[], true);
    assert!(actions
        .iter()
        .any(|a| matches!(a, Action::Host(HostAction::SetSidebarTitle(t)) if t == "ses_xyz")));
}

#[test]
fn sidebar_sections_include_context_from_last_assistant_message() {
    use raider_opencode::types::message::{
        Message as WireMsg, MessageRole, MessageTime, MessageWithParts,
    };
    use raider_opencode::types::session::{Session, SessionTime};
    let mut session_extra = serde_json::Map::new();
    session_extra.insert("cost".to_string(), serde_json::json!(0.03));
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: session_extra,
    };
    let mut msg_extra = serde_json::Map::new();
    msg_extra.insert(
        "tokens".into(),
        serde_json::json!({"input": 16_500u64, "output": 90u64}),
    );
    let msg = MessageWithParts {
        info: WireMsg {
            id: raider_opencode::MessageId::new("msg_assist"),
            session_id: None,
            role: MessageRole::Assistant,
            time: MessageTime::default(),
            extra: msg_extra,
        },
        parts: vec![],
    };
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[msg],
        &[],
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions.iter().find_map(|a| match a {
        Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
        _ => None,
    });
    let sections = sections.expect("HostSetSidebarSections must be emitted");
    let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
    assert!(
        titles.contains(&"Context"),
        "Context section must be present when an assistant message \
             carries tokens: titles={titles:?}",
    );
    let ctx = sections.iter().find(|s| s.title == "Context").unwrap();
    let body = ctx.lines().join("\n");
    assert!(
        body.contains("16,590 tokens"),
        "Context body must include comma-formatted token total \
             read from the last assistant message; got:\n{body}",
    );
    assert!(
        body.contains("$0.03 spent"),
        "Context body must include `$<cost> spent` from session.cost; \
             got:\n{body}",
    );
}

#[test]
fn sidebar_context_ignores_cumulative_session_tokens() {
    use raider_opencode::types::message::{
        Message as WireMsg, MessageRole, MessageTime, MessageWithParts,
    };
    use raider_opencode::types::session::{Session, SessionTime};

    let mut session_extra = serde_json::Map::new();
    session_extra.insert(
        "tokens".to_string(),
        serde_json::json!({
            "input": 50_000_000u64,
            "output": 25_000_000u64,
            "reasoning": 5_000_000u64,
            "cache": {"read": 15_000_000u64, "write": 5_000_000u64},
        }),
    );
    session_extra.insert("cost".to_string(), serde_json::json!(99.99));
    let s = Session {
        id: raider_opencode::SessionId::new("ses_long_lived"),
        title: "Long".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: session_extra,
    };

    let mut msg_extra = serde_json::Map::new();
    msg_extra.insert(
        "tokens".into(),
        serde_json::json!({
            "input": 10_000u64,
            "output": 2_000u64,
            "reasoning": 345u64,
            "cache": {"read": 0u64, "write": 0u64},
        }),
    );
    let msg = MessageWithParts {
        info: WireMsg {
            id: raider_opencode::MessageId::new("msg_last"),
            session_id: None,
            role: MessageRole::Assistant,
            time: MessageTime::default(),
            extra: msg_extra,
        },
        parts: vec![],
    };

    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[msg],
        &[],
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    let ctx = sections.iter().find(|s| s.title == "Context").unwrap();
    let body = ctx.lines().join("\n");
    assert!(
        body.contains("12,345 tokens"),
        "Context must read the LAST-assistant-message tokens \
             (12,345 = 10000+2000+345), NOT cumulative session.tokens; \
             body:\n{body}",
    );
    assert!(
        !body.contains("100,000,000"),
        "Context must NOT leak `session.tokens` cumulative counter; \
             body:\n{body}",
    );
    assert!(
        body.contains("$99.99 spent"),
        "Context must show cumulative cost from session.cost; \
             body:\n{body}",
    );
}

#[test]
fn sidebar_context_skips_assistant_messages_without_output() {
    use raider_opencode::types::message::{
        Message as WireMsg, MessageRole, MessageTime, MessageWithParts,
    };
    use raider_opencode::types::session::{Session, SessionTime};

    let s = Session {
        id: raider_opencode::SessionId::new("ses_streaming"),
        title: "S".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };

    let mut completed_extra = serde_json::Map::new();
    completed_extra.insert(
        "tokens".into(),
        serde_json::json!({"input": 4_000u64, "output": 1_000u64}),
    );
    let completed = MessageWithParts {
        info: WireMsg {
            id: raider_opencode::MessageId::new("msg_completed"),
            session_id: None,
            role: MessageRole::Assistant,
            time: MessageTime::default(),
            extra: completed_extra,
        },
        parts: vec![],
    };

    let mut streaming_extra = serde_json::Map::new();
    streaming_extra.insert(
        "tokens".into(),
        serde_json::json!({
            "input": 999_999u64,
            "output": 0u64,
            "reasoning": 999_999u64,
        }),
    );
    let streaming = MessageWithParts {
        info: WireMsg {
            id: raider_opencode::MessageId::new("msg_streaming"),
            session_id: None,
            role: MessageRole::Assistant,
            time: MessageTime::default(),
            extra: streaming_extra,
        },
        parts: vec![],
    };

    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[completed, streaming],
        &[],
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    let ctx = sections
        .iter()
        .find(|s| s.title == "Context")
        .expect("Context section must surface the completed assistant message");
    let body = ctx.lines().join("\n");
    assert!(
        body.contains("5,000 tokens"),
        "Context must show the COMPLETED message's 5K tokens, \
             not the streaming reasoning-only message's 1.9M; body:\n{body}",
    );
    assert!(
        !body.contains("1,999,998"),
        "Context must NOT pick up the reasoning-only in-flight message; \
             body:\n{body}",
    );
}

#[test]
fn sidebar_sections_always_include_lsp_placeholder() {
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let actions =
        super::sidebar_actions_for_session(&s, None, &[], &[], &[], &Default::default(), &[], true);
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    let lsp = sections
        .iter()
        .find(|s| s.title == "LSP")
        .expect("LSP section always present (placeholder)");
    assert!(lsp.lsp_entries().is_empty());
    match &lsp.body {
        raider_tui::SidebarBody::Lsps { placeholder, .. } => {
            assert!(
                placeholder.contains("LSPs"),
                "placeholder set: {placeholder}"
            );
        }
        other => panic!("expected Lsps body, got {other:?}"),
    }
}

#[test]
fn message_duration_none_when_completed_missing() {
    use raider_opencode::types::message::{Message as WireMsg, MessageTime, MessageWithParts};
    let info = WireMsg {
        id: raider_opencode::MessageId::new("msg_x"),
        session_id: None,
        role: raider_opencode::types::message::MessageRole::Assistant,
        time: MessageTime {
            created: Some(1_000),
            completed: None,
        },
        extra: serde_json::Map::new(),
    };
    let wrap = MessageWithParts {
        info,
        parts: vec![],
    };
    let host = message_to_host(&wrap);
    assert_eq!(host.duration, None);
}

#[test]
fn sidebar_omits_modified_files_section_when_diff_empty() {
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let actions =
        super::sidebar_actions_for_session(&s, None, &[], &[], &[], &Default::default(), &[], true);
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    assert!(
        sections.iter().all(|s| s.title != "Modified Files"),
        "Modified Files section must be omitted when diff is empty: \
             titles={:?}",
        sections
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
    );
}

#[test]
fn sidebar_emits_modified_files_section_when_diff_present() {
    use raider_opencode::types::diff::FileDiff;
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let diff = vec![
        FileDiff {
            file: "crates/raider-tui/src/sidebar.rs".into(),
            additions: 12,
            deletions: 3,
            status: Some("modified".into()),
            patch: String::new(),
        },
        FileDiff {
            file: "crates/raider-host/src/bridge.rs".into(),
            additions: 7,
            deletions: 0,
            status: Some("modified".into()),
            patch: String::new(),
        },
    ];
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[],
        &diff,
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    let files_section = sections
        .iter()
        .find(|s| s.title == "Modified Files")
        .expect("Modified Files section must be present when diff is not empty");
    let entries = files_section.files_entries();
    assert_eq!(entries.len(), 2, "two diff entries: {entries:?}");
    assert_eq!(entries[0].file, "crates/raider-tui/src/sidebar.rs");
    assert_eq!(entries[0].additions, 12);
    assert_eq!(entries[0].deletions, 3);
    assert_eq!(entries[1].file, "crates/raider-host/src/bridge.rs");
    assert_eq!(entries[1].additions, 7);
    assert_eq!(entries[1].deletions, 0);
}

#[test]
fn message_part_updated_then_delta_routes_reasoning_to_thoughts() {
    use raider_opencode::events::{MessagePartDeltaProps, MessagePartUpdatedProps};
    use raider_opencode::types::message::ReasoningPart;
    let mut mirror = PartMirror::new();
    let active = SessionId::new("s");
    let msg_id = MessageId::new("m");
    let part_id = PartId::new("p_reasoning");
    let part_updated = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: SessionId::new("s"),
        message_id: Some(msg_id.clone()),
        part: MessagePart::Reasoning(ReasoningPart {
            id: part_id.clone(),
            text: String::new(),

            message_id: None,
            extra: serde_json::Map::new(),
        }),
        part_id: None,
    });
    let _ = translate(part_updated, Some(&active), &mut mirror);

    let delta = ServerEvent::MessagePartDelta(MessagePartDeltaProps {
        session_id: SessionId::new("s"),
        message_id: msg_id,
        part_id,
        field: "text".into(),
        delta: "I'm thinking about this".into(),
    });
    let t = translate(delta, Some(&active), &mut mirror);
    let found = t.actions.iter().any(|a| {
            matches!(
                a,
                Action::Host(HostAction::AssistantDelta { text, thoughts: true }) if text == "I'm thinking about this"
            )
        });
    assert!(
        found,
        "reasoning delta must route to thoughts; actions={:?}",
        t.actions,
    );
}

#[test]
fn message_part_updated_text_then_delta_keeps_text_routing() {
    use raider_opencode::events::{MessagePartDeltaProps, MessagePartUpdatedProps};
    let mut mirror = PartMirror::new();
    let active = SessionId::new("s");
    let msg_id = MessageId::new("m");
    let part_id = PartId::new("p_text");
    let part_updated = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: SessionId::new("s"),
        message_id: Some(msg_id.clone()),
        part: MessagePart::Text(TextPart {
            id: part_id.clone(),
            text: String::new(),

            message_id: None,
            extra: serde_json::Map::new(),
        }),
        part_id: None,
    });
    let _ = translate(part_updated, Some(&active), &mut mirror);

    let delta = ServerEvent::MessagePartDelta(MessagePartDeltaProps {
        session_id: SessionId::new("s"),
        message_id: msg_id,
        part_id,
        field: "text".into(),
        delta: "Hello world".into(),
    });
    let t = translate(delta, Some(&active), &mut mirror);
    let found = t.actions.iter().any(|a| {
            matches!(
                a,
                Action::Host(HostAction::AssistantDelta { text, thoughts: false }) if text == "Hello world"
            )
        });
    assert!(
        found,
        "regular text delta must route to content; actions={:?}",
        t.actions,
    );
}

#[test]
fn message_updated_with_completed_emits_meta_patch() {
    use raider_opencode::events::MessageUpdatedProps;
    use raider_opencode::types::message::{Message as WireMsg, MessageTime, MessageWithParts};
    let mut mirror = PartMirror::new();
    let active = SessionId::new("s");
    let info = WireMsg {
        id: MessageId::new("msg_1"),
        session_id: Some(SessionId::new("s")),
        role: MessageRole::Assistant,
        time: MessageTime {
            created: Some(1_000),
            completed: Some(4_900),
        },
        extra: {
            let mut e = serde_json::Map::new();
            e.insert("agent".into(), serde_json::json!("build"));
            e.insert("modelID".into(), serde_json::json!("big-pickle"));
            e.insert("providerID".into(), serde_json::json!("opencode"));
            e
        },
    };
    let ev = ServerEvent::MessageUpdated(MessageUpdatedProps {
        info: MessageWithParts {
            info,
            parts: vec![],
        },
    });
    let t = translate(ev, Some(&active), &mut mirror);
    let meta_patch = t.actions.iter().find_map(|a| match a {
        Action::Host(HostAction::UpdateLastAssistantMeta {
            agent,
            model,
            provider_id,
            duration,
        }) => Some((agent.clone(), model.clone(), provider_id.clone(), *duration)),
        _ => None,
    });
    let (agent, model, provider_id, duration) = meta_patch
        .expect("HostUpdateLastAssistantMeta must be emitted with a finalised assistant message");
    assert_eq!(agent.as_deref(), Some("build"));
    assert_eq!(model.as_deref(), Some("big-pickle"));
    assert_eq!(provider_id.as_deref(), Some("opencode"));
    assert_eq!(duration, Some(std::time::Duration::from_millis(3_900)));
    let meta_idx = t
        .actions
        .iter()
        .position(|a| matches!(a, Action::Host(HostAction::UpdateLastAssistantMeta { .. })))
        .unwrap();
    let done_idx = t
        .actions
        .iter()
        .position(|a| matches!(a, Action::Host(HostAction::AssistantDone)))
        .unwrap();
    assert!(
        meta_idx < done_idx,
        "meta patch must precede AssistantDone (actions: {:?})",
        t.actions,
    );
}

#[test]
fn tool_part_to_call_bash_extracts_command_and_metadata_output() {
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let part = ToolPart {
        id: PartId::new("prt_x"),
        tool_name: "bash".into(),
        state: ToolState {
            status: "completed".into(),
            input: serde_json::json!({
                "command": "ls -la",
                "description": "List dir",
            }),
            output: "fallback".into(),
            title: "List dir".into(),
            metadata: serde_json::json!({ "output": "primary" }),
            error: None,
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert_eq!(call.name, "bash");
    assert_eq!(call.title, "List dir");
    assert_eq!(call.command.as_deref(), Some("ls -la"));
    assert_eq!(call.output, "primary");
    assert!(call.error.is_none());
    assert_eq!(call.status, ToolStatus::Completed);
}

#[test]
fn tool_part_to_call_picks_state_output_when_metadata_missing() {
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let part = ToolPart {
        id: PartId::new("prt_x"),
        tool_name: "read".into(),
        state: ToolState {
            status: "completed".into(),
            input: serde_json::json!({ "filePath": "/etc/hosts" }),
            output: "127.0.0.1 localhost".into(),
            title: "Read /etc/hosts".into(),
            metadata: serde_json::Value::Null,
            error: None,
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert_eq!(call.output, "127.0.0.1 localhost");
    // BUG11 parity: the title is synthesised per-tool from
    assert_eq!(call.title, "Read /etc/hosts");
}

#[test]
fn tail_bytes_keeps_short_text_intact() {
    assert_eq!(super::tail_bytes("hello", 100), "hello");
    assert_eq!(super::tail_bytes("", 100), "");
}

#[test]
fn tail_bytes_trims_to_tail_with_ellipsis_prefix() {
    let input: String = "a".repeat(100_000);
    let out = super::tail_bytes(&input, 30_000);
    assert!(
        out.len() <= 30_000 + 5,
        "tailed output must be at most cap + ellipsis prefix; got len={}",
        out.len(),
    );
    assert!(
        out.starts_with("...\n"),
        "tailed output must lead with the opencode `...\\n` ellipsis marker; got: {:?}",
        &out[..out.len().min(20)],
    );
    assert!(
        out.ends_with(&"a".repeat(100)),
        "tailed output must preserve the trailing bytes of the input",
    );
}

#[test]
fn tail_bytes_does_not_split_utf8_codepoint() {
    let input: String = "😀".repeat(100);
    let out = super::tail_bytes(&input, 199);
    assert!(out.starts_with("...\n"));
    let body = out.trim_start_matches("...\n");
    assert!(
        body.chars().all(|c| c == '😀'),
        "body must contain only whole 😀 codepoints; got: {body:?}",
    );
}

#[test]
fn tool_part_to_call_caps_runaway_bash_output_to_tail_window() {
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let huge: String = "y\n".repeat(1_000_000);
    let part = ToolPart {
        id: PartId::new("prt_yes"),
        tool_name: "bash".into(),
        state: ToolState {
            status: "running".into(),
            input: serde_json::json!({
                "command": "yes",
                "description": "Forever",
            }),
            output: huge.clone(),
            title: "Forever".into(),
            metadata: serde_json::json!({ "output": huge }),
            error: None,
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert!(
        call.output.len() <= super::MAX_TOOL_OUTPUT_BYTES + 5,
        "huge bash output must be trimmed to the 30 KB tail window; \
             got {} bytes, cap = {}",
        call.output.len(),
        super::MAX_TOOL_OUTPUT_BYTES,
    );
    assert!(
        call.output.ends_with("y\n"),
        "tail of the runaway output must remain so the live \
             scrolling effect still works; got tail: {:?}",
        &call.output[call.output.len().saturating_sub(20)..],
    );
    assert!(
        call.output.starts_with("...\n"),
        "trimmed output must lead with the opencode `...\\n` \
             ellipsis marker; got: {:?}",
        &call.output[..call.output.len().min(20)],
    );
}

#[test]
fn tool_part_to_call_leaves_small_output_untouched() {
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let part = ToolPart {
        id: PartId::new("prt_x"),
        tool_name: "bash".into(),
        state: ToolState {
            status: "completed".into(),
            input: serde_json::json!({"command": "echo hi"}),
            output: "hi\n".into(),
            title: "Echo".into(),
            metadata: serde_json::json!({"output": "hi\n"}),
            error: None,
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert_eq!(call.output, "hi\n");
    assert!(!call.output.starts_with("...\n"));
}

// BUG11: per-tool inline title synthesis

fn obj(v: serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    v.as_object().cloned().unwrap_or_default()
}

#[test]
fn synthesize_tool_title_glob_with_pattern_path_count() {
    let input = obj(serde_json::json!({"pattern": "**/*.rs", "path": "src"}));
    let meta = obj(serde_json::json!({"count": 17}));
    let t = super::synthesize_tool_title("glob", "", Some(&input), Some(&meta));
    assert_eq!(t, "Glob \"**/*.rs\" in src (17 matches)");
}

#[test]
fn synthesize_tool_title_glob_singular_match_uses_match_not_matches() {
    let input = obj(serde_json::json!({"pattern": "Cargo.toml"}));
    let meta = obj(serde_json::json!({"count": 1}));
    let t = super::synthesize_tool_title("glob", "", Some(&input), Some(&meta));
    assert_eq!(t, "Glob \"Cargo.toml\" (1 match)");
}

#[test]
fn synthesize_tool_title_glob_pending_omits_count_and_path() {
    let input = obj(serde_json::json!({"pattern": "lib/**/*.ts"}));
    let t = super::synthesize_tool_title("glob", "", Some(&input), None);
    assert_eq!(t, "Glob \"lib/**/*.ts\"");
}

#[test]
fn synthesize_tool_title_read_carries_filepath_and_extras() {
    let input = obj(serde_json::json!({
        "filePath": "crates/raider-tui/src/ui.rs",
        "offset": 1855,
        "limit": 55,
    }));
    let t = super::synthesize_tool_title("read", "ignored server label", Some(&input), None);
    assert!(
        t.starts_with("Read crates/raider-tui/src/ui.rs "),
        "title must lead with `Read <path>`; got: {t:?}",
    );
    assert!(t.contains("offset=1855"), "offset=1855 missing; got: {t:?}");
    assert!(t.contains("limit=55"), "limit=55 missing; got: {t:?}");
    assert!(
        !t.contains("filePath="),
        "filePath must be omitted from `[k=v]` extras; got: {t:?}",
    );
}

#[test]
fn tool_part_to_call_populates_loaded_for_read_from_metadata() {
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let part = ToolPart {
        id: PartId::new("prt_read_with_loaded"),
        tool_name: "read".into(),
        state: ToolState {
            status: "completed".into(),
            input: serde_json::json!({ "filePath": "/main.rs" }),
            output: "fn main() {}".into(),
            title: "ignored".into(),
            metadata: serde_json::json!({
                "loaded": ["/main.rs", "/included.rs", "/also.rs"],
            }),
            error: None,
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert_eq!(
        call.loaded,
        vec![
            "/main.rs".to_string(),
            "/included.rs".to_string(),
            "/also.rs".to_string(),
        ],
        "read tool must surface `state.metadata.loaded[]` into ToolCall.loaded",
    );
}

#[test]
fn tool_part_to_call_does_not_populate_loaded_for_non_read_tools() {
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let part = ToolPart {
        id: PartId::new("prt_glob"),
        tool_name: "glob".into(),
        state: ToolState {
            status: "completed".into(),
            input: serde_json::json!({ "pattern": "**/*.rs" }),
            output: String::new(),
            title: "ignored".into(),
            metadata: serde_json::json!({ "loaded": ["/junk.rs"] }),
            error: None,
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert!(
        call.loaded.is_empty(),
        "non-read tools must not surface `metadata.loaded` (opencode ignores it); \
             got: {:?}",
        call.loaded,
    );
}

#[test]
fn bridge_read_tool_title_includes_input_extras_suffix() {
    let input = obj(serde_json::json!({
        "filePath": "/x",
        "offset": 10,
        "limit": 100,
    }));
    let t = super::synthesize_tool_title("read", "", Some(&input), None);
    assert!(
        t.starts_with("Read /x ["),
        "read title must start with `Read /x [`; got: {t:?}",
    );
    assert!(t.ends_with(']'), "title must end with `]`; got: {t:?}");
    assert!(
        t.contains("offset=10"),
        "offset=10 must appear inside bracket; got: {t:?}",
    );
    assert!(
        t.contains("limit=100"),
        "limit=100 must appear inside bracket; got: {t:?}",
    );
    assert!(
        !t.contains("filePath="),
        "filePath must be omitted from `[k=v]` extras; got: {t:?}",
    );
    assert!(
        t == "Read /x [offset=10, limit=100]" || t == "Read /x [limit=100, offset=10]",
        "title must be `Read /x [offset=10, limit=100]` (or the \
             order-flipped variant when serde_json order differs); got: {t:?}",
    );
}

#[test]
fn synthesize_tool_title_grep_with_pattern_path_matches() {
    let input = obj(serde_json::json!({
        "pattern": "truncate_path_left|file_change_line",
        "path": "crates/raider-tui/src/ui.rs",
    }));
    let meta = obj(serde_json::json!({"matches": 5}));
    let t = super::synthesize_tool_title("grep", "", Some(&input), Some(&meta));
    assert_eq!(
        t,
        "Grep \"truncate_path_left|file_change_line\" in crates/raider-tui/src/ui.rs (5 matches)",
    );
}

#[test]
fn synthesize_tool_title_webfetch_returns_url() {
    let input = obj(serde_json::json!({"url": "https://example.com/x"}));
    let t = super::synthesize_tool_title("webfetch", "", Some(&input), None);
    assert_eq!(t, "WebFetch https://example.com/x");
}

#[test]
fn synthesize_tool_title_websearch_with_provider_and_results() {
    let input = obj(serde_json::json!({"query": "rust ratatui"}));
    let meta = obj(serde_json::json!({"provider": "Tavily", "numResults": 10}));
    let t = super::synthesize_tool_title("websearch", "", Some(&input), Some(&meta));
    assert_eq!(t, "Tavily \"rust ratatui\" (10 results)");
}

#[test]
fn synthesize_tool_title_write_returns_path() {
    let input = obj(serde_json::json!({"filePath": "src/new.rs", "content": "fn main(){}"}));
    let t = super::synthesize_tool_title("write", "", Some(&input), None);
    assert_eq!(t, "Write src/new.rs");
}

#[test]
fn synthesize_tool_title_edit_carries_replace_all_flag() {
    let input = obj(serde_json::json!({
        "filePath": "src/foo.rs",
        "oldString": "bar",
        "newString": "baz",
        "replaceAll": true,
    }));
    let t = super::synthesize_tool_title("edit", "", Some(&input), None);
    assert!(
        t.starts_with("Edit src/foo.rs "),
        "title must lead with `Edit <path>`; got: {t:?}",
    );
    assert!(
        t.contains("replaceAll=true"),
        "replaceAll=true missing; got: {t:?}"
    );
    assert!(t.contains("oldString=bar"));
    assert!(t.contains("newString=baz"));
}

#[test]
fn synthesize_tool_title_task_with_subagent_and_description() {
    let input = obj(serde_json::json!({
        "subagent_type": "explore",
        "description": "find all auth helpers",
    }));
    let t = super::synthesize_tool_title("task", "", Some(&input), None);
    assert_eq!(t, "Explore Task — find all auth helpers");
}

#[test]
fn synthesize_tool_title_task_defaults_subagent_to_general() {
    let input = obj(serde_json::json!({"description": "do thing"}));
    let t = super::synthesize_tool_title("task", "", Some(&input), None);
    assert_eq!(t, "General Task — do thing");
}

#[test]
fn synthesize_tool_title_question_counts_questions() {
    let input = obj(serde_json::json!({
        "questions": [
            {"question": "color?"},
            {"question": "size?"},
            {"question": "shape?"},
        ],
    }));
    let t = super::synthesize_tool_title("question", "", Some(&input), None);
    assert_eq!(t, "Asked 3 questions");

    let one = obj(serde_json::json!({"questions": [{"question": "q?"}]}));
    let t1 = super::synthesize_tool_title("question", "", Some(&one), None);
    assert_eq!(t1, "Asked 1 question");
}

#[test]
fn synthesize_tool_title_question_with_zero_questions_returns_bare_name() {
    let empty_input = obj(serde_json::json!({"questions": []}));
    let t_empty = super::synthesize_tool_title("question", "", Some(&empty_input), None);
    assert_eq!(
        t_empty, "Question",
        "n==0 must be a bare title so the running label `Asking questions...` wins",
    );

    let no_field = obj(serde_json::json!({}));
    let t_missing = super::synthesize_tool_title("question", "", Some(&no_field), None);
    assert_eq!(t_missing, "Question");

    let t_no_input = super::synthesize_tool_title("question", "", None, None);
    assert_eq!(t_no_input, "Question");
}

#[test]
fn synthesize_tool_title_generic_tool_lists_all_primitives() {
    let input = obj(serde_json::json!({"foo": "bar", "n": 7, "ok": true}));
    let t = super::synthesize_tool_title("custom_tool", "", Some(&input), None);
    assert!(t.starts_with("custom_tool "), "got: {t:?}");
    assert!(t.contains("foo=bar"), "got: {t:?}");
    assert!(t.contains("n=7"), "got: {t:?}");
    assert!(t.contains("ok=true"), "got: {t:?}");
}

#[test]
fn synthesize_tool_title_skill_quotes_name() {
    let input = obj(serde_json::json!({"name": "git-commit"}));
    let t = super::synthesize_tool_title("skill", "", Some(&input), None);
    assert_eq!(t, "Skill \"git-commit\"");
}

#[test]
fn synthesize_tool_title_bash_uses_description_field() {
    let input = obj(serde_json::json!({"command": "ls -la", "description": "List dir"}));
    let t = super::synthesize_tool_title("bash", "", Some(&input), None);
    assert_eq!(t, "List dir");
    let t2 = super::synthesize_tool_title("bash", "Show files", Some(&input), None);
    assert_eq!(t2, "Show files");
    let t3 = super::synthesize_tool_title("bash", "", None, None);
    assert_eq!(t3, "Shell");
}

#[test]
fn tool_part_to_call_surfaces_error_message_on_error_status() {
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let part = ToolPart {
        id: PartId::new("prt_x"),
        tool_name: "edit".into(),
        state: ToolState {
            status: "error".into(),
            input: serde_json::Value::Null,
            output: String::new(),
            title: "Apply patch".into(),
            metadata: serde_json::Value::Null,
            error: Some("file not found".into()),
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert_eq!(call.status, ToolStatus::Error);
    assert_eq!(call.error.as_deref(), Some("file not found"));
}

#[test]
fn tool_part_to_call_carries_part_id_for_upsert_matching() {
    // BUG7: streaming tool parts re-arrive via
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{ToolPart, ToolState};
    let part = ToolPart {
        id: PartId::new("prt_glob_xyz"),
        tool_name: "glob".into(),
        state: ToolState {
            status: "running".into(),
            input: serde_json::json!({"pattern": "**/*.rs"}),
            output: String::new(),
            title: String::new(),
            metadata: serde_json::Value::Null,
            error: None,
        },
        message_id: None,
        extra: serde_json::Map::new(),
    };
    let call = super::tool_part_to_call(&part);
    assert_eq!(
        call.id.as_deref(),
        Some("prt_glob_xyz"),
        "tool_part_to_call must carry through `ToolPart.id` so the \
             App's upsert path keys on opencode's stable PartId, not on \
             a mid-flight-mutating (name, title) heuristic; got: {:?}",
        call.id,
    );
}

#[test]
fn message_part_updated_tool_dispatches_host_upsert_tool_call() {
    // BUG7 core: `message.part.updated` carrying a `MessagePart::Tool`
    use raider_opencode::events::MessagePartUpdatedProps;
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{MessagePart, ToolPart, ToolState};
    let mut mirror = PartMirror::new();
    let active = SessionId::new("s");
    let msg_id = MessageId::new("m");
    let part_id = PartId::new("prt_bash_1");
    let ev = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: SessionId::new("s"),
        message_id: Some(msg_id.clone()),
        part: MessagePart::Tool(ToolPart {
            id: part_id.clone(),
            tool_name: "bash".into(),
            state: ToolState {
                status: "running".into(),
                input: serde_json::json!({
                    "command": "ls -la",
                    "description": "List dir",
                }),
                output: String::new(),
                title: "List dir".into(),
                metadata: serde_json::Value::Null,
                error: None,
            },
            message_id: None,
            extra: serde_json::Map::new(),
        }),
        part_id: None,
    });
    let t = translate(ev, Some(&active), &mut mirror);
    let upsert = t.actions.iter().find_map(|a| match a {
        Action::Host(HostAction::UpsertToolCall(call)) => Some(call),
        _ => None,
    });
    let call = upsert.expect(
        "streaming MessagePart::Tool must dispatch Action::Host(HostAction::UpsertToolCall) \
             so the App can render the tool row before the next refetch",
    );
    assert_eq!(call.name, "bash");
    assert_eq!(call.id.as_deref(), Some("prt_bash_1"));
    assert_eq!(call.status, ToolStatus::Running);
    assert_eq!(call.command.as_deref(), Some("ls -la"));
    let busy = t
        .actions
        .iter()
        .any(|a| matches!(a, Action::Host(HostAction::SetBusy(true))));
    assert!(
        busy,
        "streaming tool part must also flip HostSetBusy(true); actions={:?}",
        t.actions,
    );
}

#[test]
fn message_part_updated_tool_repeated_emissions_share_part_id() {
    // BUG7 corollary: as a tool transitions `pending → running →
    use raider_opencode::events::MessagePartUpdatedProps;
    use raider_opencode::types::common::PartId;
    use raider_opencode::types::message::{MessagePart, ToolPart, ToolState};
    let mut mirror = PartMirror::new();
    let active = SessionId::new("s");
    let msg_id = MessageId::new("m");
    let part_id = PartId::new("prt_glob_evolving");

    let ev1 = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: SessionId::new("s"),
        message_id: Some(msg_id.clone()),
        part: MessagePart::Tool(ToolPart {
            id: part_id.clone(),
            tool_name: "glob".into(),
            state: ToolState {
                status: "running".into(),
                input: serde_json::json!({"pattern": "**/*.rs"}),
                output: String::new(),
                title: String::new(),
                metadata: serde_json::Value::Null,
                error: None,
            },
            message_id: None,
            extra: serde_json::Map::new(),
        }),
        part_id: None,
    });
    let ev2 = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: SessionId::new("s"),
        message_id: Some(msg_id.clone()),
        part: MessagePart::Tool(ToolPart {
            id: part_id.clone(),
            tool_name: "glob".into(),
            state: ToolState {
                status: "completed".into(),
                input: serde_json::json!({"pattern": "**/*.rs"}),
                output: "src/foo.rs\nsrc/bar.rs\n".into(),
                title: String::new(),
                metadata: serde_json::json!({"count": 17}),
                error: None,
            },
            message_id: None,
            extra: serde_json::Map::new(),
        }),
        part_id: None,
    });
    let t1 = translate(ev1, Some(&active), &mut mirror);
    let t2 = translate(ev2, Some(&active), &mut mirror);

    let call1 = t1
        .actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::UpsertToolCall(c)) => Some(c),
            _ => None,
        })
        .expect("first emission must produce HostUpsertToolCall");
    let call2 = t2
        .actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::UpsertToolCall(c)) => Some(c),
            _ => None,
        })
        .expect("second emission must produce HostUpsertToolCall");
    assert_eq!(
        call1.id, call2.id,
        "repeated emissions for the same PartId must share `ToolCall.id` so \
             the App's `tool_call_match_key` collapses them; call1.id={:?} \
             call2.id={:?}",
        call1.id, call2.id,
    );
    assert_eq!(call1.status, ToolStatus::Running);
    assert_eq!(call2.status, ToolStatus::Completed);
    assert!(
        !call1.title.contains("matches"),
        "running emission shouldn't carry match count yet; got: {:?}",
        call1.title,
    );
    assert!(
        call2.title.contains("17 matches"),
        "completed emission must carry `(17 matches)` from metadata.count; \
             got: {:?}",
        call2.title,
    );
}

#[test]
fn sidebar_emits_mcp_section_with_status_entries() {
    use raider_opencode::types::mcp::{McpRegistry, McpStatus};
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".into(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let mut mcp = McpRegistry::new();
    mcp.insert(
        "context7".into(),
        McpStatus {
            status: "connected".into(),
            error: String::new(),
        },
    );
    mcp.insert(
        "github".into(),
        McpStatus {
            status: "needs_auth".into(),
            error: String::new(),
        },
    );
    mcp.insert(
        "filesystem".into(),
        McpStatus {
            status: "failed".into(),
            error: "spawn failed".into(),
        },
    );
    let actions = super::sidebar_actions_for_session(&s, None, &[], &[], &[], &mcp, &[], true);
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .unwrap();
    let mcp_sec = sections
        .iter()
        .find(|s| s.title == "MCP")
        .expect("MCP section must render when registry is non-empty");
    let entries = mcp_sec.mcp_entries();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].name, "context7");
    assert_eq!(entries[0].status, "connected");
    assert_eq!(entries[1].name, "filesystem");
    assert_eq!(entries[1].status, "failed");
    assert_eq!(entries[1].error, "spawn failed");
    assert_eq!(entries[2].name, "github");
    assert_eq!(entries[2].status, "needs_auth");
}

#[test]
fn sidebar_omits_mcp_when_registry_empty() {
    use raider_opencode::types::mcp::McpRegistry;
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".into(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let actions =
        super::sidebar_actions_for_session(&s, None, &[], &[], &[], &McpRegistry::new(), &[], true);
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .unwrap();
    assert!(
        sections.iter().all(|s| s.title != "MCP"),
        "MCP section must be hidden when registry is empty",
    );
}

#[test]
fn sidebar_omits_todo_section_when_all_completed() {
    use raider_opencode::types::session::{Session, SessionTime};
    use raider_opencode::types::todo::Todo;
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".into(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let todos = vec![
        Todo {
            content: "a".into(),
            status: "completed".into(),
            priority: String::new(),
        },
        Todo {
            content: "b".into(),
            status: "completed".into(),
            priority: String::new(),
        },
    ];
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[],
        &[],
        &todos,
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .unwrap();
    assert!(
        sections.iter().all(|sec| sec.title != "Todo"),
        "Todo section must be hidden when all entries are completed",
    );
}

#[test]
fn sidebar_emits_todo_section_with_pending_entries() {
    use raider_opencode::types::session::{Session, SessionTime};
    use raider_opencode::types::todo::Todo;
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".into(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let todos = vec![
        Todo {
            content: "Inspect LSP".into(),
            status: "completed".into(),
            priority: "high".into(),
        },
        Todo {
            content: "Port MCP section".into(),
            status: "in_progress".into(),
            priority: "high".into(),
        },
        Todo {
            content: "Audit remaining features".into(),
            status: "pending".into(),
            priority: "low".into(),
        },
    ];
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[],
        &[],
        &todos,
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .unwrap();
    let todo_sec = sections
        .iter()
        .find(|s| s.title == "Todo")
        .expect("Todo section must render when there are uncompleted entries");
    let entries = todo_sec.todo_entries();
    assert_eq!(entries.len(), 3, "all entries forwarded: {entries:?}");
    assert_eq!(entries[1].content, "Port MCP section");
    assert_eq!(entries[1].status, "in_progress");
}

#[test]
fn sidebar_todo_section_title_is_singular_todo() {
    use raider_opencode::types::session::{Session, SessionTime};
    use raider_opencode::types::todo::Todo;
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".into(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let todos = vec![Todo {
        content: "ship it".into(),
        status: "pending".into(),
        priority: "high".into(),
    }];
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[],
        &[],
        &todos,
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    assert!(
        sections.iter().any(|sec| sec.title == "Todo"),
        "sidebar must contain a section titled exactly \"Todo\" \
             (singular); got titles: {:?}",
        sections.iter().map(|s| &s.title).collect::<Vec<_>>(),
    );
    assert!(
        sections.iter().all(|sec| sec.title != "Todos"),
        "sidebar must NOT contain a plural \"Todos\" title \
             (opencode uses singular); got titles: {:?}",
        sections.iter().map(|s| &s.title).collect::<Vec<_>>(),
    );
}

#[test]
fn sidebar_modified_files_is_ordered_after_lsp() {
    use raider_opencode::types::diff::FileDiff;
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: raider_opencode::SessionId::new("ses_abc"),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let diff = vec![FileDiff {
        file: "a.rs".into(),
        additions: 1,
        deletions: 0,
        status: None,
        patch: String::new(),
    }];
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[],
        &diff,
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    let lsp_idx = sections
        .iter()
        .position(|s| s.title == "LSP")
        .expect("LSP section present");
    let files_idx = sections
        .iter()
        .position(|s| s.title == "Modified Files")
        .expect("Files section present");
    assert!(
        files_idx > lsp_idx,
        "Modified Files must follow LSP; got LSP@{lsp_idx} Files@{files_idx}",
    );
}

fn assistant_message_with_tokens(
    id: &str,
    total: u64,
    provider_id: Option<&str>,
    model_id: Option<&str>,
) -> raider_opencode::types::message::MessageWithParts {
    use raider_opencode::types::message::{
        Message as WireMsg, MessageRole, MessageTime, MessageWithParts,
    };
    let mut extra = serde_json::Map::new();
    extra.insert(
        "tokens".into(),
        serde_json::json!({"input": 0u64, "output": total}),
    );
    if let Some(p) = provider_id {
        extra.insert("providerID".into(), serde_json::json!(p));
    }
    if let Some(m) = model_id {
        extra.insert("modelID".into(), serde_json::json!(m));
    }
    MessageWithParts {
        info: WireMsg {
            id: raider_opencode::MessageId::new(id),
            session_id: None,
            role: MessageRole::Assistant,
            time: MessageTime::default(),
            extra,
        },
        parts: vec![],
    }
}

fn session_with_cost(id: &str, cost: f64) -> raider_opencode::types::session::Session {
    use raider_opencode::types::session::{Session, SessionTime};
    let mut extra = serde_json::Map::new();
    extra.insert("cost".to_string(), serde_json::json!(cost));
    Session {
        id: raider_opencode::SessionId::new(id),
        title: "T".to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra,
    }
}

#[test]
fn sidebar_context_section_is_first() {
    use raider_opencode::types::diff::FileDiff;
    let s = session_with_cost("ses_first", 0.10);
    let msg = assistant_message_with_tokens("msg_a", 12_345, None, None);
    let diff = vec![FileDiff {
        file: "src/lib.rs".into(),
        additions: 3,
        deletions: 1,
        status: None,
        patch: String::new(),
    }];
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[msg],
        &diff,
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    assert!(
        sections.len() >= 2,
        "expected Context + Modified Files at minimum; got {:?}",
        sections.iter().map(|s| &s.title).collect::<Vec<_>>(),
    );
    assert_eq!(
        sections[0].title,
        "Context",
        "Context must be the FIRST sidebar section (opencode slot \
             order 100); titles in order: {:?}",
        sections.iter().map(|s| &s.title).collect::<Vec<_>>(),
    );
    assert_eq!(
        sections[0].order,
        raider_tui::sidebar::slot::CONTEXT,
        "Context must carry slot::CONTEXT (100) for the renderer \
             to keep it pinned to the top of the panel",
    );
}

#[test]
fn sidebar_context_section_shows_tokens_percent_cost() {
    use raider_tui::provider::{ModelCatalog, ModelInfo, ProviderInfo};
    let s = session_with_cost("ses_pct", 0.23);
    let msg = assistant_message_with_tokens(
        "msg_a",
        12_345,
        Some("anthropic"),
        Some("claude-sonnet-4-5"),
    );
    let catalog = ModelCatalog {
        providers: vec![ProviderInfo {
            id: "anthropic".into(),
            name: Some("Anthropic".into()),
            models: vec![ModelInfo {
                id: "claude-sonnet-4-5".into(),
                name: Some("Claude Sonnet 4.5".into()),
                variants: vec![],
                context_limit: 12_500,
            }],
        }],
    };
    let actions = super::sidebar_actions_for_session(
        &s,
        Some(&catalog),
        &[msg],
        &[],
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    let ctx = sections
        .iter()
        .find(|s| s.title == "Context")
        .expect("Context section present when catalog knows the model");
    let body = ctx.lines();
    assert_eq!(
        body.len(),
        3,
        "Context body must have exactly 3 rows (tokens, percent, \
             cost) when the model's context limit is known; got: {body:?}",
    );
    assert_eq!(body[0], "12,345 tokens", "tokens row");
    assert_eq!(body[1], "99% used", "percent row (12345/12500 round = 99)");
    assert_eq!(body[2], "$0.23 spent", "cost row");
}

#[test]
fn sidebar_context_section_omits_percent_when_model_unknown() {
    let s = session_with_cost("ses_unknown", 0.05);
    let msg =
        assistant_message_with_tokens("msg_a", 7_777, Some("ghost-provider"), Some("ghost-model"));
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[msg],
        &[],
        &[],
        &Default::default(),
        &[],
        true,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s),
            _ => None,
        })
        .expect("HostSetSidebarSections emitted");
    let ctx = sections
        .iter()
        .find(|s| s.title == "Context")
        .expect("Context section still emitted (tokens + cost present)");
    let body = ctx.lines();
    assert!(
        body.iter().all(|l| !l.ends_with("% used")),
        "no body row may end with `% used` when the model is \
             unknown; body: {body:?}",
    );
    assert!(
        body.iter().any(|l| l == "7,777 tokens"),
        "tokens row must still be present; body: {body:?}",
    );
    assert!(
        body.iter().any(|l| l == "$0.05 spent"),
        "cost row must still be present; body: {body:?}",
    );
    assert_eq!(
        body.len(),
        2,
        "exactly 2 rows when percent is omitted (tokens + cost); \
             body: {body:?}",
    );
}

#[test]
fn message_part_updated_compaction_emits_host_mark_compaction() {
    use raider_opencode::events::{MessagePartUpdatedProps, ServerEvent};
    use raider_opencode::types::common::{MessageId, PartId, SessionId};
    use raider_opencode::types::message::{CompactionPart, MessagePart, MessageRole};

    let active = SessionId::new("ses-active");
    let msg_id = MessageId::new("msg-compact-1");
    let mut mirror = super::PartMirror::new();
    mirror.remember_role(msg_id.clone(), MessageRole::User);

    let part = MessagePart::Compaction(CompactionPart {
        id: PartId::new("prt-comp"),
        message_id: Some(msg_id.clone()),
        auto: false,
        overflow: None,
        tail_start_id: None,
        extra: serde_json::Map::new(),
    });
    let ev = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: active.clone(),
        message_id: Some(msg_id.clone()),
        part,
        part_id: Some(PartId::new("prt-comp")),
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let marker = t.actions.iter().find_map(|a| match a {
        raider_tui::action::Action::Host(HostAction::MarkCompaction { message_id, marker }) => {
            Some((message_id.clone(), *marker))
        }
        _ => None,
    });
    let (mid, marker) = marker.expect(
        "compaction part on the active session must emit HostMarkCompaction; \
             actions: {t.actions:?}",
    );
    assert_eq!(mid, "msg-compact-1");
    assert!(!marker.auto, "manual /compact must set auto=false");
}

#[test]
fn vcs_branch_updated_event_dispatches_set_vcs_branch_action() {
    use raider_opencode::events::{ServerEvent, VcsBranchUpdatedProps};
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::VcsBranchUpdated(VcsBranchUpdatedProps {
        branch: Some("feat/foo".into()),
    });
    let t = super::translate(ev, None, &mut mirror);
    let branch = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::SetVcsBranch(b)) => Some(b.clone()),
            _ => None,
        })
        .expect("vcs.branch.updated must emit HostSetVcsBranch");
    assert_eq!(branch.as_deref(), Some("feat/foo"));
}

#[test]
fn vcs_branch_updated_with_null_branch_forwards_none() {
    use raider_opencode::events::{ServerEvent, VcsBranchUpdatedProps};
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::VcsBranchUpdated(VcsBranchUpdatedProps { branch: None });
    let t = super::translate(ev, None, &mut mirror);
    let branch = t.actions.iter().find_map(|a| match a {
        raider_tui::action::Action::Host(HostAction::SetVcsBranch(b)) => Some(b.clone()),
        _ => None,
    });
    assert_eq!(branch, Some(None));
}

#[test]
fn session_status_busy_dispatches_set_session_busy_true() {
    use raider_opencode::events::{ServerEvent, SessionStatusKind, SessionStatusProps};
    use raider_opencode::types::common::SessionId;
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionStatus(SessionStatusProps {
        session_id: SessionId::new("ses-x"),
        status: SessionStatusKind::Busy,
    });
    let t = super::translate(ev, None, &mut mirror);
    let (sid, busy) = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::SetSessionBusy { session_id, busy }) => {
                Some((session_id.clone(), *busy))
            }
            _ => None,
        })
        .expect("session.status must emit HostSetSessionBusy");
    assert_eq!(sid, "ses-x");
    assert!(busy);
}

#[test]
fn session_status_idle_dispatches_set_session_busy_false() {
    use raider_opencode::events::{ServerEvent, SessionStatusKind, SessionStatusProps};
    use raider_opencode::types::common::SessionId;
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionStatus(SessionStatusProps {
        session_id: SessionId::new("ses-y"),
        status: SessionStatusKind::Idle,
    });
    let t = super::translate(ev, None, &mut mirror);
    let busy = t.actions.iter().find_map(|a| match a {
        raider_tui::action::Action::Host(HostAction::SetSessionBusy { busy, .. }) => Some(*busy),
        _ => None,
    });
    assert_eq!(busy, Some(false));
}

#[test]
fn session_status_retry_is_treated_as_busy() {
    use raider_opencode::events::{ServerEvent, SessionStatusKind, SessionStatusProps};
    use raider_opencode::types::common::SessionId;
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionStatus(SessionStatusProps {
        session_id: SessionId::new("ses-z"),
        status: SessionStatusKind::Retry {
            attempt: Some(2),
            message: Some("rate-limited".into()),
            next: Some(5_000),
        },
    });
    let t = super::translate(ev, None, &mut mirror);
    let busy = t.actions.iter().find_map(|a| match a {
        raider_tui::action::Action::Host(HostAction::SetSessionBusy { busy, .. }) => Some(*busy),
        _ => None,
    });
    assert_eq!(busy, Some(true), "retry must surface as busy=true");
}

#[test]
fn session_status_retry_preserves_retry_metadata_for_prompt_footer() {
    use raider_opencode::events::{ServerEvent, SessionStatusKind, SessionStatusProps};
    use raider_opencode::types::common::SessionId;
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionStatus(SessionStatusProps {
        session_id: SessionId::new("ses-rate"),
        status: SessionStatusKind::Retry {
            attempt: Some(1),
            message: Some("This request would exceed your account's rate limit.".into()),
            next: Some(9_999),
        },
    });
    let t = super::translate(ev, None, &mut mirror);
    let status = t.actions.iter().find_map(|a| match a {
        raider_tui::action::Action::Host(HostAction::SetSessionStatus { session_id, status }) => {
            Some((session_id.clone(), status.clone()))
        }
        _ => None,
    });
    let Some((session_id, status)) = status else {
        panic!(
            "session.status retry must emit HostAction::SetSessionStatus: {:?}",
            t.actions
        );
    };
    assert_eq!(session_id, "ses-rate");
    match status {
        raider_tui::SessionStatus::Retry {
            attempt,
            message,
            next,
        } => {
            assert_eq!(attempt, Some(1));
            assert_eq!(
                message.as_deref(),
                Some("This request would exceed your account's rate limit.")
            );
            assert_eq!(next, Some(9_999));
        }
        other => panic!("expected retry status, got {other:?}"),
    }
}

#[test]
fn message_removed_event_dispatches_remove_message_action() {
    use raider_opencode::events::{MessageRemovedProps, ServerEvent};
    use raider_opencode::types::common::{MessageId, SessionId};
    let active = SessionId::new("ses-active");
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::MessageRemoved(MessageRemovedProps {
        session_id: active.clone(),
        message_id: MessageId::new("msg-gone"),
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let id = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::RemoveMessage(id)) => Some(id.clone()),
            _ => None,
        })
        .expect("message.removed must dispatch HostRemoveMessage");
    assert_eq!(id, "msg-gone");
}

#[test]
fn message_part_removed_event_dispatches_remove_tool_call_action() {
    use raider_opencode::events::{MessagePartRemovedProps, ServerEvent};
    use raider_opencode::types::common::{MessageId, PartId, SessionId};
    let active = SessionId::new("ses-active");
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::MessagePartRemoved(MessagePartRemovedProps {
        session_id: active.clone(),
        message_id: MessageId::new("msg-x"),
        part_id: PartId::new("prt-gone"),
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let id = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::RemoveToolCall(id)) => Some(id.clone()),
            _ => None,
        })
        .expect("message.part.removed must dispatch HostRemoveToolCall");
    assert_eq!(id, "prt-gone");
}

#[test]
fn session_updated_event_dispatches_upsert_session_action() {
    use raider_opencode::events::{ServerEvent, SessionUpdatedProps};
    use raider_opencode::types::common::SessionId;
    use raider_opencode::types::session::Session;

    let active = SessionId::new("ses-self");
    let mut mirror = super::PartMirror::new();
    let info: Session = serde_json::from_value(serde_json::json!({
        "id": "ses-from-other-tui",
        "title": "Hello from Tab 2",
        "time": { "created": 100, "updated": 200 },
    }))
    .expect("decode session");
    let ev = ServerEvent::SessionUpdated(SessionUpdatedProps {
        session_id: Some(SessionId::new("ses-from-other-tui")),
        info,
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let entry = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::UpsertSession(e)) => Some(e.clone()),
            _ => None,
        })
        .expect("session.updated must emit HostUpsertSession");
    assert_eq!(entry.id, "ses-from-other-tui");
    assert_eq!(entry.title, "Hello from Tab 2");
}

#[test]
fn session_updated_for_active_session_refreshes_sidebar_title() {
    use raider_opencode::events::{ServerEvent, SessionUpdatedProps};
    use raider_opencode::types::common::SessionId;
    use raider_opencode::types::session::Session;
    let active = SessionId::new("ses-active");
    let mut mirror = super::PartMirror::new();
    let info: Session = serde_json::from_value(serde_json::json!({
        "id": "ses-active",
        "title": "Server-Generated Title",
        "time": { "created": 100, "updated": 200 },
    }))
    .expect("decode session");
    let ev = ServerEvent::SessionUpdated(SessionUpdatedProps {
        session_id: Some(SessionId::new("ses-active")),
        info,
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let title = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::SetSidebarTitle(s)) => Some(s.clone()),
            _ => None,
        })
        .expect("active-session update must refresh sidebar title");
    assert_eq!(title, "Server-Generated Title");
}

#[test]
fn session_updated_for_inactive_session_does_not_touch_sidebar_title() {
    use raider_opencode::events::{ServerEvent, SessionUpdatedProps};
    use raider_opencode::types::common::SessionId;
    use raider_opencode::types::session::Session;
    let active = SessionId::new("ses-active");
    let mut mirror = super::PartMirror::new();
    let info: Session = serde_json::from_value(serde_json::json!({
        "id": "ses-other",
        "title": "Other Tab's Title",
        "time": { "created": 100, "updated": 200 },
    }))
    .expect("decode session");
    let ev = ServerEvent::SessionUpdated(SessionUpdatedProps {
        session_id: Some(SessionId::new("ses-other")),
        info,
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let any_title = t.actions.iter().any(|a| {
        matches!(
            a,
            raider_tui::action::Action::Host(HostAction::SetSidebarTitle(_))
        )
    });
    assert!(
        !any_title,
        "inactive-session updates must NOT touch the sidebar title; \
             got actions: {:?}",
        t.actions,
    );
}

#[test]
fn session_deleted_event_dispatches_remove_session_action() {
    use raider_opencode::events::{ServerEvent, SessionDeletedProps};
    use raider_opencode::types::common::SessionId;

    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionDeleted(SessionDeletedProps {
        session_id: Some(SessionId::new("ses-gone")),
        info: None,
    });
    let t = super::translate(ev, None, &mut mirror);
    let id = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::RemoveSession(id)) => Some(id.clone()),
            _ => None,
        })
        .expect("session.deleted must emit HostRemoveSession");
    assert_eq!(id, "ses-gone");
}

#[test]
fn user_role_compaction_part_is_not_swallowed_by_user_guard() {
    use raider_opencode::events::{MessagePartUpdatedProps, ServerEvent};
    use raider_opencode::types::common::{MessageId, PartId, SessionId};
    use raider_opencode::types::message::{CompactionPart, MessagePart, MessageRole};

    let active = SessionId::new("ses-active");
    let msg_id = MessageId::new("msg-user-compaction");
    let mut mirror = super::PartMirror::new();
    mirror.remember_role(msg_id.clone(), MessageRole::User);

    let part = MessagePart::Compaction(CompactionPart {
        id: PartId::new("prt-c"),
        message_id: Some(msg_id.clone()),
        auto: true,
        overflow: Some(true),
        tail_start_id: None,
        extra: serde_json::Map::new(),
    });
    let ev = ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
        session_id: active.clone(),
        message_id: Some(msg_id),
        part,
        part_id: Some(PartId::new("prt-c")),
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let emitted_marker = t.actions.iter().any(|a| {
        matches!(
            a,
            raider_tui::action::Action::Host(HostAction::MarkCompaction { .. })
        )
    });
    assert!(
        emitted_marker,
        "compaction part on a user-role message MUST surface a HostMarkCompaction \
             action (the user-message guard must let it through); actions: {:?}",
        t.actions,
    );
}

#[test]
fn session_updated_with_parent_id_propagates_parent_to_entry() {
    use raider_opencode::events::{ServerEvent, SessionUpdatedProps};
    use raider_opencode::types::common::SessionId;
    use raider_opencode::types::session::Session;
    let mut mirror = super::PartMirror::new();
    let info: Session = serde_json::from_value(serde_json::json!({
        "id": "ses-child",
        "title": "find auth helpers (@explore subagent)",
        "parentID": "ses-parent",
        "time": { "created": 100, "updated": 200 },
    }))
    .expect("decode session");
    let ev = ServerEvent::SessionUpdated(SessionUpdatedProps {
        session_id: Some(SessionId::new("ses-child")),
        info,
    });
    let t = super::translate(ev, None, &mut mirror);
    let entry = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::UpsertSession(e)) => Some(e.clone()),
            _ => None,
        })
        .expect("session.updated must emit UpsertSession");
    assert_eq!(
        entry.parent_id.as_deref(),
        Some("ses-parent"),
        "parent_id must be propagated from wire `parentID` (drives subagent navigation)",
    );
}

#[test]
fn session_to_entry_preserves_parent_id_even_without_active_session() {
    use raider_opencode::types::common::SessionId;
    use raider_opencode::types::session::{Session, SessionTime};
    let s = Session {
        id: SessionId::new("ses-child"),
        title: "child".into(),
        parent_id: Some(SessionId::new("ses-parent")),
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let entry = super::session_to_entry(&s, None);
    assert_eq!(entry.parent_id.as_deref(), Some("ses-parent"));
}

#[test]
fn session_error_unwraps_anthropic_nested_data_message() {
    use raider_opencode::events::{ServerEvent, SessionErrorProps};
    let active = SessionId::new("ses-a");
    let mut mirror = super::PartMirror::new();
    let payload = serde_json::json!({
        "name": "ProviderError",
        "data": {
            "isRetryable": false,
            "message": "messages.2: `tool_use` ids were found without `tool_result` blocks immediately after: toolu_01NSdWts9SNy3xPxtN1CWnTP. Each `tool_use` block must have a corresponding `tool_result` block in the next message.",
            "metadata": { "url": "https://api.anthropic.com/v1/messages" },
            "responseBody": "{\"type\":\"error\",\"error\":{\"type\":\"invalid_request_error\",\"message\":\"...\"}}",
            "responseHeaders": {
                "set-cookie": "_cfuvid=...; HttpOnly; SameSite=None",
                "x-envoy-upstream-service-time": "123",
            },
        },
    });
    let ev = ServerEvent::SessionError(SessionErrorProps {
        session_id: Some(active.clone()),
        error: payload,
    });
    let t = super::translate(ev, Some(&active), &mut mirror);

    let toast_msg = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::View(raider_tui::ViewAction::ShowToast(t)) => {
                Some(t.message.clone())
            }
            _ => None,
        })
        .expect("session.error must surface a ShowToast");
    assert!(
        toast_msg.contains("tool_use") && toast_msg.contains("tool_result"),
        "toast must carry the unwrapped data.message; got {toast_msg:?}",
    );
    assert!(
        !toast_msg.contains("responseBody"),
        "toast must NOT carry the JSON wire blob; got {toast_msg:?}",
    );
    assert!(
        !toast_msg.contains("set-cookie") && !toast_msg.contains("_cfuvid"),
        "toast must NOT leak response headers/cookies; got {toast_msg:?}",
    );

    let attached = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::Host(HostAction::SetLastAssistantError(s)) => {
                Some(s.clone())
            }
            _ => None,
        })
        .expect("must attach error to the live assistant message");
    assert_eq!(attached, toast_msg);

    let legacy_system = t.actions.iter().any(|a| matches!(
        a,
        raider_tui::action::Action::Host(HostAction::SystemMessage(s)) if s.contains("session error")
    ));
    assert!(
        !legacy_system,
        "must not push the legacy `session error: …` system row; actions={:#?}",
        t.actions,
    );

    assert!(
        t.actions.iter().any(|a| matches!(
            a,
            raider_tui::action::Action::Host(HostAction::AssistantDone)
        )),
        "must finish the streaming assistant message",
    );
    assert!(
        t.actions.iter().any(|a| matches!(
            a,
            raider_tui::action::Action::Host(HostAction::SetBusy(false))
        )),
        "must clear busy flag",
    );
}

#[test]
fn session_error_falls_back_to_session_error_label_when_data_missing() {
    use raider_opencode::events::{ServerEvent, SessionErrorProps};
    let active = SessionId::new("ses-a");
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionError(SessionErrorProps {
        session_id: Some(active.clone()),
        error: serde_json::json!({}),
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    let toast_msg = t
        .actions
        .iter()
        .find_map(|a| match a {
            raider_tui::action::Action::View(raider_tui::ViewAction::ShowToast(t)) => {
                Some(t.message.clone())
            }
            _ => None,
        })
        .expect("empty payload must still toast");
    assert_eq!(toast_msg, "Session error");
}

#[test]
fn session_error_message_aborted_is_silently_swallowed() {
    use raider_opencode::events::{ServerEvent, SessionErrorProps};
    let active = SessionId::new("ses-a");
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionError(SessionErrorProps {
        session_id: Some(active.clone()),
        error: serde_json::json!({ "name": "MessageAbortedError" }),
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    assert!(
        !t.actions.iter().any(|a| matches!(
            a,
            raider_tui::action::Action::View(raider_tui::ViewAction::ShowToast(_))
        )),
        "MessageAbortedError must not produce a toast; actions={:#?}",
        t.actions,
    );
    assert!(
        !t.actions.iter().any(|a| matches!(
            a,
            raider_tui::action::Action::Host(HostAction::SetLastAssistantError(_))
        )),
        "MessageAbortedError must not attach an error to the assistant message",
    );
    assert!(t.actions.iter().any(|a| matches!(
        a,
        raider_tui::action::Action::Host(HostAction::AssistantDone)
    )),);
}

#[test]
fn session_error_for_inactive_session_is_ignored() {
    use raider_opencode::events::{ServerEvent, SessionErrorProps};
    let active = SessionId::new("ses-active");
    let other = SessionId::new("ses-other");
    let mut mirror = super::PartMirror::new();
    let ev = ServerEvent::SessionError(SessionErrorProps {
        session_id: Some(other),
        error: serde_json::json!({ "data": { "message": "noise from elsewhere" } }),
    });
    let t = super::translate(ev, Some(&active), &mut mirror);
    assert!(
        t.actions.is_empty(),
        "errors from non-active sessions must be silent; actions={:#?}",
        t.actions,
    );
}

fn lsp_placeholder_for(lsp_enabled: bool) -> String {
    use raider_opencode::types::session::{Session, SessionTime};
    use raider_tui::sidebar::SidebarBody;
    let s = Session {
        id: raider_opencode::SessionId::new("ses_lsp"),
        title: "T".into(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    };
    let actions = super::sidebar_actions_for_session(
        &s,
        None,
        &[],
        &[],
        &[],
        &Default::default(),
        &[],
        lsp_enabled,
    );
    let sections = actions
        .iter()
        .find_map(|a| match a {
            Action::Host(HostAction::SetSidebarSections(s)) => Some(s.clone()),
            _ => None,
        })
        .expect("sidebar sections emitted");
    let lsp_section = sections
        .into_iter()
        .find(|s| s.title == "LSP")
        .expect("LSP section emitted");
    match lsp_section.body {
        SidebarBody::Lsps {
            entries,
            placeholder,
            ..
        } => {
            assert!(entries.is_empty(), "test setup: no LSP entries fed");
            placeholder
        }
        other => panic!("expected LSP section body, got {other:?}"),
    }
}

#[test]
fn lsp_placeholder_says_disabled_when_config_lsp_is_off() {
    let placeholder = lsp_placeholder_for(false);
    assert_eq!(placeholder, "LSPs are disabled");
}

#[test]
fn lsp_placeholder_says_will_activate_when_config_lsp_is_on() {
    let placeholder = lsp_placeholder_for(true);
    assert_eq!(placeholder, "LSPs will activate as files are read");
}

#[test]
fn app_config_lsp_enabled_matches_opencode_truthiness() {
    use raider_opencode::types::config::AppConfig;
    use serde_json::json;
    let off = AppConfig {
        lsp: Some(json!(false)),
        ..Default::default()
    };
    assert!(!off.lsp_enabled(), "lsp: false → disabled");

    let omitted = AppConfig::default();
    assert!(!omitted.lsp_enabled(), "lsp omitted → disabled");
    let nulled = AppConfig {
        lsp: Some(json!(null)),
        ..Default::default()
    };
    assert!(!nulled.lsp_enabled(), "lsp: null → disabled");

    let on = AppConfig {
        lsp: Some(json!(true)),
        ..Default::default()
    };
    assert!(on.lsp_enabled(), "lsp: true → enabled");

    let obj = AppConfig {
        lsp: Some(json!({ "rust-analyzer": { "disabled": false } })),
        ..Default::default()
    };
    assert!(obj.lsp_enabled(), "lsp: {{...}} → enabled");
}
