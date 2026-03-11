use crate::types::{AttemptNumber, CommitSha, RepoName};
use crate::workflow::{WorkflowJob, WorkflowStep};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::{mpsc, watch, Semaphore};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepOutcome {
    Success,
    Failed { exit_code: Option<i32> },
    Cancelled,
    Skipped,
    TimedOut,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StepResult {
    pub step_name: String,
    pub outcome: StepOutcome,
    pub stdout_path: PathBuf,
    pub stderr_path: PathBuf,
    pub duration_secs: f64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JobResult {
    pub job_name: String,
    pub steps: Vec<StepResult>,
    pub duration_secs: f64,
}

impl JobResult {
    pub fn succeeded(&self) -> bool {
        self.steps.iter().all(|s| s.outcome == StepOutcome::Success)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunResult {
    pub repo_name: RepoName,
    pub sha: CommitSha,
    pub commit_message: String,
    pub attempt: AttemptNumber,
    pub jobs: Vec<JobResult>,
    pub total_jobs: usize,
    pub all_job_names: Vec<String>,
    pub cancelled: bool,
}

impl RunResult {
    pub fn all_passed(&self) -> bool {
        !self.cancelled
            && self.jobs.len() == self.total_jobs
            && self.jobs.iter().all(|j| j.succeeded())
    }

    pub fn jobs_passed(&self) -> usize {
        self.jobs.iter().filter(|j| j.succeeded()).count()
    }

    pub fn jobs_failed(&self) -> usize {
        self.jobs.iter().filter(|j| !j.succeeded()).count()
    }

    pub fn is_complete(&self) -> bool {
        self.jobs.len() == self.total_jobs
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_run(
    repo_name: &RepoName,
    sha: &CommitSha,
    commit_message: &str,
    attempt: AttemptNumber,
    jobs: &[WorkflowJob],
    repo_dir: &Path,
    log_dir: &Path,
    cancel_rx: watch::Receiver<bool>,
    semaphore: Arc<Semaphore>,
    job_tx: mpsc::Sender<JobResult>,
    timeout_secs: Option<u64>,
) -> RunResult {
    let mut id_to_indices: HashMap<&str, Vec<usize>> = HashMap::new();
    for (i, j) in jobs.iter().enumerate() {
        id_to_indices.entry(&j.job_id).or_default().push(i);
    }

    let dep_indices: Vec<Vec<usize>> = jobs
        .iter()
        .map(|j| {
            let mut deps = Vec::new();
            for need in &j.needs {
                if let Some(indices) = id_to_indices.get(need.as_str()) {
                    deps.extend(indices);
                } else {
                    for (id, indices) in &id_to_indices {
                        if id.starts_with(need.as_str())
                            && id
                                .get(need.len()..)
                                .map(|rest| rest.starts_with(" ("))
                                .unwrap_or(false)
                        {
                            deps.extend(indices);
                        }
                    }
                }
            }
            deps.sort();
            deps.dedup();
            deps
        })
        .collect();

    let results: Arc<tokio::sync::Mutex<Vec<Option<JobResult>>>> =
        Arc::new(tokio::sync::Mutex::new(vec![None; jobs.len()]));

    let notifiers: Vec<(watch::Sender<bool>, watch::Receiver<bool>)> =
        (0..jobs.len()).map(|_| watch::channel(false)).collect();

    let mut handles = Vec::with_capacity(jobs.len());

    for (idx, job) in jobs.iter().enumerate() {
        let job = job.clone();
        let repo_dir = repo_dir.to_path_buf();
        let job_log_dir = log_dir.join(sanitize_dir_name(&job.job_id));
        let cancel_rx = cancel_rx.clone();
        let semaphore = Arc::clone(&semaphore);
        let results = Arc::clone(&results);
        let done_tx = notifiers[idx].0.clone();
        let job_tx = job_tx.clone();

        let dep_rxs: Vec<watch::Receiver<bool>> = dep_indices[idx]
            .iter()
            .map(|&di| notifiers[di].1.clone())
            .collect();

        let dep_idxs = dep_indices[idx].clone();
        let results_for_dep_check = Arc::clone(&results);

        let handle = tokio::spawn(async move {
            for mut rx in dep_rxs {
                while !*rx.borrow_and_update() {
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
            }

            if *cancel_rx.borrow() {
                let result = make_cancelled_job(&job);
                let _ = job_tx.send(result.clone()).await;
                let mut lock = results.lock().await;
                lock[idx] = Some(result);
                let _ = done_tx.send(true);
                return;
            }

            {
                let lock = results_for_dep_check.lock().await;
                for &di in &dep_idxs {
                    if let Some(ref dep_result) = lock[di] {
                        if !dep_result.succeeded() {
                            let result = make_skipped_job(&job);
                            let _ = job_tx.send(result.clone()).await;
                            let mut lock2 = results.lock().await;
                            lock2[idx] = Some(result);
                            let _ = done_tx.send(true);
                            return;
                        }
                    }
                }
            }

            let _permit = semaphore.acquire().await;
            let result = run_job(&job, &repo_dir, &job_log_dir, cancel_rx, timeout_secs).await;
            let _ = job_tx.send(result.clone()).await;
            let mut lock = results.lock().await;
            lock[idx] = Some(result);
            let _ = done_tx.send(true);
        });
        handles.push(handle);
    }

    drop(job_tx);

    for handle in handles {
        let _ = handle.await;
    }

    let final_results = results.lock().await;
    let job_results: Vec<JobResult> = final_results
        .iter()
        .map(|slot| {
            slot.clone().unwrap_or_else(|| JobResult {
                job_name: "unknown".to_string(),
                steps: vec![],
                duration_secs: 0.0,
            })
        })
        .collect();

    let cancelled = *cancel_rx.borrow();
    let total_jobs = jobs.len();
    let all_job_names: Vec<String> = jobs.iter().map(|j| j.name.clone()).collect();

    RunResult {
        repo_name: repo_name.clone(),
        sha: sha.clone(),
        commit_message: commit_message.to_string(),
        attempt,
        jobs: job_results,
        total_jobs,
        all_job_names,
        cancelled,
    }
}

fn sanitize_dir_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn make_cancelled_job(job: &WorkflowJob) -> JobResult {
    JobResult {
        job_name: job.name.clone(),
        steps: job
            .steps
            .iter()
            .map(|s| StepResult {
                step_name: s.name.clone(),
                outcome: StepOutcome::Cancelled,
                stdout_path: PathBuf::new(),
                stderr_path: PathBuf::new(),
                duration_secs: 0.0,
            })
            .collect(),
        duration_secs: 0.0,
    }
}

fn make_skipped_job(job: &WorkflowJob) -> JobResult {
    JobResult {
        job_name: job.name.clone(),
        steps: job
            .steps
            .iter()
            .map(|s| StepResult {
                step_name: s.name.clone(),
                outcome: StepOutcome::Skipped,
                stdout_path: PathBuf::new(),
                stderr_path: PathBuf::new(),
                duration_secs: 0.0,
            })
            .collect(),
        duration_secs: 0.0,
    }
}

async fn run_job(
    job: &WorkflowJob,
    repo_dir: &Path,
    log_dir: &Path,
    cancel_rx: watch::Receiver<bool>,
    timeout_secs: Option<u64>,
) -> JobResult {
    let job_start = std::time::Instant::now();

    if let Err(e) = tokio::fs::create_dir_all(log_dir).await {
        tracing::error!(job = %job.name, "failed to create log dir: {e}");
        return JobResult {
            job_name: job.name.clone(),
            steps: job
                .steps
                .iter()
                .map(|s| StepResult {
                    step_name: s.name.clone(),
                    outcome: StepOutcome::Failed { exit_code: None },
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                })
                .collect(),
            duration_secs: 0.0,
        };
    }

    let mut step_results = Vec::with_capacity(job.steps.len());

    for (step_idx, step) in job.steps.iter().enumerate() {
        if *cancel_rx.borrow() {
            step_results.push(StepResult {
                step_name: step.name.clone(),
                outcome: StepOutcome::Cancelled,
                stdout_path: PathBuf::new(),
                stderr_path: PathBuf::new(),
                duration_secs: 0.0,
            });
            continue;
        }

        if let Some(t) = timeout_secs {
            if job_start.elapsed().as_secs() >= t {
                step_results.push(StepResult {
                    step_name: step.name.clone(),
                    outcome: StepOutcome::TimedOut,
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                });
                for remaining in &job.steps[step_idx + 1..] {
                    step_results.push(StepResult {
                        step_name: remaining.name.clone(),
                        outcome: StepOutcome::Skipped,
                        stdout_path: PathBuf::new(),
                        stderr_path: PathBuf::new(),
                        duration_secs: 0.0,
                    });
                }
                break;
            }
        }

        let step_timeout = timeout_secs.map(|t| {
            let elapsed = job_start.elapsed().as_secs();
            t.saturating_sub(elapsed)
        });

        let result = run_step(
            step,
            step_idx,
            repo_dir,
            log_dir,
            cancel_rx.clone(),
            step_timeout,
        )
        .await;
        let failed = result.outcome != StepOutcome::Success;
        step_results.push(result);

        if failed {
            for remaining in &job.steps[step_idx + 1..] {
                step_results.push(StepResult {
                    step_name: remaining.name.clone(),
                    outcome: StepOutcome::Skipped,
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                });
            }
            break;
        }
    }

    JobResult {
        job_name: job.name.clone(),
        steps: step_results,
        duration_secs: job_start.elapsed().as_secs_f64(),
    }
}

async fn run_step(
    step: &WorkflowStep,
    step_idx: usize,
    repo_dir: &Path,
    log_dir: &Path,
    cancel_rx: watch::Receiver<bool>,
    timeout_secs: Option<u64>,
) -> StepResult {
    let safe_name = sanitize_dir_name(&step.name);
    let stdout_path = log_dir.join(format!("{step_idx:02}_{safe_name}_stdout.log"));
    let stderr_path = log_dir.join(format!("{step_idx:02}_{safe_name}_stderr.log"));

    let start = std::time::Instant::now();

    let mut cmd = Command::new("bash");
    cmd.arg("-e")
        .arg("-c")
        .arg(&step.run)
        .current_dir(repo_dir)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);

    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(step = %step.name, "spawn failed: {e}");
            return StepResult {
                step_name: step.name.clone(),
                outcome: StepOutcome::Failed { exit_code: None },
                stdout_path,
                stderr_path,
                duration_secs: start.elapsed().as_secs_f64(),
            };
        }
    };

    let child_pid = child.id();
    let child_stdout = child.stdout.take();
    let child_stderr = child.stderr.take();

    let stdout_path_clone = stdout_path.clone();
    let stderr_path_clone = stderr_path.clone();

    let stdout_handle = tokio::spawn(async move {
        if let Some(mut reader) = child_stdout {
            if let Ok(mut file) = tokio::fs::File::create(&stdout_path_clone).await {
                let _ = tokio::io::copy(&mut reader, &mut file).await;
                let _ = file.flush().await;
            }
        }
    });

    let stderr_handle = tokio::spawn(async move {
        if let Some(mut reader) = child_stderr {
            if let Ok(mut file) = tokio::fs::File::create(&stderr_path_clone).await {
                let _ = tokio::io::copy(&mut reader, &mut file).await;
                let _ = file.flush().await;
            }
        }
    });

    let cancel_fut = {
        let mut rx = cancel_rx;
        async move {
            loop {
                if *rx.borrow_and_update() {
                    break;
                }
                if rx.changed().await.is_err() {
                    std::future::pending::<()>().await;
                }
            }
        }
    };

    let timeout_fut = async {
        match timeout_secs {
            Some(t) if t > 0 => tokio::time::sleep(std::time::Duration::from_secs(t)).await,
            _ => std::future::pending::<()>().await,
        }
    };

    let outcome = tokio::select! {
        status = child.wait() => {
            match status {
                Ok(s) if s.success() => StepOutcome::Success,
                Ok(s) => StepOutcome::Failed { exit_code: s.code() },
                Err(_) => StepOutcome::Failed { exit_code: None },
            }
        }
        _ = cancel_fut => {
            if let Some(pid) = child_pid {
                unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
            }
            let _ = child.kill().await;
            StepOutcome::Cancelled
        }
        _ = timeout_fut => {
            tracing::warn!(step = %step.name, "step timed out after {}s", timeout_secs.unwrap_or(0));
            if let Some(pid) = child_pid {
                unsafe { libc::kill(-(pid as i32), libc::SIGKILL); }
            }
            let _ = child.kill().await;
            StepOutcome::TimedOut
        }
    };

    let _ = stdout_handle.await;
    let _ = stderr_handle.await;

    StepResult {
        step_name: step.name.clone(),
        outcome,
        stdout_path,
        stderr_path,
        duration_secs: start.elapsed().as_secs_f64(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sha() -> CommitSha {
        CommitSha::try_from("b".repeat(40)).unwrap()
    }

    fn sample_repo() -> RepoName {
        RepoName::try_from("test-repo".to_string()).unwrap()
    }

    #[test]
    fn job_result_all_success() {
        let job = JobResult {
            job_name: "build".into(),
            steps: vec![
                StepResult {
                    step_name: "s1".into(),
                    outcome: StepOutcome::Success,
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                },
                StepResult {
                    step_name: "s2".into(),
                    outcome: StepOutcome::Success,
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                },
            ],
            duration_secs: 0.0,
        };
        assert!(job.succeeded());
    }

    #[test]
    fn job_result_with_failure() {
        let job = JobResult {
            job_name: "build".into(),
            steps: vec![
                StepResult {
                    step_name: "s1".into(),
                    outcome: StepOutcome::Success,
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                },
                StepResult {
                    step_name: "s2".into(),
                    outcome: StepOutcome::Failed { exit_code: Some(1) },
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                },
            ],
            duration_secs: 0.0,
        };
        assert!(!job.succeeded());
    }

    #[test]
    fn job_result_with_cancelled_step() {
        let job = JobResult {
            job_name: "build".into(),
            steps: vec![StepResult {
                step_name: "s1".into(),
                outcome: StepOutcome::Cancelled,
                stdout_path: PathBuf::new(),
                stderr_path: PathBuf::new(),
                duration_secs: 0.0,
            }],
            duration_secs: 0.0,
        };
        assert!(!job.succeeded());
    }

    #[test]
    fn job_result_with_skipped_step() {
        let job = JobResult {
            job_name: "build".into(),
            steps: vec![StepResult {
                step_name: "s1".into(),
                outcome: StepOutcome::Skipped,
                stdout_path: PathBuf::new(),
                stderr_path: PathBuf::new(),
                duration_secs: 0.0,
            }],
            duration_secs: 0.0,
        };
        assert!(!job.succeeded());
    }

    #[test]
    fn run_result_all_passed() {
        let result = RunResult {
            repo_name: sample_repo(),
            sha: sample_sha(),
            commit_message: "test".into(),
            attempt: AttemptNumber::first(),
            jobs: vec![
                JobResult {
                    job_name: "j1".into(),
                    steps: vec![StepResult {
                        step_name: "s".into(),
                        outcome: StepOutcome::Success,
                        stdout_path: PathBuf::new(),
                        stderr_path: PathBuf::new(),
                        duration_secs: 0.0,
                    }],
                    duration_secs: 0.0,
                },
                JobResult {
                    job_name: "j2".into(),
                    steps: vec![StepResult {
                        step_name: "s".into(),
                        outcome: StepOutcome::Success,
                        stdout_path: PathBuf::new(),
                        stderr_path: PathBuf::new(),
                        duration_secs: 0.0,
                    }],
                    duration_secs: 0.0,
                },
            ],
            total_jobs: 2,
            all_job_names: vec!["j1".into(), "j2".into()],
            cancelled: false,
        };
        assert!(result.all_passed());
        assert_eq!(result.jobs_passed(), 2);
        assert_eq!(result.jobs_failed(), 0);
    }

    #[test]
    fn run_result_cancelled_not_all_passed() {
        let result = RunResult {
            repo_name: sample_repo(),
            sha: sample_sha(),
            commit_message: "test".into(),
            attempt: AttemptNumber::first(),
            jobs: vec![JobResult {
                job_name: "j1".into(),
                steps: vec![StepResult {
                    step_name: "s".into(),
                    outcome: StepOutcome::Success,
                    stdout_path: PathBuf::new(),
                    stderr_path: PathBuf::new(),
                    duration_secs: 0.0,
                }],
                duration_secs: 0.0,
            }],
            total_jobs: 1,
            all_job_names: vec!["j1".into()],
            cancelled: true,
        };
        assert!(!result.all_passed());
    }

    #[test]
    fn run_result_mixed() {
        let result = RunResult {
            repo_name: sample_repo(),
            sha: sample_sha(),
            commit_message: "test".into(),
            attempt: AttemptNumber::first(),
            jobs: vec![
                JobResult {
                    job_name: "pass".into(),
                    steps: vec![StepResult {
                        step_name: "s".into(),
                        outcome: StepOutcome::Success,
                        stdout_path: PathBuf::new(),
                        stderr_path: PathBuf::new(),
                        duration_secs: 0.0,
                    }],
                    duration_secs: 0.0,
                },
                JobResult {
                    job_name: "fail".into(),
                    steps: vec![StepResult {
                        step_name: "s".into(),
                        outcome: StepOutcome::Failed { exit_code: Some(2) },
                        stdout_path: PathBuf::new(),
                        stderr_path: PathBuf::new(),
                        duration_secs: 0.0,
                    }],
                    duration_secs: 0.0,
                },
            ],
            total_jobs: 2,
            all_job_names: vec!["pass".into(), "fail".into()],
            cancelled: false,
        };
        assert!(!result.all_passed());
        assert_eq!(result.jobs_passed(), 1);
        assert_eq!(result.jobs_failed(), 1);
    }

    #[test]
    fn step_outcome_equality() {
        assert_eq!(StepOutcome::Success, StepOutcome::Success);
        assert_ne!(StepOutcome::Success, StepOutcome::Cancelled);
        assert_eq!(
            StepOutcome::Failed { exit_code: Some(1) },
            StepOutcome::Failed { exit_code: Some(1) }
        );
        assert_ne!(
            StepOutcome::Failed { exit_code: Some(1) },
            StepOutcome::Failed { exit_code: Some(2) }
        );
    }

    #[test]
    fn make_cancelled_job_marks_all_steps() {
        let job = WorkflowJob {
            name: "build".into(),
            needs: vec![],
            steps: vec![
                WorkflowStep {
                    name: "s1".into(),
                    run: "nix build .#x".into(),
                },
                WorkflowStep {
                    name: "s2".into(),
                    run: "nix build .#y".into(),
                },
            ],
            job_id: "build".into(),
        };
        let result = super::make_cancelled_job(&job);
        assert_eq!(result.job_name, "build");
        assert_eq!(result.steps.len(), 2);
        for step in &result.steps {
            assert_eq!(step.outcome, StepOutcome::Cancelled);
        }
    }

    #[test]
    fn make_skipped_job_marks_all_steps() {
        let job = WorkflowJob {
            name: "test".into(),
            needs: vec!["build".into()],
            steps: vec![WorkflowStep {
                name: "run".into(),
                run: "nix run .#test".into(),
            }],
            job_id: "test".into(),
        };
        let result = super::make_skipped_job(&job);
        assert_eq!(result.job_name, "test");
        assert_eq!(result.steps.len(), 1);
        assert_eq!(result.steps[0].outcome, StepOutcome::Skipped);
    }

    #[tokio::test]
    async fn execute_run_immediate_cancel() {
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(true);
        let _ = cancel_tx;

        let jobs = vec![WorkflowJob {
            name: "build".into(),
            needs: vec![],
            steps: vec![WorkflowStep {
                name: "compile".into(),
                run: "nix build .#pkg".into(),
            }],
            job_id: "build".into(),
        }];

        let tmp = tempfile::tempdir().unwrap();
        let log_dir = tmp.path().join("logs");
        std::fs::create_dir_all(&log_dir).unwrap();
        let repo_dir = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();

        let sem = std::sync::Arc::new(Semaphore::new(4));
        let (job_tx, _job_rx) = mpsc::channel(64);
        let result = execute_run(
            &sample_repo(),
            &sample_sha(),
            "test",
            AttemptNumber::first(),
            &jobs,
            &repo_dir,
            &log_dir,
            cancel_rx,
            sem,
            job_tx,
            None,
        )
        .await;

        assert!(result.cancelled);
        assert_eq!(result.jobs.len(), 1);
        for step in &result.jobs[0].steps {
            assert_eq!(step.outcome, StepOutcome::Cancelled);
        }
    }

    #[test]
    fn run_result_serialization() {
        let result = RunResult {
            repo_name: sample_repo(),
            sha: sample_sha(),
            commit_message: "hello".into(),
            attempt: AttemptNumber::first(),
            jobs: vec![JobResult {
                job_name: "j".into(),
                steps: vec![StepResult {
                    step_name: "s".into(),
                    outcome: StepOutcome::Success,
                    stdout_path: PathBuf::from("/tmp/out"),
                    stderr_path: PathBuf::from("/tmp/err"),
                    duration_secs: 1.23,
                }],
                duration_secs: 1.23,
            }],
            total_jobs: 1,
            all_job_names: vec!["j".into()],
            cancelled: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("test-repo"));
        assert!(json.contains("hello"));
        assert!(json.contains("success"));
    }

    #[test]
    fn sanitize_dir_name_handles_special_chars() {
        assert_eq!(sanitize_dir_name("build"), "build");
        assert_eq!(sanitize_dir_name("checks (unit)"), "checks__unit_");
        assert_eq!(sanitize_dir_name("checks (a=1, b=2)"), "checks__a_1__b_2_");
    }
}
