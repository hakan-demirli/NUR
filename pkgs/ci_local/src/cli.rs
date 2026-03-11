use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "ci-local", about = "Local nix-based CI runner daemon")]
pub struct Cli {
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[arg(short = 'r', long = "repo")]
    pub repo: Option<String>,

    #[arg(short = 'j', long = "jobs")]
    pub jobs: Option<u16>,

    #[command(subcommand)]
    pub command: Command,
}

impl Cli {
    pub fn resolve_config(&self) -> Option<PathBuf> {
        if let Some(ref p) = self.config {
            return Some(p.clone());
        }

        let local = PathBuf::from("ci-local.toml");
        if local.exists() {
            return Some(local);
        }

        if let Some(config_dir) = dirs() {
            let global = config_dir.join("ci-local.toml");
            if global.exists() {
                return Some(global);
            }
        }

        None
    }
}

fn dirs() -> Option<PathBuf> {
    std::env::var("XDG_CONFIG_HOME")
        .ok()
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".config"))
        })
        .map(|p| p.join("ci-local"))
}

#[derive(Debug, Subcommand)]
pub enum Command {
    Start,

    Status {
        #[arg(short, long)]
        repo: Option<String>,
    },

    Cancel {
        sha: String,
        #[arg(short, long)]
        repo: Option<String>,
    },

    CancelAll,

    Retry {
        #[arg(short, long)]
        repo: String,
        sha: String,
    },

    Shutdown,
}
