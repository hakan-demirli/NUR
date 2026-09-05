use std::fs;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{anyhow, Context, Result};
use log::{info, warn};

use super::{
    AuthConfig, Client, Ethernet, EthernetMode, FanMode, FanStatus, System, SystemInfo, Temps,
    Uplink, WifiInfo, WifiKind,
};

const SYS_BL: &str = "/sys/class/backlight/backlight/brightness";
const SYS_BL_MAX: &str = "/sys/class/backlight/backlight/max_brightness";
const SYS_FB_BLANK: &str = "/sys/class/graphics/fb0/blank";

const CAPTIVE_STATUS: &str = "/usr/libexec/router-captive-status";
const ETHERNET_STATUS: &str = "/usr/libexec/router-ethernet-status";
const ETHERNET_APPLY: &str = "/usr/libexec/router-ethernet-apply";
const DHCP_LEASES: &str = "/tmp/dhcp.leases";
const PROC_LOADAVG: &str = "/proc/loadavg";
const PROC_MEMINFO: &str = "/proc/meminfo";
const PROC_UPTIME: &str = "/proc/uptime";
const PROC_NET_ARP: &str = "/proc/net/arp";

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

fn status_json(path: &str) -> Option<serde_json::Value> {
    let out = Command::new(path).output().ok()?;
    if !out.status.success() {
        warn!("{path} exited {}", out.status);
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

fn meminfo_kb(body: &str, key: &str) -> Option<u64> {
    body.lines()
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|v| v.parse().ok())
}

fn wireless_macs() -> Vec<String> {
    let Ok(list) = Command::new("ubus").args(["list", "hostapd.*"]).output() else {
        return Vec::new();
    };

    let mut macs = Vec::new();
    for object in String::from_utf8_lossy(&list.stdout).lines() {
        let object = object.trim();
        if object.is_empty() {
            continue;
        }
        let Ok(out) = Command::new("ubus")
            .args(["call", object, "get_clients"])
            .output()
        else {
            continue;
        };
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
            continue;
        };
        if let Some(clients) = value.get("clients").and_then(serde_json::Value::as_object) {
            macs.extend(clients.keys().map(|m| m.to_ascii_uppercase()));
        }
    }
    macs
}

fn df_kb(path: &str) -> Option<(u64, u64)> {
    let out = Command::new("df").arg("-k").arg(path).output().ok()?;
    let body = String::from_utf8_lossy(&out.stdout);
    let row = body.lines().nth(1)?;
    let mut f = row.split_whitespace().skip(1);
    let total: u64 = f.next()?.parse().ok()?;
    let used: u64 = f.next()?.parse().ok()?;
    Some((used, total))
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

    fn uplink(&self) -> Result<Uplink> {
        let Some(v) = status_json(CAPTIVE_STATUS) else {
            return Ok(Uplink::default());
        };
        Ok(Uplink {
            state: v
                .get("state")
                .and_then(serde_json::Value::as_str)
                .and_then(|s| match s {
                    "online" => Some(super::UplinkState::Online),
                    "portal" => Some(super::UplinkState::Portal),
                    "offline" => Some(super::UplinkState::Offline),
                    _ => None,
                }),
            portal_host: v
                .get("portal_host")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned),
            bypass_active: v
                .pointer("/bypass/active")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
        })
    }

    fn ethernet(&self) -> Result<Ethernet> {
        let Some(v) = status_json(ETHERNET_STATUS) else {
            return Ok(Ethernet::default());
        };
        Ok(Ethernet {
            mode: v
                .get("mode")
                .and_then(serde_json::Value::as_str)
                .and_then(EthernetMode::parse),
            wan_up: v.pointer("/wan/up").and_then(serde_json::Value::as_bool),
            wan_address: v
                .pointer("/wan/address")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned),
        })
    }

    fn set_ethernet_mode(&self, mode: EthernetMode) -> Result<()> {
        let arg = mode.as_str().to_string();
        std::thread::spawn(
            move || match Command::new(ETHERNET_APPLY).arg(&arg).status() {
                Ok(st) if st.success() => info!("ethernet mode -> {arg}"),
                Ok(st) => warn!("{ETHERNET_APPLY} {arg} exited {st}"),
                Err(e) => warn!("{ETHERNET_APPLY} {arg}: {e}"),
            },
        );
        Ok(())
    }

    fn clients(&self) -> Result<Option<u32>> {
        Ok(fs::read_to_string(DHCP_LEASES)
            .ok()
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u32))
    }

    fn client_list(&self) -> Result<Vec<Client>> {
        let leases = fs::read_to_string(DHCP_LEASES).unwrap_or_default();
        let arp = fs::read_to_string(PROC_NET_ARP).unwrap_or_default();
        let wireless = wireless_macs();

        let mut out = Vec::new();
        for line in leases.lines() {
            let mut f = line.split_whitespace();
            let (_expiry, mac, ip) = (f.next(), f.next(), f.next());
            let (Some(mac), Some(ip)) = (mac, ip) else {
                continue;
            };
            let name = match f.next() {
                Some("*") | None => ip.to_string(),
                Some(n) => n.to_string(),
            };
            let mac_up = mac.to_ascii_uppercase();
            out.push(Client {
                name,
                ip: ip.to_string(),
                mac: mac_up.clone(),
                wireless: wireless.iter().any(|m| *m == mac_up),
            });
        }

        for line in arp.lines().skip(1) {
            let mut f = line.split_whitespace();
            let (Some(ip), Some(_hw), Some(flags)) = (f.next(), f.next(), f.next()) else {
                continue;
            };
            if flags == "0x0" {
                continue;
            }
            let Some(mac) = f.next().map(str::to_ascii_uppercase) else {
                continue;
            };
            if mac == "00:00:00:00:00:00" || out.iter().any(|c| c.ip == ip) {
                continue;
            }
            out.push(Client {
                name: ip.to_string(),
                ip: ip.to_string(),
                mac: mac.clone(),
                wireless: wireless.iter().any(|m| *m == mac),
            });
        }

        out.sort_by(|a, b| a.ip.cmp(&b.ip));
        Ok(out)
    }

    fn system_info(&self) -> Result<SystemInfo> {
        let mem = fs::read_to_string(PROC_MEMINFO).unwrap_or_default();
        let total = meminfo_kb(&mem, "MemTotal:");
        let avail = meminfo_kb(&mem, "MemAvailable:");
        let (flash_used, flash_total) = df_kb("/overlay")
            .or_else(|| df_kb("/"))
            .map_or((None, None), |(u, t)| (Some(u), Some(t)));

        Ok(SystemInfo {
            load1: fs::read_to_string(PROC_LOADAVG)
                .ok()
                .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse().ok())),
            mem_used_kb: total.zip(avail).map(|(t, a)| t.saturating_sub(a)),
            mem_total_kb: total,
            flash_used_kb: flash_used,
            flash_total_kb: flash_total,
            uptime_secs: fs::read_to_string(PROC_UPTIME).ok().and_then(|s| {
                s.split_whitespace()
                    .next()
                    .and_then(|v| v.parse::<f64>().ok())
                    .map(|v| v as u64)
            }),
        })
    }

    fn reboot(&self) -> Result<()> {
        info!("reboot requested from touchscreen");
        std::thread::spawn(|| {
            let _ = Command::new("reboot").status();
        });
        Ok(())
    }

    fn backlight(&self) -> Result<u32> {
        Ok(read_int(SYS_BL).unwrap_or(0).max(0) as u32)
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
