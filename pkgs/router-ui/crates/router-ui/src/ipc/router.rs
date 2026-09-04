use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use log::{info, warn};

use super::{AuthConfig, FanMode, FanStatus, System, Temps, WifiInfo, WifiKind};

const SYS_BL: &str = "/sys/class/backlight/backlight/brightness";
const SYS_BL_MAX: &str = "/sys/class/backlight/backlight/max_brightness";
const SYS_FB_BLANK: &str = "/sys/class/graphics/fb0/blank";

const HWMON_ROOT: &str = "/sys/class/hwmon";

const HWMON_CPU: &str = "cpu_thermal";
const HWMON_PHY: &str = "mdio_bus:07";
const HWMON_FAN: &str = "pwmfan";

#[derive(Debug)]
pub struct RouterSystem {
    cpu_temp: Option<PathBuf>,
    phy_temp: Option<PathBuf>,
    fan_pwm: Option<PathBuf>,
    fan_rpm: Option<PathBuf>,
}

impl Default for RouterSystem {
    fn default() -> Self {
        let cpu = hwmon_by_name(HWMON_CPU);
        let phy = hwmon_by_name(HWMON_PHY);
        let fan = hwmon_by_name(HWMON_FAN);

        for (label, found) in [("cpu", &cpu), ("phy", &phy), ("fan", &fan)] {
            match found {
                Some(p) => info!("hwmon {label}: {}", p.display()),
                None => warn!("hwmon {label}: not found; readings report unavailable"),
            }
        }

        Self {
            cpu_temp: cpu.map(|p| p.join("temp1_input")),
            phy_temp: phy.map(|p| p.join("temp1_input")),
            fan_pwm: fan.as_ref().map(|p| p.join("pwm1")),
            fan_rpm: fan.map(|p| p.join("fan1_input")),
        }
    }
}

fn hwmon_by_name(name: &str) -> Option<PathBuf> {
    let mut found = None;
    for entry in fs::read_dir(HWMON_ROOT).ok()?.flatten() {
        let dir = entry.path();
        let Ok(actual) = fs::read_to_string(dir.join("name")) else {
            continue;
        };
        if actual.trim() == name {
            found = Some(dir);
            break;
        }
    }
    found
}

fn read_int(path: &str) -> Result<i64> {
    let s = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    s.trim()
        .parse::<i64>()
        .with_context(|| format!("parse {path} ({s:?})"))
}

fn read_opt(path: Option<&PathBuf>) -> Option<i64> {
    let path = path?;
    let raw = fs::read_to_string(path).ok()?;
    raw.trim().parse::<i64>().ok()
}

fn uci_get(key: &str) -> Result<String> {
    let out = Command::new("uci")
        .arg("get")
        .arg(key)
        .output()
        .with_context(|| format!("exec uci get {key}"))?;
    if !out.status.success() {
        return Err(anyhow!(
            "uci get {key} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

fn uci_set(key: &str, value: &str) -> Result<()> {
    let st = Command::new("uci")
        .args(["set", &format!("{key}={value}")])
        .status()?;
    if !st.success() {
        return Err(anyhow!("uci set {key}={value} failed: {st}"));
    }
    Ok(())
}

fn uci_commit(pkg: &str) -> Result<()> {
    let st = Command::new("uci").args(["commit", pkg]).status()?;
    if !st.success() {
        return Err(anyhow!("uci commit {pkg} failed: {st}"));
    }
    Ok(())
}

fn initd_reload(svc: &str) -> Result<()> {
    let st = Command::new(format!("/etc/init.d/{svc}"))
        .arg("reload")
        .status()?;
    if !st.success() {
        return Err(anyhow!("/etc/init.d/{svc} reload failed: {st}"));
    }
    Ok(())
}

impl System for RouterSystem {
    fn auth_config(&self) -> Result<AuthConfig> {
        Ok(AuthConfig {
            admin_pin_hash: uci_get("router-ui.auth.admin_pin_hash").unwrap_or_default(),
            guest_pin_hash: uci_get("router-ui.auth.guest_pin_hash").unwrap_or_default(),
        })
    }

    fn idle_seconds(&self) -> Option<u32> {
        uci_get("router-ui.session.idle_seconds")
            .ok()
            .and_then(|s| s.parse().ok())
    }

    fn temps(&self) -> Result<Temps> {
        Ok(Temps {
            cpu: read_opt(self.cpu_temp.as_ref()).map(|v| v as i32),
            phy: read_opt(self.phy_temp.as_ref()).map(|v| v as i32),
        })
    }

    fn fan_status(&self) -> Result<FanStatus> {
        let mode_str = uci_get("router.fan.mode").unwrap_or_else(|_| "auto".into());
        let mode = FanMode::parse(&mode_str).unwrap_or(FanMode::Auto);
        Ok(FanStatus {
            mode,
            pwm: read_opt(self.fan_pwm.as_ref()).map(|v| v.clamp(0, 255) as u8),
            rpm: read_opt(self.fan_rpm.as_ref()).map(|v| v.max(0) as u32),
        })
    }

    fn set_fan_mode(&self, mode: FanMode) -> Result<()> {
        uci_set("router.fan.mode", mode.as_str())?;
        uci_commit("router")?;
        initd_reload("router-fan")?;
        Ok(())
    }

    fn wifi_info(&self, kind: WifiKind) -> Result<Option<WifiInfo>> {
        match kind {
            WifiKind::Guest => Ok(None),
            WifiKind::Admin => {
                let section = if uci_get("wireless.ap_2g.ssid").is_ok() {
                    "ap_2g"
                } else {
                    "@wifi-iface[0]"
                };
                let ssid = uci_get(&format!("wireless.{section}.ssid")).ok();
                let key = uci_get(&format!("wireless.{section}.key")).ok();
                match (ssid, key) {
                    (Some(s), Some(k)) if !s.is_empty() => Ok(Some(WifiInfo {
                        ssid: s,
                        password: k,
                    })),
                    _ => Ok(None),
                }
            }
        }
    }

    fn set_backlight(&self, brightness: u32) -> Result<()> {
        fs::write(SYS_BL, brightness.to_string()).context("write backlight")?;
        Ok(())
    }

    fn max_backlight(&self) -> Result<u32> {
        Ok(read_int(SYS_BL_MAX).unwrap_or(255) as u32)
    }

    fn blank_display(&self, on: bool) -> Result<()> {
        fs::write(SYS_FB_BLANK, if on { "4" } else { "0" }).context("write fb0/blank")?;
        Ok(())
    }
}
