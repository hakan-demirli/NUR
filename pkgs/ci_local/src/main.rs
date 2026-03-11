mod cli;
mod client;
mod config;
mod daemon;
mod error;
mod ipc;
mod poller;
mod runner;
mod summary;
mod types;
mod workflow;

use clap::Parser;

fn default_socket_path() -> std::path::PathBuf {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(runtime_dir).join("ci-local.sock")
}

fn check_nix_installed() {
    match std::process::Command::new("nix").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            tracing::info!("nix found: {}", version.trim());
        }
        Ok(_) => {
            eprintln!("error: nix is installed but 'nix --version' failed. ci-local requires a working nix installation.");
            std::process::exit(1);
        }
        Err(_) => {
            eprintln!("error: nix is not installed. ci-local has a hard dependency on nix.");
            eprintln!("install nix: https://nixos.org/download.html");
            std::process::exit(1);
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    check_nix_installed();

    let cli = cli::Cli::parse();
    let socket_path = default_socket_path();

    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(e) => {
            eprintln!("failed to create tokio runtime: {e}");
            std::process::exit(1);
        }
    };

    let exit = |e: error::CiError| -> ! {
        eprintln!("{e}");
        std::process::exit(1);
    };

    let result = rt.block_on(async {
        match cli.command {
            cli::Command::Start => {
                let config_path = cli.resolve_config();
                let cfg = config::Config::load(&config_path).unwrap_or_else(|e| exit(e));
                daemon::run(cfg, socket_path).await
            }
            cli::Command::Status { repo } => {
                let req = client::status_request(repo).unwrap_or_else(|e| exit(e));
                client::send_request(&socket_path, req).await
            }
            cli::Command::Cancel { sha, repo } => {
                let req = client::cancel_request(sha, repo).unwrap_or_else(|e| exit(e));
                client::send_request(&socket_path, req).await
            }
            cli::Command::CancelAll => {
                client::send_request(&socket_path, client::cancel_all_request()).await
            }
            cli::Command::Retry { repo, sha } => {
                let req = client::retry_request(repo, sha).unwrap_or_else(|e| exit(e));
                client::send_request(&socket_path, req).await
            }
            cli::Command::Shutdown => {
                client::send_request(&socket_path, client::shutdown_request()).await
            }
        }
    });

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
