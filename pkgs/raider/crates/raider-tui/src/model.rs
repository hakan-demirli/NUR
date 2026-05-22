use ratatui::prelude::Line;
use serde::{Deserialize, Serialize};

use crate::state::Version;

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
    pub output_tokens: u64,

    #[serde(default)]
    pub tokens_approx: bool,

    #[serde(skip)]
    pub started_at: Option<std::time::Instant>,

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
    pub legacy_cache_key: Option<LegacyCacheKey>,
    #[serde(skip)]
    pub tool_render_cache: std::collections::HashMap<String, ToolRenderCacheEntry>,

    #[serde(skip)]
    pub part_render_cache: std::collections::HashMap<String, PartRenderCacheEntry>,

    #[serde(skip)]
    pub(crate) version: Version,

    #[serde(skip)]
    pub token: u64,

    #[serde(default)]
    pub queued_under: Option<u64>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct LegacyCacheKey {
    pub version: Version,
    pub width: usize,
    pub theme_mode: crate::ui::theme::Mode,
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
            output_tokens: 0,
            tokens_approx: false,
            started_at: None,
            error: None,
            tool_calls: Vec::new(),
            parts: Vec::new(),
            compaction: None,
            rendered_content_cache: None,
            rendered_thoughts_cache: None,
            legacy_cache_key: None,
            tool_render_cache: std::collections::HashMap::new(),
            part_render_cache: std::collections::HashMap::new(),
            version: Version::default(),
            token: 0,
            queued_under: None,
        }
    }
}

pub fn approx_tokens(s: &str) -> u64 {
    approx_tokens_from_chars(s.chars().count() as u64)
}

pub fn approx_tokens_from_chars(n: u64) -> u64 {
    if n == 0 {
        0
    } else {
        n.div_ceil(4)
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
            started_at: Some(std::time::Instant::now()),
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
            started_at: Some(std::time::Instant::now()),
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
        self.legacy_cache_key = None;
        self.part_render_cache.clear();
        self.version.bump();
    }

    pub fn version(&self) -> Version {
        self.version
    }

    pub fn bump_version(&mut self) {
        self.version.bump();
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
