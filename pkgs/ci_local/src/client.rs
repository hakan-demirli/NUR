use crate::error::CiError;
use crate::ipc::{Request, Response, RunState};
use crate::types::{CommitSha, RepoName};
use std::path::Path;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

pub async fn send_request(socket_path: &Path, request: Request) -> Result<(), CiError> {
    let stream = UnixStream::connect(socket_path)
        .await
        .map_err(|e| CiError::SocketConnect {
            path: socket_path.to_path_buf(),
            source: e,
        })?;

    let (reader, mut writer) = stream.into_split();

    let mut json = serde_json::to_string(&request).map_err(|e| CiError::Ipc {
        detail: format!("failed to serialize request: {e}"),
    })?;
    json.push('\n');

    writer
        .write_all(json.as_bytes())
        .await
        .map_err(|e| CiError::Ipc {
            detail: format!("failed to send request: {e}"),
        })?;
    writer.flush().await.map_err(|e| CiError::Ipc {
        detail: format!("failed to flush: {e}"),
    })?;

    drop(writer);

    let mut lines = BufReader::new(reader).lines();

    while let Some(line) = lines.next_line().await.map_err(|e| CiError::Ipc {
        detail: format!("failed to read response: {e}"),
    })? {
        let response: Response = serde_json::from_str(&line).map_err(|e| CiError::Ipc {
            detail: format!("invalid response: {e}"),
        })?;

        print_response(&response);
    }

    Ok(())
}

fn print_response(response: &Response) {
    match response {
        Response::Ok { message } => {
            println!("{message}");
        }
        Response::Error { message } => {
            eprintln!("error: {message}");
        }
        Response::Status { runs } => {
            if runs.is_empty() {
                println!("no runs recorded");
                return;
            }

            let mut by_repo: std::collections::BTreeMap<String, Vec<_>> =
                std::collections::BTreeMap::new();
            for run in runs {
                by_repo
                    .entry(run.repo.as_str().to_string())
                    .or_default()
                    .push(run);
            }

            for (repo_name, repo_runs) in &by_repo {
                println!("{}:", repo_name);
                for run in repo_runs {
                    let state_str = match run.state {
                        RunState::Running => "RUNNING",
                        RunState::Passed => "PASSED",
                        RunState::Failed => "FAILED",
                        RunState::Cancelled => "CANCELLED",
                    };

                    let msg_part = if run.commit_message.is_empty() {
                        String::new()
                    } else {
                        format!(" {}", run.commit_message)
                    };

                    let failed_part = if run.jobs_failed > 0 {
                        format!(", {} failed", run.jobs_failed)
                    } else {
                        String::new()
                    };

                    println!(
                        "  `{}`{msg_part} [{}/{} passed{failed_part}] attempt {} {state_str}",
                        run.sha.short(),
                        run.jobs_passed,
                        run.jobs_total,
                        run.attempt,
                    );
                }
            }
        }
    }
}

pub fn status_request(repo: Option<String>) -> Result<Request, CiError> {
    let repo = repo
        .map(|r| {
            RepoName::try_from(r).map_err(|e| CiError::Ipc {
                detail: e.to_string(),
            })
        })
        .transpose()?;
    Ok(Request::Status { repo })
}

pub fn cancel_request(sha_str: String, repo: Option<String>) -> Result<Request, CiError> {
    let sha = CommitSha::try_from(sha_str).map_err(|e| CiError::Ipc {
        detail: e.to_string(),
    })?;
    let repo = repo
        .map(|r| {
            RepoName::try_from(r).map_err(|e| CiError::Ipc {
                detail: e.to_string(),
            })
        })
        .transpose()?;
    Ok(Request::Cancel { sha, repo })
}

pub fn cancel_all_request() -> Request {
    Request::CancelAll
}

pub fn retry_request(repo_str: String, sha_str: String) -> Result<Request, CiError> {
    let repo = RepoName::try_from(repo_str).map_err(|e| CiError::Ipc {
        detail: e.to_string(),
    })?;
    let sha = CommitSha::try_from(sha_str).map_err(|e| CiError::Ipc {
        detail: e.to_string(),
    })?;
    Ok(Request::Retry { repo, sha })
}

pub fn shutdown_request() -> Request {
    Request::Shutdown
}
