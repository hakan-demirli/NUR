use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use log::info;
use router_auth::{Argon2Hasher, User};
use serde::{Deserialize, Serialize};

use super::{AuthConfig, FanMode, FanStatus, System, Temps, WifiInfo, WifiKind};

#[derive(Debug, Serialize, Deserialize)]
struct Fixture {
    auth: AuthConfig,
    temps: Temps,
    fan: FanStatus,
    wifi_admin: Option<WifiInfo>,
    wifi_guest: Option<WifiInfo>,
    #[serde(default = "default_max_bl")]
    max_brightness: u32,
}

const fn default_max_bl() -> u32 {
    3124
}

#[derive(Debug)]
pub(crate) struct DesktopSystem {
    state: Mutex<Fixture>,
    source_path: PathBuf,
}

impl DesktopSystem {
    pub(crate) fn load(fixtures_dir: &Path) -> Result<Self> {
        let p = fixtures_dir.join("state.json");
        let mut fx: Fixture = if p.exists() {
            let raw = fs::read_to_string(&p).with_context(|| format!("read {}", p.display()))?;
            serde_json::from_str(&raw).with_context(|| format!("parse {}", p.display()))?
        } else {
            info!("no fixtures found, generating defaults at {}", p.display());
            Fixture {
                auth: AuthConfig::default(),
                temps: Temps {
                    cpu: 49_000,
                    phy: 47_000,
                },
                fan: FanStatus {
                    mode: FanMode::Auto,
                    pwm: 128,
                    rpm: 3640,
                },
                wifi_admin: Some(WifiInfo {
                    ssid: "example-ssid".into(),
                    password: "changeme".into(),
                }),
                wifi_guest: None,
                max_brightness: default_max_bl(),
            }
        };

        let h = Argon2Hasher;
        if fx.auth.admin_pin_hash.is_empty() {
            fx.auth.admin_pin_hash = h.hash("1234").map_err(|e| anyhow!("{e}"))?;
            info!("desktop: bootstrapped admin PIN = 1234");
        }
        if fx.auth.guest_pin_hash.is_empty() {
            fx.auth.guest_pin_hash = h.hash("0000").map_err(|e| anyhow!("{e}"))?;
            info!("desktop: bootstrapped guest PIN = 0000");
        }

        Ok(Self {
            state: Mutex::new(fx),
            source_path: p,
        })
    }
}

impl System for DesktopSystem {
    fn auth_config(&self) -> Result<AuthConfig> {
        Ok(self.state.lock().unwrap().auth.clone())
    }

    fn temps(&self) -> Result<Temps> {
        Ok(self.state.lock().unwrap().temps.clone())
    }

    fn fan_status(&self) -> Result<FanStatus> {
        Ok(self.state.lock().unwrap().fan.clone())
    }

    fn set_fan_mode(&self, mode: FanMode) -> Result<()> {
        {
            let mut s = self.state.lock().unwrap();
            info!(
                "desktop: set_fan_mode({}) [was {}]",
                mode.as_str(),
                s.fan.mode.as_str()
            );
            s.fan.mode = mode;
            match mode {
                FanMode::Auto => {
                    s.fan.pwm = 128;
                    s.fan.rpm = 3640;
                }
                FanMode::Quiet => {
                    s.fan.pwm = 0;
                    s.fan.rpm = 0;
                }
                FanMode::Aggressive => {
                    s.fan.pwm = 255;
                    s.fan.rpm = 6800;
                }
                FanMode::Manual => {}
            }
        }
        Ok(())
    }

    fn wifi_info(&self, kind: WifiKind) -> Result<Option<WifiInfo>> {
        let s = self.state.lock().unwrap();
        Ok(match kind {
            WifiKind::Admin => s.wifi_admin.clone(),
            WifiKind::Guest => s.wifi_guest.clone(),
        })
    }

    fn set_backlight(&self, brightness: u32) -> Result<()> {
        info!("desktop: set_backlight({brightness})");
        Ok(())
    }

    fn max_backlight(&self) -> Result<u32> {
        Ok(self.state.lock().unwrap().max_brightness)
    }

    fn blank_display(&self, on: bool) -> Result<()> {
        info!("desktop: blank_display({on})");
        let _ = &self.source_path;
        let _ = User::Admin;
        Ok(())
    }
}
