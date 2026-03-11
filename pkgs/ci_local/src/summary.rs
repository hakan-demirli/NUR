use crate::runner::{JobResult, RunResult, StepOutcome};
use std::path::Path;

pub fn format_duration(secs: f64) -> String {
    let total = secs.round() as u64;
    if total < 60 {
        format!("{}s", total)
    } else if total < 3600 {
        let m = total / 60;
        let s = total % 60;
        if s == 0 {
            format!("{}m", m)
        } else {
            format!("{}m {}s", m, s)
        }
    } else {
        let h = total / 3600;
        let m = (total % 3600) / 60;
        let s = total % 60;
        if s == 0 && m == 0 {
            format!("{}h", h)
        } else if s == 0 {
            format!("{}h {}m", h, m)
        } else {
            format!("{}h {}m {}s", h, m, s)
        }
    }
}

fn job_status_icon(job: &JobResult) -> &'static str {
    if job.succeeded() {
        "\u{2713}"
    } else if job
        .steps
        .iter()
        .any(|s| s.outcome == StepOutcome::Cancelled)
    {
        "\u{2717}"
    } else if job.steps.iter().any(|s| s.outcome == StepOutcome::TimedOut) {
        "\u{29D6}"
    } else {
        "\u{2717}"
    }
}

pub async fn write_json_summary(result: &RunResult, log_dir: &Path) -> Result<(), std::io::Error> {
    let path = log_dir.join("summary.json");
    let json = serde_json::to_string_pretty(result).map_err(std::io::Error::other)?;
    tokio::fs::write(&path, json).await?;
    Ok(())
}

pub async fn write_commit_summary_md(
    result: &RunResult,
    log_dir: &Path,
) -> Result<(), std::io::Error> {
    let mut md = format!("# {} {}\n", result.sha.short(), result.commit_message);

    let completed_names: std::collections::HashSet<&str> =
        result.jobs.iter().map(|j| j.job_name.as_str()).collect();

    for job in &result.jobs {
        let icon = job_status_icon(job);
        let dur = format_duration(job.duration_secs);
        md.push_str(&format!("* {} {} {}\n", job.job_name, icon, dur));
    }

    for name in &result.all_job_names {
        if !completed_names.contains(name.as_str()) {
            md.push_str(&format!("* {} \u{2026} running\n", name));
        }
    }

    let path = log_dir.join("summary.md");
    tokio::fs::write(&path, md).await?;
    Ok(())
}

pub fn upsert_repo_summary(
    repo_name: &str,
    result: &RunResult,
    base_dir: &Path,
) -> Result<(), std::io::Error> {
    use std::io::{Read, Seek, Write};

    let json_path = base_dir.join("summary.json");
    let md_path = base_dir.join("summary.md");

    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&json_path)?;

    let fd = std::os::unix::io::AsRawFd::as_raw_fd(&file);
    let ret = unsafe { libc::flock(fd, libc::LOCK_EX) };
    if ret != 0 {
        return Err(std::io::Error::last_os_error());
    }

    let mut content = String::new();
    file.read_to_string(&mut content)?;
    let mut runs: Vec<RunResult> = if content.trim().is_empty() {
        Vec::new()
    } else {
        serde_json::from_str(&content).unwrap_or_default()
    };

    let sha_str = result.sha.as_str();
    let attempt = result.attempt;
    if let Some(pos) = runs
        .iter()
        .position(|r| r.sha.as_str() == sha_str && r.attempt == attempt)
    {
        runs[pos] = result.clone();
    } else {
        runs.push(result.clone());
    }

    let json = serde_json::to_string_pretty(&runs).map_err(std::io::Error::other)?;
    file.set_len(0)?;
    file.seek(std::io::SeekFrom::Start(0))?;
    file.write_all(json.as_bytes())?;
    file.flush()?;

    let md = render_repo_summary_md(repo_name, &runs);
    std::fs::write(&md_path, md)?;

    unsafe { libc::flock(fd, libc::LOCK_UN) };

    Ok(())
}

fn render_repo_summary_md(repo_name: &str, runs: &[RunResult]) -> String {
    let mut md = format!("# {repo_name}\n\n");

    if runs.is_empty() {
        md.push_str("No runs recorded.\n");
    } else {
        for (i, run) in runs.iter().enumerate() {
            let ordinal = i + 1;
            let passed = run.jobs_passed();
            let total = run.total_jobs;
            let failed = run.jobs_failed();

            let msg = if run.commit_message.is_empty() {
                String::new()
            } else {
                format!(" {}", run.commit_message)
            };

            let status_tag = if run.cancelled {
                " CANCELLED"
            } else if !run.is_complete() {
                " RUNNING"
            } else if run.all_passed() {
                ""
            } else {
                " FAILED"
            };

            let failed_part = if failed > 0 {
                format!(", {failed} failed")
            } else {
                String::new()
            };

            md.push_str(&format!(
                "{ordinal}. `{}`{msg} [{passed}/{total} passed{failed_part}] attempt {}{status_tag}\n",
                run.sha.short(),
                run.attempt,
            ));
        }
    }

    md
}

pub fn format_job_log_line(repo_name: &str, sha_short: &str, job: &JobResult) -> String {
    let status = if job.succeeded() { "ok" } else { "FAIL" };
    let dur = format_duration(job.duration_secs);
    format!(
        "job finished: repo={} sha={} job=\"{}\" status={} duration={}",
        repo_name, sha_short, job.job_name, status, dur
    )
}

#[cfg(test)]
fn format_summary(result: &RunResult) -> String {
    let mut out = String::new();

    let status = if result.cancelled {
        "CANCELLED"
    } else if result.all_passed() {
        "PASSED"
    } else {
        "FAILED"
    };

    out.push_str(&format!(
        "=== {} | {} attempt {} — {} ===\n",
        result.repo_name,
        result.sha.short(),
        result.attempt,
        status
    ));

    for job in &result.jobs {
        let icon = job_status_icon(job);
        let dur = format_duration(job.duration_secs);
        out.push_str(&format!("  {} {} {}\n", icon, job.job_name, dur));

        for step in &job.steps {
            let step_icon = match &step.outcome {
                StepOutcome::Success => "ok",
                StepOutcome::Failed { exit_code } => {
                    if let Some(c) = exit_code {
                        out.push_str(&format!(
                            "    [FAIL exit={}] {} ({})\n",
                            c,
                            step.step_name,
                            format_duration(step.duration_secs)
                        ));
                        continue;
                    }
                    "FAIL"
                }
                StepOutcome::Cancelled => "CANCEL",
                StepOutcome::Skipped => "SKIP",
                StepOutcome::TimedOut => "TIMEOUT",
            };
            out.push_str(&format!(
                "    [{}] {} ({})\n",
                step_icon,
                step.step_name,
                format_duration(step.duration_secs)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runner::{JobResult, StepResult};
    use crate::types::{AttemptNumber, CommitSha, RepoName};

    fn sample_sha() -> CommitSha {
        CommitSha::try_from("a".repeat(40)).unwrap()
    }

    fn sample_repo() -> RepoName {
        RepoName::try_from("my-project".to_string()).unwrap()
    }

    fn make_run_result(jobs: Vec<JobResult>, cancelled: bool) -> RunResult {
        let total_jobs = jobs.len();
        let all_job_names = jobs.iter().map(|j| j.job_name.clone()).collect();
        RunResult {
            repo_name: sample_repo(),
            sha: sample_sha(),
            commit_message: "test commit".into(),
            attempt: AttemptNumber::first(),
            jobs,
            total_jobs,
            all_job_names,
            cancelled,
        }
    }

    fn passing_job(name: &str) -> JobResult {
        JobResult {
            job_name: name.to_string(),
            steps: vec![StepResult {
                step_name: "step1".into(),
                outcome: StepOutcome::Success,
                stdout_path: "/dev/null".into(),
                stderr_path: "/dev/null".into(),
                duration_secs: 1.5,
            }],
            duration_secs: 1.5,
        }
    }

    fn passing_job_with_duration(name: &str, dur: f64) -> JobResult {
        JobResult {
            job_name: name.to_string(),
            steps: vec![StepResult {
                step_name: "step1".into(),
                outcome: StepOutcome::Success,
                stdout_path: "/dev/null".into(),
                stderr_path: "/dev/null".into(),
                duration_secs: dur,
            }],
            duration_secs: dur,
        }
    }

    fn failing_job(name: &str) -> JobResult {
        JobResult {
            job_name: name.to_string(),
            steps: vec![StepResult {
                step_name: "step1".into(),
                outcome: StepOutcome::Failed { exit_code: Some(1) },
                stdout_path: "/dev/null".into(),
                stderr_path: "/dev/null".into(),
                duration_secs: 0.3,
            }],
            duration_secs: 0.3,
        }
    }

    #[test]
    fn format_duration_seconds_only() {
        assert_eq!(format_duration(5.0), "5s");
        assert_eq!(format_duration(0.0), "0s");
        assert_eq!(format_duration(59.4), "59s");
    }

    #[test]
    fn format_duration_minutes_and_seconds() {
        assert_eq!(format_duration(60.0), "1m");
        assert_eq!(format_duration(61.0), "1m 1s");
        assert_eq!(format_duration(305.0), "5m 5s");
        assert_eq!(format_duration(908.0), "15m 8s");
    }

    #[test]
    fn format_duration_hours() {
        assert_eq!(format_duration(3600.0), "1h");
        assert_eq!(format_duration(3660.0), "1h 1m");
        assert_eq!(format_duration(3661.0), "1h 1m 1s");
        assert_eq!(format_duration(8022.0), "2h 13m 42s");
    }

    #[test]
    fn format_summary_all_passed() {
        let result = make_run_result(vec![passing_job("build"), passing_job("test")], false);
        let text = format_summary(&result);
        assert!(text.contains("PASSED"));
        assert!(text.contains("my-project"));
        assert!(text.contains("build"));
        assert!(text.contains("test"));
    }

    #[test]
    fn format_summary_with_failure() {
        let result = make_run_result(vec![passing_job("build"), failing_job("test")], false);
        let text = format_summary(&result);
        assert!(text.contains("FAILED"));
        assert!(text.contains("build"));
        assert!(text.contains("[FAIL exit=1]"));
    }

    #[test]
    fn format_summary_cancelled() {
        let result = make_run_result(vec![passing_job("build")], true);
        let text = format_summary(&result);
        assert!(text.contains("CANCELLED"));
    }

    #[test]
    fn format_job_log_line_ok() {
        let job = passing_job("Fast Checks");
        let line = format_job_log_line("ci-local", "ce49b0f4", &job);
        assert!(line.contains("repo=ci-local"));
        assert!(line.contains("sha=ce49b0f4"));
        assert!(line.contains("job=\"Fast Checks\""));
        assert!(line.contains("status=ok"));
        assert!(line.contains("duration="));
    }

    #[test]
    fn format_job_log_line_fail() {
        let job = failing_job("test");
        let line = format_job_log_line("proj", "abcdef01", &job);
        assert!(line.contains("status=FAIL"));
    }

    #[tokio::test]
    async fn write_json_summary_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = make_run_result(vec![passing_job("build")], false);

        write_json_summary(&result, tmp.path()).await.unwrap();

        let path = tmp.path().join("summary.json");
        assert!(path.exists());

        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(parsed["repo_name"], "my-project");
        assert_eq!(parsed["commit_message"], "test commit");
        assert_eq!(parsed["cancelled"], false);
    }

    #[tokio::test]
    async fn write_commit_summary_md_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let result = make_run_result(
            vec![
                passing_job_with_duration("Fast Checks", 305.0),
                passing_job_with_duration("Check unit-tests", 908.0),
            ],
            false,
        );

        write_commit_summary_md(&result, tmp.path()).await.unwrap();

        let md = std::fs::read_to_string(tmp.path().join("summary.md")).unwrap();
        assert!(md.starts_with("# aaaaaaaa test commit\n"));
        assert!(md.contains("Fast Checks"));
        assert!(md.contains("5m 5s"), "expected '5m 5s' in: {md}");
        assert!(md.contains("15m 8s"), "expected '15m 8s' in: {md}");
        assert!(md.contains('\u{2713}'), "expected checkmark in: {md}");
    }

    #[test]
    fn upsert_repo_summary_creates_files() {
        let tmp = tempfile::tempdir().unwrap();
        let result = make_run_result(vec![passing_job("build")], false);

        upsert_repo_summary("my-project", &result, tmp.path()).unwrap();

        let json_content = std::fs::read_to_string(tmp.path().join("summary.json")).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_content).unwrap();
        assert!(parsed.is_array());
        assert_eq!(parsed[0]["repo_name"], "my-project");

        let md = std::fs::read_to_string(tmp.path().join("summary.md")).unwrap();
        assert!(md.starts_with("# my-project\n"));
        assert!(md.contains("`aaaaaaaa`"));
        assert!(md.contains("test commit"));
        assert!(md.contains("1/1 passed"));
        assert!(!md.contains("FAILED"));
    }

    #[test]
    fn upsert_repo_summary_with_failure() {
        let tmp = tempfile::tempdir().unwrap();
        let result = make_run_result(vec![passing_job("build"), failing_job("test")], false);

        upsert_repo_summary("my-project", &result, tmp.path()).unwrap();

        let md = std::fs::read_to_string(tmp.path().join("summary.md")).unwrap();
        assert!(md.contains("FAILED"));
        assert!(md.contains("1/2 passed"));
        assert!(md.contains("1 failed"));
    }

    #[test]
    fn upsert_repo_summary_preserves_prior_runs() {
        let tmp = tempfile::tempdir().unwrap();

        let mut run1 = make_run_result(vec![passing_job("build")], false);
        run1.commit_message = "first commit".into();
        run1.sha = CommitSha::try_from("b".repeat(40)).unwrap();
        upsert_repo_summary("proj", &run1, tmp.path()).unwrap();

        let mut run2 = make_run_result(vec![failing_job("test")], false);
        run2.commit_message = "second commit".into();
        upsert_repo_summary("proj", &run2, tmp.path()).unwrap();

        let json_content = std::fs::read_to_string(tmp.path().join("summary.json")).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_content).unwrap();
        assert_eq!(parsed.len(), 2, "should have 2 runs, got: {parsed:?}");

        let md = std::fs::read_to_string(tmp.path().join("summary.md")).unwrap();
        assert!(md.contains("first commit"), "md should have first: {md}");
        assert!(md.contains("second commit"), "md should have second: {md}");
    }

    #[test]
    fn upsert_repo_summary_updates_same_run_in_place() {
        let tmp = tempfile::tempdir().unwrap();

        let mut partial = make_run_result(vec![passing_job("build")], false);
        partial.total_jobs = 2;
        partial.all_job_names = vec!["build".into(), "test".into()];
        upsert_repo_summary("proj", &partial, tmp.path()).unwrap();

        let md = std::fs::read_to_string(tmp.path().join("summary.md")).unwrap();
        assert!(md.contains("1/2 passed"), "should show 1/2: {md}");
        assert!(md.contains("RUNNING"), "should show RUNNING: {md}");

        let complete = make_run_result(vec![passing_job("build"), passing_job("test")], false);
        upsert_repo_summary("proj", &complete, tmp.path()).unwrap();

        let json_content = std::fs::read_to_string(tmp.path().join("summary.json")).unwrap();
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_content).unwrap();
        assert_eq!(
            parsed.len(),
            1,
            "should have 1 run (upserted), got: {parsed:?}"
        );

        let md = std::fs::read_to_string(tmp.path().join("summary.md")).unwrap();
        assert!(md.contains("2/2 passed"), "should show 2/2: {md}");
        assert!(!md.contains("RUNNING"), "should not show RUNNING: {md}");
    }

    #[test]
    fn upsert_repo_summary_cancelled_run() {
        let tmp = tempfile::tempdir().unwrap();
        let result = make_run_result(vec![passing_job("build")], true);

        upsert_repo_summary("proj", &result, tmp.path()).unwrap();

        let md = std::fs::read_to_string(tmp.path().join("summary.md")).unwrap();
        assert!(md.contains("CANCELLED"));
    }
}
