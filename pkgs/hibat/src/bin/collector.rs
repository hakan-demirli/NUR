use std::thread;
use std::time::Duration;

use clap::Parser;

use hibat::collector;
use hibat::config::Config;
use hibat::db::Database;

#[derive(Parser)]
#[command(name = "hibat-collector", about = "Battery stats collector daemon")]
struct Cli {
    #[arg(long)]
    once: bool,

    #[arg(long)]
    interval: Option<u64>,

    #[arg(long)]
    verbose: bool,
}

fn main() {
    let cli = Cli::parse();
    let config = Config::load();
    config.save_default_if_missing();

    let db_path = Config::db_path();
    let db = Database::open_or_exit(&db_path);

    match db.purge_old(config.collector.retention_days) {
        Ok(n) if n > 0 && cli.verbose => {
            eprintln!("Purged {} old records", n);
        }
        _ => {}
    }

    let interval = cli.interval.unwrap_or(config.polling_interval_secs);
    let battery_path = &config.collector.battery_path;

    if cli.once {
        collect_and_store(&db, battery_path, cli.verbose);
    } else {
        eprintln!(
            "hibat-collector: logging every {}s to {}",
            interval,
            db_path.display()
        );
        loop {
            collect_and_store(&db, battery_path, cli.verbose);
            thread::sleep(Duration::from_secs(interval));
        }
    }
}

fn collect_and_store(db: &Database, battery_path: &str, verbose: bool) {
    let records = collector::collect_all(battery_path);
    if records.is_empty() {
        if verbose {
            eprintln!("No batteries found at {}", battery_path);
        }
        return;
    }

    for record in &records {
        if verbose {
            eprintln!(
                "[{}] {} capacity={}% status={} power={:.2}W voltage={:.2}V",
                record.timestamp_utc().format("%Y-%m-%d %H:%M:%S"),
                record.battery,
                record.capacity.unwrap_or(-1),
                record.status,
                record.power_watts().unwrap_or(0.0),
                record.voltage_volts().unwrap_or(0.0),
            );
        }
        if let Err(e) = db.insert(record) {
            eprintln!("Failed to insert record: {}", e);
        }
    }
}
