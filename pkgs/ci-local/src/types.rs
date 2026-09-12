use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String")]
pub struct CommitSha(String);

#[derive(Debug)]
pub struct InvalidCommitSha {
    value: String,
    reason: &'static str,
}

impl fmt::Display for InvalidCommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid commit sha '{}': {}", self.value, self.reason)
    }
}

impl std::error::Error for InvalidCommitSha {}

impl TryFrom<String> for CommitSha {
    type Error = InvalidCommitSha;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim().to_lowercase();
        if trimmed.len() != 40 {
            return Err(InvalidCommitSha {
                value,
                reason: "must be exactly 40 characters",
            });
        }
        if !trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(InvalidCommitSha {
                value,
                reason: "must contain only hexadecimal characters",
            });
        }
        Ok(Self(trimmed))
    }
}

impl CommitSha {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn short(&self) -> &str {
        &self.0[..8]
    }
}

impl fmt::Display for CommitSha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String")]
pub struct BranchName(String);

#[derive(Debug)]
pub struct InvalidBranchName;

impl fmt::Display for InvalidBranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("branch name must be non-empty")
    }
}

impl std::error::Error for InvalidBranchName {}

impl TryFrom<String> for BranchName {
    type Error = InvalidBranchName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            return Err(InvalidBranchName);
        }
        Ok(Self(trimmed))
    }
}

impl BranchName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BranchName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String")]
pub struct RepoName(String);

#[derive(Debug)]
pub struct InvalidRepoName {
    pub detail: String,
}

impl fmt::Display for InvalidRepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid repo name: {}", self.detail)
    }
}

impl std::error::Error for InvalidRepoName {}

impl TryFrom<String> for RepoName {
    type Error = InvalidRepoName;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let trimmed = value.trim().to_string();
        if trimmed.is_empty() {
            return Err(InvalidRepoName {
                detail: "must be non-empty".to_string(),
            });
        }
        if !trimmed
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(InvalidRepoName {
                detail: format!(
                    "'{}' contains invalid characters (only a-z, 0-9, -, _ allowed)",
                    trimmed
                ),
            });
        }
        Ok(Self(trimmed))
    }
}

impl RepoName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RepoName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Debug, Clone)]
pub struct WorkDir(PathBuf);

#[derive(Debug)]
pub struct InvalidWorkDir {
    path: PathBuf,
    reason: &'static str,
}

impl fmt::Display for InvalidWorkDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid work directory '{}': {}",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for InvalidWorkDir {}

impl WorkDir {
    pub fn ensure(path: PathBuf) -> Result<Self, InvalidWorkDir> {
        if path.as_os_str().is_empty() {
            return Err(InvalidWorkDir {
                path,
                reason: "path is empty",
            });
        }
        if !path.is_absolute() {
            return Err(InvalidWorkDir {
                path,
                reason: "path must be absolute",
            });
        }
        if path.exists() && !path.is_dir() {
            return Err(InvalidWorkDir {
                path,
                reason: "path exists but is not a directory",
            });
        }
        if !path.exists() {
            std::fs::create_dir_all(&path).map_err(|_| InvalidWorkDir {
                path: path.clone(),
                reason: "could not create directory",
            })?;
        }
        Ok(Self(path))
    }

    pub fn path(&self) -> &Path {
        &self.0
    }

    pub fn join(&self, component: &str) -> PathBuf {
        self.0.join(component)
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u16")]
pub struct MaxParallel(u16);

#[derive(Debug)]
pub struct InvalidMaxParallel;

impl fmt::Display for InvalidMaxParallel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("max_parallel must be >= 1")
    }
}

impl std::error::Error for InvalidMaxParallel {}

impl TryFrom<u16> for MaxParallel {
    type Error = InvalidMaxParallel;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvalidMaxParallel);
        }
        Ok(Self(value))
    }
}

impl MaxParallel {
    pub fn get(self) -> u16 {
        self.0
    }
}

impl Default for MaxParallel {
    fn default() -> Self {
        let cpus = std::thread::available_parallelism()
            .map(|n| n.get() as u16)
            .unwrap_or(4);
        Self(cpus.max(1))
    }
}

#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u64")]
pub struct PollInterval(u64);

#[derive(Debug)]
pub struct InvalidPollInterval;

impl fmt::Display for InvalidPollInterval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("poll_interval_secs must be >= 5")
    }
}

impl std::error::Error for InvalidPollInterval {}

impl TryFrom<u64> for PollInterval {
    type Error = InvalidPollInterval;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        if value < 5 {
            return Err(InvalidPollInterval);
        }
        Ok(Self(value))
    }
}

impl PollInterval {
    pub fn as_duration(self) -> std::time::Duration {
        std::time::Duration::from_secs(self.0)
    }
}

impl Default for PollInterval {
    fn default() -> Self {
        Self(30)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "u32")]
pub struct AttemptNumber(u32);

#[derive(Debug)]
pub struct InvalidAttemptNumber;

impl fmt::Display for InvalidAttemptNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("attempt number must be >= 1")
    }
}

impl std::error::Error for InvalidAttemptNumber {}

impl TryFrom<u32> for AttemptNumber {
    type Error = InvalidAttemptNumber;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        if value == 0 {
            return Err(InvalidAttemptNumber);
        }
        Ok(Self(value))
    }
}

impl AttemptNumber {
    pub fn first() -> Self {
        Self(1)
    }

    #[allow(dead_code)]
    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }

    #[allow(dead_code)]
    pub fn get(self) -> u32 {
        self.0
    }
}

impl fmt::Display for AttemptNumber {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GitSource {
    Local { path: PathBuf },
    Remote { url: String },
    Github { owner: String, repo: String },
}

#[derive(Debug)]
pub struct InvalidGitSource {
    pub detail: String,
}

impl fmt::Display for InvalidGitSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid git source: {}", self.detail)
    }
}

impl std::error::Error for InvalidGitSource {}

impl GitSource {
    pub fn parse(value: &str) -> Result<Self, InvalidGitSource> {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            return Err(InvalidGitSource {
                detail: "source must be non-empty".to_string(),
            });
        }

        if trimmed.starts_with('/') || trimmed.starts_with('.') || trimmed.starts_with('~') {
            let path = PathBuf::from(shellexpand(trimmed));
            return Ok(GitSource::Local { path });
        }

        if trimmed.contains("://") || trimmed.starts_with("git@") {
            return Ok(GitSource::Remote {
                url: trimmed.to_string(),
            });
        }

        let parts: Vec<&str> = trimmed.splitn(2, '/').collect();
        match parts.as_slice() {
            [owner, repo] if !owner.is_empty() && !repo.is_empty() => Ok(GitSource::Github {
                owner: owner.to_string(),
                repo: repo.to_string(),
            }),
            _ => Err(InvalidGitSource {
                detail: format!("'{trimmed}' is not a local path, URL, or owner/repo slug"),
            }),
        }
    }

    pub fn display_name(&self) -> String {
        match self {
            GitSource::Local { path } => format!("local:{}", path.display()),
            GitSource::Remote { url } => format!("remote:{url}"),
            GitSource::Github { owner, repo } => format!("github:{owner}/{repo}"),
        }
    }

    pub fn short_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        match self {
            GitSource::Local { path } => path.hash(&mut hasher),
            GitSource::Remote { url } => url.hash(&mut hasher),
            GitSource::Github { owner, repo } => {
                let url = format!("https://github.com/{owner}/{repo}");
                url.hash(&mut hasher);
            }
        }
        let h = hasher.finish();
        format!("{:016x}", h)[..8].to_string()
    }
}

impl fmt::Display for GitSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.display_name())
    }
}

fn shellexpand(s: &str) -> String {
    if let Some(rest) = s.strip_prefix('~') {
        if let Ok(home) = std::env::var("HOME") {
            return format!("{home}{rest}");
        }
    }
    s.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commit_sha_valid_lowercase() {
        let sha = CommitSha::try_from("a".repeat(40)).unwrap();
        assert_eq!(sha.as_str(), "a".repeat(40));
    }

    #[test]
    fn commit_sha_valid_mixed_case_normalizes() {
        let input = format!("{}{}", "A".repeat(20), "b".repeat(20));
        let sha = CommitSha::try_from(input).unwrap();
        assert_eq!(
            sha.as_str(),
            format!("{}{}", "a".repeat(20), "b".repeat(20))
        );
    }

    #[test]
    fn commit_sha_valid_with_whitespace_trimmed() {
        let input = format!("  {}  ", "f".repeat(40));
        let sha = CommitSha::try_from(input).unwrap();
        assert_eq!(sha.as_str(), "f".repeat(40));
    }

    #[test]
    fn commit_sha_too_short() {
        assert!(CommitSha::try_from("abc123".to_string()).is_err());
    }

    #[test]
    fn commit_sha_too_long() {
        assert!(CommitSha::try_from("a".repeat(41)).is_err());
    }

    #[test]
    fn commit_sha_non_hex() {
        let mut bad = "a".repeat(39);
        bad.push('g');
        assert!(CommitSha::try_from(bad).is_err());
    }

    #[test]
    fn commit_sha_short_returns_first_8() {
        let sha = CommitSha::try_from("abcdef01".to_string() + &"0".repeat(32)).unwrap();
        assert_eq!(sha.short(), "abcdef01");
    }

    #[test]
    fn commit_sha_display() {
        let hex = "1".repeat(40);
        let sha = CommitSha::try_from(hex.clone()).unwrap();
        assert_eq!(format!("{sha}"), hex);
    }

    #[test]
    fn branch_name_valid() {
        let b = BranchName::try_from("main".to_string()).unwrap();
        assert_eq!(b.as_str(), "main");
    }

    #[test]
    fn branch_name_trims_whitespace() {
        let b = BranchName::try_from("  develop  ".to_string()).unwrap();
        assert_eq!(b.as_str(), "develop");
    }

    #[test]
    fn branch_name_empty_rejected() {
        assert!(BranchName::try_from("".to_string()).is_err());
    }

    #[test]
    fn branch_name_whitespace_only_rejected() {
        assert!(BranchName::try_from("   ".to_string()).is_err());
    }

    #[test]
    fn repo_name_alphanumeric() {
        let r = RepoName::try_from("my-project_01".to_string()).unwrap();
        assert_eq!(r.as_str(), "my-project_01");
    }

    #[test]
    fn repo_name_empty_rejected() {
        assert!(RepoName::try_from("".to_string()).is_err());
    }

    #[test]
    fn repo_name_special_chars_rejected() {
        assert!(RepoName::try_from("my project".to_string()).is_err());
        assert!(RepoName::try_from("foo/bar".to_string()).is_err());
        assert!(RepoName::try_from("a.b".to_string()).is_err());
    }

    #[test]
    fn max_parallel_valid() {
        let m = MaxParallel::try_from(8u16).unwrap();
        assert_eq!(m.get(), 8);
    }

    #[test]
    fn max_parallel_one_is_ok() {
        assert!(MaxParallel::try_from(1u16).is_ok());
    }

    #[test]
    fn max_parallel_zero_rejected() {
        assert!(MaxParallel::try_from(0u16).is_err());
    }

    #[test]
    fn max_parallel_default_is_cpu_count() {
        let expected = std::thread::available_parallelism()
            .map(|n| n.get() as u16)
            .unwrap_or(4);
        assert_eq!(MaxParallel::default().get(), expected);
    }

    #[test]
    fn poll_interval_valid() {
        let p = PollInterval::try_from(60u64).unwrap();
        assert_eq!(p.as_duration(), std::time::Duration::from_secs(60));
    }

    #[test]
    fn poll_interval_minimum_5() {
        assert!(PollInterval::try_from(5u64).is_ok());
        assert!(PollInterval::try_from(4u64).is_err());
    }

    #[test]
    fn poll_interval_default_is_30() {
        assert_eq!(
            PollInterval::default().as_duration(),
            std::time::Duration::from_secs(30)
        );
    }

    #[test]
    fn attempt_number_first_is_1() {
        assert_eq!(AttemptNumber::first().get(), 1);
    }

    #[test]
    fn attempt_number_zero_rejected() {
        assert!(AttemptNumber::try_from(0u32).is_err());
    }

    #[test]
    fn attempt_number_next() {
        let a = AttemptNumber::first();
        let b = a.next().unwrap();
        assert_eq!(b.get(), 2);
    }

    #[test]
    fn attempt_number_next_at_max_returns_none() {
        let a = AttemptNumber::try_from(u32::MAX).unwrap();
        assert!(a.next().is_none());
    }

    #[test]
    fn git_source_local_absolute() {
        match GitSource::parse("/home/me/repo").unwrap() {
            GitSource::Local { path } => assert_eq!(path, PathBuf::from("/home/me/repo")),
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn git_source_local_relative_dot() {
        match GitSource::parse("./my-repo").unwrap() {
            GitSource::Local { path } => assert_eq!(path, PathBuf::from("./my-repo")),
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn git_source_local_tilde() {
        let result = GitSource::parse("~/projects/foo").unwrap();
        match result {
            GitSource::Local { path } => {
                let expected = if let Ok(home) = std::env::var("HOME") {
                    PathBuf::from(format!("{home}/projects/foo"))
                } else {
                    PathBuf::from("~/projects/foo")
                };
                assert_eq!(path, expected);
            }
            other => panic!("expected Local, got {other:?}"),
        }
    }

    #[test]
    fn git_source_remote_https() {
        match GitSource::parse("https://github.com/foo/bar.git").unwrap() {
            GitSource::Remote { url } => {
                assert_eq!(url, "https://github.com/foo/bar.git");
            }
            other => panic!("expected Remote, got {other:?}"),
        }
    }

    #[test]
    fn git_source_remote_git_at() {
        match GitSource::parse("git@github.com:foo/bar.git").unwrap() {
            GitSource::Remote { url } => {
                assert_eq!(url, "git@github.com:foo/bar.git");
            }
            other => panic!("expected Remote, got {other:?}"),
        }
    }

    #[test]
    fn git_source_github_slug() {
        match GitSource::parse("octocat/hello-world").unwrap() {
            GitSource::Github { owner, repo } => {
                assert_eq!(owner, "octocat");
                assert_eq!(repo, "hello-world");
            }
            other => panic!("expected Github, got {other:?}"),
        }
    }

    #[test]
    fn git_source_empty_rejected() {
        assert!(GitSource::parse("").is_err());
        assert!(GitSource::parse("   ").is_err());
    }

    #[test]
    fn git_source_display_name() {
        let local = GitSource::Local {
            path: PathBuf::from("/tmp/repo"),
        };
        assert!(local.display_name().starts_with("local:"));

        let remote = GitSource::Remote {
            url: "https://example.com/repo.git".into(),
        };
        assert!(remote.display_name().starts_with("remote:"));

        let gh = GitSource::Github {
            owner: "foo".into(),
            repo: "bar".into(),
        };
        assert_eq!(gh.display_name(), "github:foo/bar");
    }

    #[test]
    fn workdir_creates_missing_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let new_dir = tmp.path().join("sub").join("nested");
        assert!(!new_dir.exists());

        let wd = WorkDir::ensure(new_dir.clone()).unwrap();
        assert!(new_dir.is_dir());
        assert_eq!(wd.path(), new_dir);
    }

    #[test]
    fn workdir_existing_dir_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let wd = WorkDir::ensure(tmp.path().to_path_buf()).unwrap();
        assert_eq!(wd.path(), tmp.path());
    }

    #[test]
    fn workdir_empty_path_rejected() {
        assert!(WorkDir::ensure(PathBuf::new()).is_err());
    }

    #[test]
    fn workdir_relative_path_rejected() {
        assert!(WorkDir::ensure(PathBuf::from("relative/path")).is_err());
    }

    #[test]
    fn workdir_file_not_dir_rejected() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("a-file");
        std::fs::write(&file_path, "contents").unwrap();
        assert!(WorkDir::ensure(file_path).is_err());
    }

    #[test]
    fn workdir_join() {
        let tmp = tempfile::tempdir().unwrap();
        let wd = WorkDir::ensure(tmp.path().to_path_buf()).unwrap();
        assert_eq!(wd.join("foo"), tmp.path().join("foo"));
    }
}
