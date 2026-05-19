use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use ratatui::style::Color;
use serde::Deserialize;

pub const BUNDLED_THEMES: &[(&str, &str)] = &[
    ("aura", include_str!("../../themes/aura.json")),
    ("ayu", include_str!("../../themes/ayu.json")),
    ("carbonfox", include_str!("../../themes/carbonfox.json")),
    ("catppuccin", include_str!("../../themes/catppuccin.json")),
    (
        "catppuccin-frappe",
        include_str!("../../themes/catppuccin-frappe.json"),
    ),
    (
        "catppuccin-macchiato",
        include_str!("../../themes/catppuccin-macchiato.json"),
    ),
    ("cobalt2", include_str!("../../themes/cobalt2.json")),
    ("cursor", include_str!("../../themes/cursor.json")),
    ("dracula", include_str!("../../themes/dracula.json")),
    ("everforest", include_str!("../../themes/everforest.json")),
    ("flexoki", include_str!("../../themes/flexoki.json")),
    ("github", include_str!("../../themes/github.json")),
    ("gruvbox", include_str!("../../themes/gruvbox.json")),
    ("kanagawa", include_str!("../../themes/kanagawa.json")),
    ("lucent-orng", include_str!("../../themes/lucent-orng.json")),
    ("material", include_str!("../../themes/material.json")),
    ("matrix", include_str!("../../themes/matrix.json")),
    ("mercury", include_str!("../../themes/mercury.json")),
    ("monokai", include_str!("../../themes/monokai.json")),
    ("nightowl", include_str!("../../themes/nightowl.json")),
    ("nord", include_str!("../../themes/nord.json")),
    ("one-dark", include_str!("../../themes/one-dark.json")),
    ("opencode", include_str!("../../themes/opencode.json")),
    ("orng", include_str!("../../themes/orng.json")),
    ("osaka-jade", include_str!("../../themes/osaka-jade.json")),
    ("palenight", include_str!("../../themes/palenight.json")),
    ("rosepine", include_str!("../../themes/rosepine.json")),
    ("solarized", include_str!("../../themes/solarized.json")),
    ("synthwave84", include_str!("../../themes/synthwave84.json")),
    ("system", include_str!("../../themes/system.json")),
    ("tokyonight", include_str!("../../themes/tokyonight.json")),
    ("vercel", include_str!("../../themes/vercel.json")),
    ("vesper", include_str!("../../themes/vesper.json")),
    ("zenburn", include_str!("../../themes/zenburn.json")),
];

pub const DEFAULT_THEME: &str = "opencode";

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum ColorValue {
    Plain(String),
    Variant {
        dark: Box<ColorValue>,
        light: Box<ColorValue>,
    },
    Ansi(u32),
}

#[derive(Debug, Clone, Deserialize)]
pub struct ThemeJson {
    #[serde(default)]
    pub defs: HashMap<String, ColorValue>,
    pub theme: HashMap<String, ColorValue>,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct ThemeName(String);

impl ThemeName {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn opencode_default() -> Self {
        Self(DEFAULT_THEME.to_string())
    }

    pub(crate) fn opencode_default_with(name: &str) -> Self {
        Self(name.to_string())
    }
}

impl std::fmt::Display for ThemeName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl AsRef<str> for ThemeName {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl std::ops::Deref for ThemeName {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl PartialEq<str> for ThemeName {
    fn eq(&self, other: &str) -> bool {
        self.0 == other
    }
}

impl PartialEq<&str> for ThemeName {
    fn eq(&self, other: &&str) -> bool {
        self.0 == *other
    }
}

impl PartialEq<String> for ThemeName {
    fn eq(&self, other: &String) -> bool {
        &self.0 == other
    }
}

impl PartialEq<ThemeName> for str {
    fn eq(&self, other: &ThemeName) -> bool {
        self == other.0.as_str()
    }
}

impl PartialEq<ThemeName> for &str {
    fn eq(&self, other: &ThemeName) -> bool {
        *self == other.0.as_str()
    }
}

impl PartialEq<ThemeName> for String {
    fn eq(&self, other: &ThemeName) -> bool {
        self == &other.0
    }
}

#[derive(Clone, Debug)]
pub struct Theme {
    pub name: ThemeName,
    pub mode: Mode,
    pub thinking_opacity: f32,

    pub primary: Color,
    pub secondary: Color,
    pub accent: Color,
    pub error: Color,
    pub warning: Color,
    pub success: Color,
    pub info: Color,

    pub text: Color,
    pub text_muted: Color,
    pub selected_list_item_text: Color,

    pub background: Color,
    pub background_panel: Color,
    pub background_element: Color,
    pub background_menu: Color,

    pub border: Color,
    pub border_active: Color,
    pub border_subtle: Color,

    pub diff_added: Color,
    pub diff_removed: Color,
    pub diff_context: Color,
    pub diff_hunk_header: Color,
    pub diff_highlight_added: Color,
    pub diff_highlight_removed: Color,
    pub diff_added_bg: Color,
    pub diff_removed_bg: Color,
    pub diff_context_bg: Color,
    pub diff_line_number: Color,
    pub diff_added_line_number_bg: Color,
    pub diff_removed_line_number_bg: Color,

    pub markdown_text: Color,
    pub markdown_heading: Color,
    pub markdown_link: Color,
    pub markdown_link_text: Color,
    pub markdown_code: Color,
    pub markdown_block_quote: Color,
    pub markdown_emph: Color,
    pub markdown_strong: Color,
    pub markdown_horizontal_rule: Color,
    pub markdown_list_item: Color,
    pub markdown_list_enumeration: Color,
    pub markdown_image: Color,
    pub markdown_image_text: Color,
    pub markdown_code_block: Color,

    pub syntax_comment: Color,
    pub syntax_keyword: Color,
    pub syntax_function: Color,
    pub syntax_variable: Color,
    pub syntax_string: Color,
    pub syntax_number: Color,
    pub syntax_type: Color,
    pub syntax_operator: Color,
    pub syntax_punctuation: Color,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Mode {
    Dark,
    Light,
}

impl Default for Theme {
    fn default() -> Self {
        let json: ThemeJson = serde_json::from_str(
            BUNDLED_THEMES
                .iter()
                .find(|(n, _)| *n == DEFAULT_THEME)
                .map(|(_, src)| *src)
                .expect("opencode theme bundled"),
        )
        .expect("opencode theme parses");
        resolve(ThemeName::opencode_default(), &json, Mode::Dark).expect("opencode theme resolves")
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ThemeError {
    #[error("missing required color key: {0}")]
    Missing(&'static str),
    #[error("invalid hex color: {0}")]
    Hex(String),
    #[error("circular color reference: {0}")]
    Circular(String),
    #[error("unresolvable reference: {0}")]
    Unresolved(String),
}

pub fn resolve(name: ThemeName, json: &ThemeJson, mode: Mode) -> Result<Theme, ThemeError> {
    let ctx = ResolveCtx {
        defs: &json.defs,
        theme: &json.theme,
        mode,
    };

    let required = |key: &'static str| -> Result<Color, ThemeError> {
        let v = json.theme.get(key).ok_or(ThemeError::Missing(key))?;
        ctx.color(v, &mut Vec::new())
    };

    let primary = required("primary")?;
    let secondary = required("secondary")?;
    let accent = required("accent")?;
    let error = required("error")?;
    let warning = required("warning")?;
    let success = required("success")?;
    let info = required("info")?;
    let text = required("text")?;
    let text_muted = required("textMuted")?;
    let background = required("background")?;
    let background_panel = required("backgroundPanel")?;
    let background_element = required("backgroundElement")?;
    let border = required("border")?;
    let border_active = required("borderActive")?;
    let border_subtle = required("borderSubtle")?;

    let optional = |key: &str, fallback: Color| -> Result<Color, ThemeError> {
        match json.theme.get(key) {
            Some(v) => ctx.color(v, &mut Vec::new()),
            None => Ok(fallback),
        }
    };

    let selected_list_item_text = optional("selectedListItemText", background)?;
    let background_menu = optional("backgroundMenu", background_element)?;

    let thinking_opacity = json
        .theme
        .get("thinkingOpacity")
        .and_then(|v| match v {
            ColorValue::Ansi(n) => Some(*n as f32),
            _ => None,
        })
        .unwrap_or(0.6);

    let m = |key: &str| -> Result<Color, ThemeError> { optional(key, text) };

    Ok(Theme {
        name,
        mode,
        thinking_opacity,
        primary,
        secondary,
        accent,
        error,
        warning,
        success,
        info,
        text,
        text_muted,
        selected_list_item_text,
        background,
        background_panel,
        background_element,
        background_menu,
        border,
        border_active,
        border_subtle,
        diff_added: m("diffAdded")?,
        diff_removed: m("diffRemoved")?,
        diff_context: m("diffContext")?,
        diff_hunk_header: m("diffHunkHeader")?,
        diff_highlight_added: m("diffHighlightAdded")?,
        diff_highlight_removed: m("diffHighlightRemoved")?,
        diff_added_bg: optional("diffAddedBg", background_panel)?,
        diff_removed_bg: optional("diffRemovedBg", background_panel)?,
        diff_context_bg: optional("diffContextBg", background_panel)?,
        diff_line_number: optional("diffLineNumber", text_muted)?,
        diff_added_line_number_bg: optional("diffAddedLineNumberBg", background_panel)?,
        diff_removed_line_number_bg: optional("diffRemovedLineNumberBg", background_panel)?,
        markdown_text: m("markdownText")?,
        markdown_heading: m("markdownHeading")?,
        markdown_link: m("markdownLink")?,
        markdown_link_text: m("markdownLinkText")?,
        markdown_code: m("markdownCode")?,
        markdown_block_quote: m("markdownBlockQuote")?,
        markdown_emph: m("markdownEmph")?,
        markdown_strong: m("markdownStrong")?,
        markdown_horizontal_rule: m("markdownHorizontalRule")?,
        markdown_list_item: m("markdownListItem")?,
        markdown_list_enumeration: m("markdownListEnumeration")?,
        markdown_image: m("markdownImage")?,
        markdown_image_text: m("markdownImageText")?,
        markdown_code_block: m("markdownCodeBlock")?,
        syntax_comment: m("syntaxComment")?,
        syntax_keyword: m("syntaxKeyword")?,
        syntax_function: m("syntaxFunction")?,
        syntax_variable: m("syntaxVariable")?,
        syntax_string: m("syntaxString")?,
        syntax_number: m("syntaxNumber")?,
        syntax_type: m("syntaxType")?,
        syntax_operator: m("syntaxOperator")?,
        syntax_punctuation: m("syntaxPunctuation")?,
    })
}

struct ResolveCtx<'a> {
    defs: &'a HashMap<String, ColorValue>,
    theme: &'a HashMap<String, ColorValue>,
    mode: Mode,
}

impl<'a> ResolveCtx<'a> {
    fn color(&self, value: &ColorValue, chain: &mut Vec<String>) -> Result<Color, ThemeError> {
        match value {
            ColorValue::Plain(s) => self.resolve_string(s, chain),
            ColorValue::Variant { dark, light } => {
                let pick = match self.mode {
                    Mode::Dark => dark,
                    Mode::Light => light,
                };
                self.color(pick, chain)
            }
            ColorValue::Ansi(n) => Ok(ansi_to_color(*n)),
        }
    }

    fn resolve_string(&self, s: &str, chain: &mut Vec<String>) -> Result<Color, ThemeError> {
        if s == "transparent" || s == "none" {
            return Ok(Color::Reset);
        }
        if let Some(stripped) = s.strip_prefix('#') {
            return parse_hex(stripped).ok_or_else(|| ThemeError::Hex(s.to_string()));
        }
        if chain.iter().any(|c| c == s) {
            chain.push(s.to_string());
            return Err(ThemeError::Circular(chain.join(" -> ")));
        }
        chain.push(s.to_string());
        let next = self
            .defs
            .get(s)
            .or_else(|| self.theme.get(s))
            .ok_or_else(|| ThemeError::Unresolved(s.to_string()))?;
        let out = self.color(next, chain);
        chain.pop();
        out
    }
}

fn parse_hex(s: &str) -> Option<Color> {
    let bytes = s.as_bytes();
    match bytes.len() {
        3 => {
            let r = (u8::from_str_radix(&s[0..1], 16).ok()?) * 17;
            let g = (u8::from_str_radix(&s[1..2], 16).ok()?) * 17;
            let b = (u8::from_str_radix(&s[2..3], 16).ok()?) * 17;
            Some(Color::Rgb(r, g, b))
        }
        6 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            Some(Color::Rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            let a = u8::from_str_radix(&s[6..8], 16).ok()?;
            if a == 0 {
                Some(Color::Reset)
            } else {
                Some(Color::Rgb(r, g, b))
            }
        }
        _ => None,
    }
}

pub(crate) fn ratatui_to_syntect_color(c: Color) -> syntect::highlighting::Color {
    use syntect::highlighting::Color as SynColor;
    let (r, g, b) = match c {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Black => (0, 0, 0),
        Color::Red => (170, 0, 0),
        Color::Green => (0, 170, 0),
        Color::Yellow => (170, 85, 0),
        Color::Blue => (0, 0, 170),
        Color::Magenta => (170, 0, 170),
        Color::Cyan => (0, 170, 170),
        Color::Gray => (170, 170, 170),
        Color::DarkGray => (85, 85, 85),
        Color::LightRed => (255, 85, 85),
        Color::LightGreen => (85, 255, 85),
        Color::LightYellow => (255, 255, 85),
        Color::LightBlue => (85, 85, 255),
        Color::LightMagenta => (255, 85, 255),
        Color::LightCyan => (85, 255, 255),
        Color::White => (255, 255, 255),
        Color::Indexed(n) if (16..=231).contains(&n) => {
            let i = (n - 16) as u32;
            let r = (i / 36) % 6;
            let g = (i / 6) % 6;
            let b = i % 6;
            let scale = |v: u32| -> u8 {
                if v == 0 {
                    0
                } else {
                    (v * 40 + 55) as u8
                }
            };
            (scale(r), scale(g), scale(b))
        }
        Color::Indexed(n) if n >= 232 => {
            let v = ((n - 232) as u32) * 10 + 8;
            (v as u8, v as u8, v as u8)
        }
        Color::Indexed(n) => {
            return ratatui_to_syntect_color(ansi_to_color(n as u32));
        }
        Color::Reset => (255, 255, 255),
    };
    SynColor { r, g, b, a: 255 }
}

pub fn syntect_theme(theme: &Theme) -> syntect::highlighting::Theme {
    use std::str::FromStr;
    use syntect::highlighting::{
        ScopeSelectors, StyleModifier, Theme as SynTheme, ThemeItem, ThemeSettings,
    };

    let mk = |scope_str: &str, fg: Color| -> ThemeItem {
        ThemeItem {
            scope: ScopeSelectors::from_str(scope_str).unwrap_or_default(),
            style: StyleModifier {
                foreground: Some(ratatui_to_syntect_color(fg)),
                background: None,
                font_style: None,
            },
        }
    };

    let settings = ThemeSettings {
        foreground: Some(ratatui_to_syntect_color(theme.text)),
        background: Some(ratatui_to_syntect_color(theme.background)),
        ..ThemeSettings::default()
    };

    SynTheme {
        name: Some(format!("raider-{:?}", theme.mode)),
        author: Some("raider".into()),
        settings,
        scopes: vec![
            mk("comment", theme.syntax_comment),
            mk("string", theme.syntax_string),
            mk("constant.numeric, constant.language", theme.syntax_number),
            mk(
                "keyword, keyword.control, storage.modifier",
                theme.syntax_keyword,
            ),
            mk("keyword.operator", theme.syntax_operator),
            mk(
                "entity.name.function, support.function, meta.function-call",
                theme.syntax_function,
            ),
            mk(
                "entity.name.type, entity.name.class, support.type, support.class, storage.type",
                theme.syntax_type,
            ),
            mk("variable, variable.parameter", theme.syntax_variable),
            mk("punctuation", theme.syntax_punctuation),
        ],
    }
}

fn ansi_to_color(code: u32) -> Color {
    if code < 16 {
        match code {
            0 => Color::Black,
            1 => Color::Red,
            2 => Color::Green,
            3 => Color::Yellow,
            4 => Color::Blue,
            5 => Color::Magenta,
            6 => Color::Cyan,
            7 => Color::Gray,
            8 => Color::DarkGray,
            9 => Color::LightRed,
            10 => Color::LightGreen,
            11 => Color::LightYellow,
            12 => Color::LightBlue,
            13 => Color::LightMagenta,
            14 => Color::LightCyan,
            15 => Color::White,
            _ => Color::Reset,
        }
    } else {
        Color::Indexed(code as u8)
    }
}

pub struct ThemeRegistry {
    sources: HashMap<String, ThemeJson>,
    mode: Mode,
}

impl Default for ThemeRegistry {
    fn default() -> Self {
        let mut sources = HashMap::new();
        for (name, src) in BUNDLED_THEMES {
            match serde_json::from_str::<ThemeJson>(src) {
                Ok(j) => {
                    sources.insert(name.to_string(), j);
                }
                Err(e) => {
                    eprintln!("raider: failed to parse bundled theme {name}: {e}");
                }
            }
        }
        Self {
            sources,
            mode: Mode::Dark,
        }
    }
}

impl ThemeRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_user_themes() -> Self {
        let mut reg = Self::new();
        for dir in user_theme_dirs() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    if let Ok(text) = fs::read_to_string(&path) {
                        match serde_json::from_str::<ThemeJson>(&text) {
                            Ok(json) => {
                                reg.sources.insert(name.to_string(), json);
                            }
                            Err(e) => {
                                eprintln!("raider: skipping invalid theme {path:?}: {e}");
                            }
                        }
                    }
                }
            }
        }
        reg
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
    }

    pub fn names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.sources.keys().cloned().collect();
        v.sort_by_key(|a| a.to_lowercase());
        v
    }

    pub fn lookup(&self, name: &str) -> Option<ThemeName> {
        if self.sources.contains_key(name) {
            Some(ThemeName(name.to_string()))
        } else {
            None
        }
    }

    pub fn get(&self, name: &ThemeName) -> Theme {
        match self.sources.get(name.as_str()) {
            Some(j) => resolve(name.clone(), j, self.mode).unwrap_or_default(),
            None => Theme::default(),
        }
    }

    pub fn upsert(&mut self, name: impl Into<String>, json: ThemeJson) {
        self.sources.insert(name.into(), json);
    }
}

fn user_theme_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        dirs.push(PathBuf::from(xdg).join("raider").join("themes"));
    } else if let Some(home) = std::env::var_os("HOME") {
        dirs.push(
            PathBuf::from(home)
                .join(".config")
                .join("raider")
                .join("themes"),
        );
    }
    if let Ok(cwd) = std::env::current_dir() {
        dirs.push(cwd.join(".raider").join("themes"));
    }
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw_name(s: &str) -> ThemeName {
        ThemeName(s.to_string())
    }

    #[test]
    fn all_bundled_themes_parse_and_resolve_dark_and_light() {
        for (name, src) in BUNDLED_THEMES {
            let json: ThemeJson =
                serde_json::from_str(src).unwrap_or_else(|e| panic!("{name} parse: {e}"));
            resolve(raw_name(name), &json, Mode::Dark)
                .unwrap_or_else(|e| panic!("{name} dark: {e}"));
            resolve(raw_name(name), &json, Mode::Light)
                .unwrap_or_else(|e| panic!("{name} light: {e}"));
        }
    }

    #[test]
    fn dracula_dark_known_colors() {
        let reg = ThemeRegistry::new();
        let name = reg.lookup("dracula").expect("bundled");
        let t = reg.get(&name);
        assert_eq!(t.primary, Color::Rgb(0xbd, 0x93, 0xf9));
        assert_eq!(t.background, Color::Rgb(0x28, 0x2a, 0x36));
        assert_eq!(t.secondary, Color::Rgb(0xff, 0x79, 0xc6));
        assert_eq!(t.text_muted, Color::Rgb(0x62, 0x72, 0xa4));
    }

    #[test]
    fn registry_lists_at_least_the_bundled_set() {
        let reg = ThemeRegistry::new();
        let names = reg.names();
        assert!(names.contains(&"dracula".to_string()));
        assert!(names.contains(&"opencode".to_string()));
        assert!(names.contains(&"tokyonight".to_string()));
        assert_eq!(names.len(), BUNDLED_THEMES.len());
    }

    #[test]
    fn lookup_rejects_unknown_name_so_typestate_is_unforgeable() {
        let reg = ThemeRegistry::new();
        assert!(reg.lookup("nonsense-xyz").is_none());
        assert!(reg.lookup("dracula").is_some());
    }

    #[test]
    fn circular_reference_errors() {
        let src = r#"{"defs":{"a":"b","b":"a"},"theme":{
            "primary":"a","secondary":"a","accent":"a","error":"a","warning":"a",
            "success":"a","info":"a","text":"a","textMuted":"a","background":"a",
            "backgroundPanel":"a","backgroundElement":"a","border":"a",
            "borderActive":"a","borderSubtle":"a"
        }}"#;
        let json: ThemeJson = serde_json::from_str(src).unwrap();
        let err = resolve(raw_name("circ"), &json, Mode::Dark).unwrap_err();
        assert!(matches!(err, ThemeError::Circular(_)));
    }

    #[test]
    fn missing_required_key_errors() {
        let src = "{\"theme\":{\"primary\":\"#ffffff\"}}";
        let json: ThemeJson = serde_json::from_str(src).unwrap();
        let err = resolve(raw_name("partial"), &json, Mode::Dark).unwrap_err();
        assert!(matches!(err, ThemeError::Missing(_)));
    }

    #[test]
    fn syntect_theme_uses_raider_palette_for_keyword_scope() {
        use std::str::FromStr;
        use syntect::highlighting::Color as SynColor;
        use syntect::parsing::ScopeStack;
        let reg = ThemeRegistry::new();
        let name = reg.lookup("dracula").expect("bundled dracula");
        let t = reg.get(&name);
        let expected_keyword = SynColor {
            r: 0xff,
            g: 0x79,
            b: 0xc6,
            a: 255,
        };
        assert_eq!(
            ratatui_to_syntect_color(t.syntax_keyword),
            expected_keyword,
            "Dracula syntax_keyword must round-trip to syntect Color",
        );
        let synth = syntect_theme(&t);
        let kw_scope = ScopeStack::from_str("keyword").expect("keyword is a valid TextMate scope");
        let mut hit_fg: Option<SynColor> = None;
        for item in &synth.scopes {
            if item.scope.does_match(kw_scope.as_slice()).is_some() {
                if let Some(fg) = item.style.foreground {
                    hit_fg = Some(fg);
                    break;
                }
            }
        }
        assert_eq!(
            hit_fg,
            Some(expected_keyword),
            "syntect_theme(dracula) must paint `keyword` tokens with \
             Dracula's syntax_keyword (#ff79c6); got {hit_fg:?}",
        );
    }

    #[test]
    fn ratatui_to_syntect_color_handles_indexed_palette() {
        use syntect::highlighting::Color as SynColor;
        assert_eq!(
            ratatui_to_syntect_color(Color::Indexed(1)),
            SynColor {
                r: 170,
                g: 0,
                b: 0,
                a: 255
            },
            "indexed=1 must equal Color::Red triple",
        );
        let red = ratatui_to_syntect_color(Color::Indexed(196));
        assert_eq!(red.r, 255);
        assert_eq!(red.g, 0);
        assert_eq!(red.b, 0);
        let gray = ratatui_to_syntect_color(Color::Indexed(244));
        assert_eq!(gray.r, gray.g);
        assert_eq!(gray.g, gray.b);
    }
}
