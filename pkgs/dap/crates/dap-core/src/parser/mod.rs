pub mod command_parser;
pub mod diff_parser;
pub mod udiff_parser;

use crate::types::Patch;
use std::path::PathBuf;
use udiff_parser::{PathStyle, UdiffSection};

pub fn parse(content: &str) -> Vec<Patch> {
    let mut patches = Vec::new();
    let mut state = ParserState::Idle;
    let mut previous_line = String::new();
    let mut file_path = PathBuf::new();
    let mut search_lines = Vec::new();
    let mut replace_lines = Vec::new();
    let mut section = UdiffSection::default();

    for line in content.split_inclusive('\n') {
        let stripped = line.trim();

        match state {
            ParserState::InSearch => {
                if stripped == diff_parser::MARKER_DIVIDER {
                    state = ParserState::InReplace;
                } else {
                    search_lines.push(line);
                }
            }
            ParserState::InReplace => {
                if stripped == diff_parser::MARKER_REPLACE_END {
                    patches.push(Patch {
                        file_path: file_path.clone(),
                        op: crate::types::PatchOp::Modify {
                            search: search_lines.concat(),
                            replace: replace_lines.concat(),
                        },
                    });
                    state = ParserState::Idle;
                    previous_line.clear();
                } else {
                    replace_lines.push(line);
                }
            }
            ParserState::Idle | ParserState::InGitHeader | ParserState::InUdiff => {
                if line.starts_with(udiff_parser::UDIFF_GIT_HEADER_PREFIX) {
                    patches.extend(section.finalize());
                    section.set_style(PathStyle::Git);
                    state = ParserState::InGitHeader;
                    previous_line.clear();
                } else if let Some(value) =
                    stripped.strip_prefix(udiff_parser::UDIFF_OLD_FILE_PREFIX)
                {
                    if section.has_file_paths() {
                        patches.extend(section.finalize());
                    }
                    section.set_old_path(value);
                    state = ParserState::InUdiff;
                    previous_line.clear();
                } else if let Some(value) =
                    stripped.strip_prefix(udiff_parser::UDIFF_NEW_FILE_PREFIX)
                {
                    section.set_new_path(value);
                    state = ParserState::InUdiff;
                } else {
                    if state != ParserState::Idle {
                        let (new_state, new_patches) = if state == ParserState::InGitHeader {
                            udiff_parser::handle_git_header_line(
                                line,
                                stripped,
                                &mut section,
                                &mut previous_line,
                            )
                        } else {
                            udiff_parser::handle_udiff_line(
                                line,
                                stripped,
                                &mut section,
                                &mut previous_line,
                            )
                        };

                        state = new_state;
                        patches.extend(new_patches);
                    }

                    if state == ParserState::Idle {
                        if stripped == diff_parser::MARKER_SEARCH_START {
                            let potential_path = previous_line.trim();
                            if !potential_path.is_empty() {
                                file_path = PathBuf::from(potential_path);
                            }
                            state = ParserState::InSearch;
                            search_lines.clear();
                            replace_lines.clear();
                        } else if let Some(patch) = command_parser::parse_line_command(line) {
                            patches.push(patch);
                            previous_line.clear();
                        } else if stripped.starts_with("```") {
                        } else if stripped.is_empty() {
                            previous_line.clear();
                        } else {
                            previous_line = line.to_string();
                        }
                    }
                }
            }
        }
    }

    patches.extend(section.finalize());

    patches
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParserState {
    Idle,
    InSearch,
    InReplace,
    InGitHeader,
    InUdiff,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::command_parser::*;
    use crate::parser::diff_parser::*;
    use std::path::PathBuf;

    #[test]
    fn test_parse_diff_fenced() {
        let patch_text = format!(
            "src/main.rs\n{}\nold\n{}\nnew\n{}\n",
            MARKER_SEARCH_START, MARKER_DIVIDER, MARKER_REPLACE_END
        );
        let patches = parse(&patch_text);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path, PathBuf::from("src/main.rs"));
        if let crate::types::PatchOp::Modify { search, .. } = &patches[0].op {
            assert!(search.contains("old"));
        } else {
            panic!("Wrong op");
        }
    }

    #[test]
    fn test_parse_marker_edge_cases() {
        let indented = format!(
            "\n    file1.rs\n      {}\n    old\n    {}\n    new\n    {}\n    ",
            MARKER_SEARCH_START, MARKER_DIVIDER, MARKER_REPLACE_END
        );
        let patches = parse(&indented);
        assert_eq!(patches.len(), 1, "Should parse indented start markers");
        assert_eq!(patches[0].file_path, PathBuf::from("file1.rs"));

        let polluted = format!(
            "\n    file2.rs\n    some_code {}\n    old\n    {}\n    new\n    {}\n    ",
            MARKER_SEARCH_START, MARKER_DIVIDER, MARKER_REPLACE_END
        );
        let patches_bad = parse(&polluted);
        assert_eq!(
            patches_bad.len(),
            0,
            "Should ignore markers preceded by text"
        );
    }

    #[test]
    fn test_parse_move_delete() {
        let content = format!(
            "file_to_delete.rs {}\nsrc/old.rs {} src/new.rs",
            MARKER_DELETE, MARKER_MOVE
        );
        let patches = parse(&content);
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].op, crate::types::PatchOp::Delete);
        if let crate::types::PatchOp::Move(dest) = &patches[1].op {
            assert_eq!(dest, &PathBuf::from("src/new.rs"));
        } else {
            panic!("Expected Move op");
        }
    }

    #[test]
    fn test_parse_mixed_formats() {
        let mixed_content = format!(
            r#"--- file1.py
+++ file1.py
@@ -1,1 +1,2 @@
 print("hello")
+print("world")

file2.rs
{}
old code
{}
new code
{}

file3.txt {}
"#,
            MARKER_SEARCH_START, MARKER_DIVIDER, MARKER_REPLACE_END, MARKER_DELETE
        );

        let patches = parse(&mixed_content);
        assert_eq!(patches.len(), 3);

        match &patches[0].op {
            crate::types::PatchOp::Udiff(_) => {}
            _ => panic!("Expected Udiff op"),
        }

        match &patches[1].op {
            crate::types::PatchOp::Modify { .. } => {}
            _ => panic!("Expected Modify op"),
        }

        match &patches[2].op {
            crate::types::PatchOp::Delete => {}
            _ => panic!("Expected Delete op"),
        }
    }

    #[test]
    fn test_parse_git_diff_modify_and_create() {
        let content = r#"diff --git a/config/aliases.sh b/config/aliases.sh
index b114f32..15d52fb 100644
--- a/config/aliases.sh
+++ b/config/aliases.sh
@@ -39,6 +39,7 @@ alias gc='git commit'
 alias gcm='git commit -m'
+alias gsu='gsu.sh'
 
diff --git a/bin/gsu.sh b/bin/gsu.sh
new file mode 100755
index 0000000..325dc66
--- /dev/null
+++ b/bin/gsu.sh
@@ -0,0 +1,2 @@
+#!/usr/bin/env bash
+set -euo pipefail
"#;

        let patches = parse(content);
        assert_eq!(patches.len(), 2);

        assert_eq!(patches[0].file_path, PathBuf::from("config/aliases.sh"));
        match &patches[0].op {
            crate::types::PatchOp::Udiff(hunks) => assert_eq!(hunks[0].old_start, 39),
            _ => panic!("Expected Udiff op"),
        }

        assert_eq!(patches[1].file_path, PathBuf::from("bin/gsu.sh"));
        match &patches[1].op {
            crate::types::PatchOp::Udiff(hunks) => assert_eq!(hunks[0].old_start, 0),
            _ => panic!("Expected Udiff op"),
        }
    }

    #[test]
    fn test_parse_git_diff_delete() {
        let content = r#"diff --git a/src/old.rs b/src/old.rs
deleted file mode 100644
index 325dc66..0000000
--- a/src/old.rs
+++ /dev/null
@@ -1,1 +0,0 @@
-fn main() {}
"#;

        let patches = parse(content);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path, PathBuf::from("src/old.rs"));
        assert_eq!(patches[0].op, crate::types::PatchOp::Delete);
    }

    #[test]
    fn test_parse_git_diff_pure_rename() {
        let content = r#"diff --git a/src/old.rs b/src/new.rs
similarity index 100%
rename from src/old.rs
rename to src/new.rs
"#;

        let patches = parse(content);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path, PathBuf::from("src/old.rs"));
        assert_eq!(
            patches[0].op,
            crate::types::PatchOp::Move(PathBuf::from("src/new.rs"))
        );
    }

    #[test]
    fn test_parse_git_diff_rename_with_changes() {
        let content = r#"diff --git a/src/old.rs b/src/new.rs
similarity index 87%
rename from src/old.rs
rename to src/new.rs
index b114f32..15d52fb 100644
--- a/src/old.rs
+++ b/src/new.rs
@@ -1,1 +1,1 @@
-fn main() {}
+fn main() { run() }
"#;

        let patches = parse(content);
        assert_eq!(patches.len(), 2);
        assert_eq!(patches[0].file_path, PathBuf::from("src/old.rs"));
        assert_eq!(
            patches[0].op,
            crate::types::PatchOp::Move(PathBuf::from("src/new.rs"))
        );

        assert_eq!(patches[1].file_path, PathBuf::from("src/new.rs"));
        match &patches[1].op {
            crate::types::PatchOp::Udiff(_) => {}
            _ => panic!("Expected Udiff op"),
        }
    }

    #[test]
    fn test_parse_git_diff_keeps_literal_prefix_directories() {
        let content = r#"diff --git a/a/main.rs b/a/main.rs
index b114f32..15d52fb 100644
--- a/a/main.rs
+++ b/a/main.rs
@@ -1,1 +1,1 @@
-fn main() {}
+fn main() { run() }
"#;

        let patches = parse(content);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path, PathBuf::from("a/main.rs"));
    }

    #[test]
    fn test_parse_udiff_without_git_header_keeps_paths_verbatim() {
        let content = r#"--- a/main.rs
+++ a/main.rs
@@ -1,1 +1,1 @@
-fn main() {}
+fn main() { run() }
"#;

        let patches = parse(content);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path, PathBuf::from("a/main.rs"));
        match &patches[0].op {
            crate::types::PatchOp::Udiff(_) => {}
            _ => panic!("Expected Udiff op"),
        }
    }

    #[test]
    fn test_parse_udiff_header_timestamps() {
        let content = "--- main.rs\t2024-01-01 00:00:00.000000000 +0000\n\
                       +++ main.rs\t2024-01-02 00:00:00.000000000 +0000\n\
                       @@ -1,1 +1,1 @@\n\
                       -fn main() {}\n\
                       +fn main() { run() }\n";

        let patches = parse(content);
        assert_eq!(patches.len(), 1);
        assert_eq!(patches[0].file_path, PathBuf::from("main.rs"));
    }

    #[test]
    fn test_parse_git_diff_mode_change_only() {
        let content = r#"diff --git a/bin/run.sh b/bin/run.sh
old mode 100644
new mode 100755
"#;

        assert!(parse(content).is_empty());
    }
}
