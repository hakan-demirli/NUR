use crate::types::{Hunk, HunkLine, Patch, PatchOp};
use std::path::{Path, PathBuf};

pub const UDIFF_OLD_FILE_PREFIX: &str = "--- ";
pub const UDIFF_NEW_FILE_PREFIX: &str = "+++ ";
pub const UDIFF_HUNK_HEADER_PREFIX: &str = "@@ ";
pub const UDIFF_GIT_HEADER_PREFIX: &str = "diff --git ";

const UDIFF_RENAME_FROM_PREFIX: &str = "rename from ";
const UDIFF_RENAME_TO_PREFIX: &str = "rename to ";
const UDIFF_BINARY_NOTICE_PREFIX: &str = "Binary files ";
const UDIFF_NO_NEWLINE_PREFIX: &str = "\\";

const GIT_OLD_PATH_PREFIX: &str = "a/";
const GIT_NEW_PATH_PREFIX: &str = "b/";

const DEV_NULL: &str = "/dev/null";

const GIT_METADATA_PREFIXES: [&str; 10] = [
    "index ",
    "old mode ",
    "new mode ",
    "new file mode ",
    "deleted file mode ",
    "similarity index ",
    "dissimilarity index ",
    "copy from ",
    "copy to ",
    "GIT binary patch",
];

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum PathStyle {
    #[default]
    Verbatim,
    Git,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum HeaderSide {
    Old,
    New,
}

#[derive(Debug, Default)]
pub struct UdiffSection {
    style: PathStyle,
    old_path: Option<PathBuf>,
    new_path: Option<PathBuf>,
    rename_from: Option<PathBuf>,
    rename_to: Option<PathBuf>,
    hunks: Vec<Hunk>,
}

impl UdiffSection {
    pub fn set_style(&mut self, style: PathStyle) {
        self.style = style;
    }

    pub fn has_file_paths(&self) -> bool {
        self.old_path.is_some() || self.new_path.is_some()
    }

    pub fn set_old_path(&mut self, value: &str) {
        self.old_path = Some(parse_header_path(value, self.style, HeaderSide::Old));
    }

    pub fn set_new_path(&mut self, value: &str) {
        self.new_path = Some(parse_header_path(value, self.style, HeaderSide::New));
    }

    pub fn set_rename_from(&mut self, value: &str) {
        self.rename_from = Some(parse_header_path(
            value,
            PathStyle::Verbatim,
            HeaderSide::Old,
        ));
    }

    pub fn set_rename_to(&mut self, value: &str) {
        self.rename_to = Some(parse_header_path(
            value,
            PathStyle::Verbatim,
            HeaderSide::New,
        ));
    }

    pub fn finalize(&mut self) -> Vec<Patch> {
        let hunks = std::mem::take(&mut self.hunks);
        let rename = self.rename_from.take().zip(self.rename_to.take());
        let old_path = self.old_path.take();
        let new_path = self.new_path.take();
        self.style = PathStyle::Verbatim;

        let (source, target) = match (rename, old_path, new_path) {
            (Some((from, to)), _, _) => (from, to),
            (None, Some(old), Some(new)) => (old, new),
            (None, Some(path), None) | (None, None, Some(path)) => (path.clone(), path),
            (None, None, None) => return Vec::new(),
        };

        resolve_patches(source, target, hunks)
    }
}

pub fn handle_git_header_line(
    line: &str,
    stripped: &str,
    section: &mut UdiffSection,
    previous_line: &mut String,
) -> (super::ParserState, Vec<Patch>) {
    if let Some(value) = stripped.strip_prefix(UDIFF_RENAME_FROM_PREFIX) {
        section.set_rename_from(value);
    } else if let Some(value) = stripped.strip_prefix(UDIFF_RENAME_TO_PREFIX) {
        section.set_rename_to(value);
    } else if stripped.is_empty() || is_git_metadata_line(stripped) {
    } else {
        *previous_line = line.to_string();
        return (super::ParserState::Idle, section.finalize());
    }

    (super::ParserState::InGitHeader, Vec::new())
}

pub fn handle_udiff_line(
    line: &str,
    stripped: &str,
    section: &mut UdiffSection,
    previous_line: &mut String,
) -> (super::ParserState, Vec<Patch>) {
    if stripped.starts_with(UDIFF_HUNK_HEADER_PREFIX) {
        if let Some(hunk) = parse_udiff_hunk_header(stripped) {
            section.hunks.push(hunk);
        }
        return (super::ParserState::InUdiff, Vec::new());
    }

    if stripped.starts_with(UDIFF_BINARY_NOTICE_PREFIX) {
        return (super::ParserState::InUdiff, section.finalize());
    }

    if let Some(hunk) = section.hunks.last_mut() {
        if let Some(hunk_line) = classify_hunk_line(line, stripped) {
            hunk.lines.push(hunk_line);
            return (super::ParserState::InUdiff, Vec::new());
        }

        if stripped.starts_with(UDIFF_NO_NEWLINE_PREFIX) {
            return (super::ParserState::InUdiff, Vec::new());
        }
    } else if stripped.is_empty() {
        return (super::ParserState::InUdiff, Vec::new());
    }

    *previous_line = line.to_string();
    (super::ParserState::Idle, section.finalize())
}

pub fn parse_udiff_hunk_header(header: &str) -> Option<Hunk> {
    if !header.starts_with("@@") {
        return None;
    }

    let mut old_start = 0;

    for part in header.split_whitespace() {
        if part.starts_with('-') && part.len() > 1 {
            let num_part = &part[1..];

            let start_str = num_part.split(',').next().unwrap_or("");
            if let Ok(num) = start_str.parse::<usize>() {
                old_start = num;
                break;
            }
        }
    }

    Some(Hunk {
        old_start,
        old_len: 0,
        new_start: 0,
        new_len: 0,
        lines: Vec::new(),
    })
}

fn resolve_patches(source: PathBuf, target: PathBuf, hunks: Vec<Hunk>) -> Vec<Patch> {
    let dev_null = Path::new(DEV_NULL);
    let is_creation = source.as_path() == dev_null;
    let is_deletion = target.as_path() == dev_null;

    if is_creation && is_deletion {
        return Vec::new();
    }

    if is_deletion {
        return vec![Patch {
            file_path: source,
            op: PatchOp::Delete,
        }];
    }

    if is_creation {
        return vec![Patch {
            file_path: target,
            op: PatchOp::Udiff(hunks),
        }];
    }

    if source == target {
        if hunks.is_empty() {
            return Vec::new();
        }

        return vec![Patch {
            file_path: target,
            op: PatchOp::Udiff(hunks),
        }];
    }

    let mut patches = vec![Patch {
        file_path: source,
        op: PatchOp::Move(target.clone()),
    }];

    if !hunks.is_empty() {
        patches.push(Patch {
            file_path: target,
            op: PatchOp::Udiff(hunks),
        });
    }

    patches
}

fn parse_header_path(value: &str, style: PathStyle, side: HeaderSide) -> PathBuf {
    let name = value.split('\t').next().unwrap_or(value).trim();

    if name == DEV_NULL {
        return PathBuf::from(DEV_NULL);
    }

    let prefix = match (style, side) {
        (PathStyle::Verbatim, _) => None,
        (PathStyle::Git, HeaderSide::Old) => Some(GIT_OLD_PATH_PREFIX),
        (PathStyle::Git, HeaderSide::New) => Some(GIT_NEW_PATH_PREFIX),
    };

    PathBuf::from(
        prefix
            .and_then(|prefix| name.strip_prefix(prefix))
            .unwrap_or(name),
    )
}

fn classify_hunk_line(line: &str, stripped: &str) -> Option<HunkLine> {
    if line.starts_with('-') {
        Some(HunkLine::Remove(line.to_string()))
    } else if line.starts_with('+') {
        Some(HunkLine::Add(line.to_string()))
    } else if line.starts_with(' ') || stripped.is_empty() {
        Some(HunkLine::Context(line.to_string()))
    } else {
        None
    }
}

fn is_git_metadata_line(stripped: &str) -> bool {
    stripped.starts_with(UDIFF_BINARY_NOTICE_PREFIX)
        || GIT_METADATA_PREFIXES
            .iter()
            .any(|prefix| stripped.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_udiff_hunk_header_variations() {
        let zero_hunk = Hunk {
            old_start: 0,
            old_len: 0,
            new_start: 0,
            new_len: 0,
            lines: Vec::new(),
        };

        let hunk_standard = Hunk {
            old_start: 10,
            ..zero_hunk.clone()
        };
        assert_eq!(
            parse_udiff_hunk_header("@@ -10,5 +12,8 @@"),
            Some(hunk_standard)
        );

        assert_eq!(
            parse_udiff_hunk_header("@@ ... @@"),
            Some(zero_hunk.clone())
        );

        assert_eq!(parse_udiff_hunk_header("@@"), Some(zero_hunk.clone()));

        assert_eq!(
            parse_udiff_hunk_header("@@ nonsense @@"),
            Some(zero_hunk.clone())
        );

        assert_eq!(parse_udiff_hunk_header("no markers"), None);
    }

    #[test]
    fn test_parse_header_path_variations() {
        assert_eq!(
            parse_header_path("a/src/main.rs", PathStyle::Git, HeaderSide::Old),
            PathBuf::from("src/main.rs")
        );
        assert_eq!(
            parse_header_path("b/src/main.rs", PathStyle::Git, HeaderSide::New),
            PathBuf::from("src/main.rs")
        );

        assert_eq!(
            parse_header_path("a/a/main.rs", PathStyle::Git, HeaderSide::Old),
            PathBuf::from("a/main.rs")
        );

        assert_eq!(
            parse_header_path("b/src/main.rs", PathStyle::Git, HeaderSide::Old),
            PathBuf::from("b/src/main.rs")
        );

        assert_eq!(
            parse_header_path("a/src/main.rs", PathStyle::Verbatim, HeaderSide::Old),
            PathBuf::from("a/src/main.rs")
        );

        assert_eq!(
            parse_header_path(
                "src/main.rs\t2024-01-01 00:00:00.000000000 +0000",
                PathStyle::Verbatim,
                HeaderSide::Old
            ),
            PathBuf::from("src/main.rs")
        );

        assert_eq!(
            parse_header_path("/dev/null", PathStyle::Git, HeaderSide::Old),
            PathBuf::from("/dev/null")
        );
    }
}
