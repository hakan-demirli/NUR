use std::collections::{HashMap, HashSet};

use crate::action::{self, HostMessagePart};
use crate::model::{CompactionMarker, Message, Sender};

pub struct MessageStore {
    pub messages: Vec<Message>,

    pub compaction_message_ids: HashSet<String>,

    pub tool_block_rects: Vec<(String, ratatui::layout::Rect)>,
    pub user_message_rects: Vec<(String, ratatui::layout::Rect)>,

    pub show_timestamps: bool,
    pub thinking_hidden: bool,
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
        }
    }

    pub fn clear(&mut self) {
        self.messages.clear();
        self.compaction_message_ids.clear();
    }

    pub fn push(&mut self, msg: Message) {
        self.messages.push(msg);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Message> {
        self.messages.iter()
    }

    pub fn iter_mut(&mut self) -> std::slice::IterMut<'_, Message> {
        self.messages.iter_mut()
    }

    pub fn is_empty(&self) -> bool {
        self.messages.is_empty()
    }

    pub fn len(&self) -> usize {
        self.messages.len()
    }

    pub fn toggle_timestamps(&mut self) {
        self.show_timestamps = !self.show_timestamps;
        for msg in &mut self.messages {
            msg.invalidate_render_cache();
        }
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
            agent: m.agent,
            model: m.model,
            provider_id: m.provider_id,
            duration: m.duration,
            error: m.error,
            tool_calls: m.tool_calls,
            parts: m.parts,
            compaction: m.compaction,
            rendered_content_cache: None,
            rendered_thoughts_cache: None,
            last_render_width: 0,
            content_fingerprint: 0,
            thoughts_fingerprint: 0,
            tool_render_cache: HashMap::new(),
            part_render_cache: HashMap::new(),
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
    }

    pub fn host_append(&mut self, message: action::HostMessage, now_hhmm: impl Fn() -> String) {
        let m = self.host_to_tui_message(message, &now_hhmm);
        self.messages.push(m);
    }

    pub fn mark_compaction(
        &mut self,
        message_id: String,
        marker: CompactionMarker,
        now_hhmm: impl Fn() -> String,
    ) -> bool {
        if !self.compaction_message_ids.insert(message_id) {
            return false;
        }
        let mut host_msg = action::HostMessage::user(String::new());
        host_msg.compaction = Some(marker);
        let m = self.host_to_tui_message(host_msg, &now_hhmm);
        self.messages.push(m);
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
    }

    fn resolve_streaming_target(
        &mut self,
        server_message_id: Option<&str>,
        make_streaming_message: impl FnOnce() -> Message,
    ) -> usize {
        if let Some(mid) = server_message_id {
            if let Some(idx) = self
                .messages
                .iter()
                .position(|m| m.server_id.as_deref() == Some(mid))
            {
                return idx;
            }
            if let Some(idx) = self.messages.iter().position(|m| {
                m.sender == Sender::Assistant && m.is_streaming && m.server_id.is_none()
            }) {
                self.messages[idx].server_id = Some(mid.to_string());
                return idx;
            }
        }
        if let Some(idx) = self
            .messages
            .iter()
            .position(|m| m.sender == Sender::Assistant && m.is_streaming)
        {
            return idx;
        }
        self.messages.push(make_streaming_message());
        self.messages.len() - 1
    }

    pub fn finish_streaming_assistant(&mut self, message_id: Option<&str>) {
        match message_id {
            Some(mid) => {
                if let Some(msg) = self.messages.iter_mut().find(|m| {
                    m.sender == Sender::Assistant
                        && m.is_streaming
                        && m.server_id.as_deref() == Some(mid)
                }) {
                    msg.is_streaming = false;
                    msg.invalidate_render_cache();
                }
            }
            None => {
                for msg in &mut self.messages {
                    if msg.sender == Sender::Assistant && msg.is_streaming {
                        msg.is_streaming = false;
                        msg.invalidate_render_cache();
                    }
                }
            }
        }
    }

    pub fn remove_by_server_id(&mut self, id: &str) {
        self.messages.retain(|m| m.server_id.as_deref() != Some(id));
    }

    pub fn remove_tool_call_by_id(&mut self, id: &str) {
        for msg in &mut self.messages {
            let before = msg.tool_calls.len();
            msg.tool_calls.retain(|t| t.id.as_deref() != Some(id));
            msg.parts.retain(|p| match p {
                HostMessagePart::Tool(t) => t.id.as_deref() != Some(id),
                _ => true,
            });
            if msg.tool_calls.len() != before {
                msg.tool_render_cache.remove(id);
                msg.invalidate_render_cache();
            }
        }
    }

    pub fn upsert_tool_call(&mut self, tool: action::ToolCall) -> bool {
        let key = tool_call_match_key(&tool);
        let target = self
            .messages
            .iter()
            .position(|m| m.sender == Sender::Assistant && m.is_streaming)
            .or_else(|| {
                self.messages
                    .iter()
                    .rposition(|m| m.sender == Sender::Assistant)
            });
        let Some(idx) = target else {
            return false;
        };
        let msg = &mut self.messages[idx];
        let existing = msg
            .tool_calls
            .iter_mut()
            .find(|t| tool_call_match_key(t) == key);
        match existing {
            Some(slot) => {
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
                if let Some(HostMessagePart::Tool(part_tool)) = msg.parts.iter_mut().find(
                    |p| matches!(p, HostMessagePart::Tool(t) if tool_call_match_key(t) == key),
                ) {
                    *part_tool = Box::new(slot.clone());
                }
            }
            None => {
                let mut t = tool;
                if t.name == "task" {
                    stamp_task_timing(&mut t, action::ToolStatus::Pending);
                }
                msg.tool_calls.push(t);
                if let Some(inserted) = msg.tool_calls.last().cloned() {
                    msg.parts.push(HostMessagePart::Tool(Box::new(inserted)));
                }
            }
        }
        msg.invalidate_render_cache();
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
        for msg in self.messages.iter_mut().rev() {
            if let Some(tool) = msg
                .tool_calls
                .iter_mut()
                .find(|t| t.id.as_deref() == Some(parent_tool_id))
            {
                let unchanged =
                    tool.current_child == child && tool.child_tool_count == child_tool_count;
                if unchanged {
                    return;
                }
                tool.current_child = child;
                tool.child_tool_count = tool.child_tool_count.max(child_tool_count);
                msg.tool_render_cache.remove(parent_tool_id);
                msg.invalidate_render_cache();
                return;
            }
        }
    }

    pub fn toggle_tool_expanded(&mut self, id: &str) -> bool {
        if id.is_empty() {
            return false;
        }
        for msg in self.messages.iter_mut().rev() {
            let mut new_expanded = msg
                .tool_calls
                .iter_mut()
                .find(|t| t.id.as_deref() == Some(id))
                .map(|tool| {
                    tool.expanded = !tool.expanded;
                    tool.expanded
                });

            if new_expanded.is_none() {
                new_expanded = msg.parts.iter_mut().find_map(|part| {
                    if let HostMessagePart::Tool(part_tool) = part {
                        if part_tool.id.as_deref() == Some(id) {
                            part_tool.expanded = !part_tool.expanded;
                            return Some(part_tool.expanded);
                        }
                    }
                    None
                });
            }

            let Some(new_expanded) = new_expanded else {
                continue;
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
            msg.invalidate_render_cache();
            return true;
        }
        false
    }

    pub fn set_last_assistant_error(&mut self, error: String) -> bool {
        if error.is_empty() {
            return false;
        }
        let target = self
            .messages
            .iter()
            .position(|m| m.sender == Sender::Assistant && m.is_streaming)
            .or_else(|| {
                self.messages
                    .iter()
                    .rposition(|m| m.sender == Sender::Assistant)
            });
        let Some(idx) = target else {
            return false;
        };
        let msg = &mut self.messages[idx];
        if msg.error.as_deref() == Some(error.as_str()) {
            return false;
        }
        msg.error = Some(error);
        msg.invalidate_render_cache();
        true
    }

    pub fn update_last_assistant_meta(
        &mut self,
        agent: Option<String>,
        model: Option<String>,
        provider_id: Option<String>,
        duration: Option<std::time::Duration>,
    ) {
        let target = self
            .messages
            .iter()
            .position(|m| m.sender == Sender::Assistant && m.is_streaming)
            .or_else(|| {
                self.messages
                    .iter()
                    .rposition(|m| m.sender == Sender::Assistant)
            });
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
            msg.invalidate_render_cache();
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
