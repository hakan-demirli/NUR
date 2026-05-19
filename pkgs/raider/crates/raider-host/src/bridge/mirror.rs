use std::collections::HashMap;

use raider_opencode::types::{
    common::{MessageId, PartId, SessionId},
    message::MessagePart,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PartKind {
    Text,
    Reasoning,
    Tool,
    Other,
}

impl PartKind {
    pub(crate) fn from_part(part: &MessagePart) -> Self {
        match part {
            MessagePart::Text(_) => Self::Text,
            MessagePart::Reasoning(_) => Self::Reasoning,
            MessagePart::Tool(_) => Self::Tool,
            _ => Self::Other,
        }
    }
}

#[derive(Default, Debug)]
pub struct PartMirror {
    pub(crate) text: HashMap<(MessageId, PartId), String>,
    reasoning: HashMap<(MessageId, PartId), String>,
    roles: HashMap<MessageId, raider_opencode::types::message::MessageRole>,
    kinds: HashMap<(MessageId, PartId), PartKind>,
    task_child_session_to_part: HashMap<SessionId, PartId>,
    child_session_to_parent: HashMap<SessionId, SessionId>,
    child_session_current_tool: HashMap<SessionId, PartId>,
    task_child_tool_ids: HashMap<PartId, std::collections::HashSet<PartId>>,
}

impl PartMirror {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        self.text.clear();
        self.reasoning.clear();
        self.roles.clear();
        self.kinds.clear();
        self.task_child_session_to_part.clear();
        self.child_session_to_parent.clear();
        self.child_session_current_tool.clear();
        self.task_child_tool_ids.clear();
    }

    pub(crate) fn record_child_tool(&mut self, parent_part: PartId, child_part: PartId) -> u32 {
        let set = self.task_child_tool_ids.entry(parent_part).or_default();
        set.insert(child_part);
        set.len() as u32
    }

    pub(crate) fn remember_task_child_session(&mut self, child: SessionId, parent_part: PartId) {
        self.task_child_session_to_part.insert(child, parent_part);
    }

    pub(crate) fn remember_child_parent(&mut self, child: SessionId, parent: SessionId) {
        self.child_session_to_parent.insert(child, parent);
    }

    pub(crate) fn parent_part_for_child(&self, child: &SessionId) -> Option<PartId> {
        self.task_child_session_to_part.get(child).cloned()
    }

    #[allow(dead_code)]
    pub(crate) fn parent_session_for_child(&self, child: &SessionId) -> Option<&SessionId> {
        self.child_session_to_parent.get(child)
    }

    pub(crate) fn note_child_session_current_tool(&mut self, child: SessionId, part: PartId) {
        self.child_session_current_tool.insert(child, part);
    }

    #[allow(dead_code)]
    pub(crate) fn child_session_current_tool(&self, child: &SessionId) -> Option<&PartId> {
        self.child_session_current_tool.get(child)
    }

    pub(crate) fn remember_kind(&mut self, message_id: MessageId, part_id: PartId, kind: PartKind) {
        self.kinds.insert((message_id, part_id), kind);
    }

    pub(crate) fn kind_of(&self, message_id: &MessageId, part_id: &PartId) -> Option<PartKind> {
        self.kinds
            .get(&(message_id.clone(), part_id.clone()))
            .copied()
    }

    pub fn remember_role(
        &mut self,
        message_id: MessageId,
        role: raider_opencode::types::message::MessageRole,
    ) {
        self.roles.insert(message_id, role);
    }

    pub fn is_user_message(&self, message_id: &MessageId) -> bool {
        matches!(
            self.roles.get(message_id),
            Some(raider_opencode::types::message::MessageRole::User),
        )
    }

    pub(crate) fn note_streamed_part(
        &mut self,
        message_id: MessageId,
        part_id: PartId,
        kind: PartKind,
        delta: &str,
    ) {
        let map = match kind {
            PartKind::Reasoning => &mut self.reasoning,
            PartKind::Text => &mut self.text,
            _ => return,
        };
        let entry = map.entry((message_id, part_id)).or_default();
        entry.push_str(delta);
    }

    pub fn diff_text(
        &mut self,
        message_id: MessageId,
        part_id: PartId,
        full_text: &str,
    ) -> Option<String> {
        diff_into(&mut self.text, (message_id, part_id), full_text)
    }

    pub fn diff_reasoning(
        &mut self,
        message_id: MessageId,
        part_id: PartId,
        full_text: &str,
    ) -> Option<String> {
        diff_into(&mut self.reasoning, (message_id, part_id), full_text)
    }
}

pub(crate) fn diff_into(
    map: &mut HashMap<(MessageId, PartId), String>,
    key: (MessageId, PartId),
    full: &str,
) -> Option<String> {
    let prev = map.entry(key).or_default();
    if full.len() < prev.len() || !full.starts_with(prev.as_str()) {
        let delta = full.to_string();
        *prev = full.to_string();
        return if delta.is_empty() { None } else { Some(delta) };
    }
    let delta = full[prev.len()..].to_string();
    if delta.is_empty() {
        return None;
    }
    *prev = full.to_string();
    Some(delta)
}
