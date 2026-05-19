use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::widgets::ListState;

#[derive(Clone, Debug)]
pub struct CompletionMatch {
    pub text: String,
    pub description: String,
    pub score: i64,
    pub indices: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct SlashEntry {
    pub slash: String,
    pub description: String,
}

impl SlashEntry {
    pub fn new(slash: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            slash: slash.into(),
            description: description.into(),
        }
    }
}

#[derive(PartialEq, Debug)]
pub enum CompletionMode {
    Command,
    File,
    Model,
    Variant,
    Inactive,
}

pub struct CompletionManager {
    pub active: bool,
    pub mode: CompletionMode,
    pub candidates: Vec<CompletionMatch>,
    pub state: ListState,

    files: Vec<String>,
    models: Vec<String>,
    variants: Vec<String>,
    commands: Vec<SlashEntry>,
    matcher: SkimMatcherV2,
}

impl Default for CompletionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CompletionManager {
    pub fn new() -> Self {
        Self {
            active: false,
            mode: CompletionMode::Inactive,
            candidates: Vec::new(),
            state: ListState::default(),
            files: Vec::new(),
            models: Vec::new(),
            variants: Vec::new(),
            matcher: SkimMatcherV2::default(),
            commands: default_commands(),
        }
    }

    pub fn set_commands(&mut self, commands: Vec<SlashEntry>) {
        self.commands = commands;
    }

    pub fn set_files(&mut self, files: Vec<String>) {
        self.files = files;
    }

    pub fn set_models(&mut self, models: Vec<String>) {
        self.models = models;
    }

    pub fn set_variants(&mut self, variants: Vec<String>) {
        self.variants = variants;
    }

    pub fn update(&mut self, input: &str) {
        if input.is_empty() {
            self.active = false;
            return;
        }

        let (mode, query) = if input.starts_with('/') {
            if let Some((cmd, args)) = input.split_once(' ') {
                match cmd {
                    "/add" | "/open" => (CompletionMode::File, args.trim()),
                    "/model" => (CompletionMode::Model, args.trim()),
                    "/variant" => (CompletionMode::Variant, args.trim()),
                    _ => (CompletionMode::Inactive, ""),
                }
            } else {
                (CompletionMode::Command, input)
            }
        } else {
            (CompletionMode::Inactive, "")
        };

        if mode == CompletionMode::Inactive {
            self.active = false;
            return;
        }

        self.mode = mode;
        self.candidates.clear();

        match self.mode {
            CompletionMode::Command => {
                self.match_commands(query);
            }
            CompletionMode::File => {
                self.match_strings(query, &self.files.clone());
                self.candidates.sort_by(|a, b| b.score.cmp(&a.score));
            }
            CompletionMode::Model => {
                self.match_strings(query, &self.models.clone());
                self.candidates.sort_by(|a, b| b.score.cmp(&a.score));
            }
            CompletionMode::Variant => {
                self.match_strings(query, &self.variants.clone());
                self.candidates.sort_by(|a, b| b.score.cmp(&a.score));
            }
            CompletionMode::Inactive => {}
        }

        if self.candidates.is_empty() {
            self.active = false;
        } else {
            self.active = true;
            self.state.select(Some(0));
        }
    }

    fn match_commands(&mut self, query: &str) {
        let q = query.trim_start_matches('/');
        for entry in &self.commands {
            let (score, idxs) = if q.is_empty() {
                (1, Vec::new())
            } else {
                match self.matcher.fuzzy_indices(&entry.slash, query) {
                    Some((s, idx)) if s > 0 => (s, idx),
                    _ => continue,
                }
            };
            self.candidates.push(CompletionMatch {
                text: entry.slash.clone(),
                description: entry.description.clone(),
                score,
                indices: idxs,
            });
        }
        self.candidates.sort_by(|a, b| {
            a.text
                .to_ascii_lowercase()
                .cmp(&b.text.to_ascii_lowercase())
        });
    }

    fn match_strings(&mut self, query: &str, source: &[String]) {
        for item in source {
            if let Some(score) = self.matcher.fuzzy_match(item, query) {
                if score <= 0 {
                    continue;
                }
                let idxs = self
                    .matcher
                    .fuzzy_indices(item, query)
                    .map(|x| x.1)
                    .unwrap_or_default();
                self.candidates.push(CompletionMatch {
                    text: item.clone(),
                    description: String::new(),
                    score,
                    indices: idxs,
                });
            }
        }
    }

    pub fn next(&mut self) {
        if !self.active || self.candidates.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(i) if i + 1 < self.candidates.len() => i + 1,
            Some(_) => 0,
            None => 0,
        };
        self.state.select(Some(i));
    }

    pub fn previous(&mut self) {
        if !self.active || self.candidates.is_empty() {
            return;
        }
        let i = match self.state.selected() {
            Some(0) | None => self.candidates.len() - 1,
            Some(i) => i - 1,
        };
        self.state.select(Some(i));
    }

    pub fn confirm(&mut self, current_input: &str) -> Option<String> {
        if !self.active {
            return None;
        }
        let idx = self.state.selected()?;
        self.replacement_for(current_input, idx)
    }

    pub fn top_replacement(&self, current_input: &str) -> Option<String> {
        if !self.active || self.candidates.is_empty() {
            return None;
        }
        self.replacement_for(current_input, 0)
    }

    pub fn input_matches_top(&self, current_input: &str) -> bool {
        if !self.active {
            return false;
        }
        let Some(top) = self.candidates.first() else {
            return false;
        };
        match self.mode {
            CompletionMode::Command => current_input == top.text,
            CompletionMode::File | CompletionMode::Model | CompletionMode::Variant => current_input
                .split_once(' ')
                .map(|(_, arg)| arg.trim() == top.text)
                .unwrap_or(false),
            CompletionMode::Inactive => false,
        }
    }

    fn replacement_for(&self, current_input: &str, idx: usize) -> Option<String> {
        let selection = self.candidates.get(idx)?.text.clone();
        match self.mode {
            CompletionMode::Command => Some(selection),
            CompletionMode::File | CompletionMode::Model | CompletionMode::Variant => {
                let (cmd, _) = current_input.split_once(' ')?;
                Some(format!("{} {}", cmd, selection))
            }
            CompletionMode::Inactive => None,
        }
    }
}

fn default_commands() -> Vec<SlashEntry> {
    [
        ("/agents", "Switch agent"),
        ("/exit", "Exit the app"),
        ("/help", "Show commands"),
        ("/models", "Switch model"),
        ("/sessions", "Switch session"),
        ("/themes", "Switch theme"),
        ("/variants", "Switch model variant"),
    ]
    .into_iter()
    .map(|(s, d)| SlashEntry::new(s, d))
    .collect()
}
