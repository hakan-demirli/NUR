use std::path::{Path, PathBuf};

use crate::ui::theme::{Mode, ThemeRegistry};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Detected {
    pub theme: Option<String>,
    pub mode: Option<Mode>,
}

impl Detected {
    fn merge_mode(mut self, mode: Option<Mode>) -> Self {
        if self.mode.is_none() {
            self.mode = mode;
        }
        self
    }

    fn merge_theme(mut self, theme: Option<String>) -> Self {
        if self.theme.is_none() {
            self.theme = theme;
        }
        self
    }
}

pub trait DetectEnv {
    fn var(&self, key: &str) -> Option<String>;
    fn read_file(&self, path: &Path) -> Option<String>;
    fn home_dir(&self) -> Option<PathBuf>;
    fn xdg_config_home(&self) -> Option<PathBuf>;
}

pub struct OsEnv;

impl DetectEnv for OsEnv {
    fn var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }

    fn read_file(&self, path: &Path) -> Option<String> {
        std::fs::read_to_string(path).ok()
    }

    fn home_dir(&self) -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }

    fn xdg_config_home(&self) -> Option<PathBuf> {
        if let Some(x) = std::env::var_os("XDG_CONFIG_HOME") {
            return Some(PathBuf::from(x));
        }
        self.home_dir().map(|h| h.join(".config"))
    }
}

pub fn detect(env: &dyn DetectEnv, registry: &ThemeRegistry) -> Detected {
    let mut out = Detected::default();

    out = out.merge_mode(parse_forced_mode(
        env.var("RAIDER_FORCE_THEME_MODE").as_deref(),
    ));
    if env.var("NO_COLOR").is_some() {
        out = out.merge_mode(Some(Mode::Dark));
    }

    if let Some(raw) = env.var("RAIDER_THEME") {
        let (name, mode) = split_name_and_mode(&raw);
        if let Some(matched) = match_bundled(&name, registry) {
            return Detected {
                theme: Some(matched),
                mode: out.mode.or(mode),
            };
        }
    }

    if let Some(raw) = env.var("GTK_THEME") {
        let (name, mode) = split_name_and_mode(&raw);
        if let Some(matched) = match_bundled(&name, registry) {
            out = out.merge_theme(Some(matched)).merge_mode(mode);
        } else {
            out = out.merge_mode(mode);
        }
    }

    if out.theme.is_none() {
        if let Some(cfg) = env.xdg_config_home() {
            for sub in ["gtk-3.0", "gtk-4.0"] {
                let path = cfg.join(sub).join("settings.ini");
                if let Some(text) = env.read_file(&path) {
                    if let Some(name) = parse_ini_key(&text, "gtk-theme-name") {
                        let (clean, mode) = split_name_and_mode(&name);
                        if let Some(matched) = match_bundled(&clean, registry) {
                            out = out.merge_theme(Some(matched));
                        }
                        out = out.merge_mode(mode);
                        if out.theme.is_some() {
                            break;
                        }
                    }
                }
            }
        }
    }

    if out.theme.is_none() {
        if let Some(cfg) = env.xdg_config_home() {
            for rel in ["kdedefaults/kdeglobals", "kdeglobals"] {
                let path = cfg.join(rel);
                if let Some(text) = env.read_file(&path) {
                    if let Some(name) = parse_ini_key(&text, "ColorScheme") {
                        let (clean, mode) = split_name_and_mode(&name);
                        if let Some(matched) = match_bundled(&clean, registry) {
                            out = out.merge_theme(Some(matched));
                        }
                        out = out.merge_mode(mode);
                        if out.theme.is_some() {
                            break;
                        }
                    }
                }
            }
        }
    }

    out = out.merge_mode(parse_colorfgbg(env.var("COLORFGBG").as_deref()));

    out
}

pub fn parse_forced_mode(value: Option<&str>) -> Option<Mode> {
    let v = value?.trim().to_ascii_lowercase();
    match v.as_str() {
        "dark" => Some(Mode::Dark),
        "light" => Some(Mode::Light),
        _ => None,
    }
}

pub fn split_name_and_mode(raw: &str) -> (String, Option<Mode>) {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed.strip_suffix(":dark") {
        return (normalize(stripped), Some(Mode::Dark));
    }
    if let Some(stripped) = trimmed.strip_suffix(":light") {
        return (normalize(stripped), Some(Mode::Light));
    }
    let mode = if trimmed.to_ascii_lowercase().contains("dark") {
        Some(Mode::Dark)
    } else if trimmed.to_ascii_lowercase().contains("light") {
        Some(Mode::Light)
    } else {
        None
    };
    (normalize(trimmed), mode)
}

fn normalize(s: &str) -> String {
    s.trim().to_ascii_lowercase().replace([' ', '_'], "-")
}

pub fn match_bundled(name: &str, registry: &ThemeRegistry) -> Option<String> {
    let names = registry.names();
    if name.is_empty() {
        return None;
    }
    if let Some(hit) = names.iter().find(|n| n.as_str() == name) {
        return Some(hit.clone());
    }
    let aliased = match name {
        "nordic" => Some("nord"),
        "onedark" | "one_dark" => Some("one-dark"),
        _ => None,
    };
    if let Some(alias) = aliased {
        if let Some(hit) = names.iter().find(|n| n.as_str() == alias) {
            return Some(hit.clone());
        }
    }
    let mut best: Option<&String> = None;
    for n in &names {
        if name.contains(n.as_str()) && best.map(|b| n.len() > b.len()).unwrap_or(true) {
            best = Some(n);
        }
    }
    best.cloned()
}

pub fn parse_ini_key(text: &str, key: &str) -> Option<String> {
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            if k.trim().eq_ignore_ascii_case(key) {
                let value = v.trim().trim_matches('"');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

pub fn parse_colorfgbg(value: Option<&str>) -> Option<Mode> {
    let raw = value?.trim();
    let parts: Vec<&str> = raw.split(';').collect();
    let bg = parts.last()?.trim();
    let n: u32 = bg.parse().ok()?;
    Some(if n <= 6 { Mode::Dark } else { Mode::Light })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct MapEnv {
        vars: HashMap<String, String>,
        files: HashMap<PathBuf, String>,
        home: Option<PathBuf>,
        xdg: Option<PathBuf>,
    }

    impl DetectEnv for MapEnv {
        fn var(&self, key: &str) -> Option<String> {
            self.vars.get(key).cloned()
        }
        fn read_file(&self, path: &Path) -> Option<String> {
            self.files.get(path).cloned()
        }
        fn home_dir(&self) -> Option<PathBuf> {
            self.home.clone()
        }
        fn xdg_config_home(&self) -> Option<PathBuf> {
            self.xdg
                .clone()
                .or_else(|| self.home.clone().map(|h| h.join(".config")))
        }
    }

    #[test]
    fn split_strips_colon_dark_suffix() {
        let (name, mode) = split_name_and_mode("Dracula:dark");
        assert_eq!(name, "dracula");
        assert_eq!(mode, Some(Mode::Dark));
    }

    #[test]
    fn split_handles_light_suffix() {
        let (name, mode) = split_name_and_mode("Adwaita:light");
        assert_eq!(name, "adwaita");
        assert_eq!(mode, Some(Mode::Light));
    }

    #[test]
    fn split_infers_mode_from_dark_substring() {
        let (name, mode) = split_name_and_mode("Adwaita-dark");
        assert_eq!(name, "adwaita-dark");
        assert_eq!(mode, Some(Mode::Dark));
    }

    #[test]
    fn split_returns_no_mode_when_no_hint() {
        let (name, mode) = split_name_and_mode("Adwaita");
        assert_eq!(name, "adwaita");
        assert_eq!(mode, None);
    }

    #[test]
    fn match_bundled_exact() {
        let reg = ThemeRegistry::new();
        assert_eq!(match_bundled("dracula", &reg).as_deref(), Some("dracula"));
    }

    #[test]
    fn match_bundled_substring_catppuccin() {
        let reg = ThemeRegistry::new();
        assert_eq!(
            match_bundled("catppuccin-mocha", &reg).as_deref(),
            Some("catppuccin"),
            "longest-match substring picks the parent flavour"
        );
    }

    #[test]
    fn match_bundled_substring_gruvbox_dark() {
        let reg = ThemeRegistry::new();
        assert_eq!(
            match_bundled("gruvbox-dark", &reg).as_deref(),
            Some("gruvbox")
        );
    }

    #[test]
    fn match_bundled_alias_onedark() {
        let reg = ThemeRegistry::new();
        assert_eq!(
            match_bundled("onedark", &reg).as_deref(),
            Some("one-dark"),
            "manual alias bridges `onedark` → `one-dark`"
        );
    }

    #[test]
    fn match_bundled_alias_nordic() {
        let reg = ThemeRegistry::new();
        assert_eq!(match_bundled("nordic", &reg).as_deref(), Some("nord"));
    }

    #[test]
    fn match_bundled_unknown_is_none() {
        let reg = ThemeRegistry::new();
        assert!(match_bundled("adwaita", &reg).is_none());
    }

    #[test]
    fn parse_colorfgbg_dark() {
        assert_eq!(parse_colorfgbg(Some("15;0")), Some(Mode::Dark));
        assert_eq!(parse_colorfgbg(Some("0;0")), Some(Mode::Dark));
        assert_eq!(parse_colorfgbg(Some("default;6")), Some(Mode::Dark));
    }

    #[test]
    fn parse_colorfgbg_light() {
        assert_eq!(parse_colorfgbg(Some("0;15")), Some(Mode::Light));
        assert_eq!(parse_colorfgbg(Some("0;7")), Some(Mode::Light));
    }

    #[test]
    fn parse_colorfgbg_handles_empty_and_garbage() {
        assert_eq!(parse_colorfgbg(None), None);
        assert_eq!(parse_colorfgbg(Some("")), None);
        assert_eq!(parse_colorfgbg(Some("garbage")), None);
    }

    #[test]
    fn parse_ini_key_finds_value() {
        let text = "[Settings]\ngtk-theme-name = Dracula\n# comment\nkey=other\n";
        assert_eq!(
            parse_ini_key(text, "gtk-theme-name").as_deref(),
            Some("Dracula")
        );
    }

    #[test]
    fn parse_ini_key_strips_quotes_and_ignores_case() {
        let text = "GTK-THEME-NAME=\"Dracula\"\n";
        assert_eq!(
            parse_ini_key(text, "gtk-theme-name").as_deref(),
            Some("Dracula")
        );
    }

    #[test]
    fn parse_ini_key_returns_none_when_missing() {
        let text = "[Settings]\nfoo=bar\n";
        assert!(parse_ini_key(text, "gtk-theme-name").is_none());
    }

    fn env_with_vars(pairs: &[(&str, &str)]) -> MapEnv {
        MapEnv {
            vars: pairs
                .iter()
                .map(|(k, v)| ((*k).into(), (*v).into()))
                .collect(),
            ..Default::default()
        }
    }

    #[test]
    fn detect_prefers_explicit_raider_theme_override() {
        let env = env_with_vars(&[("RAIDER_THEME", "nord:light"), ("GTK_THEME", "Dracula")]);
        let reg = ThemeRegistry::new();
        let d = detect(&env, &reg);
        assert_eq!(d.theme.as_deref(), Some("nord"));
        assert_eq!(d.mode, Some(Mode::Light));
    }

    #[test]
    fn detect_uses_gtk_theme_env_var() {
        let env = env_with_vars(&[("GTK_THEME", "Dracula")]);
        let reg = ThemeRegistry::new();
        let d = detect(&env, &reg);
        assert_eq!(d.theme.as_deref(), Some("dracula"));
    }

    #[test]
    fn detect_reads_gtk3_settings_ini_when_env_missing() {
        let env = MapEnv {
            home: Some(PathBuf::from("/home/test")),
            xdg: Some(PathBuf::from("/home/test/.config")),
            files: [(
                PathBuf::from("/home/test/.config/gtk-3.0/settings.ini"),
                "[Settings]\ngtk-theme-name=Tokyonight\n".to_string(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let reg = ThemeRegistry::new();
        let d = detect(&env, &reg);
        assert_eq!(d.theme.as_deref(), Some("tokyonight"));
    }

    #[test]
    fn detect_reads_kdeglobals_when_gtk_missing() {
        let env = MapEnv {
            xdg: Some(PathBuf::from("/x")),
            files: [(
                PathBuf::from("/x/kdeglobals"),
                "[General]\nColorScheme=Nord\n".to_string(),
            )]
            .into_iter()
            .collect(),
            ..Default::default()
        };
        let reg = ThemeRegistry::new();
        let d = detect(&env, &reg);
        assert_eq!(d.theme.as_deref(), Some("nord"));
    }

    #[test]
    fn detect_falls_back_to_colorfgbg_for_mode_when_no_theme_matches() {
        let env = env_with_vars(&[("GTK_THEME", "Adwaita"), ("COLORFGBG", "15;0")]);
        let reg = ThemeRegistry::new();
        let d = detect(&env, &reg);
        assert!(d.theme.is_none(), "no theme match");
        assert_eq!(d.mode, Some(Mode::Dark));
    }

    #[test]
    fn detect_force_mode_wins_over_inferred() {
        let env = env_with_vars(&[
            ("RAIDER_FORCE_THEME_MODE", "light"),
            ("GTK_THEME", "Dracula:dark"),
        ]);
        let reg = ThemeRegistry::new();
        let d = detect(&env, &reg);
        assert_eq!(d.theme.as_deref(), Some("dracula"));
        assert_eq!(
            d.mode,
            Some(Mode::Light),
            "explicit force wins over GTK_THEME's :dark suffix"
        );
    }
}
