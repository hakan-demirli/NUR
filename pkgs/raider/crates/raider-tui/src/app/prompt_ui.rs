use super::builtin::PromptInfo;

pub struct PromptUiState {
    pub prompt_info: PromptInfo,

    pub prompt_placeholders: Vec<String>,
    pub prompt_placeholder_index: usize,
}

impl PromptUiState {
    pub fn new() -> Self {
        Self {
            prompt_info: PromptInfo::default(),
            prompt_placeholders: default_placeholders(),
            prompt_placeholder_index: pick_initial_placeholder_index(),
        }
    }

    pub fn set_footer_right(&mut self, right: Option<String>) {
        self.prompt_info.right = right;
    }

    pub fn set_usage(&mut self, usage: Option<String>) {
        self.prompt_info.usage = usage;
    }

    pub fn set_build_label(&mut self, label: Option<String>) {
        self.prompt_info.build_label = label;
    }

    pub fn set_busy(&mut self, busy: bool) {
        self.prompt_info.busy = busy;
    }

    pub fn set_placeholders(&mut self, placeholders: Vec<String>) {
        self.prompt_placeholders = placeholders;
        self.prompt_placeholder_index = if self.prompt_placeholders.is_empty() {
            0
        } else {
            pick_initial_placeholder_index() % self.prompt_placeholders.len()
        };
    }

    pub fn current_placeholder(&self) -> Option<&str> {
        if self.prompt_placeholders.is_empty() {
            return None;
        }
        let i = self.prompt_placeholder_index % self.prompt_placeholders.len();
        Some(self.prompt_placeholders[i].as_str())
    }

    pub fn cycle_placeholder(&mut self) {
        if self.prompt_placeholders.is_empty() {
            return;
        }
        self.prompt_placeholder_index =
            (self.prompt_placeholder_index + 1) % self.prompt_placeholders.len();
    }
}

impl Default for PromptUiState {
    fn default() -> Self {
        Self::new()
    }
}

fn default_placeholders() -> Vec<String> {
    [
        "Fix a TODO in the codebase",
        "What is the tech stack of this project?",
        "Fix broken tests",
    ]
    .into_iter()
    .map(String::from)
    .collect()
}

fn pick_initial_placeholder_index() -> usize {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as usize)
        .unwrap_or(0)
}
