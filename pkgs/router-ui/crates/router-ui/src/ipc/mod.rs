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
    #[serde(default)]
    pub cpu: Option<i32>,
    #[serde(default)]
    pub phy: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct FanStatus {
    pub mode: FanMode,
    #[serde(default)]
    pub pwm: Option<u8>,
    #[serde(default)]
    pub rpm: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum UplinkState {
    Online,
    Portal,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EthernetMode {
    DualLan,
    WiredWan,
}

impl EthernetMode {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::DualLan => "dual-lan",
            Self::WiredWan => "wired-wan",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "dual-lan" => Self::DualLan,
            "wired-wan" => Self::WiredWan,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Uplink {
    #[serde(default)]
    pub state: Option<UplinkState>,
    #[serde(default)]
    pub portal_host: Option<String>,
    #[serde(default)]
    pub bypass_active: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Ethernet {
    #[serde(default)]
    pub mode: Option<EthernetMode>,
    #[serde(default)]
    pub wan_up: Option<bool>,
    #[serde(default)]
    pub wan_address: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Client {
    pub name: String,
    pub ip: String,
    pub mac: String,
    #[serde(default)]
    pub wireless: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct SystemInfo {
    #[serde(default)]
    pub load1: Option<f32>,
    #[serde(default)]
    pub mem_used_kb: Option<u64>,
    #[serde(default)]
    pub mem_total_kb: Option<u64>,
    #[serde(default)]
    pub flash_used_kb: Option<u64>,
    #[serde(default)]
    pub flash_total_kb: Option<u64>,
    #[serde(default)]
    pub uptime_secs: Option<u64>,
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

    fn uplink(&self) -> Result<Uplink>;
    fn ethernet(&self) -> Result<Ethernet>;
    fn set_ethernet_mode(&self, mode: EthernetMode) -> Result<()>;
    fn clients(&self) -> Result<Option<u32>>;
    fn client_list(&self) -> Result<Vec<Client>>;
    fn system_info(&self) -> Result<SystemInfo>;
    fn reboot(&self) -> Result<()>;
    fn backlight(&self) -> Result<u32>;

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
