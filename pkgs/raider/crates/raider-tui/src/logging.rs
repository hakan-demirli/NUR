use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{
    fmt::{format::Writer, time::FormatTime, FmtContext, FormatEvent, FormatFields},
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
    EnvFilter,
};

#[derive(Debug, Clone, Copy)]
pub struct LoggingConfig {
    pub max_files: usize,
    pub max_age_days: u64,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            max_files: 10,
            max_age_days: 14,
        }
    }
}

pub fn init(config: LoggingConfig) -> Result<WorkerGuard, std::io::Error> {
    let logs_dir = logs_dir().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "could not resolve XDG_STATE_HOME or $HOME for raider log dir",
        )
    })?;
    fs::create_dir_all(&logs_dir)?;
    rotate_logs(&logs_dir, "raider_", &config);

    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H-%M-%S");
    let pid = std::process::id();
    let filename = format!("raider_{timestamp}_{pid}.log");
    let log_path = logs_dir.join(&filename);

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    refresh_symlink(&logs_dir, &filename);

    let (non_blocking, guard) = tracing_appender::non_blocking(log_file);

    let env_filter = EnvFilter::try_from_env("RAIDER_LOG_LEVEL")
        .or_else(|_| EnvFilter::try_from_default_env())
        .unwrap_or_else(|_| EnvFilter::new("info"));

    let workspace_root = workspace_root();

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_target(false)
        .with_thread_ids(false)
        .with_thread_names(false)
        .with_line_number(true)
        .with_file(true)
        .with_level(true)
        .event_format(RaiderFormatter {
            workspace_root: workspace_root.clone(),
        });

    let registry = tracing_subscriber::registry()
        .with(env_filter)
        .with(file_layer);

    if std::env::var_os("RAIDER_LOG_STDERR").is_some() {
        let stderr_layer = tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr)
            .with_ansi(true)
            .with_target(false)
            .with_line_number(true)
            .with_file(true)
            .with_level(true)
            .event_format(RaiderFormatter { workspace_root });
        registry.with(stderr_layer).init();
    } else {
        registry.init();
    }

    tracing::info!(target: "raider::logging", path = %log_path.display(), "logger initialised");
    Ok(guard)
}

fn logs_dir() -> Option<PathBuf> {
    if let Some(xdg) = std::env::var_os("XDG_STATE_HOME") {
        return Some(PathBuf::from(xdg).join("raider").join("logs"));
    }
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".local")
            .join("state")
            .join("raider")
            .join("logs"),
    )
}

fn refresh_symlink(logs_dir: &Path, filename: &str) {
    let symlink_path = logs_dir.join("raider.log");
    let _ = fs::remove_file(&symlink_path);
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink as unix_symlink;
        let _ = unix_symlink(Path::new(filename), &symlink_path);
    }
}

fn workspace_root() -> PathBuf {
    if let Ok(p) = std::env::var("CARGO_MANIFEST_DIR") {
        let path = PathBuf::from(p);
        if let Some(parent) = path.parent().and_then(|p| p.parent()) {
            return parent.to_path_buf();
        }
    }
    let mut current = std::env::current_dir().unwrap_or_default();
    loop {
        if current.join("Cargo.toml").is_file() {
            if let Ok(content) = fs::read_to_string(current.join("Cargo.toml")) {
                if content.contains("[workspace]") {
                    return current;
                }
            }
        }
        match current.parent() {
            Some(p) => current = p.to_path_buf(),
            None => break,
        }
    }
    std::env::current_dir().unwrap_or_default()
}

fn rotate_logs(logs_dir: &Path, prefix: &str, config: &LoggingConfig) {
    let entries: Vec<PathBuf> = match fs::read_dir(logs_dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(prefix) && n.ends_with(".log"))
            })
            .collect(),
        Err(_) => return,
    };
    let mut entries = entries;
    entries.sort();

    if config.max_files > 0 && entries.len() > config.max_files {
        let to_delete = entries.len() - config.max_files;
        for path in entries.drain(0..to_delete) {
            let _ = fs::remove_file(path);
        }
    }

    if config.max_age_days > 0 {
        let now = SystemTime::now();
        let max_age = Duration::from_secs(config.max_age_days * 24 * 60 * 60);
        for path in &entries {
            let name = match path.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            let date_str = match name
                .strip_prefix(prefix)
                .and_then(|rest| rest.split('_').next())
            {
                Some(s) => s,
                None => continue,
            };
            let date = match chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                Ok(d) => d,
                Err(_) => continue,
            };
            let Some(midnight) = date.and_hms_opt(0, 0, 0) else {
                continue;
            };
            let Some(log_time) = midnight.and_local_timezone(chrono::Local).single() else {
                continue;
            };
            if let Ok(age) = now.duration_since(SystemTime::from(log_time)) {
                if age > max_age {
                    let _ = fs::remove_file(path);
                }
            }
        }
    }
}

struct LocalTime;

impl FormatTime for LocalTime {
    fn format_time(&self, w: &mut Writer<'_>) -> std::fmt::Result {
        write!(w, "{}", chrono::Local::now().format("%Y-%m-%d %H:%M:%S"))
    }
}

struct RaiderFormatter {
    workspace_root: PathBuf,
}

impl<S, N> FormatEvent<S, N> for RaiderFormatter
where
    S: tracing::Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &tracing::Event<'_>,
    ) -> std::fmt::Result {
        let metadata = event.metadata();
        write!(writer, "[")?;
        LocalTime.format_time(&mut writer)?;
        write!(writer, "] [{:5}] ", metadata.level())?;

        if let Some(file) = metadata.file() {
            let display = if file.starts_with('/') {
                let p = Path::new(file);
                match p.strip_prefix(&self.workspace_root) {
                    Ok(rel) => rel.display().to_string(),
                    Err(_) => file.to_string(),
                }
            } else if let Some(module) = metadata.module_path() {
                let crate_name = module.split("::").next().unwrap_or("");
                if crate_name.is_empty() {
                    file.to_string()
                } else {
                    format!("crates/{}/{}", crate_name.replace('_', "-"), file)
                }
            } else {
                file.to_string()
            };
            write!(writer, "{}:{} ", display, metadata.line().unwrap_or(0))?;
        }

        ctx.field_format().format_fields(writer.by_ref(), event)?;
        writeln!(writer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn rotate_logs_drops_oldest_by_count() {
        let dir = tempdir();
        let names = [
            "raider_2023-01-01_10-00-00_1.log",
            "raider_2023-01-02_10-00-00_1.log",
            "raider_2023-01-03_10-00-00_1.log",
            "raider_2023-01-04_10-00-00_1.log",
            "raider_2023-01-05_10-00-00_1.log",
        ];
        for n in names {
            File::create(dir.join(n)).unwrap();
        }
        File::create(dir.join("other.txt")).unwrap();

        rotate_logs(
            &dir,
            "raider_",
            &LoggingConfig {
                max_files: 3,
                max_age_days: 0,
            },
        );

        assert!(!dir.join(names[0]).exists(), "oldest gone");
        assert!(!dir.join(names[1]).exists(), "second-oldest gone");
        assert!(dir.join(names[2]).exists());
        assert!(dir.join(names[3]).exists());
        assert!(dir.join(names[4]).exists(), "newest kept");
        assert!(
            dir.join("other.txt").exists(),
            "non-log files left untouched"
        );

        cleanup(&dir);
    }

    #[test]
    fn rotate_logs_drops_aged_files() {
        let dir = tempdir();
        let today = chrono::Local::now();
        let yesterday = today - chrono::Duration::days(1);
        let old = today - chrono::Duration::days(8);
        let fmt = "%Y-%m-%d";
        let n_today = format!("raider_{}_10-00-00_1.log", today.format(fmt));
        let n_yesterday = format!("raider_{}_10-00-00_1.log", yesterday.format(fmt));
        let n_old = format!("raider_{}_10-00-00_1.log", old.format(fmt));
        File::create(dir.join(&n_today)).unwrap();
        File::create(dir.join(&n_yesterday)).unwrap();
        File::create(dir.join(&n_old)).unwrap();

        rotate_logs(
            &dir,
            "raider_",
            &LoggingConfig {
                max_files: 0,
                max_age_days: 7,
            },
        );

        assert!(dir.join(&n_today).exists());
        assert!(dir.join(&n_yesterday).exists());
        assert!(!dir.join(&n_old).exists(), "8-day-old file pruned");

        cleanup(&dir);
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "raider-log-test-{}-{}",
            std::process::id(),
            rand_suffix()
        ));
        fs::create_dir_all(&dir).unwrap();
        dir
    }
    fn cleanup(dir: &Path) {
        let _ = fs::remove_dir_all(dir);
    }
    fn rand_suffix() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos() as u64)
            .unwrap_or(0)
    }
}
