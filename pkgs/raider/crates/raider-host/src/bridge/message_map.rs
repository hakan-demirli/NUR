use raider_opencode::types::message::{MessagePart, MessageRole, MessageWithParts};
use raider_tui::{Action, HostAction, HostMessage, HostMessagePart, Sender, ToolCall};

use super::extra::{
    extract_agent, extract_assistant_error, extract_model_display, extract_provider,
};
use super::tool::tool_part_to_call;

pub fn message_to_host(m: &MessageWithParts) -> HostMessage {
    let sender = match m.info.role {
        MessageRole::User => Sender::User,
        MessageRole::Assistant => Sender::Assistant,
        MessageRole::System => Sender::System,
    };
    let mut content = String::new();
    let mut thoughts = String::new();
    let mut tool_calls: Vec<ToolCall> = Vec::new();
    let mut parts: Vec<HostMessagePart> = Vec::new();
    let mut compaction: Option<raider_tui::model::CompactionMarker> = None;
    for p in &m.parts {
        match p {
            MessagePart::Text(t) => {
                content.push_str(&t.text);
                parts.push(HostMessagePart::Text(t.text.clone()));
            }
            MessagePart::Reasoning(r) => {
                thoughts.push_str(&r.text);
                parts.push(HostMessagePart::Thought(r.text.clone()));
            }
            MessagePart::Tool(t) => {
                let call = tool_part_to_call(t);
                tool_calls.push(call.clone());
                parts.push(HostMessagePart::Tool(Box::new(call)));
            }
            MessagePart::Compaction(c) => {
                compaction = Some(raider_tui::model::CompactionMarker { auto: c.auto });
            }
            MessagePart::StepStart(_) | MessagePart::StepFinish(_) | MessagePart::Other => {}
        }
    }
    let is_streaming =
        matches!(m.info.role, MessageRole::Assistant) && m.info.time.completed.is_none();

    let agent = extract_agent(&m.info.extra);
    let model = extract_model_display(&m.info.extra);
    let provider_id = extract_provider(&m.info.extra);
    let error = extract_assistant_error(&m.info.extra);
    let duration = match (m.info.time.created, m.info.time.completed) {
        (Some(start), Some(end)) if end >= start => {
            Some(std::time::Duration::from_millis((end - start) as u64))
        }
        _ => None,
    };

    HostMessage {
        sender,
        content,
        thoughts,
        server_id: Some(m.info.id.as_str().to_string()),
        timestamp: String::new(),
        is_streaming,
        agent,
        model,
        provider_id,
        duration,
        error,
        tool_calls,
        parts,
        compaction,
    }
}

pub fn messages_refresh_actions(messages: &[MessageWithParts]) -> Vec<Action> {
    let host_msgs: Vec<HostMessage> = messages.iter().map(message_to_host).collect();
    vec![Action::Host(HostAction::ReplaceMessages(host_msgs))]
}
