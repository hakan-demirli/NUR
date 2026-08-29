use anyhow::Result;
use serde::{Deserialize, Serialize};

use router_auth::User;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum FanMode {
    Auto,
    Quiet,
    Aggressive,
    Manual,
}

impl FanMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Quiet => "quiet",
            Self::Aggressive => "aggressive",
            Self::Manual => "manual",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "auto" => Self::Auto,
            "quiet" => Self::Quiet,
            "aggressive" => Self::Aggressive,
            "manual" => Self::Manual,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Temps {
    pub cpu: i32,
    pub phy: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FanStatus {
    pub mode: FanMode,
    pub pwm: u8,
    pub rpm: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct WifiInfo {
    pub ssid: String,
    pub password: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WifiKind {
    Admin,
    Guest,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct AuthConfig {
    pub admin_pin_hash: String,
    pub guest_pin_hash: String,
}

impl AuthConfig {
    pub(crate) fn hash_for(&self, user: User) -> &str {
        match user {
            User::Admin => &self.admin_pin_hash,
            User::Guest => &self.guest_pin_hash,
        }
    }
}

pub(crate) trait System: std::fmt::Debug {
    fn auth_config(&self) -> Result<AuthConfig>;
    fn temps(&self) -> Result<Temps>;
    fn fan_status(&self) -> Result<FanStatus>;
    fn set_fan_mode(&self, mode: FanMode) -> Result<()>;
    fn wifi_info(&self, kind: WifiKind) -> Result<Option<WifiInfo>>;

    fn idle_seconds(&self) -> Option<u32> {
        None
    }

    fn set_backlight(&self, brightness: u32) -> Result<()>;
    fn max_backlight(&self) -> Result<u32>;

    fn blank_display(&self, on: bool) -> Result<()>;
}

#[cfg(feature = "desktop")]
mod desktop;
#[cfg(feature = "desktop")]
pub(crate) use desktop::DesktopSystem;

#[cfg(feature = "router")]
mod router;
#[cfg(feature = "router")]
pub use router::RouterSystem;
