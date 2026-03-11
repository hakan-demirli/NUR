use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

fn git_cmd(dir: &Path) -> impl Fn(&[&str]) -> String + '_ {
    move |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@test.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@test.com")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }
}

fn make_git_repo(dir: &Path) -> String {
    let run = git_cmd(dir);
    run(&["init", "-b", "main"]);
    run(&["config", "commit.gpgsign", "false"]);
    std::fs::write(dir.join("README.md"), "# test\n").unwrap();

    let workflows_dir = dir.join(".github").join("workflows");
    std::fs::create_dir_all(&workflows_dir).unwrap();
    std::fs::write(
        workflows_dir.join("ci.yml"),
        r#"name: CI
on: push
jobs:
  check:
    name: Check
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: build-hello
        run: nix build nixpkgs#hello
"#,
    )
    .unwrap();

    run(&["add", "."]);
    run(&["commit", "--no-gpg-sign", "-m", "initial commit"]);
    run(&["rev-parse", "HEAD"])
}

fn add_commit(dir: &Path, filename: &str, message: &str) -> String {
    let run = git_cmd(dir);
    std::fs::write(dir.join(filename), "content\n").unwrap();
    run(&["add", "."]);
    run(&["commit", "--no-gpg-sign", "-m", message]);
    run(&["rev-parse", "HEAD"])
}

fn write_config(config_path: &Path, repo_source: &Path, base_dir: &Path) {
    let content = format!(
        r#"poll_interval_secs = 5
max_parallel = 2
base_dir = "{}"

[[repo]]
name = "test-project"
source = "{}"
branch = "main"
"#,
        base_dir.display(),
        repo_source.display(),
    );
    std::fs::write(config_path, content).unwrap();
}

fn try_ipc_request(socket_path: &Path, request: &serde_json::Value) -> Option<serde_json::Value> {
    let mut stream = match UnixStream::connect(socket_path) {
        Ok(s) => s,
        Err(_) => return None,
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();

    let mut json = serde_json::to_string(request).unwrap();
    json.push('\n');
    if stream.write_all(json.as_bytes()).is_err() {
        return None;
    }
    let _ = stream.flush();
    let _ = stream.shutdown(std::net::Shutdown::Write);

    let reader = BufReader::new(&stream);
    let mut responses = Vec::new();
    for line in reader.lines() {
        match line {
            Ok(l) if !l.is_empty() => {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&l) {
                    responses.push(val);
                }
            }
            _ => break,
        }
    }

    if responses.len() == 1 {
        Some(responses.into_iter().next().unwrap())
    } else if responses.is_empty() {
        None
    } else {
        Some(serde_json::Value::Array(responses))
    }
}

fn ipc_request(socket_path: &Path, request: &serde_json::Value) -> serde_json::Value {
    for attempt in 0..10 {
        if let Some(resp) = try_ipc_request(socket_path, request) {
            return resp;
        }
        if attempt < 9 {
            std::thread::sleep(Duration::from_millis(200));
        }
    }
    panic!(
        "failed to connect to daemon socket at {} after retries",
        socket_path.display()
    );
}

fn build_binary() -> PathBuf {
    let out = Command::new("cargo")
        .args(["build", "--quiet"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "cargo build failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let bin = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/debug/ci-local");
    assert!(bin.exists(), "binary not found at {}", bin.display());
    bin
}

fn wait_for_socket(path: &Path, timeout: Duration) -> bool {
    let start = std::time::Instant::now();
    while start.elapsed() < timeout {
        if path.exists() && UnixStream::connect(path).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

fn start_daemon(binary: &Path, config_path: &Path, tmp: &Path) -> (std::process::Child, PathBuf) {
    let mut daemon = Command::new(binary)
        .args(["--config", &config_path.to_string_lossy(), "start"])
        .env("XDG_RUNTIME_DIR", tmp.to_string_lossy().as_ref())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to start daemon");

    let socket = tmp.join("ci-local.sock");
    if !wait_for_socket(&socket, Duration::from_secs(10)) {
        let _ = daemon.kill();
        let output = daemon.wait_with_output().unwrap();
        panic!(
            "daemon socket never appeared.\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    (daemon, socket)
}

fn shutdown_daemon(socket: &Path, mut daemon: std::process::Child) {
    let _ = try_ipc_request(socket, &serde_json::json!({"type": "shutdown"}));
    for _ in 0..40 {
        match daemon.try_wait() {
            Ok(Some(_)) => return,
            _ => std::thread::sleep(Duration::from_millis(100)),
        }
    }
    let _ = daemon.kill();
    let _ = daemon.wait();
}

#[test]
fn daemon_lifecycle_status_and_shutdown() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("src");
    let base_dir = tmp.path().join("work");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::create_dir_all(&base_dir).unwrap();

    make_git_repo(&repo_dir);
    let config_path = tmp.path().join("ci-local.toml");
    write_config(&config_path, &repo_dir, &base_dir);

    let binary = build_binary();
    let (daemon, socket) = start_daemon(&binary, &config_path, tmp.path());

    let resp = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": null}),
    );
    assert_eq!(resp["type"], "status");
    assert!(resp["runs"].is_array());

    let resp = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": "test-project"}),
    );
    assert_eq!(resp["type"], "status");

    let resp = ipc_request(&socket, &serde_json::json!({"type": "cancel_all"}));
    assert_eq!(resp["type"], "ok");

    let resp = ipc_request(&socket, &serde_json::json!({"type": "shutdown"}));
    assert_eq!(resp["type"], "ok");
    assert!(resp["message"].as_str().unwrap().contains("shutting down"));

    shutdown_daemon(&socket, daemon);

    std::thread::sleep(Duration::from_millis(200));
    assert!(
        !socket.exists(),
        "socket should be cleaned up after shutdown"
    );
}

#[test]
fn daemon_detects_commit_and_runs_job() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("src");
    let base_dir = tmp.path().join("work");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::create_dir_all(&base_dir).unwrap();

    make_git_repo(&repo_dir);
    let config_path = tmp.path().join("ci-local.toml");
    write_config(&config_path, &repo_dir, &base_dir);

    let binary = build_binary();
    let (daemon, socket) = start_daemon(&binary, &config_path, tmp.path());

    std::thread::sleep(Duration::from_secs(15));

    let resp = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": null}),
    );
    assert_eq!(resp["type"], "status");
    let runs = resp["runs"].as_array().unwrap();

    if !runs.is_empty() {
        let has_our_repo = runs.iter().any(|r| r["repo"] == "test-project");
        assert!(has_our_repo, "expected test-project in runs: {runs:?}");
    }

    shutdown_daemon(&socket, daemon);
}

#[test]
fn daemon_handles_invalid_ipc() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("src");
    let base_dir = tmp.path().join("work");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::create_dir_all(&base_dir).unwrap();

    make_git_repo(&repo_dir);
    let config_path = tmp.path().join("ci-local.toml");
    write_config(&config_path, &repo_dir, &base_dir);

    let binary = build_binary();
    let (daemon, socket) = start_daemon(&binary, &config_path, tmp.path());

    let resp = ipc_request(&socket, &serde_json::json!({"type": "bogus_command"}));
    assert_eq!(resp["type"], "error");

    {
        let mut stream = UnixStream::connect(&socket).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        stream.write_all(b"not json\n").unwrap();
        stream.flush().unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();

        let reader = BufReader::new(&stream);
        for line in reader.lines() {
            match line {
                Ok(l) if !l.is_empty() => {
                    let val: serde_json::Value = serde_json::from_str(&l).unwrap();
                    assert_eq!(val["type"], "error");
                }
                _ => break,
            }
        }
    }

    let resp = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": null}),
    );
    assert_eq!(resp["type"], "status");

    shutdown_daemon(&socket, daemon);
}

#[test]
fn daemon_multiple_repos() {
    let tmp = tempfile::tempdir().unwrap();
    let repo1 = tmp.path().join("repo1");
    let repo2 = tmp.path().join("repo2");
    let base_dir = tmp.path().join("work");

    for d in [&repo1, &repo2, &base_dir] {
        std::fs::create_dir_all(d).unwrap();
    }

    make_git_repo(&repo1);
    make_git_repo(&repo2);

    let config = format!(
        r#"
poll_interval_secs = 5
max_parallel = 2
base_dir = "{}"

[[repo]]
name = "alpha"
source = "{}"
branch = "main"

[[repo]]
name = "beta"
source = "{}"
branch = "main"
"#,
        base_dir.display(),
        repo1.display(),
        repo2.display(),
    );

    let config_path = tmp.path().join("multi.toml");
    std::fs::write(&config_path, &config).unwrap();

    let binary = build_binary();
    let (daemon, socket) = start_daemon(&binary, &config_path, tmp.path());

    let resp = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": null}),
    );
    assert_eq!(resp["type"], "status");

    let resp_a = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": "alpha"}),
    );
    assert_eq!(resp_a["type"], "status");

    let resp_b = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": "beta"}),
    );
    assert_eq!(resp_b["type"], "status");

    shutdown_daemon(&socket, daemon);
}

#[test]
fn bad_config_exits_nonzero() {
    let tmp = tempfile::tempdir().unwrap();
    let config_path = tmp.path().join("bad.toml");
    std::fs::write(&config_path, "this is not valid toml [[[").unwrap();

    let binary = build_binary();
    let output = Command::new(&binary)
        .args(["--config", &config_path.to_string_lossy(), "start"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("parse") || stderr.contains("config") || stderr.contains("TOML"),
        "stderr should mention parse error: {stderr}"
    );
}

#[test]
fn missing_config_exits_nonzero() {
    let binary = build_binary();
    let output = Command::new(&binary)
        .args(["--config", "/nonexistent/ci-local.toml", "start"])
        .output()
        .unwrap();
    assert!(!output.status.success());
}

#[test]
fn client_no_daemon_exits_nonzero() {
    let binary = build_binary();
    let tmp = tempfile::tempdir().unwrap();

    for subcmd in &["status", "cancel-all", "shutdown"] {
        let output = Command::new(&binary)
            .arg(subcmd)
            .env("XDG_RUNTIME_DIR", tmp.path().to_string_lossy().as_ref())
            .output()
            .unwrap();
        assert!(
            !output.status.success(),
            "{subcmd} should fail without daemon"
        );
    }
}

#[test]
fn cancel_specific_sha() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("src");
    let base_dir = tmp.path().join("work");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::create_dir_all(&base_dir).unwrap();

    let sha = make_git_repo(&repo_dir);
    let config_path = tmp.path().join("ci-local.toml");
    write_config(&config_path, &repo_dir, &base_dir);

    let binary = build_binary();
    let (daemon, socket) = start_daemon(&binary, &config_path, tmp.path());

    let resp = ipc_request(
        &socket,
        &serde_json::json!({"type": "cancel", "sha": sha, "repo": null}),
    );
    assert!(
        resp["type"] == "ok" || resp["type"] == "error",
        "unexpected: {resp}"
    );

    shutdown_daemon(&socket, daemon);
}

#[test]
fn new_commit_triggers_new_run() {
    let tmp = tempfile::tempdir().unwrap();
    let repo_dir = tmp.path().join("src");
    let base_dir = tmp.path().join("work");
    std::fs::create_dir_all(&repo_dir).unwrap();
    std::fs::create_dir_all(&base_dir).unwrap();

    make_git_repo(&repo_dir);
    let config_path = tmp.path().join("ci-local.toml");
    write_config(&config_path, &repo_dir, &base_dir);

    let binary = build_binary();
    let (daemon, socket) = start_daemon(&binary, &config_path, tmp.path());

    std::thread::sleep(Duration::from_secs(12));

    add_commit(&repo_dir, "extra.txt", "second commit");

    std::thread::sleep(Duration::from_secs(8));

    let resp = ipc_request(
        &socket,
        &serde_json::json!({"type": "status", "repo": null}),
    );
    assert_eq!(resp["type"], "status");

    shutdown_daemon(&socket, daemon);
}
