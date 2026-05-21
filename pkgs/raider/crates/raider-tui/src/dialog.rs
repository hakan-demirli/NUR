use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::widgets::ListState;

use crate::provider::ModelRef;
use crate::ui::theme::ThemeName;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DialogKind {
    ThemePicker,
    CommandPalette,
    AgentPicker,
    ModelPicker,
    VariantPicker,
    SessionPicker,
    SessionRename,
    PluginSelect,
    PluginAlert,
    PluginManager,
    PluginInstall,
    MessageActions,
    ForkPicker,
}

#[derive(Clone, Debug)]
pub enum DialogPayload {
    ThemePicker {
        current: ThemeName,
    },
    CommandPalette {
        current: String,
    },
    AgentPicker {
        current: String,
    },
    ModelPicker {
        current: Option<ModelRef>,
    },
    VariantPicker {
        current: Option<String>,
    },
    SessionPicker {
        current: Option<String>,
    },
    SessionRename {
        session_id: String,
        title: String,
    },
    PluginSelect {
        callback_id: u64,
        current: Option<String>,
    },
    PluginAlert {
        message: String,
    },
    PluginManager {
        current: String,
    },
    PluginInstall {
        path: String,
        scope: PluginInstallScope,
    },
    MessageActions {
        message_id: String,
    },
    ForkPicker {
        current: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginInstallScope {
    Global,
    Local,
}

impl PluginInstallScope {
    pub fn label(self) -> &'static str {
        match self {
            PluginInstallScope::Global => "global",
            PluginInstallScope::Local => "local",
        }
    }
}

impl DialogPayload {
    pub fn kind(&self) -> DialogKind {
        match self {
            Self::ThemePicker { .. } => DialogKind::ThemePicker,
            Self::CommandPalette { .. } => DialogKind::CommandPalette,
            Self::AgentPicker { .. } => DialogKind::AgentPicker,
            Self::ModelPicker { .. } => DialogKind::ModelPicker,
            Self::VariantPicker { .. } => DialogKind::VariantPicker,
            Self::SessionPicker { .. } => DialogKind::SessionPicker,
            Self::SessionRename { .. } => DialogKind::SessionRename,
            Self::PluginSelect { .. } => DialogKind::PluginSelect,
            Self::PluginAlert { .. } => DialogKind::PluginAlert,
            Self::PluginManager { .. } => DialogKind::PluginManager,
            Self::PluginInstall { .. } => DialogKind::PluginInstall,
            Self::MessageActions { .. } => DialogKind::MessageActions,
            Self::ForkPicker { .. } => DialogKind::ForkPicker,
        }
    }

    pub fn current_value(&self) -> String {
        match self {
            Self::ThemePicker { current } => current.as_str().to_string(),
            Self::CommandPalette { current } => current.clone(),
            Self::AgentPicker { current } => current.clone(),
            Self::ModelPicker { current } => current.as_ref().map(|m| m.wire()).unwrap_or_default(),
            Self::VariantPicker { current } => current.clone().unwrap_or_default(),
            Self::SessionPicker { current } => current.clone().unwrap_or_default(),
            Self::SessionRename { title, .. } => title.clone(),
            Self::PluginSelect { current, .. } => current.clone().unwrap_or_default(),
            Self::PluginAlert { .. } => String::new(),
            Self::PluginManager { current } => current.clone(),
            Self::PluginInstall { path, .. } => path.clone(),
            Self::MessageActions { .. } => String::new(),
            Self::ForkPicker { current } => current.clone().unwrap_or_default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct DialogOption {
    pub title: String,
    pub value: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub footer: Option<String>,
    pub disabled: bool,
    pub is_header: bool,
}

impl DialogOption {
    pub fn new(title: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            value: value.into(),
            description: None,
            category: None,
            footer: None,
            disabled: false,
            is_header: false,
        }
    }

    pub fn header(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            value: String::new(),
            description: None,
            category: None,
            footer: None,
            disabled: true,
            is_header: true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct DialogAction {
    pub label: String,
    pub key_hint: String,
}

type SelectionParser = Box<dyn Fn(&str) -> DialogPayload + Send + Sync>;

pub struct Dialog {
    pub title: String,
    pub filter: String,
    pub filter_cursor_position: usize,

    pub payload: DialogPayload,

    pub current_value: String,

    pub initial_value: String,

    options: Vec<DialogOption>,
    filtered: Vec<usize>,
    matcher: SkimMatcherV2,

    parser: SelectionParser,

    pub list_state: ListState,

    pub actions: Vec<DialogAction>,
}

impl Dialog {
    pub fn new(
        title: impl Into<String>,
        payload: DialogPayload,
        options: Vec<DialogOption>,
        parser: SelectionParser,
    ) -> Self {
        let initial_value = payload.current_value();
        let mut d = Self {
            title: title.into(),
            filter: String::new(),
            filter_cursor_position: 0,
            payload,
            current_value: initial_value.clone(),
            initial_value: initial_value.clone(),
            options,
            filtered: Vec::new(),
            matcher: SkimMatcherV2::default(),
            parser,
            list_state: ListState::default(),
            actions: Vec::new(),
        };
        d.refilter();
        if let Some(pos) = d
            .filtered
            .iter()
            .position(|&i| d.options.get(i).map(|o| &o.value) == Some(&initial_value))
            .filter(|&pos| d.filtered_position_enabled(pos))
        {
            d.list_state.select(Some(pos));
            d.set_selection(initial_value);
        }
        d
    }

    pub fn prompt(
        title: impl Into<String>,
        payload: DialogPayload,
        parser: SelectionParser,
    ) -> Self {
        let initial_value = payload.current_value();
        Self {
            title: title.into(),
            filter: initial_value.clone(),
            filter_cursor_position: initial_value.len(),
            payload,
            current_value: initial_value.clone(),
            initial_value,
            options: Vec::new(),
            filtered: Vec::new(),
            matcher: SkimMatcherV2::default(),
            parser,
            list_state: ListState::default(),
            actions: Vec::new(),
        }
    }

    pub fn kind(&self) -> DialogKind {
        self.payload.kind()
    }

    fn set_selection(&mut self, value: String) {
        self.payload = (self.parser)(&value);
        self.current_value = value;
    }

    fn sync_prompt_value(&mut self) {
        if !self.is_prompt_kind() {
            return;
        }
        let value = self.filter.clone();
        match &mut self.payload {
            DialogPayload::SessionRename { title, .. } => {
                *title = value.clone();
            }
            DialogPayload::PluginInstall { path, .. } => {
                *path = value.clone();
            }
            _ => {
                self.payload = (self.parser)(&value);
            }
        }
        self.current_value = value;
    }

    fn is_prompt_kind(&self) -> bool {
        matches!(
            self.payload.kind(),
            DialogKind::SessionRename | DialogKind::PluginInstall
        )
    }

    fn filtered_position_enabled(&self, filtered_pos: usize) -> bool {
        self.filtered
            .get(filtered_pos)
            .and_then(|&option_idx| self.options.get(option_idx))
            .map(|o| !o.disabled && !o.is_header)
            .unwrap_or(false)
    }

    pub fn with_actions(mut self, actions: Vec<DialogAction>) -> Self {
        self.actions = actions;
        self
    }

    pub fn replace_options(&mut self, options: Vec<DialogOption>) {
        let preserved_value = self.current_value.clone();
        self.options = options;
        self.refilter();
        if let Some(pos) = self
            .filtered
            .iter()
            .position(|&i| {
                self.options
                    .get(i)
                    .map(|o| o.value == preserved_value)
                    .unwrap_or(false)
            })
            .filter(|&pos| self.filtered_position_enabled(pos))
        {
            self.list_state.select(Some(pos));
            self.set_selection(preserved_value);
        }
    }

    pub fn selected_option(&self) -> Option<&DialogOption> {
        let pos = self.list_state.selected()?;
        let option_idx = *self.filtered.get(pos)?;
        self.options.get(option_idx)
    }

    fn select_filtered_position(&mut self, filtered_pos: usize) {
        self.list_state.select(Some(filtered_pos));
        let value = self.options[self.filtered[filtered_pos]].value.clone();
        self.set_selection(value);
    }

    fn first_enabled_filtered_position(&self) -> Option<usize> {
        self.filtered
            .iter()
            .enumerate()
            .find_map(|(pos, &option_idx)| {
                self.options
                    .get(option_idx)
                    .filter(|o| !o.disabled)
                    .map(|_| pos)
            })
    }

    pub fn visible_options(&self) -> Vec<DialogOption> {
        self.filtered
            .iter()
            .filter_map(|&i| self.options.get(i).cloned())
            .collect()
    }

    fn clamp_filter_cursor(&mut self) {
        if self.filter_cursor_position > self.filter.len() {
            self.filter_cursor_position = self.filter.len();
        }
        while !self.filter.is_char_boundary(self.filter_cursor_position) {
            self.filter_cursor_position = self.filter_cursor_position.saturating_sub(1);
        }
    }

    pub fn insert_filter_char(&mut self, c: char) {
        self.clamp_filter_cursor();
        self.filter.insert(self.filter_cursor_position, c);
        self.filter_cursor_position += c.len_utf8();
        if self.is_prompt_kind() {
            self.sync_prompt_value();
            return;
        }
        self.refilter();
    }

    pub fn backspace_filter(&mut self) {
        self.clamp_filter_cursor();
        if self.filter_cursor_position == 0 {
            return;
        }
        self.move_filter_cursor_left();
        if self.filter_cursor_position < self.filter.len() {
            self.filter.remove(self.filter_cursor_position);
        }
        if self.is_prompt_kind() {
            self.sync_prompt_value();
            return;
        }
        self.refilter();
    }

    pub fn delete_filter_char(&mut self) {
        self.clamp_filter_cursor();
        if self.filter_cursor_position >= self.filter.len() {
            return;
        }
        self.filter.remove(self.filter_cursor_position);
        if self.is_prompt_kind() {
            self.sync_prompt_value();
            return;
        }
        self.refilter();
    }

    pub fn move_filter_cursor_left(&mut self) {
        self.clamp_filter_cursor();
        if self.filter_cursor_position == 0 {
            return;
        }
        let mut p = self.filter_cursor_position - 1;
        while p > 0 && !self.filter.is_char_boundary(p) {
            p -= 1;
        }
        self.filter_cursor_position = p;
    }

    pub fn move_filter_cursor_right(&mut self) {
        self.clamp_filter_cursor();
        if self.filter_cursor_position >= self.filter.len() {
            return;
        }
        let mut p = self.filter_cursor_position + 1;
        while p < self.filter.len() && !self.filter.is_char_boundary(p) {
            p += 1;
        }
        self.filter_cursor_position = p;
    }

    pub fn refilter(&mut self) {
        self.clamp_filter_cursor();
        if self.filter.is_empty() {
            self.filtered = (0..self.options.len()).collect();
        } else {
            let mut scored: Vec<(i64, usize)> = self
                .options
                .iter()
                .enumerate()
                .filter(|(_, o)| !o.is_header)
                .filter_map(|(i, o)| {
                    let haystack = match &o.category {
                        Some(c) if !c.is_empty() => format!("{} {}", o.title, c),
                        _ => o.title.clone(),
                    };
                    self.matcher
                        .fuzzy_match(&haystack, &self.filter)
                        .map(|s| (s, i))
                })
                .collect();
            scored.sort_by(|a, b| b.0.cmp(&a.0));
            self.filtered = scored.into_iter().map(|(_, i)| i).collect();
        }
        if self.filtered.is_empty() {
            self.list_state.select(None);
        } else if let Some(pos) = self.first_enabled_filtered_position() {
            self.select_filtered_position(pos);
        } else {
            self.list_state.select(None);
        }
    }

    pub fn move_next(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len();
        let start = self.list_state.selected().unwrap_or(len.saturating_sub(1));
        for offset in 1..=len {
            let i = (start + offset) % len;
            if self.filtered_position_enabled(i) {
                self.select_filtered_position(i);
                return;
            }
        }
        self.list_state.select(None);
    }

    pub fn move_prev(&mut self) {
        if self.filtered.is_empty() {
            return;
        }
        let len = self.filtered.len();
        let start = self.list_state.selected().unwrap_or(0);
        for offset in 1..=len {
            let i = (start + len - offset) % len;
            if self.filtered_position_enabled(i) {
                self.select_filtered_position(i);
                return;
            }
        }
        self.list_state.select(None);
    }
}
