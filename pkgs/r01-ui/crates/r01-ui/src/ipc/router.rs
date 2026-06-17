use std::fs;
use std::process::Command;

use anyhow::{anyhow, Context, Result};

use super::{AuthConfig, FanMode, FanStatus, System, Temps, WifiInfo, WifiKind};

const SYS_CPU_TEMP: &str = "/sys/class/thermal/thermal_zone0/temp";
const SYS_PHY_TEMP: &str = "/sys/class/hwmon/hwmon1/temp1_input";
const SYS_FAN_PWM: &str = "/sys/class/hwmon/hwmon2/pwm1";
const SYS_FAN_RPM: &str = "/sys/class/hwmon/hwmon2/fan1_input";
const SYS_BL: &str = "/sys/class/backlight/backlight/brightness";
const SYS_BL_MAX: &str = "/sys/class/backlight/backlight/max_brightness";
const SYS_FB_BLANK: &str = "/sys/class/graphics/fb0/blank";

#[derive(Debug, Default)]
pub struct RouterSystem;

fn read_int(path: &str) -> Result<i64> {
    let s = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    s.trim()
        .parse::<i64>()
        .with_context(|| format!("parse {path} ({s:?})"))
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
            admin_pin_hash: uci_get("r01-ui.auth.admin_pin_hash").unwrap_or_default(),
            guest_pin_hash: uci_get("r01-ui.auth.guest_pin_hash").unwrap_or_default(),
        })
    }

    fn idle_seconds(&self) -> Option<u32> {
        uci_get("r01-ui.session.idle_seconds")
            .ok()
            .and_then(|s| s.parse().ok())
    }

    fn temps(&self) -> Result<Temps> {
        Ok(Temps {
            cpu: read_int(SYS_CPU_TEMP).unwrap_or(0) as i32,
            phy: read_int(SYS_PHY_TEMP).unwrap_or(0) as i32,
        })
    }

    fn fan_status(&self) -> Result<FanStatus> {
        let mode_str = uci_get("r01.fan.mode").unwrap_or_else(|_| "auto".into());
        let mode = FanMode::parse(&mode_str).unwrap_or(FanMode::Auto);
        Ok(FanStatus {
            mode,
            pwm: read_int(SYS_FAN_PWM).unwrap_or(0).clamp(0, 255) as u8,
            rpm: read_int(SYS_FAN_RPM).unwrap_or(0).max(0) as u32,
        })
    }

    fn set_fan_mode(&self, mode: FanMode) -> Result<()> {
        uci_set("r01.fan.mode", mode.as_str())?;
        uci_commit("r01")?;
        initd_reload("r01-fan")?;
        Ok(())
    }

    fn wifi_info(&self, kind: WifiKind) -> Result<Option<WifiInfo>> {
        match kind {
            WifiKind::Guest => Ok(None),
            WifiKind::Admin => {
                let ssid = uci_get("wireless.@wifi-iface[0].ssid").ok();
                let key = uci_get("wireless.@wifi-iface[0].key").ok();
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
