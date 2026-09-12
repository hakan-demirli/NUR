use crate::types::{AttemptNumber, CommitSha, RepoName};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Request {
    Status {
        repo: Option<RepoName>,
    },
    Cancel {
        sha: CommitSha,
        repo: Option<RepoName>,
    },
    CancelAll,
    Retry {
        repo: RepoName,
        sha: CommitSha,
    },
    Shutdown,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Response {
    Ok { message: String },
    Status { runs: Vec<RunSummary> },
    Error { message: String },
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RunSummary {
    pub repo: RepoName,
    pub sha: CommitSha,
    pub commit_message: String,
    pub attempt: AttemptNumber,
    pub state: RunState,
    pub jobs_total: usize,
    pub jobs_passed: usize,
    pub jobs_failed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    Passed,
    Failed,
    Cancelled,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_sha() -> CommitSha {
        CommitSha::try_from("a".repeat(40)).unwrap()
    }

    fn sample_repo() -> RepoName {
        RepoName::try_from("test-repo".to_string()).unwrap()
    }

    #[test]
    fn request_status_roundtrip() {
        let req = Request::Status { repo: None };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Request::Status { repo: None }));
    }

    #[test]
    fn request_status_with_repo_roundtrip() {
        let req = Request::Status {
            repo: Some(sample_repo()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::Status { repo: Some(r) } => assert_eq!(r.as_str(), "test-repo"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn request_cancel_roundtrip() {
        let req = Request::Cancel {
            sha: sample_sha(),
            repo: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::Cancel { sha, repo } => {
                assert_eq!(sha.as_str(), "a".repeat(40));
                assert!(repo.is_none());
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn request_cancel_all_roundtrip() {
        let req = Request::CancelAll;
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Request::CancelAll));
    }

    #[test]
    fn request_retry_roundtrip() {
        let req = Request::Retry {
            repo: sample_repo(),
            sha: sample_sha(),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        match back {
            Request::Retry { repo, sha } => {
                assert_eq!(repo.as_str(), "test-repo");
                assert_eq!(sha.as_str(), "a".repeat(40));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn request_shutdown_roundtrip() {
        let req = Request::Shutdown;
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert!(matches!(back, Request::Shutdown));
    }

    #[test]
    fn response_ok_roundtrip() {
        let resp = Response::Ok {
            message: "done".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::Ok { message } => assert_eq!(message, "done"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn response_error_roundtrip() {
        let resp = Response::Error {
            message: "something broke".into(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::Error { message } => assert_eq!(message, "something broke"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn response_status_roundtrip() {
        let resp = Response::Status {
            runs: vec![RunSummary {
                repo: sample_repo(),
                sha: sample_sha(),
                commit_message: "fix bug".into(),
                attempt: AttemptNumber::first(),
                state: RunState::Passed,
                jobs_total: 3,
                jobs_passed: 2,
                jobs_failed: 1,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::Status { runs } => {
                assert_eq!(runs.len(), 1);
                assert_eq!(runs[0].repo.as_str(), "test-repo");
                assert_eq!(runs[0].commit_message, "fix bug");
                assert_eq!(runs[0].state, RunState::Passed);
                assert_eq!(runs[0].jobs_total, 3);
                assert_eq!(runs[0].jobs_passed, 2);
                assert_eq!(runs[0].jobs_failed, 1);
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn response_status_empty_runs() {
        let resp = Response::Status { runs: vec![] };
        let json = serde_json::to_string(&resp).unwrap();
        let back: Response = serde_json::from_str(&json).unwrap();
        match back {
            Response::Status { runs } => assert!(runs.is_empty()),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn run_state_all_variants() {
        for (state, expected) in [
            (RunState::Running, "running"),
            (RunState::Passed, "passed"),
            (RunState::Failed, "failed"),
            (RunState::Cancelled, "cancelled"),
        ] {
            let json = serde_json::to_value(&state).unwrap();
            assert_eq!(json.as_str().unwrap(), expected);
        }
    }

    #[test]
    fn newline_delimited_protocol() {
        let req = Request::Retry {
            repo: sample_repo(),
            sha: sample_sha(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(
            !json.contains('\n'),
            "JSON must be single-line for IPC protocol"
        );

        let resp = Response::Status {
            runs: vec![RunSummary {
                repo: sample_repo(),
                sha: sample_sha(),
                commit_message: "a message with spaces".into(),
                attempt: AttemptNumber::first(),
                state: RunState::Running,
                jobs_total: 0,
                jobs_passed: 0,
                jobs_failed: 0,
            }],
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(!json.contains('\n'));
    }
}
