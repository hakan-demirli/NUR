use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};

use crate::ui::theme::{self, Mode as ThemeMode, Theme, ThemeName, ThemeRegistry, DEFAULT_THEME};

pub struct ThemeState {
    pub ps: SyntaxSet,
    pub ts: ThemeSet,
    pub theme: Theme,
    pub synth_theme: syntect::highlighting::Theme,
    pub theme_registry: ThemeRegistry,
    pub theme_before_preview: Option<ThemeName>,
}

impl ThemeState {
    pub fn new() -> Self {
        let theme_registry = ThemeRegistry::new();
        let theme = theme_registry.get(&default_theme_name(&theme_registry));
        Self::from_registry(theme_registry, theme)
    }

    pub fn with_user_themes() -> Self {
        let theme_registry = ThemeRegistry::with_user_themes();
        let theme = theme_registry.get(&default_theme_name(&theme_registry));
        Self::from_registry(theme_registry, theme)
    }

    pub fn with_user_themes_and_mode(mode: ThemeMode) -> Self {
        let mut theme_registry = ThemeRegistry::with_user_themes();
        theme_registry.set_mode(mode);
        let theme = theme_registry.get(&default_theme_name(&theme_registry));
        Self::from_registry(theme_registry, theme)
    }

    pub fn with_user_themes_and_detection(
        theme_name: Option<&str>,
        mode: Option<ThemeMode>,
    ) -> Self {
        let mut theme_registry = ThemeRegistry::with_user_themes();
        if let Some(m) = mode {
            theme_registry.set_mode(m);
        }
        let picked: ThemeName = theme_name
            .and_then(|n| theme_registry.lookup(n))
            .unwrap_or_else(|| default_theme_name(&theme_registry));
        let theme = theme_registry.get(&picked);
        Self::from_registry(theme_registry, theme)
    }

    fn from_registry(theme_registry: ThemeRegistry, theme: Theme) -> Self {
        let synth_theme = theme::syntect_theme(&theme);
        Self {
            ps: two_face::syntax::extra_newlines(),
            ts: ThemeSet::load_defaults(),
            theme,
            synth_theme,
            theme_registry,
            theme_before_preview: None,
        }
    }

    pub fn lookup(&self, name: &str) -> Option<ThemeName> {
        self.theme_registry.lookup(name)
    }

    pub fn apply_theme_name(&mut self, name: ThemeName) -> bool {
        if self.theme.name == name {
            return false;
        }
        self.theme = self.theme_registry.get(&name);
        self.synth_theme = theme::syntect_theme(&self.theme);
        true
    }

    pub fn mode(&self) -> ThemeMode {
        self.theme_registry.mode()
    }

    pub fn toggle_mode(&mut self) -> ThemeMode {
        let next = match self.mode() {
            ThemeMode::Dark => ThemeMode::Light,
            ThemeMode::Light => ThemeMode::Dark,
        };
        self.set_mode(next);
        next
    }

    pub fn set_mode(&mut self, mode: ThemeMode) -> bool {
        if self.mode() == mode {
            return false;
        }
        self.theme_registry.set_mode(mode);
        let name = self.theme.name.clone();
        self.theme = self.theme_registry.get(&name);
        self.synth_theme = theme::syntect_theme(&self.theme);
        true
    }

    pub fn snapshot_for_preview(&mut self) {
        self.theme_before_preview = Some(self.theme.name.clone());
    }

    pub fn clear_preview_snapshot(&mut self) {
        self.theme_before_preview = None;
    }

    pub fn restore_preview(&mut self) -> Option<ThemeName> {
        let prev = self.theme_before_preview.take()?;
        if self.theme.name != prev {
            self.apply_theme_name(prev.clone());
            return Some(prev);
        }
        None
    }
}

impl Default for ThemeState {
    fn default() -> Self {
        Self::new()
    }
}

pub fn default_theme_name(reg: &ThemeRegistry) -> ThemeName {
    if let Some(n) = reg.lookup(DEFAULT_THEME) {
        return n;
    }
    if let Some(first) = reg.names().into_iter().next() {
        if let Some(n) = reg.lookup(&first) {
            return n;
        }
    }
    ThemeName::opencode_default()
}
