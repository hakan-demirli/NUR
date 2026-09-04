use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use anyhow::{anyhow, Context, Result};
use log::info;
use router_auth::{Argon2Hasher, User};
use serde::{Deserialize, Serialize};

use super::{
    AuthConfig, Client, Ethernet, EthernetMode, FanMode, FanStatus, System, SystemInfo, Temps,
    Uplink, UplinkState, WifiInfo, WifiKind,
};

#[derive(Debug, Serialize, Deserialize)]
struct Fixture {
    auth: AuthConfig,
    temps: Temps,
    fan: FanStatus,
    wifi_admin: Option<WifiInfo>,
    wifi_guest: Option<WifiInfo>,
    #[serde(default = "default_max_bl")]
    max_brightness: u32,
    #[serde(default)]
    uplink: Uplink,
    #[serde(default)]
    ethernet: Ethernet,
    #[serde(default)]
    clients: Option<u32>,
    #[serde(default)]
    client_list: Vec<Client>,
    #[serde(default)]
    system: SystemInfo,
    #[serde(default = "default_backlight")]
    backlight: u32,
}

const fn default_backlight() -> u32 {
    3124
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
                    cpu: Some(49_000),
                    phy: Some(47_000),
                },
                fan: FanStatus {
                    mode: FanMode::Auto,
                    pwm: Some(128),
                    rpm: Some(3640),
                },
                wifi_admin: Some(WifiInfo {
                    ssid: "example-ssid".into(),
                    password: "changeme".into(),
                }),
                wifi_guest: None,
                max_brightness: default_max_bl(),
                uplink: Uplink {
                    state: Some(UplinkState::Portal),
                    portal_host: Some("login.hotel.example.net".into()),
                    bypass_active: true,
                },
                ethernet: Ethernet {
                    mode: Some(EthernetMode::DualLan),
                    wan_up: Some(false),
                    wan_address: None,
                },
                clients: Some(3),
                client_list: vec![
                    Client {
                        name: "laptop-0".into(),
                        ip: "192.168.69.104".into(),
                        mac: "2C:1B:3A:B9:3C:19".into(),
                        wireless: true,
                    },
                    Client {
                        name: "s01".into(),
                        ip: "192.168.69.249".into(),
                        mac: "38:05:25:36:05:BC".into(),
                        wireless: false,
                    },
                    Client {
                        name: "poco-x7-pro".into(),
                        ip: "192.168.69.152".into(),
                        mac: "9A:31:0C:44:7E:02".into(),
                        wireless: true,
                    },
                ],
                system: SystemInfo {
                    load1: Some(0.14),
                    mem_used_kb: Some(212_992),
                    mem_total_kb: Some(1_012_736),
                    flash_used_kb: Some(18_432),
                    flash_total_kb: Some(102_400),
                    uptime_secs: Some(86_432),
                },
                backlight: default_backlight(),
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
                    s.fan.pwm = Some(128);
                    s.fan.rpm = Some(3640);
                }
                FanMode::Quiet => {
                    s.fan.pwm = Some(0);
                    s.fan.rpm = Some(0);
                }
                FanMode::Aggressive => {
                    s.fan.pwm = Some(255);
                    s.fan.rpm = Some(6800);
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

    fn uplink(&self) -> Result<Uplink> {
        Ok(self.state.lock().unwrap().uplink.clone())
    }

    fn ethernet(&self) -> Result<Ethernet> {
        Ok(self.state.lock().unwrap().ethernet.clone())
    }

    fn set_ethernet_mode(&self, mode: EthernetMode) -> Result<()> {
        let mut s = self.state.lock().unwrap();
        info!("desktop: set_ethernet_mode({})", mode.as_str());
        s.ethernet.mode = Some(mode);
        match mode {
            EthernetMode::DualLan => {
                s.ethernet.wan_up = Some(false);
                s.ethernet.wan_address = None;
            }
            EthernetMode::WiredWan => {
                s.ethernet.wan_up = Some(true);
                s.ethernet.wan_address = Some("192.168.1.117".into());
            }
        }
        Ok(())
    }

    fn clients(&self) -> Result<Option<u32>> {
        Ok(self.state.lock().unwrap().clients)
    }

    fn client_list(&self) -> Result<Vec<Client>> {
        Ok(self.state.lock().unwrap().client_list.clone())
    }

    fn system_info(&self) -> Result<SystemInfo> {
        Ok(self.state.lock().unwrap().system.clone())
    }

    fn reboot(&self) -> Result<()> {
        info!("desktop: reboot()");
        Ok(())
    }

    fn backlight(&self) -> Result<u32> {
        Ok(self.state.lock().unwrap().backlight)
    }

    fn set_backlight(&self, brightness: u32) -> Result<()> {
        info!("desktop: set_backlight({brightness})");
        self.state.lock().unwrap().backlight = brightness;
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
