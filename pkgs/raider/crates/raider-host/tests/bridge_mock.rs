use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::Stream;

use raider_opencode::{
    events::{MessagePartUpdatedProps, ServerEvent, SessionIdleProps, StreamItem},
    types::{
        common::{MessageId, PartId, SessionId},
        message::{MessagePart, MessageRole, MessageWithParts, TextPart},
        provider::ProviderList,
        session::{PromptPayload, Session, SessionCreatePayload, SessionTime},
    },
    Error,
};
use raider_tui::{Action, HostAction, ViewAction};

use raider_host::backend::{
    EventBackend, MessageBackend, PermissionBackend, PromptBackend, ProviderBackend,
    QuestionBackend, SessionBackend, ToolingBackend,
};
use raider_host::{Runtime, RuntimeConfig};

struct MockBackend {
    sessions: Vec<Session>,
    messages: Vec<MessageWithParts>,
    messages_delay: Duration,
    providers: ProviderList,
    events: Mutex<Option<tokio::sync::mpsc::UnboundedReceiver<StreamItem>>>,
    create_calls: Mutex<Vec<SessionCreatePayload>>,
    prompt_calls: Mutex<Vec<(SessionId, PromptPayload)>>,
    messages_calls: Mutex<Vec<SessionId>>,
    summarize_calls: Mutex<Vec<(SessionId, String, String)>>,
    summarize_fails: AtomicBool,
    share_calls: Mutex<Vec<SessionId>>,
    unshare_calls: Mutex<Vec<SessionId>>,
    abort_calls: Mutex<Vec<SessionId>>,
    revert_calls: Mutex<Vec<(SessionId, String)>>,
    unrevert_calls: Mutex<Vec<SessionId>>,
    rename_calls: Mutex<Vec<(SessionId, String)>>,
}

#[async_trait]
impl SessionBackend for MockBackend {
    async fn sessions_list(&self) -> Result<Vec<Session>, Error> {
        Ok(self.sessions.clone())
    }

    async fn session_get(&self, id: &SessionId) -> Result<Session, Error> {
        self.sessions
            .iter()
            .find(|s| &s.id == id)
            .cloned()
            .ok_or_else(|| Error::NotFound(format!("session {} not found", id.as_str())))
    }

    async fn session_create(&self, payload: &SessionCreatePayload) -> Result<Session, Error> {
        self.create_calls
            .lock()
            .expect("create_calls mutex poisoned")
            .push(payload.clone());
        Ok(Session {
            id: SessionId::new("ses-mock-new"),
            title: "Mock new session".to_string(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_rename(&self, session_id: &SessionId, title: &str) -> Result<Session, Error> {
        self.rename_calls
            .lock()
            .expect("rename_calls mutex poisoned")
            .push((session_id.clone(), title.to_string()));
        Ok(Session {
            id: session_id.clone(),
            title: title.to_string(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_revert(&self, session_id: &SessionId, message_id: &str) -> Result<(), Error> {
        self.revert_calls
            .lock()
            .expect("revert_calls mutex poisoned")
            .push((session_id.clone(), message_id.to_string()));
        Ok(())
    }

    async fn session_unrevert(&self, session_id: &SessionId) -> Result<(), Error> {
        self.unrevert_calls
            .lock()
            .expect("unrevert_calls mutex poisoned")
            .push(session_id.clone());
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

    async fn session_abort(&self, session_id: &SessionId) -> Result<(), Error> {
        self.abort_calls
            .lock()
            .expect("abort_calls mutex poisoned")
            .push(session_id.clone());
        Ok(())
    }

    async fn session_share(&self, session_id: &SessionId) -> Result<Session, Error> {
        self.share_calls
            .lock()
            .expect("share_calls mutex poisoned")
            .push(session_id.clone());
        let mut extra = serde_json::Map::new();
        extra.insert(
            "share".into(),
            serde_json::json!({ "url": "https://example.com/s/mock" }),
        );
        Ok(Session {
            id: session_id.clone(),
            title: "shared".into(),
            parent_id: None,
            time: SessionTime::default(),
            extra,
        })
    }

    async fn session_unshare(&self, session_id: &SessionId) -> Result<Session, Error> {
        self.unshare_calls
            .lock()
            .expect("unshare_calls mutex poisoned")
            .push(session_id.clone());
        Ok(Session {
            id: session_id.clone(),
            title: "unshared".into(),
            parent_id: None,
            time: SessionTime::default(),
            extra: serde_json::Map::new(),
        })
    }

    async fn session_summarize(
        &self,
        session_id: &SessionId,
        provider_id: &str,
        model_id: &str,
    ) -> Result<(), Error> {
        self.summarize_calls
            .lock()
            .expect("summarize_calls mutex poisoned")
            .push((
                session_id.clone(),
                provider_id.to_string(),
                model_id.to_string(),
            ));
        if self.summarize_fails.load(Ordering::SeqCst) {
            return Err(Error::Http {
                status: 504,
                path: format!("/session/{}/summarize", session_id.as_str()),
                body: "simulated transport timeout".to_string(),
            });
        }
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
impl MessageBackend for MockBackend {
    async fn session_messages(&self, id: &SessionId) -> Result<Vec<MessageWithParts>, Error> {
        if !self.messages_delay.is_zero() {
            tokio::time::sleep(self.messages_delay).await;
        }
        self.messages_calls
            .lock()
            .expect("messages_calls mutex poisoned")
            .push(id.clone());
        Ok(self.messages.clone())
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
impl PromptBackend for MockBackend {
    async fn session_prompt(
        &self,
        session_id: &SessionId,
        payload: &PromptPayload,
    ) -> Result<(), Error> {
        self.prompt_calls
            .lock()
            .expect("prompt_calls mutex poisoned")
            .push((session_id.clone(), payload.clone()));
        Ok(())
    }
}

#[async_trait]
impl ProviderBackend for MockBackend {
    async fn provider_list(&self) -> Result<ProviderList, Error> {
        Ok(self.providers.clone())
    }
}

#[async_trait]
impl ToolingBackend for MockBackend {
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
impl PermissionBackend for MockBackend {
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
impl QuestionBackend for MockBackend {
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

impl EventBackend for MockBackend {
    fn events(&self) -> Pin<Box<dyn Stream<Item = StreamItem> + Send>> {
        let rx = self
            .events
            .lock()
            .expect("events mutex poisoned")
            .take()
            .expect("events() called more than once");
        Box::pin(tokio_stream_wrap(rx))
    }
}

fn tokio_stream_wrap(
    rx: tokio::sync::mpsc::UnboundedReceiver<StreamItem>,
) -> impl Stream<Item = StreamItem> + Send {
    futures::stream::unfold(rx, |mut rx| async move {
        let v = rx.recv().await?;
        Some((v, rx))
    })
}

fn fake_session(id: &str, title: &str) -> Session {
    Session {
        id: SessionId::new(id),
        title: title.to_string(),
        parent_id: None,
        time: SessionTime::default(),
        extra: serde_json::Map::new(),
    }
}

fn fake_text_message(session: &str, message: &str, part: &str, text: &str) -> MessageWithParts {
    MessageWithParts {
        info: raider_opencode::types::message::Message {
            id: MessageId::new(message),
            session_id: Some(SessionId::new(session)),
            role: MessageRole::Assistant,
            time: Default::default(),
            extra: serde_json::Map::new(),
        },
        parts: vec![MessagePart::Text(TextPart {
            id: PartId::new(part),
            text: text.to_string(),
            message_id: None,
            extra: serde_json::Map::new(),
        })],
    }
}

#[tokio::test(flavor = "current_thread")]
async fn read_only_viewer_end_to_end() {
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();

    let backend = Arc::new(MockBackend {
        sessions: vec![
            fake_session("ses-a", "Session A"),
            fake_session("ses-b", "Session B"),
        ],
        messages: vec![fake_text_message("ses-a", "msg-1", "prt-1", "hello")],
        messages_delay: Duration::ZERO,
        providers: ProviderList::default(),
        events: Mutex::new(Some(event_rx)),
        create_calls: Mutex::new(Vec::new()),
        prompt_calls: Mutex::new(Vec::new()),
        messages_calls: Mutex::new(Vec::new()),
        summarize_calls: Mutex::new(Vec::new()),
        summarize_fails: AtomicBool::new(false),
        share_calls: Mutex::new(Vec::new()),
        unshare_calls: Mutex::new(Vec::new()),
        abort_calls: Mutex::new(Vec::new()),
        revert_calls: Mutex::new(Vec::new()),
        unrevert_calls: Mutex::new(Vec::new()),
        rename_calls: Mutex::new(Vec::new()),
    });

    let mut handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-a")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let actions = drain_for(&handle, 20, Duration::from_millis(500)).await;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetSessions(s)) if s.len() == 2)),
        "expected HostSetSessions with 2 entries, got {actions:#?}",
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Host(HostAction::SetCurrentSession(Some(id))) if id == "ses-a"
        )),
        "expected HostSetCurrentSession(Some(\"ses-a\"))",
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(m)) if !m.is_empty())),
        "expected HostReplaceMessages with non-empty transcript",
    );

    event_tx
        .send(StreamItem::Event(Box::new(
            ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
                session_id: SessionId::new("ses-a"),
                message_id: Some(MessageId::new("msg-2")),
                part: MessagePart::Text(TextPart {
                    id: PartId::new("prt-2"),
                    text: "hi".into(),

                    message_id: None,
                    extra: serde_json::Map::new(),
                }),
                part_id: None,
            }),
        )))
        .unwrap();
    let actions = drain_for(&handle, 5, Duration::from_millis(500)).await;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::AssistantDelta { text, thoughts: false, .. }) if text == "hi")),
        "expected AssistantDelta(hi), got {actions:#?}",
    );

    event_tx
        .send(StreamItem::Event(Box::new(ServerEvent::SessionIdle(
            SessionIdleProps {
                session_id: SessionId::new("ses-a"),
            },
        ))))
        .unwrap();
    let actions = drain_for(&handle, 5, Duration::from_millis(500)).await;
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::AssistantDone { .. }))),
        "expected AssistantDone, got {actions:#?}",
    );

    event_tx
        .send(StreamItem::Event(Box::new(
            ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
                session_id: SessionId::new("ses-other"),
                message_id: Some(MessageId::new("msg-x")),
                part: MessagePart::Text(TextPart {
                    id: PartId::new("prt-x"),
                    text: "noise".into(),

                    message_id: None,
                    extra: serde_json::Map::new(),
                }),
                part_id: None,
            }),
        )))
        .unwrap();
    let actions = drain_for(&handle, 1, Duration::from_millis(150)).await;
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::AssistantDelta { text, .. }) if text == "noise")),
        "events for other sessions must not produce deltas",
    );

    handle.shutdown();
}

async fn drain_for(handle: &raider_host::HostHandle, cap: usize, budget: Duration) -> Vec<Action> {
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
            Ok(None) => break,
            Err(_) => break,
        }
    }
    out
}

fn empty_backend_with_events(
    event_rx: tokio::sync::mpsc::UnboundedReceiver<StreamItem>,
) -> Arc<MockBackend> {
    Arc::new(MockBackend {
        sessions: Vec::new(),
        messages: Vec::new(),
        messages_delay: Duration::ZERO,
        providers: ProviderList::default(),
        events: Mutex::new(Some(event_rx)),
        create_calls: Mutex::new(Vec::new()),
        prompt_calls: Mutex::new(Vec::new()),
        messages_calls: Mutex::new(Vec::new()),
        summarize_calls: Mutex::new(Vec::new()),
        summarize_fails: AtomicBool::new(false),
        share_calls: Mutex::new(Vec::new()),
        unshare_calls: Mutex::new(Vec::new()),
        abort_calls: Mutex::new(Vec::new()),
        revert_calls: Mutex::new(Vec::new()),
        unrevert_calls: Mutex::new(Vec::new()),
        rename_calls: Mutex::new(Vec::new()),
    })
}

fn count_system_messages(actions: &[Action], needle: &str) -> usize {
    actions
        .iter()
        .filter(|a| match a {
            Action::Host(HostAction::SystemMessage(s)) => s.contains(needle),
            _ => false,
        })
        .count()
}

#[tokio::test(flavor = "current_thread")]
async fn disconnect_message_rate_limited_under_storm() {
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 3,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    for _ in 0..50 {
        event_tx
            .send(StreamItem::Error(raider_opencode::Error::StreamClosed))
            .unwrap();
    }

    let actions = drain_for(&handle, 200, Duration::from_millis(300)).await;
    let disconnects = count_system_messages(&actions, "disconnected from opencode");
    let reconnects = count_system_messages(&actions, "reconnected to opencode");
    assert_eq!(
        disconnects, 1,
        "exactly one disconnect line per 30s dedup window, got {disconnects}: {actions:#?}",
    );
    assert_eq!(
        reconnects, 0,
        "no reconnect line should fire without a real event arriving, got {reconnects}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn reconnected_message_only_after_disconnect_warning() {
    let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 3,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    event_tx
        .send(StreamItem::Error(raider_opencode::Error::StreamClosed))
        .unwrap();
    event_tx
        .send(StreamItem::Error(raider_opencode::Error::StreamClosed))
        .unwrap();
    event_tx
        .send(StreamItem::Event(Box::new(ServerEvent::SessionIdle(
            SessionIdleProps {
                session_id: SessionId::new("ses-a"),
            },
        ))))
        .unwrap();

    let actions = drain_for(&handle, 50, Duration::from_millis(200)).await;
    assert_eq!(
        count_system_messages(&actions, "disconnected from opencode"),
        0,
        "below-threshold errors must not surface a disconnect line: {actions:#?}",
    );
    assert_eq!(
        count_system_messages(&actions, "reconnected to opencode"),
        0,
        "a clean first-connect event must not surface a reconnect line: {actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn user_message_creates_session_then_submits_prompt() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    handle
        .ui_events
        .send(raider_tui::Event::ModelChanged {
            model: raider_tui::ModelRef::new("opencode", "claude-opus"),
            variant: None,
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    handle
        .ui_events
        .send(raider_tui::Event::UserMessage("yo".to_string()))
        .unwrap();

    let actions = drain_for(&handle, 20, Duration::from_millis(300)).await;

    let creates = backend.create_calls.lock().unwrap().clone();
    let prompts = backend.prompt_calls.lock().unwrap().clone();

    let bind = actions.iter().find_map(|a| match a {
        Action::Host(HostAction::BindLastUserMessage { server_id, agent }) => {
            Some((server_id.clone(), agent.clone()))
        }
        _ => None,
    });
    let (bind_id, bind_agent) =
        bind.expect("expected BindLastUserMessage to be emitted before session_prompt");
    assert_eq!(bind_agent.as_deref(), Some("build"));
    assert!(
        !bind_id.is_empty(),
        "bound server_id must be non-empty (got: {bind_id:?})",
    );

    assert_eq!(
        creates.len(),
        1,
        "exactly one session_create when starting with no active session, got {creates:#?}",
    );
    let create = &creates[0];
    let model = create.model.as_ref().expect("create payload carries model");
    assert_eq!(model.provider_id, "opencode");
    assert_eq!(model.id, "claude-opus");
    assert_eq!(create.agent.as_deref(), Some("build"));

    assert_eq!(
        prompts.len(),
        1,
        "exactly one session_prompt after create, got {prompts:#?}",
    );
    let (session_id, prompt) = &prompts[0];
    assert_eq!(
        session_id.as_str(),
        "ses-mock-new",
        "prompt routed to the newly-minted session id",
    );
    let model = prompt.model.as_ref().expect("prompt payload carries model");
    assert_eq!(model.provider_id, "opencode");
    assert_eq!(model.model_id, "claude-opus");
    assert_eq!(prompt.agent.as_deref(), Some("build"));
    assert!(
        prompt.message_id.is_some(),
        "prompt payload should carry a freshly-minted message id",
    );
    assert_eq!(
        prompt.message_id.as_ref().map(|m| m.as_str().to_string()),
        Some(bind_id.clone()),
        "BindLastUserMessage.server_id must match the MessageId sent in the prompt payload",
    );
    assert_eq!(prompt.parts.len(), 1);
    match &prompt.parts[0] {
        raider_opencode::types::session::PromptPart::Text(t) => {
            assert_eq!(t.text, "yo");
            assert!(t.id.is_none(), "raider lets the server mint part ids");
        }
        raider_opencode::types::session::PromptPart::File(_) => {
            panic!("plain UserMessage must not produce a file part");
        }
    }

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn fresh_session_creation_does_not_wipe_local_transcript() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    handle
        .ui_events
        .send(raider_tui::Event::ModelChanged {
            model: raider_tui::ModelRef::new("opencode", "claude-opus"),
            variant: None,
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;

    handle
        .ui_events
        .send(raider_tui::Event::UserMessage("yo".to_string()))
        .unwrap();

    let actions = drain_for(&handle, 30, Duration::from_millis(300)).await;

    let replaces: Vec<_> = actions
        .iter()
        .filter(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(_))))
        .collect();
    assert!(
        replaces.is_empty(),
        "fresh-session create must NOT emit HostReplaceMessages, got {replaces:#?}",
    );

    let fetches = backend.messages_calls.lock().unwrap().clone();
    for sid in &fetches {
        assert_eq!(
            sid.as_str(),
            "ses-mock-new",
            "session_messages must only be called for the freshly minted session id, \
             got {fetches:#?}",
        );
    }

    assert_eq!(backend.create_calls.lock().unwrap().len(), 1);
    assert_eq!(backend.prompt_calls.lock().unwrap().len(), 1);

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn session_switched_does_refetch_messages() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    handle
        .ui_events
        .send(raider_tui::Event::SessionSwitched(
            "ses-existing".to_string(),
        ))
        .unwrap();

    let actions = drain_for(&handle, 30, Duration::from_millis(300)).await;

    let fetches = backend.messages_calls.lock().unwrap().clone();
    assert!(
        !fetches.is_empty(),
        "SessionSwitched must trigger at least one session_messages fetch, got {fetches:#?}",
    );
    for sid in &fetches {
        assert_eq!(
            sid.as_str(),
            "ses-existing",
            "session_messages must only fetch the picked session id, got {fetches:#?}",
        );
    }

    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(_)))),
        "SessionSwitched must dispatch HostReplaceMessages (transcript refetch); \
         actions:\n{actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn switching_back_to_cached_session_does_not_replay_stale_transcript() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = Arc::new(MockBackend {
        sessions: vec![
            fake_session("ses-a", "Session A"),
            fake_session("ses-b", "Session B"),
        ],
        messages: vec![fake_text_message("ses-a", "msg-1", "prt-1", "cached")],
        messages_delay: Duration::from_millis(150),
        providers: ProviderList::default(),
        events: Mutex::new(Some(event_rx)),
        create_calls: Mutex::new(Vec::new()),
        prompt_calls: Mutex::new(Vec::new()),
        messages_calls: Mutex::new(Vec::new()),
        summarize_calls: Mutex::new(Vec::new()),
        summarize_fails: AtomicBool::new(false),
        share_calls: Mutex::new(Vec::new()),
        unshare_calls: Mutex::new(Vec::new()),
        abort_calls: Mutex::new(Vec::new()),
        revert_calls: Mutex::new(Vec::new()),
        unrevert_calls: Mutex::new(Vec::new()),
        rename_calls: Mutex::new(Vec::new()),
    });

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-a")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let startup = drain_for(&handle, 80, Duration::from_millis(600)).await;
    assert!(
        startup
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(messages)) if !messages.is_empty())),
        "initial load must populate the cache, got {startup:#?}",
    );

    handle
        .ui_events
        .send(raider_tui::Event::SessionSwitched("ses-b".to_string()))
        .unwrap();
    let _ = drain_for(&handle, 80, Duration::from_millis(600)).await;

    handle
        .ui_events
        .send(raider_tui::Event::SessionSwitched("ses-a".to_string()))
        .unwrap();

    let cached = drain_for(&handle, 40, Duration::from_millis(50)).await;
    assert!(
        !cached
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(_)))),
        "cached host replay must not overwrite the TUI's fresher local transcript before the delayed refetch completes, got {cached:#?}",
    );

    let refreshed = drain_for(&handle, 40, Duration::from_millis(300)).await;
    assert!(
        !refreshed
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::ReplaceMessages(_)))),
        "cached background refresh must not rebuild the active transcript, got {refreshed:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn compact_command_dispatches_session_summarize() {
    use raider_opencode::types::provider::{ModelInfo, ProviderInfo};
    use std::collections::HashMap;

    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();

    let mut models = HashMap::new();
    models.insert(
        "claude-opus".into(),
        ModelInfo {
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
    let providers = ProviderList {
        all: vec![ProviderInfo {
            id: "opencode".into(),
            name: "OpenCode Zen".into(),
            source: None,
            models,
            extra: serde_json::Map::new(),
        }],
        default,
        connected: vec!["opencode".into()],
    };

    let backend = Arc::new(MockBackend {
        sessions: vec![fake_session("ses-c", "Compact test")],
        messages: Vec::new(),
        messages_delay: Duration::ZERO,
        providers,
        events: Mutex::new(Some(event_rx)),
        create_calls: Mutex::new(Vec::new()),
        prompt_calls: Mutex::new(Vec::new()),
        messages_calls: Mutex::new(Vec::new()),
        summarize_calls: Mutex::new(Vec::new()),
        summarize_fails: AtomicBool::new(false),
        share_calls: Mutex::new(Vec::new()),
        unshare_calls: Mutex::new(Vec::new()),
        abort_calls: Mutex::new(Vec::new()),
        revert_calls: Mutex::new(Vec::new()),
        unrevert_calls: Mutex::new(Vec::new()),
        rename_calls: Mutex::new(Vec::new()),
    });

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-c")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 20, Duration::from_millis(500)).await;

    handle
        .ui_events
        .send(raider_tui::Event::Command {
            name: "compact".into(),
            args: String::new(),
        })
        .expect("ui_events send");

    let _ = drain_for(&handle, 5, Duration::from_millis(300)).await;

    let calls = backend
        .summarize_calls
        .lock()
        .expect("summarize_calls mutex poisoned");
    assert_eq!(
        calls.len(),
        1,
        "expected exactly one session.summarize call, got {}: {:?}",
        calls.len(),
        *calls,
    );
    let (sid, pid, mid) = &calls[0];
    assert_eq!(sid.as_str(), "ses-c");
    assert_eq!(pid, "opencode");
    assert_eq!(mid, "claude-opus");

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn compact_command_does_not_surface_backend_http_failure_to_user() {
    use raider_opencode::types::provider::{ModelInfo, ProviderInfo};
    use std::collections::HashMap;

    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();

    let mut models = HashMap::new();
    models.insert(
        "claude-opus".into(),
        ModelInfo {
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
    let providers = ProviderList {
        all: vec![ProviderInfo {
            id: "opencode".into(),
            name: "OpenCode Zen".into(),
            source: None,
            models,
            extra: serde_json::Map::new(),
        }],
        default,
        connected: vec!["opencode".into()],
    };

    let backend = Arc::new(MockBackend {
        sessions: vec![fake_session("ses-c", "Compact failure test")],
        messages: Vec::new(),
        messages_delay: Duration::ZERO,
        providers,
        events: Mutex::new(Some(event_rx)),
        create_calls: Mutex::new(Vec::new()),
        prompt_calls: Mutex::new(Vec::new()),
        messages_calls: Mutex::new(Vec::new()),
        summarize_calls: Mutex::new(Vec::new()),
        summarize_fails: AtomicBool::new(true),
        share_calls: Mutex::new(Vec::new()),
        unshare_calls: Mutex::new(Vec::new()),
        abort_calls: Mutex::new(Vec::new()),
        revert_calls: Mutex::new(Vec::new()),
        unrevert_calls: Mutex::new(Vec::new()),
        rename_calls: Mutex::new(Vec::new()),
    });

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-c")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 20, Duration::from_millis(500)).await;

    handle
        .ui_events
        .send(raider_tui::Event::Command {
            name: "compact".into(),
            args: String::new(),
        })
        .expect("ui_events send");

    let post_compact_actions = drain_for(&handle, 10, Duration::from_millis(400)).await;

    let calls = backend
        .summarize_calls
        .lock()
        .expect("summarize_calls mutex poisoned");
    assert_eq!(
        calls.len(),
        1,
        "expected exactly one session.summarize dispatch, got {}: {:?}",
        calls.len(),
        *calls,
    );
    drop(calls);

    let offending: Vec<_> = post_compact_actions
        .iter()
        .filter_map(|a| match a {
            Action::Host(HostAction::SystemMessage(text))
                if text.to_lowercase().contains("failed to compact") =>
            {
                Some(text.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        offending.is_empty(),
        "fire-and-forget /compact must NOT surface HTTP failures as user-visible \
         SystemMessage; got {offending:?}\nall actions: {post_compact_actions:?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn compact_without_active_session_emits_system_message() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Command {
            name: "compact".into(),
            args: String::new(),
        })
        .expect("ui_events send");
    let actions = drain_for(&handle, 5, Duration::from_millis(200)).await;

    let calls = backend.summarize_calls.lock().expect("mutex");
    assert!(
        calls.is_empty(),
        "no active session must yield no backend call; got {:?}",
        *calls,
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Host(HostAction::SystemMessage(s)) if s.contains("No active session")
        )),
        "expected guard SystemMessage; actions: {actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn share_command_dispatches_session_share_and_surfaces_url() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-share")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Command {
            name: "share".into(),
            args: String::new(),
        })
        .expect("ui_events send");
    let actions = drain_for(&handle, 10, Duration::from_millis(300)).await;

    let calls = backend.share_calls.lock().expect("mutex");
    assert_eq!(
        calls.len(),
        1,
        "expected exactly one session.share call; got {:?}",
        *calls,
    );
    assert_eq!(calls[0].as_str(), "ses-share");
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::View(ViewAction::CopyToClipboard { text, success_message, .. })
                if text.contains("example.com/s/mock")
                    && success_message == "Share URL copied to clipboard!"
        )),
        "shared URL must be copied via Action::View(ViewAction::CopyToClipboard); actions: {actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn unshare_command_dispatches_session_unshare() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-unshare")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Command {
            name: "unshare".into(),
            args: String::new(),
        })
        .expect("ui_events send");
    let actions = drain_for(&handle, 10, Duration::from_millis(300)).await;

    let calls = backend.unshare_calls.lock().expect("mutex");
    assert_eq!(
        calls.len(),
        1,
        "expected exactly one session.unshare call; got {:?}",
        *calls,
    );
    assert_eq!(calls[0].as_str(), "ses-unshare");
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::View(ViewAction::ShowToast(toast))
                if toast.message == "Session unshared successfully"
        )),
        "unshare confirmation must surface as a toast; actions: {actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn interrupt_event_dispatches_session_abort() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-abort")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Interrupt)
        .expect("ui_events send");
    let actions = drain_for(&handle, 10, Duration::from_millis(300)).await;

    let calls = backend.abort_calls.lock().expect("mutex");
    assert_eq!(
        calls.len(),
        1,
        "expected one session.abort call after Esc; got {:?}",
        *calls,
    );
    assert_eq!(calls[0].as_str(), "ses-abort");
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetBusy(false)))),
        "interrupt must clear the busy flag optimistically; actions: {actions:#?}",
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::AssistantDone { .. }))),
        "interrupt must surface AssistantDone so the spinner stops; \
         actions: {actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn interrupt_event_without_session_is_silent_noop() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: None,
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Interrupt)
        .expect("ui_events send");
    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;

    let calls = backend.abort_calls.lock().expect("mutex");
    assert!(
        calls.is_empty(),
        "no active session must yield no abort call; got {:?}",
        *calls,
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn undo_event_dispatches_session_revert() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-revert")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Undo {
            message_id: "msg-target".into(),
        })
        .expect("ui_events send");
    let _ = drain_for(&handle, 10, Duration::from_millis(500)).await;
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let calls = backend.revert_calls.lock().expect("mutex");
    assert_eq!(
        calls.len(),
        1,
        "expected one session.revert call; got {:?}",
        *calls,
    );
    assert_eq!(calls[0].0.as_str(), "ses-revert");
    assert_eq!(calls[0].1, "msg-target");

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn redo_event_dispatches_session_unrevert() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-unrevert")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Redo)
        .expect("ui_events send");
    let _ = drain_for(&handle, 10, Duration::from_millis(500)).await;
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let calls = backend.unrevert_calls.lock().expect("mutex");
    assert_eq!(
        calls.len(),
        1,
        "expected one session.unrevert call; got {:?}",
        *calls,
    );
    assert_eq!(calls[0].as_str(), "ses-unrevert");

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn rename_command_dispatches_session_rename() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-rn")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Command {
            name: "rename".into(),
            args: "fresh title".into(),
        })
        .expect("ui_events send");
    let _ = drain_for(&handle, 10, Duration::from_millis(500)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let calls = backend.rename_calls.lock().expect("mutex");
    assert_eq!(
        calls.len(),
        1,
        "expected one session.rename call; got {:?}",
        *calls,
    );
    assert_eq!(calls[0].0.as_str(), "ses-rn");
    assert_eq!(calls[0].1, "fresh title");

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn explicit_rename_event_dispatches_selected_session_rename() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-active")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 5, Duration::from_millis(200)).await;
    handle
        .ui_events
        .send(raider_tui::Event::RenameSession {
            session_id: "ses-selected".into(),
            title: "selected title".into(),
        })
        .expect("ui_events send");
    let _ = drain_for(&handle, 10, Duration::from_millis(500)).await;
    tokio::time::sleep(Duration::from_millis(100)).await;

    let calls = backend.rename_calls.lock().expect("mutex");
    assert_eq!(
        calls.len(),
        1,
        "expected one session.rename call; got {:?}",
        *calls,
    );
    assert_eq!(calls[0].0.as_str(), "ses-selected");
    assert_eq!(calls[0].1, "selected title");

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn new_command_resets_active_session_and_transcript() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-old")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 20, Duration::from_millis(400)).await;
    handle
        .ui_events
        .send(raider_tui::Event::Command {
            name: "new".into(),
            args: String::new(),
        })
        .expect("ui_events send");
    let actions = drain_for(&handle, 30, Duration::from_millis(500)).await;

    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetCurrentSession(None)))),
        "/new must clear the current session id; actions: {actions:#?}",
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Host(HostAction::ReplaceMessages(msgs)) if msgs.is_empty()
        )),
        "/new must wipe the transcript; actions: {actions:#?}",
    );
    assert!(
        actions.iter().all(|a| !matches!(
            a,
            Action::Host(HostAction::SystemMessage(s)) if s.to_lowercase().contains("new session")
        )),
        "/new must not add a transcript guidance message; actions: {actions:#?}",
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Host(HostAction::SetSidebarSections(s)) if s.is_empty()
        )),
        "/new must wipe sidebar sections; actions: {actions:#?}",
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetSidebarSubtitle(None)))),
        "/new must clear sidebar subtitle (the session id row); \
         actions: {actions:#?}",
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetBusy(false)))),
        "/new must clear the busy flag (wipe spinner); \
         actions: {actions:#?}",
    );
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::Host(HostAction::SetUsage(None)))),
        "/new must clear the usage cluster (`460K · $92.44`); \
         actions: {actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn subagent_navigate_does_not_abort_outgoing_session() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-parent")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 30, Duration::from_millis(150)).await;
    backend.messages_calls.lock().unwrap().clear();

    handle
        .ui_events
        .send(raider_tui::Event::SubagentNavigate("ses-child".to_string()))
        .unwrap();

    let actions = drain_for(&handle, 20, Duration::from_millis(300)).await;

    let aborts = backend.abort_calls.lock().unwrap().clone();
    assert!(
        aborts.is_empty(),
        "SubagentNavigate must NOT call session_abort; \
         called for: {aborts:#?}",
    );

    let fetches = backend.messages_calls.lock().unwrap().clone();
    assert!(
        fetches.iter().any(|sid| sid.as_str() == "ses-child"),
        "must refetch the child transcript on subagent navigation; \
         got fetches={fetches:#?}",
    );
    assert!(
        actions.iter().any(|a| matches!(
            a,
            Action::Host(HostAction::SetCurrentSession(Some(id))) if id == "ses-child"
        )),
        "must dispatch SetCurrentSession(ses-child); actions:\n{actions:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}

#[tokio::test(flavor = "current_thread")]
async fn session_switched_does_not_abort_outgoing_session() {
    let (_event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel::<StreamItem>();
    let backend = empty_backend_with_events(event_rx);

    let handle = Runtime::spawn(
        Arc::clone(&backend),
        RuntimeConfig {
            initial_session: Some(SessionId::new("ses-outgoing")),
            disconnect_warning_threshold: 100,
            workspace_directory: None,
            lua_plugin_paths: Vec::new(),
            disable_plugins: false,
        },
    );

    let _ = drain_for(&handle, 30, Duration::from_millis(150)).await;
    backend.abort_calls.lock().unwrap().clear();

    handle
        .ui_events
        .send(raider_tui::Event::SessionSwitched(
            "ses-incoming".to_string(),
        ))
        .unwrap();

    let _ = drain_for(&handle, 30, Duration::from_millis(300)).await;

    let aborts = backend.abort_calls.lock().unwrap().clone();
    assert!(
        aborts.is_empty(),
        "SessionSwitched must NOT abort the outgoing session. opencode route.navigate('session') \
         is a view change; explicit interrupt/new-session flows are responsible for aborting. \
         got aborts={aborts:#?}",
    );

    let mut handle = handle;
    handle.shutdown();
}
