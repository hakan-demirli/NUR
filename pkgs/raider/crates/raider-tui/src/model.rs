use ratatui::prelude::Line;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Sender {
    User,
    Assistant,
    System,
}

impl Sender {
    pub fn label(self) -> &'static str {
        match self {
            Sender::User => "User",
            Sender::Assistant => "Raider",
            Sender::System => "System",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Message {
    pub sender: Sender,
    pub content: String,
    pub thoughts: String,
    #[serde(default)]
    pub server_id: Option<String>,
    pub timestamp: String,

    #[serde(default)]
    pub is_streaming: bool,
    #[serde(default)]
    pub thoughts_collapsed: bool,

    #[serde(default)]
    pub interrupted: bool,

    #[serde(default)]
    pub agent: Option<String>,

    #[serde(default)]
    pub model: Option<String>,

    #[serde(default)]
    pub provider_id: Option<String>,

    #[serde(default)]
    pub duration: Option<std::time::Duration>,

    #[serde(default)]
    pub error: Option<String>,

    #[serde(skip)]
    pub tool_calls: Vec<crate::action::ToolCall>,

    #[serde(skip)]
    pub parts: Vec<crate::action::HostMessagePart>,

    #[serde(default)]
    pub compaction: Option<CompactionMarker>,

    #[serde(skip)]
    pub rendered_content_cache: Option<Vec<Line<'static>>>,
    #[serde(skip)]
    pub rendered_thoughts_cache: Option<Vec<Line<'static>>>,
    #[serde(skip)]
    pub last_render_width: usize,
    #[serde(skip)]
    pub content_fingerprint: u64,
    #[serde(skip)]
    pub thoughts_fingerprint: u64,
    #[serde(skip)]
    pub tool_render_cache: std::collections::HashMap<String, ToolRenderCacheEntry>,

    #[serde(skip)]
    pub part_render_cache: std::collections::HashMap<String, PartRenderCacheEntry>,
}

#[derive(Clone, Debug)]
pub struct ToolRenderCacheEntry {
    pub key: ToolCacheKey,
    pub items: Vec<ratatui::widgets::ListItem<'static>>,
    pub spinner_slot: Option<ToolHeaderSlot>,
}

#[derive(Clone, Debug)]
pub struct PartRenderCacheEntry {
    pub key: PartRenderCacheKey,
    pub items: Vec<ratatui::widgets::ListItem<'static>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct PartRenderCacheKey {
    pub width: usize,
    pub theme_mode: crate::ui::theme::Mode,
    pub kind: PartRenderKind,
    pub collapsed: bool,
    pub streaming: bool,
    pub content_hash: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PartRenderKind {
    Text,
    Thought,
}

#[derive(Clone, Debug)]
pub struct ToolHeaderSlot {
    pub bar_fg: ratatui::style::Color,
    pub bar_bg: ratatui::style::Color,
    pub gap_str: String,
    pub gap_bg: ratatui::style::Color,
    pub bar_str: String,
    pub row_bg: ratatui::style::Color,
    pub body_fg: ratatui::style::Color,
    pub title_fg: ratatui::style::Color,
    pub title: String,
    pub kind: ToolHeaderKind,
}

#[derive(Clone, Copy, Debug)]
pub enum ToolHeaderKind {
    Inline,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionMarker {
    pub auto: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ToolCacheKey {
    pub width: usize,
    pub theme_mode: crate::ui::theme::Mode,
    pub status: crate::action::ToolStatus,
    pub expanded: bool,
    pub content_hash: u64,
}

impl Default for Message {
    fn default() -> Self {
        Self {
            sender: Sender::System,
            content: String::new(),
            thoughts: String::new(),
            server_id: None,
            timestamp: String::new(),
            is_streaming: false,
            thoughts_collapsed: false,
            interrupted: false,
            agent: None,
            model: None,
            provider_id: None,
            duration: None,
            error: None,
            tool_calls: Vec::new(),
            parts: Vec::new(),
            compaction: None,
            rendered_content_cache: None,
            rendered_thoughts_cache: None,
            last_render_width: 0,
            content_fingerprint: 0,
            thoughts_fingerprint: 0,
            tool_render_cache: std::collections::HashMap::new(),
            part_render_cache: std::collections::HashMap::new(),
        }
    }
}

pub fn format_duration(d: std::time::Duration) -> String {
    let total_ms = d.as_millis();
    if total_ms < 1_000 {
        let secs = total_ms as f64 / 1_000.0;
        return format!("{secs:.1}s");
    }
    let total_secs = d.as_secs();
    if total_secs < 60 {
        let secs = total_ms as f64 / 1_000.0;
        return format!("{secs:.1}s");
    }
    let mins = total_secs / 60;
    let secs = total_secs % 60;
    format!("{mins}m{secs}s")
}

impl Message {
    pub fn user(content: impl Into<String>, ts: impl Into<String>) -> Self {
        Self {
            sender: Sender::User,
            content: content.into(),
            timestamp: ts.into(),
            ..Default::default()
        }
    }

    pub fn assistant_streaming(ts: impl Into<String>) -> Self {
        Self {
            sender: Sender::Assistant,
            content: String::new(),
            timestamp: ts.into(),
            is_streaming: true,
            ..Default::default()
        }
    }

    pub fn assistant_streaming_with_meta(
        ts: impl Into<String>,
        agent: Option<String>,
        model: Option<String>,
        provider_id: Option<String>,
    ) -> Self {
        Self {
            sender: Sender::Assistant,
            content: String::new(),
            timestamp: ts.into(),
            is_streaming: true,
            agent,
            model,
            provider_id,
            parts: Vec::new(),
            ..Default::default()
        }
    }

    pub fn system(content: impl Into<String>, ts: impl Into<String>) -> Self {
        Self {
            sender: Sender::System,
            content: content.into(),
            timestamp: ts.into(),
            ..Default::default()
        }
    }

    pub fn invalidate_render_cache(&mut self) {
        self.rendered_content_cache = None;
        self.rendered_thoughts_cache = None;
        self.last_render_width = 0;
        self.content_fingerprint = 0;
        self.thoughts_fingerprint = 0;
        self.part_render_cache.clear();
    }
}

pub fn content_fingerprint(s: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.len().hash(&mut h);
    let bytes = s.as_bytes();
    let tail_start = bytes.len().saturating_sub(256);
    bytes[tail_start..].hash(&mut h);
    h.finish()
}
