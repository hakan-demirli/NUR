use crate::config::{Config, RepoConfig};
use crate::error::CiError;
use crate::ipc::{Request, Response, RunState, RunSummary};
use crate::poller;
use crate::runner::{self, JobResult, RunResult};
use crate::summary;
use crate::types::{AttemptNumber, CommitSha, RepoName};
use crate::workflow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch, Mutex, Semaphore};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RunKey {
    repo: String,
    sha: String,
    attempt: AttemptNumber,
}

struct ActiveRun {
    repo_name: RepoName,
    sha: CommitSha,
    commit_message: String,
    attempt: AttemptNumber,
    cancel_tx: watch::Sender<bool>,
    handle: tokio::task::JoinHandle<RunResult>,
    repo_dir: PathBuf,
}

struct RepoState {
    last_seen_sha: Option<CommitSha>,
    cancelled_shas: std::collections::HashSet<String>,
    completed_shas: std::collections::HashSet<String>,
    initialized: bool,
    run_counter: u32,
}

impl RepoState {
    fn new(repo_dir: &Path) -> Self {
        let max_on_disk = scan_max_run_number(repo_dir);

        let mut cancelled_shas = std::collections::HashSet::new();
        let mut completed_shas = std::collections::HashSet::new();
        let mut last_seen_sha: Option<CommitSha> = None;

        let summary_path = repo_dir.join("summary.json");
        if let Ok(content) = std::fs::read_to_string(&summary_path) {
            if let Ok(runs) = serde_json::from_str::<Vec<RunResult>>(&content) {
                for run in &runs {
                    if run.cancelled {
                        cancelled_shas.insert(run.sha.as_str().to_string());
                    } else if run.is_complete() {
                        completed_shas.insert(run.sha.as_str().to_string());
                    }
                }
                if let Some(last_run) = runs.last() {
                    last_seen_sha = Some(last_run.sha.clone());
                }
            }
        }

        let initialized = last_seen_sha.is_some();

        Self {
            last_seen_sha,
            cancelled_shas,
            completed_shas,
            initialized,
            run_counter: max_on_disk,
        }
    }

    fn next_run_number(&mut self) -> u32 {
        let n = self.run_counter;
        self.run_counter = n.saturating_add(1);
        n
    }

    fn should_skip(&self, sha: &CommitSha) -> bool {
        let s = sha.as_str();
        self.cancelled_shas.contains(s) || self.completed_shas.contains(s)
    }
}

fn scan_max_run_number(repo_dir: &Path) -> u32 {
    let mut max: u32 = 0;
    if let Ok(entries) = std::fs::read_dir(repo_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(num_str) = name.split('_').next() {
                    if let Ok(n) = num_str.parse::<u32>() {
                        if n >= max {
                            max = n + 1;
                        }
                    }
                }
            }
        }
    }
    max
}

fn next_attempt_in(run_dir: &Path) -> AttemptNumber {
    let mut max: u32 = 0;
    if let Ok(entries) = std::fs::read_dir(run_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if let Some(n_str) = name.strip_prefix("attempt-") {
                    if let Ok(n) = n_str.parse::<u32>() {
                        if n > max {
                            max = n;
                        }
                    }
                }
            }
        }
    }
    if max == 0 {
        AttemptNumber::first()
    } else {
        AttemptNumber::try_from(max + 1).unwrap_or_else(|_| AttemptNumber::first())
    }
}

fn find_run_dir_for_sha(repo_dir: &Path, sha: &CommitSha) -> Option<PathBuf> {
    let suffix = format!("_{}", sha.short());
    if let Ok(entries) = std::fs::read_dir(repo_dir) {
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(&suffix) && entry.path().is_dir() {
                    return Some(entry.path());
                }
            }
        }
    }
    None
}

struct DaemonState {
    active: HashMap<RunKey, ActiveRun>,
    finished: HashMap<String, Vec<RunResult>>,
    repos: HashMap<String, RepoState>,
    timeout_secs: Option<u64>,
}

impl DaemonState {
    fn new(repo_configs: &[RepoConfig], timeout_secs: Option<u64>) -> Self {
        let mut repos = HashMap::new();
        for rc in repo_configs {
            repos.insert(rc.name.as_str().to_string(), RepoState::new(&rc.repo_dir));
        }
        Self {
            active: HashMap::new(),
            finished: HashMap::new(),
            repos,
            timeout_secs,
        }
    }
}

pub async fn run(config: Config, socket_path: PathBuf) -> Result<(), CiError> {
    if socket_path.exists() {
        let _ = std::fs::remove_file(&socket_path);
    }

    let listener = UnixListener::bind(&socket_path).map_err(|e| CiError::SocketBind {
        path: socket_path.clone(),
        source: e,
    })?;

    tracing::info!("daemon listening on {}", socket_path.display());
    tracing::info!("base_dir: {}", config.base_dir.path().display());
    tracing::info!("max_parallel: {}", config.max_parallel.get());
    if let Some(t) = config.timeout_secs {
        tracing::info!("timeout: {}s per job", t);
    }
    for repo in &config.repos {
        tracing::info!(
            "  repo '{}': {} branch {} -> {}",
            repo.name,
            repo.source,
            repo.branch,
            repo.repo_dir.display()
        );
    }

    let semaphore = Arc::new(Semaphore::new(config.max_parallel.get() as usize));

    let daemon_state = DaemonState::new(&config.repos, config.timeout_secs);
    for (name, rs) in &daemon_state.repos {
        if rs.initialized {
            let sha_short = rs
                .last_seen_sha
                .as_ref()
                .map(|s| s.short().to_string())
                .unwrap_or_else(|| "?".to_string());
            tracing::info!(
                "  repo '{}': resuming from {}, {} completed, {} cancelled",
                name,
                sha_short,
                rs.completed_shas.len(),
                rs.cancelled_shas.len(),
            );
        } else {
            tracing::info!("  repo '{}': fresh start (no prior runs)", name);
        }
    }
    let state = Arc::new(Mutex::new(daemon_state));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);

    let ipc_state = Arc::clone(&state);
    let ipc_shutdown_tx = shutdown_tx.clone();
    let ipc_config = config.clone();
    let ipc_semaphore = Arc::clone(&semaphore);
    let ipc_handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                accept = listener.accept() => {
                    match accept {
                        Ok((stream, _)) => {
                            let st = Arc::clone(&ipc_state);
                            let stx = ipc_shutdown_tx.clone();
                            let cfg = ipc_config.clone();
                            let sem = Arc::clone(&ipc_semaphore);
                            tokio::spawn(handle_client(stream, st, stx, cfg, sem));
                        }
                        Err(e) => {
                            tracing::error!("accept error: {e}");
                        }
                    }
                }
                _ = shutdown_rx.changed() => {
                    if *shutdown_rx.borrow() {
                        tracing::info!("IPC listener shutting down");
                        break;
                    }
                }
            }
        }
    });

    let poll_config = config.clone();
    let poll_state = Arc::clone(&state);
    let poll_semaphore = Arc::clone(&semaphore);
    let mut poll_shutdown_rx = shutdown_tx.subscribe();

    let poll_handle = tokio::spawn(async move {
        let mut interval = tokio::time::interval(poll_config.poll_interval.as_duration());
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    reap_finished(&poll_state, &poll_config).await;

                    for repo_cfg in &poll_config.repos {
                        if let Err(e) = poll_repo(
                            repo_cfg,
                            &poll_state,
                            Arc::clone(&poll_semaphore),
                        ).await {
                            tracing::error!(repo = %repo_cfg.name, "poll error: {e}");
                        }
                    }
                }
                _ = poll_shutdown_rx.changed() => {
                    if *poll_shutdown_rx.borrow() {
                        tracing::info!("poller shutting down");
                        break;
                    }
                }
            }
        }
    });

    let mut main_shutdown_rx = shutdown_tx.subscribe();
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received ctrl-c, shutting down");
            let _ = shutdown_tx.send(true);
        }
        _ = async {
            loop {
                if *main_shutdown_rx.borrow_and_update() {
                    break;
                }
                if main_shutdown_rx.changed().await.is_err() {
                    break;
                }
            }
        } => {
            tracing::info!("shutdown requested via IPC");
        }
    }

    kill_all_active(&state, &config).await;

    let _ = ipc_handle.await;
    let _ = poll_handle.await;

    let _ = std::fs::remove_file(&socket_path);
    tracing::info!("daemon stopped");
    Ok(())
}

async fn kill_all_active(state: &Arc<Mutex<DaemonState>>, config: &Config) {
    let mut guard = state.lock().await;
    for (_, run) in guard.active.drain() {
        let _ = run.cancel_tx.send(true);
        run.handle.abort();
        if let Some(repo_cfg) = config.repos.iter().find(|r| r.name == run.repo_name) {
            poller::cleanup_worktree(&repo_cfg.source, &run.repo_dir);
        }
    }
}

async fn poll_repo(
    repo_cfg: &RepoConfig,
    state: &Arc<Mutex<DaemonState>>,
    semaphore: Arc<Semaphore>,
) -> Result<(), CiError> {
    let mirror_dir = repo_cfg.repo_dir.join(".mirror");
    let latest = poller::latest_commit(&repo_cfg.source, &repo_cfg.branch)?;

    let mut guard = state.lock().await;

    let (to_launch, skipped) = {
        let repo_state =
            guard
                .repos
                .get_mut(repo_cfg.name.as_str())
                .ok_or_else(|| CiError::Internal {
                    detail: format!("no state for repo '{}'", repo_cfg.name),
                })?;

        if let Some(ref last) = repo_state.last_seen_sha {
            if *last == latest.sha {
                return Ok(());
            }
        }

        let commits_to_run = if !repo_state.initialized {
            repo_state.initialized = true;
            tracing::info!(
                repo = %repo_cfg.name,
                "first poll, starting from latest: {} {}",
                latest.sha.short(),
                latest.message,
            );
            vec![latest.clone()]
        } else if let Some(ref last_sha) = repo_state.last_seen_sha {
            match poller::list_commits_since(
                &repo_cfg.source,
                &repo_cfg.branch,
                last_sha,
                &mirror_dir,
            ) {
                Ok(commits) if !commits.is_empty() => {
                    tracing::info!(
                        repo = %repo_cfg.name,
                        "discovered {} new commit(s) since {}",
                        commits.len(),
                        last_sha.short(),
                    );
                    commits
                }
                Ok(_) => {
                    tracing::warn!(
                        repo = %repo_cfg.name,
                        "last known {} not reachable from branch tip; starting from latest {}",
                        last_sha.short(),
                        latest.sha.short(),
                    );
                    vec![latest.clone()]
                }
                Err(e) => {
                    tracing::warn!(
                        repo = %repo_cfg.name,
                        "failed to list commits since {}: {e}; falling back to latest",
                        last_sha.short(),
                    );
                    vec![latest.clone()]
                }
            }
        } else {
            vec![latest.clone()]
        };

        repo_state.last_seen_sha = Some(latest.sha.clone());

        let mut to_launch = Vec::new();
        let mut skipped = 0usize;
        for commit in &commits_to_run {
            if repo_state.should_skip(&commit.sha) {
                tracing::debug!(
                    repo = %repo_cfg.name,
                    "skipping {} (already completed or cancelled)",
                    commit.sha.short(),
                );
                skipped += 1;
                continue;
            }
            repo_state
                .completed_shas
                .insert(commit.sha.as_str().to_string());
            to_launch.push(commit.clone());
        }

        (to_launch, skipped)
    };

    let queued = to_launch.len();
    for commit in &to_launch {
        launch_run(
            &mut guard,
            repo_cfg,
            &commit.sha,
            &commit.message,
            Arc::clone(&semaphore),
            false,
        )?;
    }

    if queued > 0 || skipped > 0 {
        tracing::info!(
            repo = %repo_cfg.name,
            "queued {queued} run(s), skipped {skipped}",
        );
    }

    Ok(())
}

fn launch_run(
    state: &mut DaemonState,
    repo_cfg: &RepoConfig,
    sha: &CommitSha,
    commit_message: &str,
    semaphore: Arc<Semaphore>,
    is_retry: bool,
) -> Result<AttemptNumber, CiError> {
    let repo_state =
        state
            .repos
            .get_mut(repo_cfg.name.as_str())
            .ok_or_else(|| CiError::Internal {
                detail: format!("no state for repo '{}'", repo_cfg.name),
            })?;

    let run_dir = if is_retry {
        find_run_dir_for_sha(&repo_cfg.repo_dir, sha).unwrap_or_else(|| {
            let run_num = repo_state.next_run_number();
            repo_cfg
                .repo_dir
                .join(format!("{:05}_{}", run_num, sha.short()))
        })
    } else {
        let run_num = repo_state.next_run_number();
        repo_cfg
            .repo_dir
            .join(format!("{:05}_{}", run_num, sha.short()))
    };

    let attempt = next_attempt_in(&run_dir);

    tracing::info!(
        repo = %repo_cfg.name,
        "launching {} attempt {} in {}",
        sha.short(),
        attempt,
        run_dir.display(),
    );

    let attempt_dir = run_dir.join(format!("attempt-{}", attempt));
    let checkout_dir = attempt_dir.join("repo");

    if let Err(e) = std::fs::create_dir_all(&checkout_dir) {
        return Err(CiError::Internal {
            detail: format!("failed to create checkout dir: {e}"),
        });
    }

    poller::prepare_worktree(&repo_cfg.source, sha, &checkout_dir)?;

    let message = if commit_message.is_empty() {
        poller::read_commit_message(&checkout_dir, sha)
    } else {
        commit_message.to_string()
    };

    let workflows = workflow::parse_workflows(&checkout_dir)?;
    let jobs = workflow::flatten_workflows(&workflows);

    if jobs.is_empty() {
        tracing::warn!(
            repo = %repo_cfg.name,
            "no nix jobs found in workflow files, skipping"
        );
        poller::cleanup_worktree(&repo_cfg.source, &checkout_dir);
        return Ok(attempt);
    }

    let total_steps: usize = jobs.iter().map(|j| j.steps.len()).sum();
    let wf_names: Vec<&str> = workflows.iter().map(|w| w.name.as_str()).collect();
    tracing::info!(
        repo = %repo_cfg.name,
        "found {} jobs with {} nix steps from workflow(s): {}",
        jobs.len(),
        total_steps,
        wf_names.join(", "),
    );

    let (cancel_tx, cancel_rx) = watch::channel(false);

    let sha_clone = sha.clone();
    let repo_name = repo_cfg.name.clone();
    let attempt_dir_clone = attempt_dir.clone();
    let checkout_dir_clone = checkout_dir.clone();
    let repo_dir_for_summary = repo_cfg.repo_dir.clone();
    let msg = message.clone();
    let timeout_secs = state.timeout_secs;
    let total_jobs = jobs.len();
    let all_job_names: Vec<String> = jobs.iter().map(|j| j.name.clone()).collect();
    let repo_name_str_for_log = repo_cfg.name.as_str().to_string();
    let handle = tokio::spawn(async move {
        let (job_tx, mut job_rx) = mpsc::channel::<JobResult>(64);

        let repo_name_for_log = repo_name.clone();
        let sha_for_log = sha_clone.clone();
        let attempt_dir_for_log = attempt_dir_clone.clone();
        let repo_dir_for_log = repo_dir_for_summary.clone();
        let msg_for_log = msg.clone();
        let all_job_names_for_log = all_job_names.clone();

        let log_handle = tokio::spawn(async move {
            let mut completed_jobs: Vec<JobResult> = Vec::new();
            while let Some(job_result) = job_rx.recv().await {
                let line = summary::format_job_log_line(
                    repo_name_for_log.as_str(),
                    sha_for_log.short(),
                    &job_result,
                );
                tracing::info!("{line}");

                completed_jobs.push(job_result);

                let partial = RunResult {
                    repo_name: repo_name_for_log.clone(),
                    sha: sha_for_log.clone(),
                    commit_message: msg_for_log.clone(),
                    attempt,
                    jobs: completed_jobs.clone(),
                    total_jobs,
                    all_job_names: all_job_names_for_log.clone(),
                    cancelled: false,
                };

                if let Err(e) = summary::write_json_summary(&partial, &attempt_dir_for_log).await {
                    tracing::error!("failed to write live JSON summary: {e}");
                }
                if let Err(e) =
                    summary::write_commit_summary_md(&partial, &attempt_dir_for_log).await
                {
                    tracing::error!("failed to write live commit summary.md: {e}");
                }

                let repo_name_s = repo_name_str_for_log.clone();
                let partial_c = partial.clone();
                let repo_dir_c = repo_dir_for_log.clone();
                if let Err(e) = tokio::task::spawn_blocking(move || {
                    summary::upsert_repo_summary(&repo_name_s, &partial_c, &repo_dir_c)
                })
                .await
                .unwrap_or_else(|e| Err(std::io::Error::other(e)))
                {
                    tracing::error!("failed to update repo summary: {e}");
                }
            }
        });

        let result = runner::execute_run(
            &repo_name,
            &sha_clone,
            &msg,
            attempt,
            &jobs,
            &checkout_dir_clone,
            &attempt_dir_clone,
            cancel_rx,
            semaphore,
            job_tx,
            timeout_secs,
        )
        .await;

        let _ = log_handle.await;

        if let Err(e) = summary::write_json_summary(&result, &attempt_dir_clone).await {
            tracing::error!("failed to write final JSON summary: {e}");
        }
        if let Err(e) = summary::write_commit_summary_md(&result, &attempt_dir_clone).await {
            tracing::error!("failed to write final commit summary.md: {e}");
        }

        let status = if result.cancelled {
            "CANCELLED"
        } else if result.all_passed() {
            "PASSED"
        } else {
            "FAILED"
        };
        tracing::info!(
            "run finished: repo={} sha={} attempt={} status={} jobs={}/{}",
            result.repo_name,
            result.sha.short(),
            result.attempt,
            status,
            result.jobs_passed(),
            result.total_jobs,
        );

        result
    });

    let key = RunKey {
        repo: repo_cfg.name.as_str().to_string(),
        sha: sha.as_str().to_string(),
        attempt,
    };

    state.active.insert(
        key,
        ActiveRun {
            repo_name: repo_cfg.name.clone(),
            sha: sha.clone(),
            commit_message: message,
            attempt,
            cancel_tx,
            handle,
            repo_dir: checkout_dir,
        },
    );

    Ok(attempt)
}

async fn reap_finished(state: &Arc<Mutex<DaemonState>>, config: &Config) {
    let mut guard = state.lock().await;
    let mut done_keys = Vec::new();

    for (key, run) in &guard.active {
        if run.handle.is_finished() {
            done_keys.push(key.clone());
        }
    }

    let mut repos_to_update: Vec<String> = Vec::new();

    for key in done_keys {
        if let Some(run) = guard.active.remove(&key) {
            let repo_name_str = run.repo_name.as_str().to_string();

            if let Some(repo_cfg) = config.repos.iter().find(|r| r.name == run.repo_name) {
                poller::cleanup_worktree(&repo_cfg.source, &run.repo_dir);
            }

            let result = match run.handle.await {
                Ok(result) => result,
                Err(_) => RunResult {
                    repo_name: run.repo_name,
                    sha: run.sha,
                    commit_message: run.commit_message,
                    attempt: run.attempt,
                    jobs: vec![],
                    total_jobs: 0,
                    all_job_names: vec![],
                    cancelled: true,
                },
            };

            guard
                .finished
                .entry(repo_name_str.clone())
                .or_default()
                .insert(0, result);

            if !repos_to_update.contains(&repo_name_str) {
                repos_to_update.push(repo_name_str);
            }
        }
    }

    for repo_name_str in &repos_to_update {
        if let Some(repo_cfg) = config
            .repos
            .iter()
            .find(|r| r.name.as_str() == repo_name_str)
        {
            let results: Vec<RunResult> = guard
                .finished
                .get(repo_name_str.as_str())
                .cloned()
                .unwrap_or_default();
            if let Some(latest) = results.first() {
                let rn = repo_name_str.clone();
                let r = latest.clone();
                let rd = repo_cfg.repo_dir.clone();
                if let Err(e) =
                    tokio::task::spawn_blocking(move || summary::upsert_repo_summary(&rn, &r, &rd))
                        .await
                        .unwrap_or_else(|e| Err(std::io::Error::other(e)))
                {
                    tracing::error!(repo = %repo_name_str, "failed to update repo summary: {e}");
                }
            }
        }
    }
}

async fn handle_client(
    stream: tokio::net::UnixStream,
    state: Arc<Mutex<DaemonState>>,
    shutdown_tx: watch::Sender<bool>,
    config: Config,
    semaphore: Arc<Semaphore>,
) {
    let (reader, mut writer) = stream.into_split();
    let mut lines = BufReader::new(reader).lines();

    while let Ok(Some(line)) = lines.next_line().await {
        let request: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                let resp = Response::Error {
                    message: format!("invalid request: {e}"),
                };
                let _ = send_response(&mut writer, &resp).await;
                continue;
            }
        };

        let response = match request {
            Request::Status { repo } => build_status_response(&state, repo.as_ref()).await,
            Request::Cancel { sha, repo } => cancel_run(&state, &sha, repo.as_ref()).await,
            Request::CancelAll => cancel_all_runs(&state).await,
            Request::Retry { repo, sha } => {
                retry_run(&state, &config, &repo, &sha, Arc::clone(&semaphore)).await
            }
            Request::Shutdown => {
                let _ = shutdown_tx.send(true);
                Response::Ok {
                    message: "shutting down".to_string(),
                }
            }
        };

        if send_response(&mut writer, &response).await.is_err() {
            break;
        }
    }
}

async fn send_response(
    writer: &mut tokio::net::unix::OwnedWriteHalf,
    resp: &Response,
) -> Result<(), std::io::Error> {
    let mut json = serde_json::to_string(resp).map_err(std::io::Error::other)?;
    json.push('\n');
    writer.write_all(json.as_bytes()).await?;
    writer.flush().await?;
    Ok(())
}

async fn build_status_response(
    state: &Arc<Mutex<DaemonState>>,
    repo_filter: Option<&RepoName>,
) -> Response {
    let guard = state.lock().await;
    let mut runs = Vec::new();

    for run in guard.active.values() {
        if let Some(filter) = repo_filter {
            if run.repo_name != *filter {
                continue;
            }
        }
        runs.push(RunSummary {
            repo: run.repo_name.clone(),
            sha: run.sha.clone(),
            commit_message: run.commit_message.clone(),
            attempt: run.attempt,
            state: RunState::Running,
            jobs_total: 0,
            jobs_passed: 0,
            jobs_failed: 0,
        });
    }

    for (repo_name_str, repo_runs) in &guard.finished {
        for result in repo_runs {
            if let Some(filter) = repo_filter {
                if result.repo_name != *filter {
                    continue;
                }
            }
            let _ = repo_name_str;
            let total = result.jobs.len();
            let passed = result.jobs_passed();
            let failed = result.jobs_failed();
            let st = if result.cancelled {
                RunState::Cancelled
            } else if failed > 0 {
                RunState::Failed
            } else {
                RunState::Passed
            };
            runs.push(RunSummary {
                repo: result.repo_name.clone(),
                sha: result.sha.clone(),
                commit_message: result.commit_message.clone(),
                attempt: result.attempt,
                state: st,
                jobs_total: total,
                jobs_passed: passed,
                jobs_failed: failed,
            });
        }
    }

    Response::Status { runs }
}

async fn cancel_run(
    state: &Arc<Mutex<DaemonState>>,
    sha: &CommitSha,
    repo_filter: Option<&RepoName>,
) -> Response {
    let mut guard = state.lock().await;
    let mut count = 0;
    let mut affected_repos = Vec::new();
    for (key, run) in &guard.active {
        if key.sha != sha.as_str() {
            continue;
        }
        if let Some(filter) = repo_filter {
            if run.repo_name != *filter {
                continue;
            }
        }
        let _ = run.cancel_tx.send(true);
        affected_repos.push(run.repo_name.as_str().to_string());
        count += 1;
    }
    for repo_name in &affected_repos {
        if let Some(repo_state) = guard.repos.get_mut(repo_name.as_str()) {
            repo_state.cancelled_shas.insert(sha.as_str().to_string());
            repo_state.completed_shas.remove(sha.as_str());
        }
    }
    if count > 0 {
        Response::Ok {
            message: format!(
                "cancellation requested for {count} run(s) of {}",
                sha.short()
            ),
        }
    } else {
        Response::Error {
            message: format!("no active run for {}", sha.short()),
        }
    }
}

async fn cancel_all_runs(state: &Arc<Mutex<DaemonState>>) -> Response {
    let mut guard = state.lock().await;
    let count = guard.active.len();
    let mut affected: Vec<(String, String)> = Vec::new();
    for run in guard.active.values() {
        let _ = run.cancel_tx.send(true);
        affected.push((
            run.repo_name.as_str().to_string(),
            run.sha.as_str().to_string(),
        ));
    }
    for (repo_name, sha_str) in &affected {
        if let Some(repo_state) = guard.repos.get_mut(repo_name.as_str()) {
            repo_state.cancelled_shas.insert(sha_str.clone());
            repo_state.completed_shas.remove(sha_str.as_str());
        }
    }
    Response::Ok {
        message: format!("cancellation requested for {count} active run(s)"),
    }
}

async fn retry_run(
    state: &Arc<Mutex<DaemonState>>,
    config: &Config,
    repo_name: &RepoName,
    sha: &CommitSha,
    semaphore: Arc<Semaphore>,
) -> Response {
    let repo_cfg = match config.repos.iter().find(|r| r.name == *repo_name) {
        Some(c) => c,
        None => {
            return Response::Error {
                message: format!("unknown repo '{}'", repo_name),
            };
        }
    };

    let mut guard = state.lock().await;

    for (key, run) in &guard.active {
        if key.repo == repo_name.as_str() && key.sha == sha.as_str() {
            let _ = run.cancel_tx.send(true);
        }
    }

    let msg = guard
        .finished
        .get(repo_name.as_str())
        .and_then(|runs| runs.iter().find(|r| r.sha == *sha))
        .map(|r| r.commit_message.clone())
        .unwrap_or_default();

    match launch_run(&mut guard, repo_cfg, sha, &msg, semaphore, true) {
        Ok(attempt) => Response::Ok {
            message: format!(
                "retry launched for {} in repo '{}' (attempt {})",
                sha.short(),
                repo_name,
                attempt
            ),
        },
        Err(e) => Response::Error {
            message: format!("failed to launch retry: {e}"),
        },
    }
}
