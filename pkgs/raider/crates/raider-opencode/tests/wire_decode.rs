use raider_opencode::events::{parse_frame, ServerEvent};
use raider_opencode::types::message::{MessagePart, MessageRole, MessageWithParts};
use raider_opencode::types::provider::ProviderList;

const ASSISTANT_FIXTURE: &str = include_str!("fixtures/session_messages_assistant.json");
const PROVIDER_LIST_FIXTURE: &str = include_str!("fixtures/provider_list_with_free.json");
const CONVERSATION_FIXTURE: &str = include_str!("fixtures/conversation_stream.jsonl");

#[test]
fn decodes_get_session_messages_response() {
    let messages: Vec<MessageWithParts> =
        serde_json::from_str(ASSISTANT_FIXTURE).expect("decode fixture");
    assert_eq!(messages.len(), 2, "fixture has two messages");

    let user = &messages[0];
    assert_eq!(user.info.id.as_str(), "msg_user_1");
    assert_eq!(user.info.role, MessageRole::User);
    assert_eq!(user.parts.len(), 1);
    assert_eq!(user.parts[0].text(), Some("yo"));

    let assistant = &messages[1];
    assert_eq!(assistant.info.id.as_str(), "msg_assistant_1");
    assert_eq!(assistant.info.role, MessageRole::Assistant);
    assert_eq!(
        assistant.info.session_id.as_ref().unwrap().as_str(),
        "ses_abc"
    );
    assert_eq!(assistant.info.time.created, Some(1700000000010));
    assert_eq!(assistant.info.time.completed, Some(1700000001000));

    assert_eq!(assistant.parts.len(), 5);
    assert!(matches!(assistant.parts[0], MessagePart::StepStart(_)));
    assert_eq!(assistant.parts[1].text(), Some("Hello!"));
    assert!(matches!(assistant.parts[2], MessagePart::Tool(_)));
    assert!(matches!(assistant.parts[3], MessagePart::Other));
    assert!(matches!(assistant.parts[4], MessagePart::StepFinish(_)));
}

#[test]
fn decodes_provider_list_with_free_models() {
    let list: ProviderList =
        serde_json::from_str(PROVIDER_LIST_FIXTURE).expect("decode provider list");

    assert_eq!(list.all.len(), 2, "fixture has 2 providers");
    assert_eq!(list.connected.len(), 2);
    assert!(list.connected.contains(&"opencode".to_string()));
    assert_eq!(
        list.default.get("opencode").map(String::as_str),
        Some("claude-opus-47")
    );
    assert_eq!(
        list.default.get("anthropic").map(String::as_str),
        Some("claude-sonnet-46")
    );

    let opencode = list
        .all
        .iter()
        .find(|p| p.id == "opencode")
        .expect("opencode");
    assert_eq!(opencode.name, "OpenCode Zen");

    let mut free_names: Vec<&str> = opencode
        .models
        .values()
        .filter(|m| m.is_zero_input_cost() && m.status.as_deref() != Some("deprecated"))
        .map(|m| m.name.as_str())
        .collect();
    free_names.sort();
    assert_eq!(
        free_names,
        vec![
            "Big Pickle",
            "DeepSeek V4 Flash Free",
            "MiniMax M2.5 Free",
            "Nemotron 3 Super Free",
            "Qwen3.6 Plus Free",
        ],
        "expected the five free-tier opencode-zen models",
    );

    let opus = opencode.models.get("claude-opus-47").expect("opus present");
    assert!(!opus.is_zero_input_cost());
    assert_eq!(opus.name, "Claude Opus 4.7");
    assert!(opus.variants.contains_key("thinking"));

    let haiku_old = opencode
        .models
        .get("claude-haiku-deprecated")
        .expect("deprecated row");
    assert_eq!(haiku_old.status.as_deref(), Some("deprecated"));
}

#[test]
fn unknown_part_type_does_not_fail_decoding() {
    let raw = r#"{
        "info": {
            "id": "msg1",
            "sessionID": "ses1",
            "role": "assistant",
            "time": { "created": 0 },
            "parentID": "msg0",
            "modelID": "m", "providerID": "p", "mode": "build",
            "agent": "build",
            "path": { "cwd": "/", "root": "/" },
            "cost": 0,
            "tokens": {
                "input": 0, "output": 0, "reasoning": 0,
                "cache": { "read": 0, "write": 0 }
            }
        },
        "parts": [
            {
                "id": "prt_x",
                "sessionID": "ses1",
                "messageID": "msg1",
                "type": "snapshot",
                "snapshot": "abc"
            }
        ]
    }"#;
    let m: MessageWithParts = serde_json::from_str(raw).expect("decode");
    assert_eq!(m.parts.len(), 1);
    assert!(matches!(m.parts[0], MessagePart::Other));
}

#[test]
fn conversation_stream_decodes_every_frame() {
    let mut frames = 0usize;
    for (i, line) in CONVERSATION_FIXTURE.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let _event: ServerEvent = parse_frame(line)
            .unwrap_or_else(|e| panic!("line {i} decode failed: {e}\nline={line}"));
        frames += 1;
    }
    assert_eq!(frames, 11, "fixture has 11 SSE frames");
}

#[test]
fn tool_state_error_accepts_bare_string_wire_shape() {
    // BUG1: opencode's current schema (`message-v2.ts::ToolStateError`,
    let raw = r#"[{
        "info": {
            "id": "msg_x",
            "sessionID": "ses_y",
            "role": "assistant",
            "time": {"created": 1, "completed": 2}
        },
        "parts": [{
            "id": "prt_x",
            "type": "tool",
            "tool": "bash",
            "messageID": "msg_x",
            "state": {
                "status": "error",
                "input": {"command": "exit 1"},
                "output": "",
                "title": "",
                "metadata": {},
                "error": "Tool execution aborted"
            }
        }]
    }]"#;
    let msgs: Vec<MessageWithParts> = serde_json::from_str(raw)
        .expect("string-typed tool error must decode (opencode wire shape)");
    let tool = match &msgs[0].parts[0] {
        MessagePart::Tool(t) => t,
        other => panic!("expected Tool part, got {other:?}"),
    };
    assert_eq!(
        tool.state.error.as_deref(),
        Some("Tool execution aborted"),
        "string-typed error must surface verbatim",
    );
}

#[test]
fn tool_state_error_accepts_legacy_struct_wire_shape() {
    let raw = r#"[{
        "info": {
            "id": "msg_x",
            "sessionID": "ses_y",
            "role": "assistant",
            "time": {"created": 1, "completed": 2}
        },
        "parts": [{
            "id": "prt_x",
            "type": "tool",
            "tool": "edit",
            "messageID": "msg_x",
            "state": {
                "status": "error",
                "input": {},
                "output": "",
                "title": "",
                "metadata": {},
                "error": {
                    "message": "file not found",
                    "name": "NotFoundError"
                }
            }
        }]
    }]"#;
    let msgs: Vec<MessageWithParts> =
        serde_json::from_str(raw).expect("legacy struct-typed tool error must still decode");
    let tool = match &msgs[0].parts[0] {
        MessagePart::Tool(t) => t,
        other => panic!("expected Tool part, got {other:?}"),
    };
    assert_eq!(
        tool.state.error.as_deref(),
        Some("file not found"),
        "legacy struct's `message` field must be extracted; `name` is discarded",
    );
}

#[test]
fn tool_state_error_absent_when_field_missing_or_null() {
    let raw_missing = r#"{
        "id": "prt_x",
        "type": "tool",
        "tool": "read",
        "messageID": "msg_x",
        "state": {
            "status": "completed",
            "input": {},
            "output": "ok",
            "title": "",
            "metadata": {}
        }
    }"#;
    let part: MessagePart = serde_json::from_str(raw_missing).expect("decode");
    let tool = match part {
        MessagePart::Tool(t) => t,
        other => panic!("expected Tool, got {other:?}"),
    };
    assert!(
        tool.state.error.is_none(),
        "missing `error` must decode to None",
    );

    let raw_null = r#"{
        "id": "prt_y",
        "type": "tool",
        "tool": "read",
        "messageID": "msg_x",
        "state": {
            "status": "completed",
            "input": {},
            "output": "ok",
            "title": "",
            "metadata": {},
            "error": null
        }
    }"#;
    let part: MessagePart = serde_json::from_str(raw_null).expect("decode");
    let tool = match part {
        MessagePart::Tool(t) => t,
        other => panic!("expected Tool, got {other:?}"),
    };
    assert!(
        tool.state.error.is_none(),
        "explicit `error: null` must decode to None",
    );
}

#[test]
fn full_messages_response_with_string_tool_error_round_trips() {
    let raw = r#"[
        {
            "info": {"id": "msg_a", "sessionID": "ses_z", "role": "user", "time": {}},
            "parts": [{"id": "prt_a1", "type": "text", "text": "ok", "messageID": "msg_a"}]
        },
        {
            "info": {"id": "msg_b", "sessionID": "ses_z", "role": "assistant", "time": {"created": 1, "completed": 2}},
            "parts": [
                {"id": "prt_b1", "type": "text", "text": "running", "messageID": "msg_b"},
                {
                    "id": "prt_b2",
                    "type": "tool",
                    "tool": "bash",
                    "messageID": "msg_b",
                    "state": {
                        "status": "error",
                        "input": {"command": "false"},
                        "output": "",
                        "title": "Run failing command",
                        "metadata": {},
                        "error": "Tool execution aborted"
                    }
                }
            ]
        },
        {
            "info": {"id": "msg_c", "sessionID": "ses_z", "role": "assistant", "time": {"created": 3, "completed": 4}},
            "parts": [{"id": "prt_c1", "type": "text", "text": "recovered", "messageID": "msg_c"}]
        }
    ]"#;
    let msgs: Vec<MessageWithParts> =
        serde_json::from_str(raw).expect("array containing string-typed tool error must decode");
    assert_eq!(msgs.len(), 3, "every message must survive the decode");
    if let MessagePart::Tool(t) = &msgs[1].parts[1] {
        assert_eq!(t.state.error.as_deref(), Some("Tool execution aborted"));
    } else {
        panic!("expected tool part at msgs[1].parts[1]");
    }
}
