use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, Result, Row};
use std::fmt;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryStatus {
    Charging,
    Discharging,
    Full,
    NotCharging,
    Unknown,
}

impl BatteryStatus {
    pub fn from_str_opt(s: Option<&str>) -> Self {
        match s {
            Some("Charging") => Self::Charging,
            Some("Discharging") => Self::Discharging,
            Some("Full") => Self::Full,
            Some("Not charging") => Self::NotCharging,
            _ => Self::Unknown,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Charging => "Charging",
            Self::Discharging => "Discharging",
            Self::Full => "Full",
            Self::NotCharging => "Not charging",
            Self::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for BatteryStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct BatteryRecord {
    pub id: Option<i64>,
    pub timestamp: i64,
    pub battery: String,
    pub capacity: Option<i32>,
    pub status: BatteryStatus,
    pub voltage_now: Option<i64>,
    pub current_now: Option<i64>,
    pub power_now: Option<i64>,
    pub energy_now: Option<i64>,
    pub energy_full: Option<i64>,
    pub energy_full_design: Option<i64>,
    pub cycle_count: Option<i32>,
    pub temperature: Option<i32>,
}

impl BatteryRecord {
    fn from_row(row: &Row<'_>) -> rusqlite::Result<Self> {
        let status_str: Option<String> = row.get(4)?;
        Ok(Self {
            id: row.get(0)?,
            timestamp: row.get(1)?,
            battery: row.get(2)?,
            capacity: row.get(3)?,
            status: BatteryStatus::from_str_opt(status_str.as_deref()),
            voltage_now: row.get(5)?,
            current_now: row.get(6)?,
            power_now: row.get(7)?,
            energy_now: row.get(8)?,
            energy_full: row.get(9)?,
            energy_full_design: row.get(10)?,
            cycle_count: row.get(11)?,
            temperature: row.get(12)?,
        })
    }
}

impl BatteryRecord {
    pub fn timestamp_utc(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.timestamp, 0).unwrap_or_default()
    }

    pub fn capacity_pct(&self) -> f64 {
        self.capacity.unwrap_or(0) as f64
    }

    pub fn power_watts(&self) -> Option<f64> {
        self.power_now.map(|p| p as f64 / 1_000_000.0)
    }

    pub fn voltage_volts(&self) -> Option<f64> {
        self.voltage_now.map(|v| v as f64 / 1_000_000.0)
    }

    pub fn energy_wh(&self) -> Option<f64> {
        self.energy_now.map(|e| e as f64 / 1_000_000.0)
    }

    pub fn energy_full_wh(&self) -> Option<f64> {
        self.energy_full.map(|e| e as f64 / 1_000_000.0)
    }

    pub fn health_pct(&self) -> Option<f64> {
        match (self.energy_full, self.energy_full_design) {
            (Some(full), Some(design)) if design > 0 => Some(full as f64 / design as f64 * 100.0),
            _ => None,
        }
    }
}

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open_or_exit(path: &Path) -> Self {
        match Self::open(path) {
            Ok(db) => db,
            Err(e) => {
                eprintln!("Failed to open database at {}: {}", path.display(), e);
                std::process::exit(1);
            }
        }
    }

    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.init()?;
        Ok(db)
    }

    fn init(&self) -> Result<()> {
        self.conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS battery_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp INTEGER NOT NULL,
                battery TEXT NOT NULL,
                capacity INTEGER,
                status TEXT,
                voltage_now INTEGER,
                current_now INTEGER,
                power_now INTEGER,
                energy_now INTEGER,
                energy_full INTEGER,
                energy_full_design INTEGER,
                cycle_count INTEGER,
                temperature INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_battery_log_timestamp ON battery_log(timestamp);
            CREATE INDEX IF NOT EXISTS idx_battery_log_battery ON battery_log(battery);
            CREATE INDEX IF NOT EXISTS idx_battery_log_battery_timestamp ON battery_log(battery, timestamp);
            ",
        )?;
        Ok(())
    }

    pub fn insert(&self, record: &BatteryRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO battery_log (
                timestamp, battery, capacity, status, voltage_now,
                current_now, power_now, energy_now, energy_full,
                energy_full_design, cycle_count, temperature
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                record.timestamp,
                record.battery,
                record.capacity,
                record.status.as_str(),
                record.voltage_now,
                record.current_now,
                record.power_now,
                record.energy_now,
                record.energy_full,
                record.energy_full_design,
                record.cycle_count,
                record.temperature,
            ],
        )?;
        Ok(())
    }

    pub fn query_range(&self, battery: &str, start: i64, end: i64) -> Result<Vec<BatteryRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, battery, capacity, status, voltage_now,
                    current_now, power_now, energy_now, energy_full,
                    energy_full_design, cycle_count, temperature
             FROM battery_log
             WHERE battery = ?1 AND timestamp >= ?2 AND timestamp <= ?3
             ORDER BY timestamp ASC",
        )?;

        let records = stmt
            .query_map(params![battery, start, end], BatteryRecord::from_row)?
            .collect::<Result<Vec<_>>>()?;
        Ok(records)
    }

    pub fn query_latest(&self, battery: &str) -> Result<Option<BatteryRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, battery, capacity, status, voltage_now,
                    current_now, power_now, energy_now, energy_full,
                    energy_full_design, cycle_count, temperature
             FROM battery_log
             WHERE battery = ?1
             ORDER BY timestamp DESC
             LIMIT 1",
        )?;

        let mut rows = stmt.query_map(params![battery], BatteryRecord::from_row)?;
        match rows.next() {
            Some(r) => Ok(Some(r?)),
            None => Ok(None),
        }
    }

    pub fn list_batteries(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT battery FROM battery_log ORDER BY battery")?;
        let names = stmt
            .query_map([], |row| row.get(0))?
            .collect::<Result<Vec<String>>>()?;
        Ok(names)
    }

    pub fn purge_old(&self, retention_days: u64) -> Result<usize> {
        let cutoff = Utc::now().timestamp() - (retention_days as i64 * 86400);
        let count = self.conn.execute(
            "DELETE FROM battery_log WHERE timestamp < ?1",
            params![cutoff],
        )?;
        Ok(count)
    }

    pub fn record_count(&self) -> Result<i64> {
        self.conn
            .query_row("SELECT COUNT(*) FROM battery_log", [], |row| row.get(0))
    }

    pub fn oldest_timestamp(&self) -> Result<Option<i64>> {
        self.conn
            .query_row("SELECT MIN(timestamp) FROM battery_log", [], |row| {
                row.get(0)
            })
    }

    pub fn query_range_downsampled(
        &self,
        battery: &str,
        start: i64,
        end: i64,
        interval_secs: i64,
    ) -> Result<Vec<BatteryRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT
                NULL,
                (timestamp / ?4) * ?4 as ts_bucket,
                battery,
                CAST(AVG(capacity) AS INTEGER),
                status,
                CAST(AVG(voltage_now) AS INTEGER),
                CAST(AVG(current_now) AS INTEGER),
                CAST(AVG(power_now) AS INTEGER),
                CAST(AVG(energy_now) AS INTEGER),
                CAST(AVG(energy_full) AS INTEGER),
                CAST(AVG(energy_full_design) AS INTEGER),
                CAST(AVG(cycle_count) AS INTEGER),
                CAST(AVG(temperature) AS INTEGER)
             FROM battery_log
             WHERE battery = ?1 AND timestamp >= ?2 AND timestamp <= ?3
             GROUP BY ts_bucket
             ORDER BY ts_bucket ASC",
        )?;

        let records = stmt
            .query_map(
                params![battery, start, end, interval_secs],
                BatteryRecord::from_row,
            )?
            .collect::<Result<Vec<_>>>()?;
        Ok(records)
    }
}
