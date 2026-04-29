use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_polling_interval")]
    pub polling_interval_secs: u64,

    #[serde(default)]
    pub theme: ThemeConfig,

    #[serde(default)]
    pub ui: UiConfig,

    #[serde(default)]
    pub collector: CollectorConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub background: String,
    pub surface_color: String,
    pub accent_color: String,
    pub border_color: String,
    pub text_color: String,
    pub text_dim_color: String,
    pub charge_color: String,
    pub discharge_color: String,
    pub full_color: String,
    pub grid_color: String,
    pub health_good_color: String,
    pub health_warn_color: String,
    pub health_bad_color: String,
    pub dark_mode: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(default = "default_window_width")]
    pub window_width: f32,
    #[serde(default = "default_window_height")]
    pub window_height: f32,
    #[serde(default = "default_chart_line_width")]
    pub chart_line_width: f32,
    #[serde(default = "default_default_view")]
    pub default_view: String,
    #[serde(default = "default_recent_hours")]
    pub recent_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorConfig {
    #[serde(default = "default_battery_path")]
    pub battery_path: String,
    #[serde(default = "default_max_db_size_mb")]
    pub max_db_size_mb: u64,
    #[serde(default = "default_retention_days")]
    pub retention_days: u64,
}

#[derive(Debug, Clone, Copy)]
enum DesktopTheme {
    Dracula,
    CatppuccinMocha,
    Generic,
}

fn detect_desktop_theme() -> DesktopTheme {
    if let Ok(theme) = std::env::var("GTK_THEME") {
        let lower = theme.to_lowercase();
        if lower.contains("dracula") {
            return DesktopTheme::Dracula;
        }
        if lower.contains("catppuccin") {
            return DesktopTheme::CatppuccinMocha;
        }
    }

    for ref p in [dirs_gtk3_settings(), dirs_gtk4_settings()]
        .iter()
        .flatten()
    {
        if let Ok(content) = fs::read_to_string(p) {
            let lower = content.to_lowercase();
            if lower.contains("dracula") {
                return DesktopTheme::Dracula;
            }
            if lower.contains("catppuccin") {
                return DesktopTheme::CatppuccinMocha;
            }
        }
    }

    DesktopTheme::Generic
}

fn dirs_gtk3_settings() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/gtk-3.0/settings.ini"))
}

fn dirs_gtk4_settings() -> Option<PathBuf> {
    let home = std::env::var("HOME").ok()?;
    Some(PathBuf::from(home).join(".config/gtk-4.0/settings.ini"))
}

struct Palette {
    background: &'static str,
    surface: &'static str,
    accent: &'static str,
    border: &'static str,
    text: &'static str,
    text_dim: &'static str,
    charge: &'static str,
    discharge: &'static str,
    full: &'static str,
    grid: &'static str,
    health_good: &'static str,
    health_warn: &'static str,
    health_bad: &'static str,
}

impl Palette {
    fn to_theme_config(&self) -> ThemeConfig {
        ThemeConfig {
            background: self.background.into(),
            surface_color: self.surface.into(),
            accent_color: self.accent.into(),
            border_color: self.border.into(),
            text_color: self.text.into(),
            text_dim_color: self.text_dim.into(),
            charge_color: self.charge.into(),
            discharge_color: self.discharge.into(),
            full_color: self.full.into(),
            grid_color: self.grid.into(),
            health_good_color: self.health_good.into(),
            health_warn_color: self.health_warn.into(),
            health_bad_color: self.health_bad.into(),
            dark_mode: false,
        }
    }
}

const DRACULA: Palette = Palette {
    background: "#282a36",
    surface: "#44475a",
    accent: "#bd93f9",
    border: "#6272a4",
    text: "#f8f8f2",
    text_dim: "#bfbfbf",
    charge: "#50fa7b",
    discharge: "#ff5555",
    full: "#8be9fd",
    grid: "#44475a",
    health_good: "#50fa7b",
    health_warn: "#f1fa8c",
    health_bad: "#ff5555",
};

const CATPPUCCIN_MOCHA: Palette = Palette {
    background: "#1e1e2e",
    surface: "#313244",
    accent: "#cba6f7",
    border: "#585b70",
    text: "#cdd6f4",
    text_dim: "#a6adc8",
    charge: "#a6e3a1",
    discharge: "#f38ba8",
    full: "#89b4fa",
    grid: "#45475a",
    health_good: "#a6e3a1",
    health_warn: "#f9e2af",
    health_bad: "#f38ba8",
};

const GENERIC: Palette = Palette {
    background: "#2e3440",
    surface: "#3b4252",
    accent: "#88c0d0",
    border: "#4c566a",
    text: "#eceff4",
    text_dim: "#b0b8cd",
    charge: "#a3be8c",
    discharge: "#bf616a",
    full: "#81a1c1",
    grid: "#3b4252",
    health_good: "#a3be8c",
    health_warn: "#ebcb8b",
    health_bad: "#bf616a",
};

fn get_palette() -> &'static Palette {
    match detect_desktop_theme() {
        DesktopTheme::Dracula => &DRACULA,
        DesktopTheme::CatppuccinMocha => &CATPPUCCIN_MOCHA,
        DesktopTheme::Generic => &GENERIC,
    }
}

fn default_polling_interval() -> u64 {
    60
}
fn default_window_width() -> f32 {
    1000.0
}
fn default_window_height() -> f32 {
    700.0
}
fn default_chart_line_width() -> f32 {
    2.5
}
fn default_default_view() -> String {
    "recent".into()
}
fn default_recent_hours() -> u64 {
    12
}
fn default_battery_path() -> String {
    "/sys/class/power_supply".into()
}
fn default_max_db_size_mb() -> u64 {
    100
}
fn default_retention_days() -> u64 {
    365
}

impl Default for Config {
    fn default() -> Self {
        Self {
            polling_interval_secs: default_polling_interval(),
            theme: ThemeConfig::default(),
            ui: UiConfig::default(),
            collector: CollectorConfig::default(),
        }
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        get_palette().to_theme_config()
    }
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            window_width: default_window_width(),
            window_height: default_window_height(),
            chart_line_width: default_chart_line_width(),
            default_view: default_default_view(),
            recent_hours: default_recent_hours(),
        }
    }
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            battery_path: default_battery_path(),
            max_db_size_mb: default_max_db_size_mb(),
            retention_days: default_retention_days(),
        }
    }
}

impl Config {
    pub fn config_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join(".config")
            .join("hibat")
            .join("config.toml")
    }

    pub fn db_path() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".into());
        PathBuf::from(home)
            .join(".cache")
            .join("hibat")
            .join("hibat.db")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        if path.exists() {
            match fs::read_to_string(&path) {
                Ok(content) => match toml::from_str(&content) {
                    Ok(config) => return config,
                    Err(e) => {
                        eprintln!(
                            "Warning: failed to parse config at {}: {}",
                            path.display(),
                            e
                        );
                        eprintln!("Using default configuration");
                    }
                },
                Err(e) => {
                    eprintln!(
                        "Warning: failed to read config at {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        let path = Self::config_path();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let content = toml::to_string_pretty(self).unwrap_or_default();
        let _ = fs::write(&path, content);
    }

    pub fn save_default_if_missing(&self) {
        let path = Self::config_path();
        if !path.exists() {
            self.save();
        }
    }

    pub fn parse_color(hex: &str) -> egui::Color32 {
        let hex = hex.trim_start_matches('#');
        if hex.len() == 6 {
            if let (Ok(r), Ok(g), Ok(b)) = (
                u8::from_str_radix(&hex[0..2], 16),
                u8::from_str_radix(&hex[2..4], 16),
                u8::from_str_radix(&hex[4..6], 16),
            ) {
                return egui::Color32::from_rgb(r, g, b);
            }
        }
        egui::Color32::WHITE
    }
}
