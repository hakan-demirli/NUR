//! Mirrors opencode's `api.app.version` rendering in
//! `43b51f09-cache-fixes` — opencode's own version label format is
//! `CARGO_PKG_VERSION`.
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/packed-refs");

    let hash = git_short_hash().unwrap_or_default();
    println!("cargo:rustc-env=RAIDER_GIT_SHORT_HASH={hash}");
}

fn git_short_hash() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if hash.is_empty() {
        None
    } else {
        Some(hash)
    }
}
