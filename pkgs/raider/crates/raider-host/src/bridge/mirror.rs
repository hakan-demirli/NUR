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
    message_to_session: HashMap<MessageId, SessionId>,
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
        self.message_to_session.clear();
    }

    pub fn associate_message_with_session(&mut self, message_id: MessageId, session_id: SessionId) {
        self.message_to_session.insert(message_id, session_id);
    }

    pub fn mark_message_complete(&mut self, message_id: &MessageId) {
        self.text.retain(|(mid, _), _| mid != message_id);
        self.reasoning.retain(|(mid, _), _| mid != message_id);
    }

    pub fn forget_session(&mut self, session_id: &SessionId) {
        let owned_messages: Vec<MessageId> = self
            .message_to_session
            .iter()
            .filter(|(_, sid)| *sid == session_id)
            .map(|(mid, _)| mid.clone())
            .collect();
        for mid in &owned_messages {
            self.text.retain(|(m, _), _| m != mid);
            self.reasoning.retain(|(m, _), _| m != mid);
            self.kinds.retain(|(m, _), _| m != mid);
            self.roles.remove(mid);
            self.message_to_session.remove(mid);
        }
        self.task_child_session_to_part.remove(session_id);
        self.child_session_to_parent.remove(session_id);
        self.child_session_current_tool.remove(session_id);
        let stale_parents: Vec<SessionId> = self
            .child_session_to_parent
            .iter()
            .filter(|(_, parent)| *parent == session_id)
            .map(|(child, _)| child.clone())
            .collect();
        for child in stale_parents {
            self.child_session_to_parent.remove(&child);
        }
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

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use raider_opencode::types::message::MessageRole;

    fn mid(s: &str) -> MessageId {
        MessageId::from(s.to_string())
    }

    fn pid(s: &str) -> PartId {
        PartId::from(s.to_string())
    }

    fn sid(s: &str) -> SessionId {
        SessionId::from(s.to_string())
    }

    #[test]
    fn mark_message_complete_drops_text_and_reasoning_for_that_message() {
        let mut m = PartMirror::new();
        let _ = m.diff_text(mid("m1"), pid("p1"), "hello");
        let _ = m.diff_reasoning(mid("m1"), pid("p2"), "thinking");
        let _ = m.diff_text(mid("m2"), pid("p3"), "other");

        m.mark_message_complete(&mid("m1"));

        assert!(!m.text.contains_key(&(mid("m1"), pid("p1"))));
        assert!(!m.reasoning.contains_key(&(mid("m1"), pid("p2"))));
        assert_eq!(
            m.text.get(&(mid("m2"), pid("p3"))).map(String::as_str),
            Some("other"),
            "unrelated message must remain",
        );
    }

    #[test]
    fn forget_session_drops_all_state_for_that_session() {
        let mut m = PartMirror::new();
        m.associate_message_with_session(mid("m1"), sid("s1"));
        m.associate_message_with_session(mid("m2"), sid("s2"));
        let _ = m.diff_text(mid("m1"), pid("p1"), "in-s1");
        let _ = m.diff_text(mid("m2"), pid("p2"), "in-s2");
        m.remember_role(mid("m1"), MessageRole::User);
        m.remember_role(mid("m2"), MessageRole::Assistant);

        m.forget_session(&sid("s1"));

        assert!(!m.text.contains_key(&(mid("m1"), pid("p1"))));
        assert!(!m.is_user_message(&mid("m1")));
        assert_eq!(
            m.text.get(&(mid("m2"), pid("p2"))).map(String::as_str),
            Some("in-s2"),
            "other session is untouched",
        );
    }

    #[test]
    fn forget_session_drops_child_session_links_for_that_parent() {
        let mut m = PartMirror::new();
        m.remember_child_parent(sid("child"), sid("parent"));
        m.forget_session(&sid("parent"));
        assert!(m.parent_session_for_child(&sid("child")).is_none());
    }

    #[test]
    fn associate_then_forget_lets_subsequent_associations_for_same_message_work() {
        let mut m = PartMirror::new();
        m.associate_message_with_session(mid("m1"), sid("s1"));
        m.forget_session(&sid("s1"));
        m.associate_message_with_session(mid("m1"), sid("s2"));
        assert!(m.text.is_empty());
    }
}
