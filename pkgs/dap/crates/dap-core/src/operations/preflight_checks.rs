use crate::matcher::{find_occurrences, split_lines};
use crate::operations::hunks::apply_hunks;
use crate::types::{Hunk, Patch, PatchOp};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub fn run_preflight_checks(patches: &[Patch]) -> Result<(), Vec<String>> {
    println!("--- Running Preflight Checks ---");

    let mut errors = Vec::new();
    let mut tree = ProjectedTree::default();

    for (i, patch) in patches.iter().enumerate() {
        let prefix = format!("  - Patch #{} for '{}':", i + 1, patch.file_path.display());

        match check_patch(patch, &mut tree) {
            Ok(outcome) => println!("{} {}", prefix, outcome),
            Err(failure) => errors.push(format!("{} {}", prefix, failure)),
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ProjectedFile {
    Absent,
    Text(String),
    Unreadable(String),
}

#[derive(Debug, Default)]
struct ProjectedTree {
    entries: HashMap<PathBuf, ProjectedFile>,
}

impl ProjectedTree {
    fn state(&mut self, path: &Path) -> ProjectedFile {
        self.entries
            .entry(path.to_path_buf())
            .or_insert_with(|| read_projected_file(path))
            .clone()
    }

    fn set(&mut self, path: &Path, state: ProjectedFile) {
        self.entries.insert(path.to_path_buf(), state);
    }

    fn text(&mut self, path: &Path) -> Result<String, String> {
        match self.state(path) {
            ProjectedFile::Text(content) => Ok(content),
            ProjectedFile::Absent => Err("FAILED (File not found)".to_string()),
            ProjectedFile::Unreadable(error) => {
                Err(format!("FAILED (Could not read file: {})", error))
            }
        }
    }
}

fn check_patch(patch: &Patch, tree: &mut ProjectedTree) -> Result<String, String> {
    if is_read_only(&patch.file_path) {
        return Err("FAILED (File is read-only)".to_string());
    }

    match &patch.op {
        PatchOp::Move(dest) => check_move(&patch.file_path, dest, tree),
        PatchOp::Delete => check_delete(&patch.file_path, tree),
        PatchOp::Modify { search, replace } => {
            check_modify(&patch.file_path, search, replace, tree)
        }
        PatchOp::Udiff(hunks) => check_udiff(&patch.file_path, hunks, tree),
    }
}

fn check_move(source: &Path, dest: &Path, tree: &mut ProjectedTree) -> Result<String, String> {
    let moved = tree.state(source);
    if moved == ProjectedFile::Absent {
        return Err("FAILED (Source file not found)".to_string());
    }

    if tree.state(dest) != ProjectedFile::Absent {
        return Err(format!(
            "FAILED (Destination file '{}' already exists)",
            dest.display()
        ));
    }

    tree.set(source, ProjectedFile::Absent);
    tree.set(dest, moved);

    Ok(format!("OK (Move to '{}')", dest.display()))
}

fn check_delete(path: &Path, tree: &mut ProjectedTree) -> Result<String, String> {
    if tree.state(path) == ProjectedFile::Absent {
        return Err("FAILED (File not found, cannot delete)".to_string());
    }

    tree.set(path, ProjectedFile::Absent);

    Ok("OK (File scheduled for deletion)".to_string())
}

fn check_modify(
    path: &Path,
    search: &str,
    replace: &str,
    tree: &mut ProjectedTree,
) -> Result<String, String> {
    if search.trim().is_empty() {
        let overwrites = tree.state(path) != ProjectedFile::Absent;
        tree.set(path, ProjectedFile::Text(replace.to_string()));

        return Ok(if overwrites {
            "OK (File will be overwritten)".to_string()
        } else {
            "OK (New file creation)".to_string()
        });
    }

    let content = tree.text(path)?;
    let mut source_lines = split_lines(&content);
    let (matches, match_len) = find_occurrences(&source_lines, search, None);

    if matches.is_empty() {
        return Err("FAILED (Search block not found)".to_string());
    }

    if matches.len() > 1 {
        return Err(format!(
            "FAILED (Search block is ambiguous, found {} times)",
            matches.len()
        ));
    }

    let start_idx = matches[0];
    source_lines.splice(start_idx..start_idx + match_len, split_lines(replace));
    tree.set(path, ProjectedFile::Text(source_lines.concat()));

    Ok("OK".to_string())
}

fn check_udiff(path: &Path, hunks: &[Hunk], tree: &mut ProjectedTree) -> Result<String, String> {
    let creates_file = tree.state(path) == ProjectedFile::Absent;

    if creates_file && !hunks.iter().any(|hunk| hunk.old_start == 0) {
        return Err("FAILED (File not found)".to_string());
    }

    if hunks.is_empty() {
        return Err("FAILED (Udiff patch contains no hunks)".to_string());
    }

    let content = if creates_file {
        String::new()
    } else {
        tree.text(path)?
    };

    let patched = apply_hunks(&content, hunks).map_err(|failure| {
        format!(
            "FAILED (Hunk #{} failed. Expected 1 match, found {})",
            failure.index + 1,
            failure.matches
        )
    })?;

    tree.set(path, ProjectedFile::Text(patched));

    Ok(if creates_file {
        "OK (New file creation via Udiff)".to_string()
    } else {
        "OK".to_string()
    })
}

fn read_projected_file(path: &Path) -> ProjectedFile {
    if !path.exists() {
        return ProjectedFile::Absent;
    }

    match fs::read_to_string(path) {
        Ok(content) => ProjectedFile::Text(content),
        Err(error) => ProjectedFile::Unreadable(error.to_string()),
    }
}

fn is_read_only(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Hunk, HunkLine};
    use tempfile::tempdir;

    #[test]
    fn test_run_preflight_checks_modify_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        fs::write(&file_path, "def hello():\n    pass").unwrap();

        let patch = Patch {
            file_path: file_path.clone(),
            op: PatchOp::Modify {
                search: "def hello():\n    pass".to_string(),
                replace: "def world()".to_string(),
            },
        };

        let result = run_preflight_checks(&[patch]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_preflight_checks_modify_not_found() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("nonexistent.py");

        let patch = Patch {
            file_path: file_path.clone(),
            op: PatchOp::Modify {
                search: "def hello()".to_string(),
                replace: "def world()".to_string(),
            },
        };

        let result = run_preflight_checks(&[patch]);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].contains("File not found"));
    }

    #[test]
    fn test_run_preflight_checks_move_success() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("old.py");
        let dst = dir.path().join("new.py");
        fs::write(&src, "content").unwrap();

        let patch = Patch {
            file_path: src.clone(),
            op: PatchOp::Move(dst.clone()),
        };

        let result = run_preflight_checks(&[patch]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_preflight_checks_delete_success() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        fs::write(&file_path, "content").unwrap();

        let patch = Patch {
            file_path: file_path.clone(),
            op: PatchOp::Delete,
        };

        let result = run_preflight_checks(&[patch]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_preflight_checks_udiff() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        fs::write(&file_path, "def hello():\n    pass").unwrap();

        let hunk = Hunk {
            old_start: 1,
            old_len: 2,
            new_start: 1,
            new_len: 3,
            lines: vec![
                HunkLine::Context(" def hello():\n".to_string()),
                HunkLine::Add("+    print('Hello')\n".to_string()),
                HunkLine::Context("     pass\n".to_string()),
            ],
        };

        let patch = Patch {
            file_path: file_path.clone(),
            op: PatchOp::Udiff(vec![hunk]),
        };

        let result = run_preflight_checks(&[patch]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_run_preflight_checks_udiff_file_not_found() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("nonexistent.py");

        let hunk = Hunk {
            old_start: 1,
            old_len: 1,
            new_start: 1,
            new_len: 1,
            lines: vec![HunkLine::Context(" test\n".to_string())],
        };

        let patch = Patch {
            file_path: file_path.clone(),
            op: PatchOp::Udiff(vec![hunk]),
        };

        let result = run_preflight_checks(&[patch]);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].contains("File not found"));
    }

    #[test]
    fn test_run_preflight_checks_udiff_empty_hunks() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        fs::write(&file_path, "def hello():\n    pass").unwrap();

        let patch = Patch {
            file_path: file_path.clone(),
            op: PatchOp::Udiff(vec![]),
        };

        let result = run_preflight_checks(&[patch]);
        assert!(result.is_err());
        let errors = result.unwrap_err();
        assert!(errors[0].contains("contains no hunks"));
    }

    #[test]
    fn test_run_preflight_checks_move_then_patch_destination() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("old.py");
        let dst = dir.path().join("new.py");
        fs::write(&src, "print('old')\n").unwrap();

        let patches = vec![
            Patch {
                file_path: src.clone(),
                op: PatchOp::Move(dst.clone()),
            },
            Patch {
                file_path: dst.clone(),
                op: PatchOp::Udiff(vec![Hunk {
                    old_start: 1,
                    old_len: 1,
                    new_start: 1,
                    new_len: 1,
                    lines: vec![
                        HunkLine::Remove("-print('old')\n".to_string()),
                        HunkLine::Add("+print('new')\n".to_string()),
                    ],
                }]),
            },
        ];

        let result = run_preflight_checks(&patches);
        assert!(result.is_ok(), "{:?}", result.err());
    }

    #[test]
    fn test_run_preflight_checks_reports_stale_source_after_move() {
        let dir = tempdir().unwrap();
        let src = dir.path().join("old.py");
        let dst = dir.path().join("new.py");
        fs::write(&src, "print('old')\n").unwrap();

        let patches = vec![
            Patch {
                file_path: src.clone(),
                op: PatchOp::Move(dst.clone()),
            },
            Patch {
                file_path: src.clone(),
                op: PatchOp::Delete,
            },
        ];

        let result = run_preflight_checks(&patches);
        let errors = result.unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("File not found, cannot delete"));
    }

    #[test]
    fn test_run_preflight_checks_chains_edits_on_one_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        fs::write(&file_path, "one\ntwo\n").unwrap();

        let patches = vec![
            Patch {
                file_path: file_path.clone(),
                op: PatchOp::Modify {
                    search: "one\n".to_string(),
                    replace: "uno\n".to_string(),
                },
            },
            Patch {
                file_path: file_path.clone(),
                op: PatchOp::Modify {
                    search: "uno\n".to_string(),
                    replace: "first\n".to_string(),
                },
            },
        ];

        let result = run_preflight_checks(&patches);
        assert!(result.is_ok(), "{:?}", result.err());
    }
}
