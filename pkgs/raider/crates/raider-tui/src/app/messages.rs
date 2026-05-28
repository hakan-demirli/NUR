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

    next_token: u64,
    token_to_idx: HashMap<u64, usize>,
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
            next_token: 1,
            token_to_idx: HashMap::new(),
        }
    }

    fn mint_token(&mut self) -> u64 {
        let t = self.next_token;
        self.next_token = self.next_token.saturating_add(1);
        t
    }

    pub fn streaming_assistant_token(&self) -> Option<u64> {
        self.streaming_assistant_idx
            .and_then(|i| self.messages.get(i))
            .map(|m| m.token)
            .filter(|&t| t != 0)
    }

    fn ensure_token(&mut self, msg: &mut Message) {
        if msg.token == 0 {
            msg.token = self.mint_token();
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
        self.token_to_idx.clear();
        self.last_assistant_idx = None;
        self.streaming_assistant_idx = None;
        self.bump_store();
    }

    pub fn take_for_stash(&mut self) -> (Vec<Message>, HashSet<String>) {
        let messages = std::mem::take(&mut self.messages);
        let compaction_ids = std::mem::take(&mut self.compaction_message_ids);
        self.by_server_id.clear();
        self.tools.clear();
        self.token_to_idx.clear();
        self.last_assistant_idx = None;
        self.streaming_assistant_idx = None;
        self.bump_store();
        (messages, compaction_ids)
    }

    pub fn install(&mut self, messages: Vec<Message>, compaction_ids: HashSet<String>) {
        self.messages = messages;
        self.compaction_message_ids = compaction_ids;
        self.rebuild_indices();
        self.bump_store();
    }

    pub fn push(&mut self, mut msg: Message) {
        self.ensure_token(&mut msg);
        let idx = self.messages.len();
        self.index_inserted_message(idx, &msg);
        self.messages.push(msg);
        self.bump_store();
    }

    pub fn insert(&mut self, idx: usize, mut msg: Message) {
        let idx = idx.min(self.messages.len());
        self.ensure_token(&mut msg);
        self.messages.insert(idx, msg);
        self.rebuild_indices();
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

    pub fn tick_streaming_assistant(&mut self) -> bool {
        if self.streaming_assistant_idx.is_none() {
            return false;
        }
        self.bump_store();
        true
    }

    pub fn bump_streaming_output_tokens(
        &mut self,
        tokens: u64,
        server_message_id: Option<&str>,
    ) -> bool {
        if tokens == 0 {
            return false;
        }
        let idx = server_message_id
            .and_then(|mid| self.by_server_id.get(mid).copied())
            .or(self.streaming_assistant_idx);
        let Some(idx) = idx else {
            return false;
        };
        let Some(msg) = self.messages.get_mut(idx) else {
            return false;
        };
        if msg.sender != Sender::Assistant {
            return false;
        }
        msg.output_tokens = msg.output_tokens.saturating_add(tokens);
        msg.tokens_approx = true;
        if msg.started_at.is_none() && msg.duration.is_none() {
            msg.started_at = Some(std::time::Instant::now());
        }
        self.bump_store();
        true
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
            for m in &self.messages {
                let queued = m.sender == Sender::User
                    && m.queued_under
                        .and_then(|t| self.token_to_idx.get(&t).copied())
                        .and_then(|i| self.messages.get(i))
                        .map(|anchor| anchor.is_streaming)
                        .unwrap_or(false);
                flags.push(queued);
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
            output_tokens: m.output_tokens.unwrap_or(0),
            tokens_approx: m.output_tokens.is_none() && m.is_streaming,
            started_at: if m.is_streaming && m.duration.is_none() {
                Some(std::time::Instant::now())
            } else {
                None
            },
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
        let mut m = self.host_to_tui_message(message, &now_hhmm);
        self.ensure_token(&mut m);
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
        let mut m = self.host_to_tui_message(host_msg, &now_hhmm);
        self.ensure_token(&mut m);
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
            msg.output_tokens = msg
                .output_tokens
                .saturating_add(crate::model::approx_tokens(text));
            msg.tokens_approx = true;
            if msg.started_at.is_none() && msg.duration.is_none() {
                msg.started_at = Some(std::time::Instant::now());
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
                if let Some(msg) = self.messages.get_mut(idx) {
                    if msg.server_id.is_none() {
                        msg.server_id = Some(mid.to_string());
                        self.by_server_id.insert(mid.to_string(), idx);
                        return idx;
                    }
                }
            }
        }
        if let Some(idx) = self.streaming_assistant_idx {
            return idx;
        }
        let mut msg = make_streaming_message();
        if let Some(mid) = server_message_id {
            msg.server_id = Some(mid.to_string());
        }
        self.ensure_token(&mut msg);
        let idx = self.messages.len();
        self.index_inserted_message(idx, &msg);
        self.messages.push(msg);
        idx
    }

    pub fn finish_streaming_assistant(&mut self, message_id: Option<&str>) {
        match message_id {
            Some(mid) => {
                let target = self.by_server_id.get(mid).copied().filter(|&idx| {
                    self.messages
                        .get(idx)
                        .map(|m| m.sender == Sender::Assistant && m.is_streaming)
                        .unwrap_or(false)
                });
                if let Some(idx) = target {
                    let Some(msg) = self.messages.get_mut(idx) else {
                        return;
                    };
                    msg.is_streaming = false;
                    msg.invalidate_render_cache();
                    self.recompute_first_streaming_assistant();
                    self.bump_store();
                }
            }
            None => {
                let target = self.streaming_assistant_idx;
                if let Some(idx) = target {
                    let Some(msg) = self.messages.get_mut(idx) else {
                        return;
                    };
                    msg.is_streaming = false;
                    msg.invalidate_render_cache();
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
        let Some(msg) = self.messages.get_mut(loc.msg_idx) else {
            self.tools.remove(id);
            return;
        };
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
            .filter(|&i| self.messages.get(i).map(|m| m.sender) == Some(Sender::Assistant));
        let Some(idx) = target else {
            return false;
        };
        let Some(msg) = self.messages.get_mut(idx) else {
            return false;
        };
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
        let Some(msg) = self.messages.get_mut(loc.msg_idx) else {
            return;
        };
        let Some(tool) = msg.tool_calls.get_mut(loc.tool_idx) else {
            return;
        };
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
        let Some(msg) = self.messages.get_mut(loc.msg_idx) else {
            return false;
        };
        let Some(initial) = msg.tool_calls.get_mut(loc.tool_idx) else {
            return false;
        };
        let new_expanded = {
            initial.expanded = !initial.expanded;
            initial.expanded
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
        let Some(msg) = self.messages.get_mut(idx) else {
            return false;
        };
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
        let Some(msg) = self.messages.get_mut(idx) else {
            return false;
        };
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
        output_tokens: Option<u64>,
    ) {
        let target = self.streaming_assistant_idx.or(self.last_assistant_idx);
        if let Some(idx) = target {
            let Some(msg) = self.messages.get_mut(idx) else {
                return;
            };
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
            if let Some(n) = output_tokens {
                msg.output_tokens = n;
                msg.tokens_approx = false;
            }
            self.bump_message(idx);
        }
    }

    fn index_inserted_message(&mut self, idx: usize, msg: &Message) {
        if let Some(sid) = msg.server_id.clone() {
            self.by_server_id.insert(sid, idx);
        }
        if msg.token != 0 {
            self.token_to_idx.insert(msg.token, idx);
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
        self.token_to_idx.clear();
        self.last_assistant_idx = None;
        self.streaming_assistant_idx = None;
        let max_existing = self.messages.iter().map(|m| m.token).max().unwrap_or(0);
        if self.next_token <= max_existing {
            self.next_token = max_existing.saturating_add(1);
        }
        for i in 0..self.messages.len() {
            if self.messages[i].token == 0 {
                self.messages[i].token = self.mint_token();
            }
        }
        for (i, msg) in self.messages.iter().enumerate() {
            if let Some(sid) = msg.server_id.clone() {
                self.by_server_id.insert(sid, i);
            }
            self.token_to_idx.insert(msg.token, i);
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
        let anchor = s.streaming_assistant_token().expect("anchor available");
        let mut b = user("b");
        b.queued_under = Some(anchor);
        s.push(b);
        {
            let flags = s.queued_flags().to_vec();
            assert_eq!(
                flags,
                vec![false, false, true],
                "b is queued under the still-streaming anchor",
            );
        }
        s.finish_streaming_assistant(None);
        s.push(user("c"));
        let flags = s.queued_flags().to_vec();
        assert_eq!(
            flags,
            vec![false, false, false, false],
            "anchor no longer streaming → b unqueued; c never had an anchor",
        );
    }

    #[test]
    fn queued_under_makes_burst_unqueue_together() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        let anchor = s.streaming_assistant_token().expect("anchor available");

        for label in ["a", "b", "c"] {
            let mut u = user(label);
            u.queued_under = Some(anchor);
            s.push(u);
            s.push(assistant_streaming());
        }

        assert_eq!(
            s.queued_flags().to_vec(),
            vec![false, true, false, true, false, true, false],
            "3 users queued, leading + 3 trailing assistants in between",
        );

        s.finish_streaming_assistant(None);

        assert_eq!(
            s.queued_flags().to_vec(),
            vec![false, false, false, false, false, false, false],
            "burst-unqueue: all three users lose QUEUED when the anchor turn ends",
        );
    }

    #[test]
    fn queued_under_survives_message_removal_via_rebuild() {
        let mut s = MessageStore::new();
        let mut a = assistant_streaming();
        a.server_id = Some("a-1".into());
        s.push(a);
        let anchor = s.streaming_assistant_token().expect("anchor available");

        let mut u = user("u");
        u.queued_under = Some(anchor);
        u.server_id = Some("u-1".into());
        s.push(u);

        let mut tail = user("tail");
        tail.server_id = Some("u-2".into());
        s.push(tail);

        assert_eq!(s.queued_flags().to_vec(), vec![false, true, false]);

        s.remove_by_server_id("u-2");
        assert_eq!(s.len(), 2);
        assert_eq!(
            s.queued_flags().to_vec(),
            vec![false, true],
            "queued user keeps its anchor through a remove + rebuild",
        );
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

    #[test]
    fn append_delta_accumulates_approx_token_count() {
        let mut s = MessageStore::new();
        s.append_assistant_delta("hello world", false, None, assistant_streaming);
        s.append_assistant_delta("more", false, None, assistant_streaming);
        s.append_assistant_delta("thinking...", true, None, assistant_streaming);
        let msg = &s.messages[0];
        assert!(msg.tokens_approx);
        assert_eq!(
            msg.output_tokens,
            crate::model::approx_tokens("hello world")
                + crate::model::approx_tokens("more")
                + crate::model::approx_tokens("thinking...")
        );
        assert!(msg.started_at.is_some(), "started_at anchors live duration");
    }

    #[test]
    fn update_last_assistant_meta_replaces_approximation_with_exact_total() {
        let mut s = MessageStore::new();
        s.append_assistant_delta("partial", false, None, assistant_streaming);
        assert!(s.messages[0].tokens_approx);
        let before = s.messages[0].output_tokens;
        assert!(before > 0);

        s.update_last_assistant_meta(None, None, None, None, Some(1234));
        assert_eq!(s.messages[0].output_tokens, 1234);
        assert!(!s.messages[0].tokens_approx);
    }

    #[test]
    fn approx_tokens_handles_edges() {
        assert_eq!(crate::model::approx_tokens(""), 0);
        assert_eq!(crate::model::approx_tokens("a"), 1);
        assert_eq!(crate::model::approx_tokens("abcd"), 1);
        assert_eq!(crate::model::approx_tokens("abcde"), 2);
        assert_eq!(crate::model::approx_tokens("✓✓✓✓"), 1);
    }

    #[test]
    fn tick_streaming_assistant_noop_when_idle() {
        let mut s = MessageStore::new();
        s.push(user("u"));
        assert!(!s.tick_streaming_assistant());
    }

    #[test]
    fn tick_streaming_assistant_bumps_store_when_streaming() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        let before = s.version();
        assert!(s.tick_streaming_assistant());
        assert_ne!(s.version(), before, "tick should bump store version");
    }

    #[test]
    fn bump_streaming_output_tokens_credits_streaming_assistant() {
        let mut s = MessageStore::new();
        s.push(user("u"));
        s.push(assistant_streaming());
        assert!(s.bump_streaming_output_tokens(42, None));
        let m = &s.messages[1];
        assert_eq!(m.output_tokens, 42);
        assert!(m.tokens_approx);
        assert!(m.started_at.is_some(), "anchor live duration on first bump");
    }

    #[test]
    fn bump_streaming_output_tokens_is_idempotent_on_zero() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        assert!(!s.bump_streaming_output_tokens(0, None));
        assert_eq!(s.messages[0].output_tokens, 0);
    }

    #[test]
    fn bump_streaming_output_tokens_finds_target_by_server_id() {
        let mut s = MessageStore::new();
        let mut a = assistant_streaming();
        a.server_id = Some("a-1".into());
        s.push(a);
        assert!(s.bump_streaming_output_tokens(7, Some("a-1")));
        assert_eq!(s.messages[0].output_tokens, 7);
    }

    #[test]
    fn bump_streaming_output_tokens_noop_without_streaming_target() {
        let mut s = MessageStore::new();
        s.push(user("u"));
        assert!(!s.bump_streaming_output_tokens(50, None));
    }

    #[test]
    fn take_for_stash_returns_messages_and_clears_indices() {
        let mut s = MessageStore::new();
        let mut a = user("first");
        a.server_id = Some("u-1".into());
        s.push(a);
        s.push(assistant_streaming());
        s.upsert_tool_call(tool("t1", "bash"));

        let (messages, _) = s.take_for_stash();

        assert_eq!(messages.len(), 2);
        assert_eq!(s.len(), 0);
        assert!(s.message_by_server_id("u-1").is_none());
        assert!(s.tool_location("t1").is_none());
        assert_eq!(s.streaming_assistant_index(), None);
        assert_eq!(s.last_assistant_index(), None);
        assert!(s.queued_flags().is_empty());
    }

    #[test]
    fn install_rebuilds_all_indices() {
        let mut s = MessageStore::new();
        let mut a = user("first");
        a.server_id = Some("u-1".into());
        let mut b = assistant_streaming();
        b.server_id = Some("a-1".into());
        b.tool_calls.push(tool("t1", "bash"));
        let c = user("third");

        s.install(vec![a, b, c], HashSet::new());

        assert_eq!(s.len(), 3);
        assert!(s.message_by_server_id("u-1").is_some());
        assert!(s.message_by_server_id("a-1").is_some());
        let loc = s.tool_location("t1").expect("tool indexed");
        assert_eq!(loc.msg_idx, 1);
        assert_eq!(loc.tool_idx, 0);
        assert_eq!(s.streaming_assistant_index(), Some(1));
        assert_eq!(s.last_assistant_index(), Some(1));
        assert_eq!(s.queued_flags().len(), 3);
    }

    #[test]
    fn install_smaller_transcript_after_larger_does_not_leave_stale_indices() {
        let mut s = MessageStore::new();
        for i in 0..12 {
            let mut u = user(&format!("u{i}"));
            u.server_id = Some(format!("u-{i}"));
            s.push(u);
            let mut a = assistant_streaming();
            a.server_id = Some(format!("a-{i}"));
            a.is_streaming = false;
            s.push(a);
        }
        let mut last_stream = assistant_streaming();
        last_stream.server_id = Some("a-last".into());
        s.push(last_stream);

        let (big, big_cids) = s.take_for_stash();
        assert_eq!(big.len(), 25);

        let small = vec![user("only message")];
        s.install(small, HashSet::new());

        assert_eq!(s.len(), 1);
        let qflags = s.queued_flags().to_vec();
        assert_eq!(
            qflags.len(),
            s.len(),
            "queued_flags must match current message count after install",
        );
        assert_eq!(
            s.streaming_assistant_index(),
            None,
            "stream idx must NOT point into the prior transcript",
        );
        assert_eq!(
            s.last_assistant_index(),
            None,
            "last assistant idx must NOT point into the prior transcript",
        );

        assert!(!s.upsert_tool_call(tool("t-new", "bash")));

        s.install(big, big_cids);
        assert_eq!(s.len(), 25);
        assert_eq!(s.queued_flags().len(), 25);
        assert_eq!(s.streaming_assistant_index(), Some(24));
        assert!(s.message_by_server_id("u-0").is_some());
        assert!(s.message_by_server_id("a-last").is_some());
    }

    #[test]
    fn install_invalidates_queued_flags_cache() {
        let mut s = MessageStore::new();
        s.push(user("a"));
        s.push(assistant_streaming());
        s.push(user("queued"));
        let _ = s.queued_flags().to_vec();
        s.install(vec![user("only")], HashSet::new());
        let qflags = s.queued_flags();
        assert_eq!(qflags, &[false]);
    }

    #[test]
    fn take_for_stash_invalidates_queued_flags_cache() {
        let mut s = MessageStore::new();
        s.push(user("a"));
        s.push(assistant_streaming());
        s.push(user("queued"));
        let _ = s.queued_flags().to_vec();
        let _ = s.take_for_stash();
        assert!(s.queued_flags().is_empty());
    }

    #[test]
    fn upsert_tool_call_does_not_panic_when_streaming_idx_is_stale() {
        let mut s = MessageStore::new();
        s.push(user("only"));
        s.streaming_assistant_idx = Some(99);
        s.last_assistant_idx = Some(99);
        assert!(
            !s.upsert_tool_call(tool("t", "bash")),
            "upsert must return false when index is invalid, not panic",
        );
    }

    #[test]
    fn update_task_child_does_not_panic_on_stale_tool_location() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        s.upsert_tool_call(tool("parent", "task"));
        if let Some(loc) = s.tools.get_mut("parent") {
            loc.msg_idx = 99;
            loc.tool_idx = 99;
        }
        s.update_task_child("parent", None, 5);
    }

    #[test]
    fn toggle_tool_expanded_does_not_panic_on_stale_tool_location() {
        let mut s = MessageStore::new();
        s.push(assistant_streaming());
        s.upsert_tool_call(tool("t", "bash"));
        if let Some(loc) = s.tools.get_mut("t") {
            loc.msg_idx = 99;
        }
        assert!(
            !s.toggle_tool_expanded("t"),
            "must return false on stale location, not panic",
        );
    }

    #[test]
    fn set_last_assistant_error_does_not_panic_on_stale_idx() {
        let mut s = MessageStore::new();
        s.push(user("u"));
        s.last_assistant_idx = Some(99);
        assert!(!s.set_last_assistant_error("nope".into()));
    }

    #[test]
    fn update_last_assistant_meta_does_not_panic_on_stale_idx() {
        let mut s = MessageStore::new();
        s.push(user("u"));
        s.last_assistant_idx = Some(99);
        s.streaming_assistant_idx = Some(99);
        s.update_last_assistant_meta(Some("a".into()), None, None, None, None);
    }

    #[test]
    fn mark_assistant_interrupted_does_not_panic_on_stale_index() {
        let mut s = MessageStore::new();
        s.push(user("u"));
        s.by_server_id.insert("a-stale".into(), 99);
        assert!(!s.mark_assistant_interrupted("a-stale"));
    }

    #[test]
    fn round_trip_stash_install_preserves_content() {
        let mut s = MessageStore::new();
        let mut a = user("alpha");
        a.server_id = Some("u-a".into());
        s.push(a);
        let mut b = assistant_streaming();
        b.server_id = Some("a-b".into());
        b.tool_calls.push(tool("tb", "read"));
        s.push(b);
        s.push(user("gamma"));

        let (msgs, cids) = s.take_for_stash();
        assert_eq!(msgs.len(), 3);
        s.install(msgs, cids);

        assert_eq!(s.len(), 3);
        assert_eq!(s.messages[0].content, "alpha");
        assert_eq!(s.messages[2].content, "gamma");
        let loc = s.tool_location("tb").expect("tool re-indexed");
        assert_eq!(loc.msg_idx, 1);
    }

    #[test]
    fn switching_between_two_transcripts_repeatedly_stays_consistent() {
        let mut s = MessageStore::new();

        for i in 0..5 {
            let mut u = user(&format!("a-u{i}"));
            u.server_id = Some(format!("au-{i}"));
            s.push(u);
        }
        s.push(assistant_streaming());
        let (a_msgs, a_cids) = s.take_for_stash();

        s.push(user("b-only"));
        let (b_msgs, b_cids) = s.take_for_stash();

        for cycle in 0..6 {
            let (msgs, cids) = if cycle % 2 == 0 {
                (a_msgs.clone(), a_cids.clone())
            } else {
                (b_msgs.clone(), b_cids.clone())
            };
            let expected_len = msgs.len();
            let expected_last_assistant = msgs
                .iter()
                .enumerate()
                .rev()
                .find_map(|(i, m)| (m.sender == Sender::Assistant).then_some(i));
            let expected_streaming = msgs
                .iter()
                .position(|m| m.sender == Sender::Assistant && m.is_streaming);

            s.install(msgs, cids);

            assert_eq!(s.len(), expected_len, "cycle {cycle}");
            assert_eq!(s.queued_flags().len(), expected_len, "cycle {cycle}");
            assert_eq!(
                s.last_assistant_index(),
                expected_last_assistant,
                "cycle {cycle}",
            );
            assert_eq!(
                s.streaming_assistant_index(),
                expected_streaming,
                "cycle {cycle}",
            );
            let _ = s.upsert_tool_call(tool(&format!("t-cycle-{cycle}"), "bash"));
        }
    }
}
