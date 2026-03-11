use crate::error::CiError;
use crate::types::{BranchName, GitSource, MaxParallel, PollInterval, RepoName, WorkDir};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

fn expand_env(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' {
            if chars.peek() == Some(&'{') {
                chars.next();
                let var_name: String = chars.by_ref().take_while(|&ch| ch != '}').collect();
                if !var_name.is_empty() {
                    match std::env::var(&var_name) {
                        Ok(val) => result.push_str(&val),
                        Err(_) => {
                            result.push_str(&format!("${{{var_name}}}"));
                        }
                    }
                }
            } else {
                let mut var_name = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch.is_ascii_alphanumeric() || ch == '_' {
                        var_name.push(ch);
                        chars.next();
                    } else {
                        break;
                    }
                }
                if !var_name.is_empty() {
                    match std::env::var(&var_name) {
                        Ok(val) => result.push_str(&val),
                        Err(_) => {
                            result.push('$');
                            result.push_str(&var_name);
                        }
                    }
                } else {
                    result.push('$');
                }
            }
        } else {
            result.push(c);
        }
    }

    result
}

#[derive(serde::Deserialize)]
struct RawConfig {
    poll_interval_secs: Option<u64>,
    max_parallel: Option<u16>,
    timeout_secs: Option<u64>,
    base_dir: String,
    #[serde(rename = "repo")]
    repos: Vec<RawRepo>,
}

#[derive(serde::Deserialize)]
struct RawRepo {
    name: String,
    source: String,
    branch: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub poll_interval: PollInterval,
    pub max_parallel: MaxParallel,
    pub timeout_secs: Option<u64>,
    pub base_dir: WorkDir,
    pub repos: Vec<RepoConfig>,
}

#[derive(Debug, Clone)]
pub struct RepoConfig {
    pub name: RepoName,
    pub source: GitSource,
    pub branch: BranchName,
    pub repo_dir: PathBuf,
}

impl Config {
    pub fn load(path: &Path) -> Result<Self, CiError> {
        let content = std::fs::read_to_string(path).map_err(|e| CiError::ConfigIo {
            path: path.to_path_buf(),
            source: e,
        })?;
        Self::parse(&content, path)
    }

    pub fn parse(toml_str: &str, origin: &Path) -> Result<Self, CiError> {
        let raw: RawConfig =
            toml::from_str(toml_str).map_err(|e| CiError::ConfigParse { source: e })?;

        let poll_interval = match raw.poll_interval_secs {
            Some(s) => PollInterval::try_from(s).map_err(|e| CiError::ConfigValidation {
                detail: e.to_string(),
            })?,
            None => PollInterval::default(),
        };

        let max_parallel = match raw.max_parallel {
            Some(n) => MaxParallel::try_from(n).map_err(|e| CiError::ConfigValidation {
                detail: e.to_string(),
            })?,
            None => MaxParallel::default(),
        };

        let base_dir_path = {
            let p = PathBuf::from(expand_env(&raw.base_dir));
            if p.is_absolute() {
                p
            } else {
                let parent = origin.parent().unwrap_or_else(|| Path::new("."));
                parent.join(p)
            }
        };

        let base_dir = WorkDir::ensure(base_dir_path).map_err(|e| CiError::ConfigValidation {
            detail: format!("base_dir: {e}"),
        })?;

        if raw.repos.is_empty() {
            return Err(CiError::ConfigValidation {
                detail: "at least one [[repo]] must be defined".to_string(),
            });
        }

        let mut seen_names = HashSet::new();
        let mut repos = Vec::with_capacity(raw.repos.len());

        for raw_repo in raw.repos {
            let repo = parse_repo(&raw_repo, &base_dir)?;

            if !seen_names.insert(repo.name.as_str().to_string()) {
                return Err(CiError::ConfigValidation {
                    detail: format!("duplicate repo name '{}'", repo.name),
                });
            }

            repos.push(repo);
        }

        Ok(Config {
            poll_interval,
            max_parallel,
            timeout_secs: raw.timeout_secs,
            base_dir,
            repos,
        })
    }
}

fn parse_repo(raw: &RawRepo, base_dir: &WorkDir) -> Result<RepoConfig, CiError> {
    let name = RepoName::try_from(raw.name.clone()).map_err(|e| CiError::ConfigValidation {
        detail: format!("repo name: {e}"),
    })?;

    let source =
        GitSource::parse(&expand_env(&raw.source)).map_err(|e| CiError::ConfigValidation {
            detail: format!("repo '{}': {e}", raw.name),
        })?;

    let branch = match &raw.branch {
        Some(b) => BranchName::try_from(b.clone()).map_err(|e| CiError::ConfigValidation {
            detail: format!("repo '{}': {e}", raw.name),
        })?,
        None => {
            BranchName::try_from("main".to_string()).map_err(|e| CiError::ConfigValidation {
                detail: e.to_string(),
            })?
        }
    };

    let hash = source.short_hash();
    let repo_dir = base_dir.join(&format!("{}-{}", name.as_str(), hash));

    if let Err(e) = std::fs::create_dir_all(&repo_dir) {
        return Err(CiError::ConfigValidation {
            detail: format!("repo '{}': failed to create repo dir: {e}", raw.name),
        });
    }

    Ok(RepoConfig {
        name,
        source,
        branch,
        repo_dir,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_toml(base_dir: &str) -> String {
        format!(
            r#"
base_dir = "{base_dir}"

[[repo]]
name = "test-repo"
source = "/tmp/fake-source"
"#
        )
    }

    #[test]
    fn parse_minimal_valid_config() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = minimal_toml(&tmp.path().to_string_lossy());
        let origin = Path::new("/fake/ci-local.toml");
        let cfg = Config::parse(&toml, origin).unwrap();

        assert_eq!(cfg.repos.len(), 1);
        assert_eq!(cfg.repos[0].name.as_str(), "test-repo");
        assert_eq!(cfg.repos[0].branch.as_str(), "main");
        assert_eq!(cfg.max_parallel.get(), 4);
        assert_eq!(
            cfg.poll_interval.as_duration(),
            std::time::Duration::from_secs(30)
        );
        assert!(cfg.repos[0].repo_dir.starts_with(tmp.path()));
        assert!(cfg.repos[0].repo_dir.is_dir());
    }

    #[test]
    fn parse_with_custom_globals() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(
            r#"
poll_interval_secs = 60
max_parallel = 2
base_dir = "{}"

[[repo]]
name = "proj"
source = "/tmp/src"
branch = "develop"
"#,
            tmp.path().display()
        );
        let cfg = Config::parse(&toml, Path::new("/x.toml")).unwrap();
        assert_eq!(
            cfg.poll_interval.as_duration(),
            std::time::Duration::from_secs(60)
        );
        assert_eq!(cfg.max_parallel.get(), 2);
        assert_eq!(cfg.repos[0].branch.as_str(), "develop");
    }

    #[test]
    fn parse_multiple_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(
            r#"
base_dir = "{}"

[[repo]]
name = "alpha"
source = "/tmp/a"

[[repo]]
name = "beta"
source = "owner/repo"
"#,
            tmp.path().display(),
        );
        let cfg = Config::parse(&toml, Path::new("/x.toml")).unwrap();
        assert_eq!(cfg.repos.len(), 2);
        assert_eq!(cfg.repos[0].name.as_str(), "alpha");
        assert_eq!(cfg.repos[1].name.as_str(), "beta");
        assert_ne!(cfg.repos[0].repo_dir, cfg.repos[1].repo_dir);
    }

    #[test]
    fn reject_no_repos() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(r#"base_dir = "{}""#, tmp.path().display());
        let err = Config::parse(&toml, Path::new("/x.toml"));
        assert!(err.is_err());
    }

    #[test]
    fn reject_empty_repos_list() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(
            r#"
base_dir = "{}"
repo = []
"#,
            tmp.path().display()
        );
        let result = Config::parse(&toml, Path::new("/x.toml"));
        assert!(result.is_err());
    }

    #[test]
    fn reject_duplicate_repo_names() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(
            r#"
base_dir = "{}"

[[repo]]
name = "same-name"
source = "/tmp/a"

[[repo]]
name = "same-name"
source = "/tmp/b"
"#,
            tmp.path().display(),
        );
        let err = Config::parse(&toml, Path::new("/x.toml")).unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("duplicate"),
            "expected duplicate error, got: {msg}"
        );
    }

    #[test]
    fn reject_poll_interval_too_low() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(
            r#"
poll_interval_secs = 2
base_dir = "{}"

[[repo]]
name = "proj"
source = "/tmp/src"
"#,
            tmp.path().display()
        );
        assert!(Config::parse(&toml, Path::new("/x.toml")).is_err());
    }

    #[test]
    fn reject_max_parallel_zero() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(
            r#"
max_parallel = 0
base_dir = "{}"

[[repo]]
name = "proj"
source = "/tmp/src"
"#,
            tmp.path().display()
        );
        assert!(Config::parse(&toml, Path::new("/x.toml")).is_err());
    }

    #[test]
    fn reject_missing_base_dir() {
        let toml = r#"
[[repo]]
name = "proj"
source = "/tmp/src"
"#;
        assert!(Config::parse(toml, Path::new("/x.toml")).is_err());
    }

    #[test]
    fn git_source_variants_in_config() {
        let tmp = tempfile::tempdir().unwrap();

        let toml = format!(
            r#"
base_dir = "{}"

[[repo]]
name = "local-proj"
source = "/home/me/code"
"#,
            tmp.path().display()
        );
        let cfg = Config::parse(&toml, Path::new("/x.toml")).unwrap();
        assert!(matches!(cfg.repos[0].source, GitSource::Local { .. }));

        let tmp2 = tempfile::tempdir().unwrap();
        let toml2 = format!(
            r#"
base_dir = "{}"

[[repo]]
name = "gh-proj"
source = "octocat/hello"
"#,
            tmp2.path().display()
        );
        let cfg2 = Config::parse(&toml2, Path::new("/x.toml")).unwrap();
        assert!(matches!(cfg2.repos[0].source, GitSource::Github { .. }));
    }

    #[test]
    fn repo_dir_contains_name_and_hash() {
        let tmp = tempfile::tempdir().unwrap();
        let toml = format!(
            r#"
base_dir = "{}"

[[repo]]
name = "my-project"
source = "/home/me/code"
"#,
            tmp.path().display()
        );
        let cfg = Config::parse(&toml, Path::new("/x.toml")).unwrap();
        let dir_name = cfg.repos[0].repo_dir.file_name().unwrap().to_string_lossy();
        assert!(
            dir_name.starts_with("my-project-"),
            "expected dir to start with 'my-project-', got: {dir_name}"
        );
        let hash_part = dir_name.strip_prefix("my-project-").unwrap();
        assert_eq!(hash_part.len(), 8);
        assert!(hash_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn expand_env_dollar_var() {
        std::env::set_var("CI_LOCAL_TEST_VAR", "/some/path");
        assert_eq!(expand_env("$CI_LOCAL_TEST_VAR/foo"), "/some/path/foo");
        std::env::remove_var("CI_LOCAL_TEST_VAR");
    }

    #[test]
    fn expand_env_brace_var() {
        std::env::set_var("CI_LOCAL_TEST_VAR2", "/other");
        assert_eq!(expand_env("${CI_LOCAL_TEST_VAR2}/bar"), "/other/bar");
        std::env::remove_var("CI_LOCAL_TEST_VAR2");
    }

    #[test]
    fn expand_env_unset_var_kept() {
        std::env::remove_var("CI_LOCAL_NONEXISTENT_9999");
        assert_eq!(
            expand_env("$CI_LOCAL_NONEXISTENT_9999"),
            "$CI_LOCAL_NONEXISTENT_9999"
        );
        assert_eq!(
            expand_env("${CI_LOCAL_NONEXISTENT_9999}"),
            "${CI_LOCAL_NONEXISTENT_9999}"
        );
    }

    #[test]
    fn expand_env_no_vars() {
        assert_eq!(expand_env("/plain/path"), "/plain/path");
    }

    #[test]
    fn expand_env_in_source() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CI_LOCAL_TEST_SRC", "/home/me/project");
        let toml = format!(
            r#"
base_dir = "{}"

[[repo]]
name = "proj"
source = "$CI_LOCAL_TEST_SRC"
"#,
            tmp.path().display()
        );
        let cfg = Config::parse(&toml, Path::new("/x.toml")).unwrap();
        match &cfg.repos[0].source {
            GitSource::Local { path } => {
                assert_eq!(path.to_string_lossy(), "/home/me/project");
            }
            other => panic!("expected Local, got {other:?}"),
        }
        std::env::remove_var("CI_LOCAL_TEST_SRC");
    }

    #[test]
    fn expand_env_in_base_dir() {
        let tmp = tempfile::tempdir().unwrap();
        std::env::set_var("CI_LOCAL_TEST_BASE", tmp.path().to_string_lossy().as_ref());
        let toml = r#"
base_dir = "${CI_LOCAL_TEST_BASE}"

[[repo]]
name = "proj"
source = "/tmp/src"
"#;
        let cfg = Config::parse(toml, Path::new("/x.toml")).unwrap();
        assert_eq!(cfg.base_dir.path(), tmp.path());
        std::env::remove_var("CI_LOCAL_TEST_BASE");
    }
}
