pub fn split_lines(content: &str) -> Vec<String> {
    content
        .split_inclusive('\n')
        .map(|line| line.to_string())
        .collect()
}

pub fn find_occurrences(
    source_lines: &[String],
    search_block_str: &str,
    line_hint: Option<usize>,
) -> (Vec<usize>, usize) {
    let src_strict: Vec<String> = source_lines
        .iter()
        .map(|s| s.trim_end().to_string())
        .collect();

    let search_lines_strict: Vec<String> = search_block_str
        .lines()
        .map(|s| s.trim_end().to_string())
        .collect();

    let (mut matches, len) = if let Some(res) = find_in_tier(&src_strict, &search_lines_strict) {
        res
    } else {
        let search_block_trimmed = search_block_str.trim_matches(|c| c == '\n' || c == '\r');
        let search_lines_trimmed: Vec<String> = search_block_trimmed
            .lines()
            .map(|s| s.trim_end().to_string())
            .collect();

        if !search_lines_trimmed.is_empty() {
            if let Some(res) = find_in_tier(&src_strict, &search_lines_trimmed) {
                res
            } else {
                let src_loose: Vec<String> =
                    source_lines.iter().map(|s| s.trim().to_string()).collect();
                let search_lines_loose: Vec<String> = search_block_trimmed
                    .lines()
                    .map(|s| s.trim().to_string())
                    .collect();

                if search_lines_loose.is_empty() {
                    (vec![], 0)
                } else {
                    let m = find_sublist(&src_loose, &search_lines_loose);
                    (m, search_lines_loose.len())
                }
            }
        } else {
            (vec![], 0)
        }
    };

    if matches.len() > 1 {
        if let Some(hint) = line_hint {
            let target = if hint > 0 { hint - 1 } else { 0 };

            if let Some(&best_match) = matches
                .iter()
                .min_by_key(|&&idx| (idx as isize - target as isize).abs())
            {
                matches = vec![best_match];
            }
        }
    }

    (matches, len)
}

fn find_in_tier(src: &[String], search: &[String]) -> Option<(Vec<usize>, usize)> {
    let matches = find_sublist(src, search);
    if !matches.is_empty() {
        Some((matches, search.len()))
    } else {
        None
    }
}

fn find_sublist<T: PartialEq>(full_list: &[T], sub_list: &[T]) -> Vec<usize> {
    let mut matches = Vec::new();
    let n = full_list.len();
    let m = sub_list.len();

    if m == 0 {
        return matches;
    }

    for i in 0..=n.saturating_sub(m) {
        if &full_list[i..i + m] == sub_list {
            matches.push(i);
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_occurrences_strategies() {
        let src = vec!["a\n".to_string(), "  b\n".to_string(), "c\n".to_string()];

        let (idxs, len) = find_occurrences(&src, "  b", None);
        assert_eq!(idxs, vec![1]);
        assert_eq!(len, 1);

        let (idxs, len) = find_occurrences(&src, "\n  b\n", None);
        assert_eq!(idxs, vec![1]);
        assert_eq!(len, 1);

        let src_indented = vec![
            "    x\n".to_string(),
            "    y\n".to_string(),
            "    z\n".to_string(),
        ];
        let block_flat = "x\ny\nz";
        let (idxs, len) = find_occurrences(&src_indented, block_flat, None);
        assert_eq!(idxs, vec![0]);
        assert_eq!(len, 3);
    }

    #[test]
    fn test_find_occurrences_disambiguation() {
        let src = vec![
            "fn foo() {}\n".to_string(),
            "\n".to_string(),
            "fn foo() {}\n".to_string(),
            "\n".to_string(),
            "fn foo() {}\n".to_string(),
        ];
        let block = "fn foo() {}";

        let (idxs, _) = find_occurrences(&src, block, None);
        assert_eq!(idxs, vec![0, 2, 4]);

        let (idxs, _) = find_occurrences(&src, block, Some(1));
        assert_eq!(idxs, vec![0]);

        let (idxs, _) = find_occurrences(&src, block, Some(5));
        assert_eq!(idxs, vec![4]);

        let (idxs, _) = find_occurrences(&src, block, Some(3));
        assert_eq!(idxs, vec![2]);
    }
}
