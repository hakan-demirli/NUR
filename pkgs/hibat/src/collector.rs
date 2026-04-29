use std::fs;
use std::path::Path;

use crate::db::{BatteryRecord, BatteryStatus};
use chrono::Utc;

fn read_sysfs(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_sysfs_i64(path: &Path) -> Option<i64> {
    read_sysfs(path).and_then(|s| s.parse().ok())
}

fn read_sysfs_i32(path: &Path) -> Option<i32> {
    read_sysfs(path).and_then(|s| s.parse().ok())
}

pub fn discover_batteries(base_path: &str) -> Vec<String> {
    let base = Path::new(base_path);
    let mut batteries = Vec::new();
    if let Ok(entries) = fs::read_dir(base) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let type_path = entry.path().join("type");
            if let Some(typ) = read_sysfs(&type_path) {
                if typ == "Battery" && !name.starts_with("hid-") {
                    batteries.push(name);
                }
            }
        }
    }
    batteries.sort();
    batteries
}

pub fn collect_one(base_path: &str, battery: &str) -> BatteryRecord {
    let bat_path = Path::new(base_path).join(battery);

    BatteryRecord {
        id: None,
        timestamp: Utc::now().timestamp(),
        battery: battery.to_string(),
        capacity: read_sysfs_i32(&bat_path.join("capacity")),
        status: BatteryStatus::from_str_opt(read_sysfs(&bat_path.join("status")).as_deref()),
        voltage_now: read_sysfs_i64(&bat_path.join("voltage_now")),
        current_now: read_sysfs_i64(&bat_path.join("current_now")),
        power_now: read_sysfs_i64(&bat_path.join("power_now")),
        energy_now: read_sysfs_i64(&bat_path.join("energy_now")),
        energy_full: read_sysfs_i64(&bat_path.join("energy_full")),
        energy_full_design: read_sysfs_i64(&bat_path.join("energy_full_design")),
        cycle_count: read_sysfs_i32(&bat_path.join("cycle_count")),
        temperature: read_sysfs_i32(&bat_path.join("temp")),
    }
}

pub fn collect_all(base_path: &str) -> Vec<BatteryRecord> {
    discover_batteries(base_path)
        .into_iter()
        .map(|name| collect_one(base_path, &name))
        .collect()
}
