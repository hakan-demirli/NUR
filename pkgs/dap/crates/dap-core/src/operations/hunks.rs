use crate::matcher::{find_occurrences, split_lines};
use crate::types::{Hunk, HunkLine};

#[derive(Debug, Clone, PartialEq)]
pub struct HunkFailure {
    pub index: usize,
    pub matches: usize,
    pub search_block: String,
}

pub fn apply_hunks(content: &str, hunks: &[Hunk]) -> Result<String, HunkFailure> {
    let mut current = content.to_string();
    let mut line_offset: isize = 0;

    for (index, hunk) in hunks.iter().enumerate() {
        let (search_block, replace_block) = split_hunk_blocks(hunk);

        if search_block.is_empty() && current.is_empty() {
            current = replace_block;
            continue;
        }

        let source_lines = split_lines(&current);

        let hint = if hunk.old_start > 0 {
            Some((hunk.old_start as isize + line_offset).max(0) as usize)
        } else {
            None
        };

        let (matches, match_len) = find_occurrences(&source_lines, &search_block, hint);

        if matches.len() != 1 {
            return Err(HunkFailure {
                index,
                matches: matches.len(),
                search_block,
            });
        }

        let replace_lines = split_lines(&replace_block);
        line_offset += replace_lines.len() as isize - match_len as isize;

        let start_idx = matches[0];
        let end_idx = start_idx + match_len;

        let mut new_lines = source_lines;
        new_lines.splice(start_idx..end_idx, replace_lines);
        current = new_lines.concat();
    }

    Ok(current)
}

fn split_hunk_blocks(hunk: &Hunk) -> (String, String) {
    let mut search_lines = Vec::new();
    let mut replace_lines = Vec::new();

    for line in &hunk.lines {
        let content = hunk_line_content(line);

        match line {
            HunkLine::Context(_) => {
                search_lines.push(content.clone());
                replace_lines.push(content);
            }
            HunkLine::Remove(_) => search_lines.push(content),
            HunkLine::Add(_) => replace_lines.push(content),
        }
    }

    (search_lines.concat(), replace_lines.concat())
}

fn hunk_line_content(line: &HunkLine) -> String {
    let raw = match line {
        HunkLine::Context(s) | HunkLine::Remove(s) | HunkLine::Add(s) => s,
    };

    match raw.chars().next() {
        Some(marker) if raw.len() > marker.len_utf8() => raw[marker.len_utf8()..].to_string(),
        _ => "\n".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(old_start: usize, lines: Vec<HunkLine>) -> Hunk {
        Hunk {
            old_start,
            old_len: 0,
            new_start: 0,
            new_len: 0,
            lines,
        }
    }

    #[test]
    fn test_apply_hunks_creates_content_from_empty() {
        let hunks = vec![hunk(
            0,
            vec![
                HunkLine::Add("+first\n".to_string()),
                HunkLine::Add("+second\n".to_string()),
            ],
        )];

        assert_eq!(apply_hunks("", &hunks), Ok("first\nsecond\n".to_string()));
    }

    #[test]
    fn test_apply_hunks_reports_failing_hunk_index() {
        let hunks = vec![
            hunk(1, vec![HunkLine::Context(" keep\n".to_string())]),
            hunk(2, vec![HunkLine::Remove("-missing\n".to_string())]),
        ];

        let failure = apply_hunks("keep\n", &hunks).unwrap_err();
        assert_eq!(failure.index, 1);
        assert_eq!(failure.matches, 0);
        assert_eq!(failure.search_block, "missing\n");
    }

    #[test]
    fn test_hunk_line_content_handles_multi_byte_markers() {
        assert_eq!(
            hunk_line_content(&HunkLine::Context("\u{00A0}\n".to_string())),
            "\n"
        );
        assert_eq!(
            hunk_line_content(&HunkLine::Context("\n".to_string())),
            "\n"
        );
        assert_eq!(
            hunk_line_content(&HunkLine::Add("+value\n".to_string())),
            "value\n"
        );
    }
}
