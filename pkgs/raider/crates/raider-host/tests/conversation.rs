use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;

use raider_host::backend::{
    EventBackend, MessageBackend, PermissionBackend, PromptBackend, ProviderBackend,
    QuestionBackend, SessionBackend, ToolingBackend,
};
use raider_host::bridge::{provider_refresh_actions, translate, PartMirror};
use raider_host::{Runtime, RuntimeConfig};
use raider_opencode::events::{parse_frame, ServerEvent, StreamItem};
use raider_opencode::types::common::SessionId;
use raider_opencode::types::message::MessageWithParts;
use raider_opencode::types::provider::ProviderList;
use raider_opencode::types::session::{PromptPayload, Session, SessionCreatePayload, SessionTime};
use raider_opencode::Error;
use raider_tui::{Action, HostAction};

const CONVERSATION_FIXTURE: &str = include_str!("fixtures/conversation_stream.jsonl");
const PROVIDER_LIST_FIXTURE: &str = include_str!("fixtures/provider_list_with_free.json");
const REAL_SSE_CAPTURE: &str = include_str!("fixtures/real_sse_capture.jsonl");

#[test]
fn full_conversation_stream_rebuilds_assistant_reply() {
    let active = SessionId::new("ses_conv");
    let mut mirror = PartMirror::new();
    let mut deltas: Vec<String> = Vec::new();
    let mut done_count = 0usize;
    let mut busy_off_count = 0usize;
    let mut busy_on_count = 0usize;

    for (i, line) in CONVERSATION_FIXTURE.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let event: ServerEvent = parse_frame(line)
            .unwrap_or_else(|e| panic!("line {i} decode failed: {e}\nline={line}"));
        let translation = translate(event, Some(&active), &mut mirror);
        for action in translation.actions {
            match action {
                Action::Host(HostAction::AssistantDelta {
                    text,
                    thoughts: false,
                    ..
                }) => deltas.push(text),
                Action::Host(HostAction::AssistantDone { .. }) => done_count += 1,
                Action::Host(HostAction::SetBusy(true)) => busy_on_count += 1,
                Action::Host(HostAction::SetBusy(false)) => busy_off_count += 1,
                _ => {}
            }
        }
    }

    assert_eq!(
        deltas.concat(),
        "Hi there, how can I help?",
        "deltas should concatenate to the final assistant text; got {deltas:?}",
    );
    assert_eq!(
        deltas.len(),
        3,
        "expected three incremental deltas, got {deltas:?}",
    );
    assert!(
        busy_on_count >= 3,
        "expected ≥3 busy=true signals (one per delta), got {busy_on_count}",
    );
    assert_eq!(done_count, 2, "expected AssistantDone twice");
    assert_eq!(busy_off_count, 2, "expected HostSetBusy(false) twice");

    assert!(
        !deltas.iter().any(|d| d.contains("sup")),
        "user prompt text must not appear in assistant deltas: {deltas:?}",
    );
}

#[test]
fn conversation_for_other_session_is_ignored_by_translator() {
    let active = SessionId::new("ses_other");
    let mut mirror = PartMirror::new();

    for line in CONVERSATION_FIXTURE.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let event: ServerEvent = parse_frame(line).expect("decode");
        let translation = translate(event, Some(&active), &mut mirror);
        for action in translation.actions {
            assert!(
                !matches!(action, Action::Host(HostAction::AssistantDelta { .. })),
                "no deltas should fire when active session != event session, got {action:?}",
            );
            assert!(
                !matches!(action, Action::Host(HostAction::AssistantDone { .. })),
                "no AssistantDone should fire either, got {action:?}",
            );
        }
    }
}

#[test]
fn provider_list_with_free_models_picks_default_opus() {
    let list: ProviderList =
        serde_json::from_str(PROVIDER_LIST_FIXTURE).expect("decode provider list");
    let actions = provider_refresh_actions(&list, None);

    assert_eq!(actions.len(), 2, "catalog + default-model actions");
    match &actions[0] {
        Action::Host(HostAction::SetCatalog(c)) => {
            assert_eq!(c.providers[0].id, "opencode", "opencode sorts first");
            assert_eq!(c.providers[1].id, "anthropic");

            let opencode_models: Vec<&str> = c.providers[0]
                .models
                .iter()
                .map(|m| m.id.as_str())
                .collect();
            assert!(
                !opencode_models.contains(&"claude-haiku-deprecated"),
                "deprecated models filtered out: got {opencode_models:?}",
            );
            for free_id in [
                "big-pickle",
                "deepseek-v4-flash-free",
                "minimax-m25-free",
                "nemotron-3-super-free",
                "qwen36-plus-free",
            ] {
                assert!(
                    opencode_models.contains(&free_id),
                    "{free_id} missing from catalog: {opencode_models:?}",
                );
            }
            assert!(
                opencode_models.contains(&"claude-opus-47"),
                "paid Opus 4.7 also kept",
            );

            let opus = c.providers[0]
                .models
                .iter()
                .find(|m| m.id == "claude-opus-47")
                .expect("opus in catalog");
            assert_eq!(opus.variants, vec!["thinking".to_string()]);
        }
        other => panic!("expected HostSetCatalog first, got {other:?}"),
    }
    match &actions[1] {
        Action::Host(HostAction::SetCurrentModel(Some(m))) => {
            assert_eq!(m.provider_id, "opencode");
            assert_eq!(
                m.model_id, "claude-opus-47",
                "server-supplied default wins over free models",
            );
        }
        other => panic!("expected default = opencode/claude-opus-47, got {other:?}"),
    }
}

#[test]
fn picks_first_free_model_when_no_server_default_and_only_free_provider() {
    use std::collections::HashMap;

    let mut models = HashMap::new();
    for (id, name) in [
        ("big-pickle", "Big Pickle"),
        ("minimax-m25-free", "MiniMax M2.5 Free"),
    ] {
        models.insert(
            id.to_string(),
            raider_opencode::types::provider::ModelInfo {
                id: id.to_string(),
                provider_id: "opencode".to_string(),
                name: name.to_string(),
                status: Some("active".into()),
                cost: Some(raider_opencode::types::provider::ModelCost {
                    input: 0.0,
                    output: 0.0,
                    extra: serde_json::Map::new(),
                }),
                variants: HashMap::new(),
                limit: None,
                extra: serde_json::Map::new(),
            },
        );
    }
    let list = ProviderList {
        all: vec![raider_opencode::types::provider::ProviderInfo {
            id: "opencode".to_string(),
            name: "OpenCode Zen".to_string(),
            source: Some("api".into()),
            models,
            extra: serde_json::Map::new(),
        }],
        default: HashMap::new(),
        connected: vec!["opencode".into()],
    };

    let actions = provider_refresh_actions(&list, None);
    match actions.last() {
        Some(Action::Host(HostAction::SetCurrentModel(Some(m)))) => {
            assert_eq!(m.provider_id, "opencode");
            assert_eq!(
                m.model_id, "big-pickle",
                "with no `default` map, first alphabetically wins",
            );
        }
        other => panic!("expected HostSetCurrentModel(big-pickle), got {other:?}"),
    }
}

struct ConversationBackend {
    events: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<StreamItem>>>,
    event_tx: tokio::sync::mpsc::UnboundedSender<StreamItem>,
    session_messages_called: Mutex<bool>,
    session_get_calls: Mutex<usize>,
}

#[async_trait]
impl SessionBackend for ConversationBackend {
    async fn sessions_list(&self) -> Result<Vec<Session>, Error> {
        Ok(Vec::new())
    }

    async fn session_get(&self, id: &SessionId) -> Result<Session, Error> {
        *self.session_get_calls.lock().unwrap() += 1;
        let mut extra = serde_json::Map::new();
        let mut tokens = serde_json::Map::new();
        tokens.insert("input".into(), serde_json::json!(100u64));
        tokens.insert("output".into(), serde_json::json!(200u64));
        tokens.insert("reasoning".into(), serde_json::json!(0u64));
        let mut cache = serde_json::Map::new();
        cache.insert("read".into(), serde_json::json!(0u64));
        cache.insert("write".into(), serde_json::json!(0u64));
        tokens.insert("cache".into(), serde_json::Value::Object(cache));
        extra.insert("tokens".into(), serde_json::Value::Object(tokens));
        extra.insert("cost".into(), serde_json::json!(0.0125_f64));
        Ok(Session {
            id: id.clone(),
            title: "conv".into(),
            parent_id: None,
            time: SessionTime::default(),
            extra,
        })
    }

    async fn session_create(&self, _payload: &SessionCreatePayload) -> Result<Session, Error> {
        Ok(Session {
            id: SessionId::new("ses_conv"),
            title: "conv".into(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_rename(&self, session_id: &SessionId, _title: &str) -> Result<Session, Error> {
        Ok(Session {
            id: session_id.clone(),
            title: String::new(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_revert(
        &self,
        _session_id: &SessionId,
        _message_id: &str,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn session_unrevert(&self, _session_id: &SessionId) -> Result<(), Error> {
        Ok(())
    }

    async fn session_fork(
        &self,
        session_id: &SessionId,
        _message_id: Option<&str>,
    ) -> Result<Session, Error> {
        Ok(Session {
            id: session_id.clone(),
            title: String::new(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_delete(&self, _session_id: &SessionId) -> Result<(), Error> {
        Ok(())
    }

    async fn session_abort(&self, _session_id: &SessionId) -> Result<(), Error> {
        Ok(())
    }

    async fn session_share(&self, session_id: &SessionId) -> Result<Session, Error> {
        Ok(Session {
            id: session_id.clone(),
            title: String::new(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_unshare(&self, session_id: &SessionId) -> Result<Session, Error> {
        Ok(Session {
            id: session_id.clone(),
            title: String::new(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_summarize(
        &self,
        _session_id: &SessionId,
        _provider_id: &str,
        _model_id: &str,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn session_status_map(
        &self,
    ) -> Result<std::collections::HashMap<String, raider_opencode::events::SessionStatusKind>, Error>
    {
        Ok(std::collections::HashMap::new())
    }
}

#[async_trait]
impl MessageBackend for ConversationBackend {
    async fn session_messages(&self, _id: &SessionId) -> Result<Vec<MessageWithParts>, Error> {
        *self.session_messages_called.lock().unwrap() = true;
        Ok(Vec::new())
    }

    async fn session_diff(
        &self,
        _id: &SessionId,
    ) -> Result<Vec<raider_opencode::types::diff::FileDiff>, Error> {
        Ok(Vec::new())
    }

    async fn session_todo(
        &self,
        _id: &SessionId,
    ) -> Result<Vec<raider_opencode::types::todo::Todo>, Error> {
        Ok(Vec::new())
    }
}

#[async_trait]
impl PromptBackend for ConversationBackend {
    async fn session_prompt(
        &self,
        _session_id: &SessionId,
        _payload: &PromptPayload,
    ) -> Result<(), Error> {
        for line in CONVERSATION_FIXTURE.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let ev: ServerEvent = parse_frame(line).expect("fixture decode");
            tokio::task::yield_now().await;
            let _ = self.event_tx.send(StreamItem::Event(Box::new(ev)));
        }
        Ok(())
    }
}

#[async_trait]
impl ProviderBackend for ConversationBackend {
    async fn provider_list(&self) -> Result<ProviderList, Error> {
        Ok(ProviderList::default())
    }
}

#[async_trait]
impl ToolingBackend for ConversationBackend {
    async fn mcp_status(&self) -> Result<raider_opencode::types::mcp::McpRegistry, Error> {
        Ok(raider_opencode::types::mcp::McpRegistry::new())
    }

    async fn lsp_status(&self) -> Result<Vec<raider_opencode::types::lsp::LspStatus>, Error> {
        Ok(Vec::new())
    }

    async fn config_get(&self) -> Result<raider_opencode::types::config::AppConfig, Error> {
        Ok(raider_opencode::types::config::AppConfig::default())
    }

    async fn sync_start(&self, _directory: Option<&str>) -> Result<bool, Error> {
        Ok(false)
    }
}

#[async_trait]
impl PermissionBackend for ConversationBackend {
    async fn permission_list(
        &self,
    ) -> Result<Vec<raider_opencode::types::permission::PermissionRequest>, Error> {
        Ok(Vec::new())
    }

    async fn permission_reply(
        &self,
        _request_id: &str,
        _reply: raider_opencode::types::permission::PermissionReply,
        _message: Option<String>,
    ) -> Result<(), Error> {
        Ok(())
    }
}

#[async_trait]
impl QuestionBackend for ConversationBackend {
    async fn question_list(
        &self,
    ) -> Result<Vec<raider_opencode::types::question::QuestionRequest>, Error> {
        Ok(Vec::new())
    }

    async fn question_reply(
        &self,
        _request_id: &str,
        _answers: Vec<Vec<String>>,
    ) -> Result<(), Error> {
        Ok(())
    }

    async fn question_reject(&self, _request_id: &str) -> Result<(), Error> {
        Ok(())
    }
}

impl EventBackend for ConversationBackend {
    fn events(&self) -> Pin<Box<dyn Stream<Item = StreamItem> + Send>> {
        let rx = self
            .events
            .lock()
            .unwrap()
            .take()
            .expect("events taken twice");
        Box::pin(futures::stream::unfold(rx, |mut rx| async move {
            rx.recv().await.map(|item| (item, rx))
        }))
    }
}

async fn drain_actions(
    handle: &raider_host::HostHandle,
    cap: usize,
    budget: Duration,
) -> Vec<Action> {
    let deadline = tokio::time::Instant::now() + budget;
    let mut out = Vec::new();
    while out.len() < cap {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            break;
        }
        let mut rx = handle.actions.lock().await;
        match tokio::time::timeout(remaining, rx.recv()).await {
            Ok(Some(a)) => out.push(a),
            _ => break,
        }
    }
    out
}

#[tokio::test(flavor = "current_thread")]
async fn fresh_session_conversation_assistant_reply_reaches_app() {
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = Arc::new(ConversationBackend {
        events: Mutex::new(Some(event_rx)),
        event_tx,
        session_messages_called: Mutex::new(false),
        session_get_calls: Mutex::new(0),
    });

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
        },
    );

    handle
        .ui_events
        .send(raider_tui::Event::ModelChanged {
            model: raider_tui::ModelRef::new("opencode", "big-pickle"),
            variant: None,
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    handle
        .ui_events
        .send(raider_tui::Event::UserMessage("sup".to_string()))
        .unwrap();

    let actions = drain_actions(&handle, 200, Duration::from_millis(500)).await;

    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(_)))),
        "HostReplaceMessages must not fire on fresh-session create: {actions:#?}",
    );

    let deltas: Vec<&str> = actions
        .iter()
        .filter_map(|a| match a {
            Action::Host(HostAction::AssistantDelta {
                text,
                thoughts: false,
                ..
            }) => Some(text.as_str()),
            _ => None,
        })
        .collect();
    let combined: String = deltas.iter().copied().collect();
    assert_eq!(
        combined, "Hi there, how can I help?",
        "assistant reply must reach App via AssistantDelta actions; got deltas={deltas:?}",
    );

    assert!(
        !deltas.iter().any(|d| d.contains("sup")),
        "user prompt must not be re-injected as an assistant delta: {deltas:?}",
    );

    let done_count = actions
        .iter()
        .filter(|a| matches!(a, Action::Host(HostAction::AssistantDone { .. })))
        .count();
    assert_eq!(
        done_count, 2,
        "expected AssistantDone twice (completed + idle), got {done_count}; actions={actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn fresh_session_populates_sidebar_without_transcript_refetch() {
    use raider_tui::SidebarSection;

    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = Arc::new(ConversationBackend {
        events: Mutex::new(Some(event_rx)),
        event_tx,
        session_messages_called: Mutex::new(false),
        session_get_calls: Mutex::new(0),
    });

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
        },
    );

    handle
        .ui_events
        .send(raider_tui::Event::ModelChanged {
            model: raider_tui::ModelRef::new("opencode", "big-pickle"),
            variant: None,
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    handle
        .ui_events
        .send(raider_tui::Event::UserMessage("sup".to_string()))
        .unwrap();

    let actions = drain_actions(&handle, 400, Duration::from_millis(700)).await;

    let session_get_calls = *backend.session_get_calls.lock().unwrap();
    assert!(
        session_get_calls >= 1,
        "session_get must be called for sidebar refresh after fresh session create; \
         got {session_get_calls} call(s)",
    );

    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(_)))),
        "HostReplaceMessages must not fire on fresh-session create: {actions:#?}",
    );

    let title = actions.iter().rev().find_map(|a| match a {
        Action::Host(HostAction::SetSidebarTitle(t)) => Some(t.clone()),
        _ => None,
    });
    assert_eq!(
        title.as_deref(),
        Some("conv"),
        "expected HostSetSidebarTitle(\"conv\"); actions: {actions:#?}",
    );

    let sections = actions.iter().rev().find_map(|a| match a {
        Action::Host(HostAction::SetSidebarSections(s)) => Some(s.clone()),
        _ => None,
    });
    let sections: Vec<SidebarSection> =
        sections.expect("HostSetSidebarSections action must have been dispatched");

    let context = sections
        .iter()
        .find(|s| s.title == "Context")
        .unwrap_or_else(|| {
            panic!("expected a `Context` section in HostSetSidebarSections; got: {sections:#?}",)
        });
    let lines = context.lines();
    assert!(
        lines.iter().any(|l| l.contains("tokens")),
        "Context section must carry a `<N> tokens` line; got: {lines:?}",
    );
    assert!(
        lines
            .iter()
            .any(|l| l.contains("$0.01") || l.contains("$0.02")),
        "Context section must carry a `$<cost> spent` line; got: {lines:?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[test]
fn real_wire_capture_decodes_and_translates_deltas() {
    let active = SessionId::new("ses_1d210563cffeakhRZgzQjXwtbK");
    let mut mirror = PartMirror::new();
    let mut deltas: Vec<String> = Vec::new();
    let mut decoded_frames = 0usize;
    let mut delta_frames = 0usize;

    for (i, line) in REAL_SSE_CAPTURE.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        decoded_frames += 1;
        let event: ServerEvent = parse_frame(line)
            .unwrap_or_else(|e| panic!("real-wire line {i} decode failed: {e}\nline={line}"));
        if matches!(event, ServerEvent::MessagePartDelta(_)) {
            delta_frames += 1;
        }
        let translation = translate(event, Some(&active), &mut mirror);
        for action in translation.actions {
            if let Action::Host(HostAction::AssistantDelta {
                text,
                thoughts: false,
                ..
            }) = action
            {
                deltas.push(text);
            }
        }
    }

    assert_eq!(
        decoded_frames, 7,
        "fixture should have 7 frames (server.connected + session.updated + 5 deltas)",
    );
    assert_eq!(delta_frames, 5, "5 message.part.delta frames");
    assert_eq!(
        deltas.len(),
        5,
        "every delta frame must produce exactly one AssistantDelta (NO de-duplication, NO diffing); got {deltas:?}",
    );
    assert!(
        deltas.iter().all(|d| !d.is_empty()),
        "no empty deltas should survive translation: {deltas:?}",
    );
    assert!(
        deltas.iter().skip(1).any(|d| d.starts_with(' ')),
        "later deltas should carry their leading whitespace verbatim: {deltas:?}",
    );
    let concatenated: String = deltas.concat();
    assert_eq!(
        concatenated, "The user wants me to",
        "concatenated deltas must equal the streamed reply prefix verbatim; \
         got {concatenated:?} from {deltas:?}",
    );
}

#[test]
fn message_part_delta_routes_text_vs_reasoning() {
    let frames = [
        r#"{"id":"e1","type":"message.part.delta","properties":{"sessionID":"s","messageID":"m","partID":"p1","field":"text","delta":"hello"}}"#,
        r#"{"id":"e2","type":"message.part.delta","properties":{"sessionID":"s","messageID":"m","partID":"p2","field":"reasoning","delta":"thinking"}}"#,
        r#"{"id":"e3","type":"message.part.delta","properties":{"sessionID":"s","messageID":"m","partID":"p3","field":"signature","delta":"sig"}}"#,
        r#"{"id":"e4","type":"message.part.delta","properties":{"sessionID":"s","messageID":"m","partID":"p1","field":"text","delta":""}}"#,
    ];
    let active = SessionId::new("s");
    let mut mirror = PartMirror::new();
    let mut content_deltas = Vec::new();
    let mut thought_deltas = Vec::new();

    for line in frames {
        let ev: ServerEvent = parse_frame(line).expect("decode");
        let t = translate(ev, Some(&active), &mut mirror);
        for action in t.actions {
            if let Action::Host(HostAction::AssistantDelta { text, thoughts, .. }) = action {
                if thoughts {
                    thought_deltas.push(text);
                } else {
                    content_deltas.push(text);
                }
            }
        }
    }

    assert_eq!(content_deltas, vec!["hello".to_string()]);
    assert_eq!(thought_deltas, vec!["thinking".to_string()]);
}

#[test]
fn delta_for_other_session_is_ignored() {
    let line = r#"{"id":"e","type":"message.part.delta","properties":{"sessionID":"ses_other","messageID":"m","partID":"p","field":"text","delta":"noise"}}"#;
    let active = SessionId::new("ses_active");
    let mut mirror = PartMirror::new();
    let ev: ServerEvent = parse_frame(line).expect("decode");
    let t = translate(ev, Some(&active), &mut mirror);
    assert!(
        !t.actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::AssistantDelta { .. }))),
        "deltas for other sessions must produce no AssistantDelta: {t:?}",
    );
}
