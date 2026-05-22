pub fn safe_freeze_boundaries(s: &str) -> Vec<usize> {
    let mut boundaries: Vec<usize> = Vec::new();
    let bytes = s.as_bytes();
    let mut in_fence = false;
    let mut line_start = 0usize;
    let mut i = 0usize;

    while i < bytes.len() {
        if bytes[i] == b'\n' {
            let line = &s[line_start..i];
            let trimmed = line.trim_start();
            if is_fence_marker(trimmed) {
                in_fence = !in_fence;
            }
            if !in_fence && i + 1 < bytes.len() && bytes[i + 1] == b'\n' {
                let pos = i + 2;
                if boundaries.last() != Some(&pos) {
                    boundaries.push(pos);
                }
            }
            line_start = i + 1;
        }
        i += 1;
    }

    boundaries
}

pub fn split_into_segments(s: &str) -> Vec<&str> {
    let boundaries = safe_freeze_boundaries(s);
    if boundaries.is_empty() {
        return vec![s];
    }
    let mut out: Vec<&str> = Vec::with_capacity(boundaries.len() + 1);
    let mut prev = 0usize;
    for &b in &boundaries {
        out.push(&s[prev..b]);
        prev = b;
    }
    out.push(&s[prev..]);
    out
}

fn is_fence_marker(line: &str) -> bool {
    is_marker_run(line, '`') || is_marker_run(line, '~')
}

fn is_marker_run(line: &str, ch: char) -> bool {
    let count = line.chars().take_while(|c| *c == ch).count();
    count >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_boundaries_in_single_paragraph() {
        assert_eq!(safe_freeze_boundaries("hello world"), Vec::<usize>::new());
        assert_eq!(split_into_segments("hello world"), vec!["hello world"]);
    }

    #[test]
    fn single_blank_line_creates_one_boundary() {
        let s = "first paragraph\n\nsecond";
        let bs = safe_freeze_boundaries(s);
        assert_eq!(bs, vec![17]);
        let segs = split_into_segments(s);
        assert_eq!(segs, vec!["first paragraph\n\n", "second"]);
    }

    #[test]
    fn multiple_paragraphs_segment_at_each_blank_line() {
        let s = "a\n\nb\n\nc";
        let bs = safe_freeze_boundaries(s);
        assert_eq!(bs, vec![3, 6]);
        let segs = split_into_segments(s);
        assert_eq!(segs, vec!["a\n\n", "b\n\n", "c"]);
    }

    #[test]
    fn blank_line_inside_code_fence_does_not_segment() {
        let s = "before\n\n```rust\nfn x() {\n\n}\n```\n\nafter";
        let bs = safe_freeze_boundaries(s);
        assert_eq!(
            bs,
            vec![8, 33],
            "boundaries: before fence (after `before\\n\\n`) and after fence (after `````\\n\\n`)",
        );
    }

    #[test]
    fn unterminated_code_fence_blocks_all_subsequent_boundaries() {
        let s = "intro\n\n```rust\nfn x() {\n\nbody\n\nmore";
        let bs = safe_freeze_boundaries(s);
        assert_eq!(
            bs,
            vec![7],
            "only the boundary before the open fence survives"
        );
    }

    #[test]
    fn tilde_fences_are_recognized() {
        let s = "a\n\n~~~\ncode\n\n~~~\n\nb";
        let bs = safe_freeze_boundaries(s);
        assert_eq!(bs, vec![3, 18]);
    }

    #[test]
    fn segmentation_preserves_total_bytes() {
        let s = "abc\n\ndef\n\nghi";
        let segs = split_into_segments(s);
        let joined: String = segs.concat();
        assert_eq!(joined, s);
    }

    #[test]
    fn segmentation_preserves_bytes_with_unterminated_fence() {
        let s = "intro\n\n```rust\nfn body() {\n\nlive tail";
        let segs = split_into_segments(s);
        assert_eq!(segs.concat(), s);
    }

    #[test]
    fn consecutive_blank_lines_produce_stable_boundaries() {
        let s = "a\n\n\nb";
        let bs = safe_freeze_boundaries(s);
        assert!(
            bs.iter().all(|&p| p > 1 && p <= s.len()),
            "boundaries must point within the string: {bs:?}",
        );
        let s2 = "a\n\n\nb-extra";
        let bs2 = safe_freeze_boundaries(s2);
        assert_eq!(
            &bs2[..bs.len()],
            bs.as_slice(),
            "previously discovered boundaries must not shift as text grows",
        );
    }

    #[test]
    fn last_segment_is_the_live_tail() {
        let s = "frozen1\n\nfrozen2\n\nlive";
        let segs = split_into_segments(s);
        assert_eq!(segs.last(), Some(&"live"));
    }

    #[test]
    fn empty_string_yields_one_empty_segment() {
        assert_eq!(split_into_segments(""), vec![""]);
    }

    #[test]
    fn streaming_append_keeps_frozen_prefix_bytes_stable() {
        let prefix = "para one\n\npara two\n\n";
        for delta in ["l", "li", "liv", "live", "live ", "live ta", "live tail"] {
            let full = format!("{prefix}{delta}");
            let segs = split_into_segments(&full);
            assert_eq!(segs[0], "para one\n\n");
            assert_eq!(segs[1], "para two\n\n");
            assert_eq!(segs[2], delta);
        }
    }
}
