use crate::error::CiError;
use crate::types::{BranchName, CommitSha, GitSource};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CommitInfo {
    pub sha: CommitSha,
    pub message: String,
}

pub fn latest_commit(source: &GitSource, branch: &BranchName) -> Result<CommitInfo, CiError> {
    match source {
        GitSource::Local { path } => latest_commit_local(path, branch),
        GitSource::Remote { url } => latest_commit_remote(url, branch),
        GitSource::Github { owner, repo } => latest_commit_github(owner, repo, branch),
    }
}

fn latest_commit_local(repo_path: &Path, branch: &BranchName) -> Result<CommitInfo, CiError> {
    let output = Command::new("git")
        .args([
            "-C",
            &repo_path.to_string_lossy(),
            "log",
            "-1",
            "--format=%H%n%s",
            branch.as_str(),
        ])
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning git log for local repo",
            detail: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(CiError::Git {
            context: "git log on local repo",
            detail: stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let sha_str = lines.next().unwrap_or("").trim().to_string();
    let message = lines.next().unwrap_or("").trim().to_string();

    let sha = CommitSha::try_from(sha_str).map_err(|e| CiError::Git {
        context: "parsing commit sha from local git log",
        detail: e.to_string(),
    })?;

    Ok(CommitInfo { sha, message })
}

fn latest_commit_remote(url: &str, branch: &BranchName) -> Result<CommitInfo, CiError> {
    let refspec = format!("refs/heads/{}", branch.as_str());
    let output = Command::new("git")
        .args(["ls-remote", url, &refspec])
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning git ls-remote",
            detail: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(CiError::Git {
            context: "git ls-remote",
            detail: stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let sha_str = stdout
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().next())
        .unwrap_or("")
        .to_string();

    if sha_str.is_empty() {
        return Err(CiError::Git {
            context: "git ls-remote",
            detail: format!("no ref found for branch '{}'", branch),
        });
    }

    let sha = CommitSha::try_from(sha_str).map_err(|e| CiError::Git {
        context: "parsing commit sha from ls-remote",
        detail: e.to_string(),
    })?;

    Ok(CommitInfo {
        sha,
        message: String::new(),
    })
}

fn latest_commit_github(
    owner: &str,
    repo: &str,
    branch: &BranchName,
) -> Result<CommitInfo, CiError> {
    let endpoint = format!("repos/{owner}/{repo}/commits/{}", branch.as_str());

    let output = Command::new("gh")
        .args(["api", &endpoint, "--jq", r#".sha + "\n" + .commit.message"#])
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning gh api",
            detail: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(CiError::Git {
            context: "gh api query for latest commit",
            detail: stderr,
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();
    let sha_str = lines.next().unwrap_or("").trim().to_string();
    let message = lines.next().unwrap_or("").trim().to_string();

    let sha = CommitSha::try_from(sha_str).map_err(|e| CiError::Git {
        context: "parsing commit sha from gh api",
        detail: e.to_string(),
    })?;

    Ok(CommitInfo { sha, message })
}

pub fn read_commit_message(repo_dir: &Path, sha: &CommitSha) -> String {
    let output = Command::new("git")
        .args([
            "-C",
            &repo_dir.to_string_lossy(),
            "log",
            "-1",
            "--format=%s",
            sha.as_str(),
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
        _ => String::new(),
    }
}

pub fn prepare_worktree(source: &GitSource, sha: &CommitSha, dest: &Path) -> Result<(), CiError> {
    match source {
        GitSource::Local { path } => prepare_local(path, sha, dest),
        GitSource::Remote { url } => prepare_clone(url, sha, dest),
        GitSource::Github { owner, repo } => {
            let url = format!("https://github.com/{owner}/{repo}");
            prepare_clone(&url, sha, dest)
        }
    }
}

fn prepare_local(repo_path: &Path, sha: &CommitSha, dest: &Path) -> Result<(), CiError> {
    let output = Command::new("git")
        .args([
            "-C",
            &repo_path.to_string_lossy(),
            "worktree",
            "add",
            "--detach",
            &dest.to_string_lossy(),
            sha.as_str(),
        ])
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning git worktree add",
            detail: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        tracing::warn!(
            "git worktree add failed ({}), falling back to local clone",
            stderr.trim()
        );
        return prepare_clone(&repo_path.to_string_lossy(), sha, dest);
    }

    Ok(())
}

fn prepare_clone(url: &str, sha: &CommitSha, dest: &Path) -> Result<(), CiError> {
    let clone_result = Command::new("git")
        .args(["clone", "--no-checkout", url])
        .arg(dest)
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning git clone",
            detail: e.to_string(),
        })?;

    if !clone_result.status.success() {
        let stderr = String::from_utf8_lossy(&clone_result.stderr).to_string();
        return Err(CiError::Git {
            context: "git clone",
            detail: stderr,
        });
    }

    let checkout = Command::new("git")
        .args(["checkout", sha.as_str()])
        .current_dir(dest)
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning git checkout",
            detail: e.to_string(),
        })?;

    if checkout.status.success() {
        return Ok(());
    }

    let _ = Command::new("git")
        .args(["fetch", "origin", sha.as_str()])
        .current_dir(dest)
        .output();

    let checkout2 = Command::new("git")
        .args(["checkout", sha.as_str()])
        .current_dir(dest)
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning git checkout (retry)",
            detail: e.to_string(),
        })?;

    if !checkout2.status.success() {
        let stderr = String::from_utf8_lossy(&checkout2.stderr).to_string();
        return Err(CiError::Git {
            context: "git checkout",
            detail: stderr,
        });
    }

    Ok(())
}

pub fn list_commits_since(
    source: &GitSource,
    branch: &BranchName,
    since_sha: &CommitSha,
    mirror_dir: &Path,
) -> Result<Vec<CommitInfo>, CiError> {
    let git_dir = match source {
        GitSource::Local { path } => path.clone(),
        _ => {
            ensure_mirror(source, branch, mirror_dir)?;
            mirror_dir.to_path_buf()
        }
    };

    let range = format!("{}..{}", since_sha.as_str(), branch.as_str());
    rev_list_in(&git_dir, &range)
}

fn ensure_mirror(
    source: &GitSource,
    branch: &BranchName,
    mirror_dir: &Path,
) -> Result<(), CiError> {
    let url = match source {
        GitSource::Remote { url } => url.clone(),
        GitSource::Github { owner, repo } => format!("https://github.com/{owner}/{repo}"),
        GitSource::Local { .. } => return Ok(()),
    };

    let git_dir = mirror_dir.join(".git");
    if git_dir.is_dir() || mirror_dir.join("HEAD").exists() {
        let output = Command::new("git")
            .args([
                "-C",
                &mirror_dir.to_string_lossy(),
                "fetch",
                "origin",
                &format!("+refs/heads/{0}:refs/heads/{0}", branch.as_str()),
            ])
            .output()
            .map_err(|e| CiError::Git {
                context: "mirror fetch",
                detail: e.to_string(),
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(CiError::Git {
                context: "mirror fetch",
                detail: stderr,
            });
        }
    } else {
        let _ = std::fs::create_dir_all(mirror_dir);
        let output = Command::new("git")
            .args(["clone", "--bare", &url, &mirror_dir.to_string_lossy()])
            .output()
            .map_err(|e| CiError::Git {
                context: "mirror clone",
                detail: e.to_string(),
            })?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(CiError::Git {
                context: "mirror clone",
                detail: stderr,
            });
        }
    }
    Ok(())
}

fn rev_list_in(git_dir: &Path, range: &str) -> Result<Vec<CommitInfo>, CiError> {
    let output = Command::new("git")
        .args([
            "-C",
            &git_dir.to_string_lossy(),
            "rev-list",
            "--reverse",
            "--format=%s",
            range,
        ])
        .output()
        .map_err(|e| CiError::Git {
            context: "spawning git rev-list",
            detail: e.to_string(),
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(CiError::Git {
            context: "git rev-list",
            detail: stderr,
        });
    }

    parse_rev_list_output(&output.stdout)
}

fn parse_rev_list_output(stdout: &[u8]) -> Result<Vec<CommitInfo>, CiError> {
    let text = String::from_utf8_lossy(stdout);
    let mut commits = Vec::new();
    let mut lines = text.lines();

    while let Some(line) = lines.next() {
        let sha_str = if let Some(stripped) = line.strip_prefix("commit ") {
            stripped.trim().to_string()
        } else {
            line.trim().to_string()
        };

        let message = lines.next().unwrap_or("").trim().to_string();

        if sha_str.is_empty() {
            continue;
        }

        let sha = CommitSha::try_from(sha_str).map_err(|e| CiError::Git {
            context: "parsing commit sha from rev-list",
            detail: e.to_string(),
        })?;

        commits.push(CommitInfo { sha, message });
    }

    Ok(commits)
}

pub fn cleanup_worktree(source: &GitSource, dest: &Path) {
    if let GitSource::Local { path } = source {
        let _ = Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "worktree",
                "remove",
                "--force",
                &dest.to_string_lossy(),
            ])
            .output();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_git_repo() -> (tempfile::TempDir, String) {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        let run = |args: &[&str]| {
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
        };

        run(&["init", "-b", "main"]);
        run(&["config", "commit.gpgsign", "false"]);
        std::fs::write(dir.join("hello.txt"), "hello world").unwrap();
        run(&["add", "."]);
        run(&["commit", "--no-gpg-sign", "-m", "initial commit"]);

        let sha = run(&["rev-parse", "HEAD"]);
        (tmp, sha)
    }

    fn add_commit(repo_dir: &std::path::Path, filename: &str, message: &str) -> String {
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .args(args)
                .current_dir(repo_dir)
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
        };

        std::fs::write(repo_dir.join(filename), "content").unwrap();
        run(&["add", "."]);
        run(&["commit", "--no-gpg-sign", "-m", message]);
        run(&["rev-parse", "HEAD"])
    }

    #[test]
    fn latest_commit_local_returns_sha_and_message() {
        let (tmp, expected_sha) = make_git_repo();
        let source = GitSource::Local {
            path: tmp.path().to_path_buf(),
        };
        let branch = BranchName::try_from("main".to_string()).unwrap();

        let info = latest_commit(&source, &branch).unwrap();
        assert_eq!(info.sha.as_str(), expected_sha);
        assert_eq!(info.message, "initial commit");
    }

    #[test]
    fn latest_commit_tracks_new_commits() {
        let (tmp, first_sha) = make_git_repo();
        let source = GitSource::Local {
            path: tmp.path().to_path_buf(),
        };
        let branch = BranchName::try_from("main".to_string()).unwrap();

        let info1 = latest_commit(&source, &branch).unwrap();
        assert_eq!(info1.sha.as_str(), first_sha);

        let second_sha = add_commit(tmp.path(), "second.txt", "second commit");
        let info2 = latest_commit(&source, &branch).unwrap();
        assert_eq!(info2.sha.as_str(), second_sha);
        assert_eq!(info2.message, "second commit");
        assert_ne!(info1.sha, info2.sha);
    }

    #[test]
    fn latest_commit_nonexistent_branch_fails() {
        let (tmp, _) = make_git_repo();
        let source = GitSource::Local {
            path: tmp.path().to_path_buf(),
        };
        let branch = BranchName::try_from("nonexistent-branch".to_string()).unwrap();
        assert!(latest_commit(&source, &branch).is_err());
    }

    #[test]
    fn prepare_worktree_local_creates_checkout() {
        let (tmp, sha_str) = make_git_repo();
        let sha = CommitSha::try_from(sha_str).unwrap();
        let source = GitSource::Local {
            path: tmp.path().to_path_buf(),
        };

        let dest = tempfile::tempdir().unwrap();
        let worktree_path = dest.path().join("worktree");

        prepare_worktree(&source, &sha, &worktree_path).unwrap();
        assert!(worktree_path.join("hello.txt").exists());
        let content = std::fs::read_to_string(worktree_path.join("hello.txt")).unwrap();
        assert_eq!(content, "hello world");

        cleanup_worktree(&source, &worktree_path);
    }

    #[test]
    fn prepare_worktree_via_clone() {
        let (tmp, sha_str) = make_git_repo();
        let sha = CommitSha::try_from(sha_str).unwrap();

        let dest = tempfile::tempdir().unwrap();
        let clone_path = dest.path().join("cloned");

        prepare_clone(&tmp.path().to_string_lossy(), &sha, &clone_path).unwrap();
        assert!(clone_path.join("hello.txt").exists());
    }

    #[test]
    fn read_commit_message_from_clone() {
        let (tmp, sha_str) = make_git_repo();
        let sha = CommitSha::try_from(sha_str).unwrap();

        let dest = tempfile::tempdir().unwrap();
        let clone_path = dest.path().join("cloned");
        prepare_clone(&tmp.path().to_string_lossy(), &sha, &clone_path).unwrap();

        let msg = read_commit_message(&clone_path, &sha);
        assert_eq!(msg, "initial commit");
    }

    #[test]
    fn prepare_worktree_specific_commit() {
        let (tmp, first_sha_str) = make_git_repo();
        let _ = add_commit(tmp.path(), "second.txt", "second commit");

        let first_sha = CommitSha::try_from(first_sha_str).unwrap();
        let source = GitSource::Local {
            path: tmp.path().to_path_buf(),
        };

        let dest = tempfile::tempdir().unwrap();
        let worktree_path = dest.path().join("wt");

        prepare_worktree(&source, &first_sha, &worktree_path).unwrap();
        assert!(worktree_path.join("hello.txt").exists());
        assert!(!worktree_path.join("second.txt").exists());

        cleanup_worktree(&source, &worktree_path);
    }

    #[test]
    fn list_commits_since_returns_new_commits() {
        let (tmp, first_sha_str) = make_git_repo();
        let second_sha_str = add_commit(tmp.path(), "second.txt", "second commit");
        let third_sha_str = add_commit(tmp.path(), "third.txt", "third commit");

        let source = GitSource::Local {
            path: tmp.path().to_path_buf(),
        };
        let branch = BranchName::try_from("main".to_string()).unwrap();
        let since = CommitSha::try_from(first_sha_str).unwrap();
        let mirror = tempfile::tempdir().unwrap();

        let commits = list_commits_since(&source, &branch, &since, mirror.path()).unwrap();
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].sha.as_str(), second_sha_str);
        assert_eq!(commits[0].message, "second commit");
        assert_eq!(commits[1].sha.as_str(), third_sha_str);
        assert_eq!(commits[1].message, "third commit");
    }

    #[test]
    fn list_commits_since_empty_when_up_to_date() {
        let (tmp, sha_str) = make_git_repo();
        let source = GitSource::Local {
            path: tmp.path().to_path_buf(),
        };
        let branch = BranchName::try_from("main".to_string()).unwrap();
        let since = CommitSha::try_from(sha_str).unwrap();
        let mirror = tempfile::tempdir().unwrap();

        let commits = list_commits_since(&source, &branch, &since, mirror.path()).unwrap();
        assert!(commits.is_empty());
    }
}
