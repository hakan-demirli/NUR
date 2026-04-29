use clap::Parser;
use eframe::egui;

use hibat::app::HibatApp;
use hibat::collector;
use hibat::config::Config;
use hibat::db::Database;

#[derive(Parser)]
#[command(name = "hibat", about = "Battery history visualizer")]
struct Cli {
    #[arg(long)]
    collect: bool,
}

fn main() -> eframe::Result<()> {
    let cli = Cli::parse();
    let config = Config::load();
    config.save_default_if_missing();

    let db_path = Config::db_path();
    let db = Database::open_or_exit(&db_path);

    if cli.collect {
        let records = collector::collect_all(&config.collector.battery_path);
        for record in &records {
            if let Err(e) = db.insert(record) {
                eprintln!("Failed to insert: {}", e);
            }
        }
    }

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([config.ui.window_width, config.ui.window_height])
            .with_title("hibat - Battery History"),
        vsync: false,
        ..Default::default()
    };

    eframe::run_native(
        "hibat",
        options,
        Box::new(move |_cc| Ok(Box::new(HibatApp::new(config, db)))),
    )
}
