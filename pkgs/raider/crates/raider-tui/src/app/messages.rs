use std::collections::{HashMap, HashSet};

use crate::action::{self, HostMessagePart};
use crate::model::{CompactionMarker, Message, Sender};
use crate::state::Version;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct ToolLocation {
    pub msg_idx: usize,
    pub tool_idx: usize,
}

pub struct MessageStore {
    pub messages: Vec<Message>,

    pub compaction_message_ids: HashSet<String>,

    pub tool_block_rects: Vec<(String, ratatui::layout::Rect)>,
    pub user_message_rects: Vec<(String, ratatui::layout::Rect)>,

    pub show_timestamps: bool,
    pub thinking_hidden: bool,

    version: Version,
    by_server_id: HashMap<String, usize>,
    tools: HashMap<String, ToolLocation>,
    last_assistant_idx: Option<usize>,
    streaming_assistant_idx: Option<usize>,
    queued_flags_cache: Option<(Version, Vec<bool>)>,
}

impl MessageStore {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            compaction_message_ids: HashSet::new(),
            tool_block_rects: Vec::new(),
            user_message_rects: Vec::new(),
            show_timestamps: false,
            thinking_hidden: true,
            version: Version::default(),
            by_server_id: HashMap::new(),
            tools: HashMap::new(),
            last_assistant_idx: None,
            streaming_assistant_idx: None,
            queued_flags_cache: None,
        }
    }

    pub fn version(&self) -> Version {
        self.version
    }

    fn bump_store(&mut self) {
        self.version.bump();
        self.queued_flags_cache = None;
    }

    fn bump_message(&mut self, idx: usize) {
        if let Some(msg) = self.messages.get_mut(idx) {
            msg.invalidate_render_cache();
        }
        self.bump_store();
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.compaction_message_ids.clear();
        self.by_server_id.clear();
        self.tools.clear();
        self.last_assistant_idx = None;
        self.streaming_assistant_idx = None;
        self.bump_store();
    }

    pub fn push(&mut self, msg: Message) {
        let idx = self.messages.len();
        self.index_inserted_message(idx, &msg);
        self.messages.push(msg);
        self.bump_store();
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Message> {
        self.messages.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Message> {
        self.bump_store();
        self.messages.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn last_assistant_index(&self) -> Option<usize> {
        self.last_assistant_idx
    }

    pub fn streaming_assistant_index(&self) -> Option<usize> {
        self.streaming_assistant_idx
    }

    pub fn tool_location(&self, id: &str) -> Option<ToolLocation> {
        self.tools.get(id).copied()
    }

    pub fn message_by_server_id(&self, id: &str) -> Option<&Message> {
        self.by_server_id
            .get(id)
            .and_then(|&i| self.messages.get(i))
    }

    pub fn queued_flags(&mut self) -> &[bool] {
        if self
            .queued_flags_cache
            .as_ref()
            .map(|(v, _)| *v != self.version)
            .unwrap_or(true)
        {
            let mut flags = Vec::with_capacity(self.messages.len());
            let mut has_streaming_before = false;
            for m in &self.messages {
                let queued = matches!(m.sender, Sender::User) && has_streaming_before;
                flags.push(queued);
                if m.sender == Sender::Assistant && m.is_streaming {
                    has_streaming_before = true;
                }
            }
            self.queued_flags_cache = Some((self.version, flags));
        }
        &self.queued_flags_cache.as_ref().unwrap().1
    }

    pub fn toggle_timestamps(&mut self) {
        self.show_timestamps = !self.show_timestamps;
        for msg in &mut self.messages {
            msg.invalidate_render_cache();
        }
        self.bump_store();
    }

    pub fn toggle_thinking(&mut self) {
        self.thinking_hidden = !self.thinking_hidden;
        let hide = self.thinking_hidden;
        for msg in &mut self.messages {
            if msg.sender == Sender::Assistant {
                msg.thoughts_collapsed = hide;
                msg.invalidate_render_cache();
            }
        }
        self.bump_store();
    }

    pub fn set_thinking_hidden_from_persisted(&mut self, hidden: bool) {
        self.thinking_hidden = hidden;
    }

    pub fn host_to_tui_message(
        &self,
        m: action::HostMessage,
        now_hhmm: impl FnOnce() -> String,
    ) -> Message {
        let ts = if m.timestamp.is_empty() {
            now_hhmm()
        } else {
            m.timestamp
        };
        Message {
            sender: m.sender,
            content: m.content,
            thoughts: m.thoughts,
            server_id: m.server_id,
            timestamp: ts,
            is_streaming: m.is_streaming,
            thoughts_collapsed: self.thinking_hidden && matches!(m.sender, Sender::Assistant),
            interrupted: m.interrupted,
            agent: m.agent,
            model: m.model,
            provider_id: m.provider_id,
            duration: m.duration,
            error: m.error,
            tool_calls: m.tool_calls,
            parts: m.parts,
            compaction: m.compaction,
            ..Message::default()
        }
    }

    pub fn host_replace(
        &mut self,
        messages: Vec<action::HostMessage>,
        now_hhmm: impl Fn() -> String,
    ) {
        self.messages = messages
            .into_iter()
            .map(|m| self.host_to_tui_message(m, &now_hhmm))
            .collect();
        self.compaction_message_ids.clear();
        self.rebuild_indices();
        self.bump_store();
    }

    pub fn host_append(&mut self, message: action::HostMessage, now_hhmm: impl Fn() -> String) {
        let m = self.host_to_tui_message(message, &now_hhmm);
        let idx = self.messages.len();
        self.index_inserted_message(idx, &m);
        self.messages.push(m);
        self.bump_store();
    }

    pub fn bind_first_untagged_user(&mut self, server_id: String, agent: Option<String>) -> bool {
        let idx = self
            .messages
            .iter()
            .position(|m| m.sender == Sender::User && m.server_id.is_none());
        let Some(idx) = idx else {
            return false;
        };
        let msg = &mut self.messages[idx];
        msg.server_id = Some(server_id.clone());
        if msg.agent.is_none() {
            msg.agent = agent;
        }
        msg.invalidate_render_cache();
        self.by_server_id.insert(server_id, idx);
        self.bump_store();
        true
    }

    pub fn mark_compaction(
        &mut self,
        message_id: String,
        marker: CompactionMarker,
        now_hhmm: impl Fn() -> String,
    ) -> bool {
        if !self.compaction_message_ids.insert(message_id.clone()) {
            return false;
        }
        let mut host_msg = action::HostMessage::user(String::new());
        host_msg.server_id = Some(message_id);
        host_msg.compaction = Some(marker);
        let m = self.host_to_tui_message(host_msg, &now_hhmm);
        let idx = self.messages.len();
        self.index_inserted_message(idx, &m);
        self.messages.push(m);
        self.bump_store();
        true
    }

    pub fn append_assistant_delta(
        &mut self,
        text: &str,
        thoughts: bool,
        server_message_id: Option<&str>,
        make_streaming_message: impl FnOnce() -> Message,
    ) {
        let target_idx = self.resolve_streaming_target(server_message_id, make_streaming_message);

        if let Some(msg) = self.messages.get_mut(target_idx) {
            if thoughts {
                msg.thoughts.push_str(text);
                match msg.parts.last_mut() {
                    Some(HostMessagePart::Thought(existing)) => existing.push_str(text),
                    _ => msg.parts.push(HostMessagePart::Thought(text.to_string())),
                }
            } else {
                msg.content.push_str(text);
                match msg.parts.last_mut() {
                    Some(HostMessagePart::Text(existing)) => existing.push_str(text),
                    _ => msg.parts.push(HostMessagePart::Text(text.to_string())),
                }
            }
            msg.invalidate_render_cache();
        }
        self.bump_store();
    }

    fn resolve_streaming_target(
        &mut self,
        server_message_id: Option<&str>,
        make_streaming_message: impl FnOnce() -> Message,
    ) -> usize {
        if let Some(mid) = server_message_id {
            if let Some(&idx) = self.by_server_id.get(mid) {
                return idx;
            }
            if let Some(idx) = self.streaming_assistant_idx {
                if self.messages[idx].server_id.is_none() {
                    self.messages[idx].server_id = Some(mid.to_string());
                    self.by_server_id.insert(mid.to_string(), idx);
                    return idx;
                }
            }
        }
        if let Some(idx) = self.streaming_assistant_idx {
            return idx;
        }
        let msg = make_streaming_message();
        let idx = self.messages.len();
        self.index_inserted_message(idx, &msg);
        self.messages.push(msg);
        idx
    }

    pub fn finish_streaming_assistant(&mut self, message_id: Option<&str>) {
        match message_id {
            Some(mid) => {
                let target = self.by_server_id.get(mid).copied().filter(|&idx| {
                    let m = &self.messages[idx];
                    m.sender == Sender::Assistant && m.is_streaming
                });
                if let Some(idx) = target {
                    self.messages[idx].is_streaming = false;
                    self.messages[idx].invalidate_render_cache();
                    self.recompute_first_streaming_assistant();
                    self.bump_store();
                }
            }
            None => {
                let target = self.streaming_assistant_idx;
                if let Some(idx) = target {
                    self.messages[idx].is_streaming = false;
                    self.messages[idx].invalidate_render_cache();
                    self.recompute_first_streaming_assistant();
                    self.bump_store();
                }
            }
        }
    }

    pub fn remove_by_server_id(&mut self, id: &str) {
        let before = self.messages.len();
        self.messages.retain(|m| m.server_id.as_deref() != Some(id));
        if self.messages.len() != before {
            self.rebuild_indices();
            self.bump_store();
        }
    }

    pub fn remove_tool_call_by_id(&mut self, id: &str) {
        let Some(loc) = self.tools.get(id).copied() else {
            return;
        };
        let msg = &mut self.messages[loc.msg_idx];
        msg.tool_calls.retain(|t| t.id.as_deref() != Some(id));
        msg.parts.retain(|p| match p {
            HostMessagePart::Tool(t) => t.id.as_deref() != Some(id),
            _ => true,
        });
        msg.tool_render_cache.remove(id);
        msg.invalidate_render_cache();
        self.rebuild_tools_for_message(loc.msg_idx);
        self.bump_store();
    }

    pub fn upsert_tool_call(&mut self, tool: action::ToolCall) -> bool {
        let key = tool_call_match_key(&tool);
        let target = self
            .streaming_assistant_idx
            .or(self.last_assistant_idx)
            .filter(|&i| self.messages[i].sender == Sender::Assistant);
        let Some(idx) = target else {
            return false;
        };
        let msg = &mut self.messages[idx];
        let existing_pos = msg
            .tool_calls
            .iter()
            .position(|t| tool_call_match_key(t) == key);
        match existing_pos {
            Some(slot_idx) => {
                let slot = &mut msg.tool_calls[slot_idx];
                let preserved_expanded = slot.expanded;
                let preserved_child = slot.current_child.take();
                let preserved_count = slot.child_tool_count;
                let preserved_started = slot.started_at_ms;
                let preserved_completed = slot.completed_at_ms;
                let prev_status = slot.status;
                *slot = tool;
                slot.expanded = preserved_expanded;
                if slot.current_child.is_none() {
                    slot.current_child = preserved_child;
                }
                slot.child_tool_count = slot.child_tool_count.max(preserved_count);
                if slot.started_at_ms.is_none() {
                    slot.started_at_ms = preserved_started;
                }
                if slot.completed_at_ms.is_none() {
                    slot.completed_at_ms = preserved_completed;
                }
                if slot.name == "task" {
                    stamp_task_timing(slot, prev_status);
                }
                let updated = slot.clone();
                if let Some(HostMessagePart::Tool(part_tool)) = msg.parts.iter_mut().find(
                    |p| matches!(p, HostMessagePart::Tool(t) if tool_call_match_key(t) == key),
                ) {
                    *part_tool = Box::new(updated);
                }
            }
            None => {
                let mut t = tool;
                if t.name == "task" {
                    stamp_task_timing(&mut t, action::ToolStatus::Pending);
                }
                let tool_idx = msg.tool_calls.len();
                if let Some(id) = t.id.clone() {
                    if !id.is_empty() {
                        self.tools.insert(
                            id,
                            ToolLocation {
                                msg_idx: idx,
                                tool_idx,
                            },
                        );
                    }
                }
                msg.tool_calls.push(t);
                if let Some(inserted) = msg.tool_calls.last().cloned() {
                    msg.parts.push(HostMessagePart::Tool(Box::new(inserted)));
                }
            }
        }
        self.bump_message(idx);
        true
    }

    pub fn update_task_child(
        &mut self,
        parent_tool_id: &str,
        child: Option<action::ChildToolRef>,
        child_tool_count: u32,
    ) {
        if parent_tool_id.is_empty() {
            return;
        }
        let Some(loc) = self.tools.get(parent_tool_id).copied() else {
            return;
        };
        let msg = &mut self.messages[loc.msg_idx];
        let tool = &mut msg.tool_calls[loc.tool_idx];
        let unchanged = tool.current_child == child && tool.child_tool_count == child_tool_count;
        if unchanged {
            return;
        }
        tool.current_child = child;
        tool.child_tool_count = tool.child_tool_count.max(child_tool_count);
        msg.tool_render_cache.remove(parent_tool_id);
        self.bump_message(loc.msg_idx);
    }

    pub fn toggle_tool_expanded(&mut self, id: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        let Some(loc) = self.tools.get(id).copied() else {
            return false;
        };
        let msg = &mut self.messages[loc.msg_idx];
        let new_expanded = {
            let tool = &mut msg.tool_calls[loc.tool_idx];
            tool.expanded = !tool.expanded;
            tool.expanded
        };
        for tool in msg
            .tool_calls
            .iter_mut()
            .filter(|t| t.id.as_deref() == Some(id))
        {
            tool.expanded = new_expanded;
        }
        for part in &mut msg.parts {
            if let HostMessagePart::Tool(part_tool) = part {
                if part_tool.id.as_deref() == Some(id) {
                    part_tool.expanded = new_expanded;
                }
            }
        }
        self.bump_message(loc.msg_idx);
        true
    }

    pub fn set_last_assistant_error(&mut self, error: String) -> bool {
        if error.is_empty() {
            return false;
        }
        let target = self.streaming_assistant_idx.or(self.last_assistant_idx);
        let Some(idx) = target else {
            return false;
        };
        let msg = &mut self.messages[idx];
        if msg.error.as_deref() == Some(error.as_str()) {
            return false;
        }
        msg.error = Some(error);
        self.bump_message(idx);
        true
    }

    pub fn mark_assistant_interrupted(&mut self, server_id: &str) -> bool {
        let Some(&idx) = self.by_server_id.get(server_id) else {
            return false;
        };
        let msg = &mut self.messages[idx];
        if msg.sender != Sender::Assistant {
            return false;
        }
        msg.interrupted = true;
        msg.error = None;
        self.bump_message(idx);
        true
    }

    pub fn update_last_assistant_meta(
        &mut self,
        agent: Option<String>,
        model: Option<String>,
        provider_id: Option<String>,
        duration: Option<std::time::Duration>,
    ) {
        let target = self.streaming_assistant_idx.or(self.last_assistant_idx);
        if let Some(idx) = target {
            let msg = &mut self.messages[idx];
            if let Some(a) = agent {
                msg.agent = Some(a);
            }
            if let Some(m) = model {
                msg.model = Some(m);
            }
            if let Some(p) = provider_id {
                msg.provider_id = Some(p);
            }
            if let Some(d) = duration {
                msg.duration = Some(d);
            }
            self.bump_message(idx);
        }
    }

    fn index_inserted_message(&mut self, idx: usize, msg: &Message) {
        if let Some(sid) = msg.server_id.clone() {
            self.by_server_id.insert(sid, idx);
        }
        for (tool_idx, tool) in msg.tool_calls.iter().enumerate() {
            if let Some(id) = tool.id.clone() {
                if !id.is_empty() {
                    self.tools.insert(
                        id,
                        ToolLocation {
                            msg_idx: idx,
                            tool_idx,
                        },
                    );
                }
            }
        }
        if msg.sender == Sender::Assistant {
            self.last_assistant_idx = Some(idx);
            if msg.is_streaming && self.streaming_assistant_idx.is_none() {
                self.streaming_assistant_idx = Some(idx);
            }
        }
    }

    fn recompute_first_streaming_assistant(&mut self) {
        self.streaming_assistant_idx = self
            .messages
            .iter()
            .position(|m| m.sender == Sender::Assistant && m.is_streaming);
    }

    fn rebuild_tools_for_message(&mut self, msg_idx: usize) {
        self.tools.retain(|_, loc| loc.msg_idx != msg_idx);
        let Some(msg) = self.messages.get(msg_idx) else {
            return;
        };
        for (tool_idx, tool) in msg.tool_calls.iter().enumerate() {
            if let Some(id) = tool.id.clone() {
                if !id.is_empty() {
                    self.tools.insert(id, ToolLocation { msg_idx, tool_idx });
                }
            }
        }
    }

    fn rebuild_indices(&mut self) {
        self.by_server_id.clear();
        self.tools.clear();
        self.last_assistant_idx = None;
        self.streaming_assistant_idx = None;
        for (i, msg) in self.messages.iter().enumerate() {
            if let Some(sid) = msg.server_id.clone() {
                self.by_server_id.insert(sid, i);
            }
            for (tool_idx, tool) in msg.tool_calls.iter().enumerate() {
                if let Some(id) = tool.id.clone() {
                    if !id.is_empty() {
                        self.tools.insert(
                            id,
                            ToolLocation {
                                msg_idx: i,
                                tool_idx,
                            },
                        );
                    }
                }
            }
            if msg.sender == Sender::Assistant {
                self.last_assistant_idx = Some(i);
                if msg.is_streaming {
                    self.streaming_assistant_idx = Some(i);
                }
            }
        }
    }
}

impl Default for MessageStore {
    fn default() -> Self {
        Self::new()
    }
}

fn now_epoch_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

fn stamp_task_timing(tool: &mut action::ToolCall, prev_status: action::ToolStatus) {
    use action::ToolStatus;
    let running_now = matches!(tool.status, ToolStatus::Running | ToolStatus::Pending);
    let completed_now = matches!(tool.status, ToolStatus::Completed | ToolStatus::Error);
    let was_running = matches!(prev_status, ToolStatus::Running | ToolStatus::Pending);
    if running_now && tool.started_at_ms.is_none() {
        tool.started_at_ms = Some(now_epoch_ms());
    }
    if completed_now && tool.completed_at_ms.is_none() {
        if tool.started_at_ms.is_none() {
            tool.started_at_ms = Some(now_epoch_ms());
        }
        tool.completed_at_ms = Some(now_epoch_ms());
        if let (Some(s), Some(c)) = (tool.started_at_ms, tool.completed_at_ms) {
            if c < s {
                tool.completed_at_ms = Some(s);
            }
        }
    }
    let _ = was_running;
}

fn tool_call_match_key(tool: &action::ToolCall) -> (String, String) {
    if let Some(id) = &tool.id {
        if !id.is_empty() {
            return ("__id__".to_string(), id.clone());
        }
    }
    match tool.name.as_str() {
        "edit" | "write" => (
            tool.name.clone(),
            tool.file_path.clone().unwrap_or_default(),
        ),
        _ => (tool.name.clone(), tool.title.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::action::{ToolCall, ToolStatus};

    fn user(content: &str) -> Message {
        Message::user(content, "now")
    }

    fn assistant_streaming() -> Message {
        Message::assistant_streaming("now")
    }

    fn tool(id: &str, name: &str) -> ToolCall {
        ToolCall {
            id: Some(id.to_string()),
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn version_bumps_on_push() {
        let mut s = MessageStore::new();
        let v0 = s.version();
        s.push(user("hi"));
        assert!(s.version() > v0);
    }

    #[test]
    fn version_bumps_on_clear() {
        let mut s = MessageStore::new();
        s.push(user("hi"));
        let v = s.version();
        s.clear();
        assert!(s.version() > v);
    }

    #[test]
    fn last_assistant_index_tracks_pushes() {
        let mut s = MessageStore::new();
        s.push(user("a"));
        assert_eq!(s.last_assistant_index(), None);
        s.push(assistant_streaming());
        assert_eq!(s.last_assistant_index(), Some(1));
        s.push(user("b"));
        assert_eq!(s.last_assistant_index(), Some(1));
        s.push(assistant_streaming());
        assert_eq!(s.last_assistant_index(), Some(3));
    }

    #[test]
    fn streaming_assistant_index_clears_on_finish() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        assert_eq!(s.streaming_assistant_index(), Some(0));
        s.finish_streaming_assistant(None);
        assert_eq!(s.streaming_assistant_index(), None);
    }

    #[test]
    fn tool_location_lookup_is_o1_after_upsert() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        assert!(s.upsert_tool_call(tool("t1", "bash")));
        let loc = s.tool_location("t1").expect("tool indexed");
        assert_eq!(loc.msg_idx, 0);
        assert_eq!(loc.tool_idx, 0);
    }

    #[test]
    fn tool_index_drops_when_tool_is_removed() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        s.upsert_tool_call(tool("t1", "bash"));
        s.upsert_tool_call(tool("t2", "read"));
        s.remove_tool_call_by_id("t1");
        assert!(s.tool_location("t1").is_none());
        let t2 = s.tool_location("t2").expect("t2 still indexed");
        assert_eq!(t2.tool_idx, 0, "t2 must shift down when t1 was first");
    }

    #[test]
    fn update_task_child_is_o1_via_index() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        let mut task = tool("parent", "task");
        task.status = ToolStatus::Running;
        s.upsert_tool_call(task);
        s.update_task_child(
            "parent",
            Some(action::ChildToolRef {
                part_id: "prt-c".to_string(),
                name: "bash".to_string(),
                status: ToolStatus::Running,
                file_path: None,
                command: None,
                title: String::new(),
            }),
            1,
        );
        let loc = s.tool_location("parent").unwrap();
        let tool_ref = &s.messages[loc.msg_idx].tool_calls[loc.tool_idx];
        assert_eq!(tool_ref.child_tool_count, 1);
        assert!(tool_ref.current_child.is_some());
    }

    #[test]
    fn toggle_tool_expanded_uses_index() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        s.upsert_tool_call(tool("t1", "bash"));
        assert!(s.toggle_tool_expanded("t1"));
        let loc = s.tool_location("t1").unwrap();
        assert!(s.messages[loc.msg_idx].tool_calls[loc.tool_idx].expanded);
        assert!(s.toggle_tool_expanded("t1"));
        assert!(!s.messages[loc.msg_idx].tool_calls[loc.tool_idx].expanded);
    }

    #[test]
    fn message_by_server_id_uses_index() {
        let mut s = MessageStore::new();
        let mut a = assistant_streaming();
        a.server_id = Some("sid-1".to_string());
        s.push(a);
        assert!(s.message_by_server_id("sid-1").is_some());
        assert!(s.message_by_server_id("missing").is_none());
    }

    fn user_with_id(content: &str, sid: &str) -> Message {
        let mut m = user(content);
        m.server_id = Some(sid.to_string());
        m
    }

    #[test]
    fn bind_first_untagged_user_indexes_server_id() {
        let mut s = MessageStore::new();
        s.push(user("first"));
        s.push(user_with_id("already-tagged", "x"));
        assert!(s.bind_first_untagged_user("u-1".into(), None));
        let m = s.message_by_server_id("u-1").expect("u-1 indexed");
        assert_eq!(m.content, "first");
        assert!(
            !s.bind_first_untagged_user("u-2".into(), None),
            "no untagged user remains; second bind must return false"
        );
        assert!(s.message_by_server_id("u-2").is_none());
    }

    #[test]
    fn queued_flags_cache_invalidates_on_mutation() {
        let mut s = MessageStore::new();
        s.push(user("a"));
        s.push(assistant_streaming());
        s.push(user("b"));
        {
            let flags = s.queued_flags().to_vec();
            assert_eq!(flags, vec![false, false, true]);
        }
        s.finish_streaming_assistant(None);
        s.push(user("c"));
        let flags = s.queued_flags().to_vec();
        assert_eq!(flags, vec![false, false, false, false]);
    }

    #[test]
    fn rebuild_indices_after_remove_by_server_id() {
        let mut s = MessageStore::new();
        let mut a = user("a");
        a.server_id = Some("u-1".into());
        s.push(a);
        let mut b = assistant_streaming();
        b.server_id = Some("a-1".into());
        s.push(b);
        let mut c = user("c");
        c.server_id = Some("u-2".into());
        s.push(c);

        s.remove_by_server_id("u-1");
        assert_eq!(s.len(), 2);
        assert_eq!(
            s.message_by_server_id("a-1").map(|m| m.content.as_str()),
            Some("")
        );
        let u2 = s.message_by_server_id("u-2").unwrap();
        assert_eq!(u2.content, "c");
        assert_eq!(s.streaming_assistant_index(), Some(0));
        assert_eq!(s.last_assistant_index(), Some(0));
    }

    #[test]
    fn finish_streaming_assistant_by_id_uses_index() {
        let mut s = MessageStore::new();
        let mut a = assistant_streaming();
        a.server_id = Some("a-1".into());
        s.push(a);
        s.finish_streaming_assistant(Some("a-1"));
        assert_eq!(s.streaming_assistant_index(), None);
        assert!(!s.messages[0].is_streaming);
    }

    #[test]
    fn mark_assistant_interrupted_uses_index() {
        let mut s = MessageStore::new();
        let mut a = assistant_streaming();
        a.server_id = Some("a-1".into());
        s.push(a);
        assert!(s.mark_assistant_interrupted("a-1"));
        assert!(s.messages[0].interrupted);
        assert!(!s.mark_assistant_interrupted("missing"));
    }

    #[test]
    fn append_delta_uses_streaming_index_without_scan() {
        let mut s = MessageStore::new();
        s.append_assistant_delta("hello ", false, None, assistant_streaming);
        s.append_assistant_delta("world", false, None, assistant_streaming);
        assert_eq!(s.len(), 1);
        assert_eq!(s.messages[0].content, "hello world");
        assert_eq!(s.streaming_assistant_index(), Some(0));
    }
}
