use crossterm::event::{KeyCode, KeyModifiers};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::completion::CompletionManager;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrapResult {
    pub rows: Vec<String>,
    pub row_byte_starts: Vec<usize>,
    pub cursor: Option<(usize, usize)>,
}

pub fn wrap_for_display(input: &str, cursor: usize, width: usize) -> WrapResult {
    let width = width.max(1);
    let cursor = cursor.min(input.len());

    let mut rows: Vec<String> = Vec::new();
    let mut row_byte_starts: Vec<usize> = Vec::new();
    let mut cursor_pos: Option<(usize, usize)> = None;

    let mut cur = String::new();
    let mut cur_width: usize = 0;
    let mut cur_start: usize = 0;

    let bytes_total = input.len();

    for (byte_idx, ch) in input.char_indices() {
        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
        let will_wrap = ch != '\n' && ch_w > 0 && cur_width + ch_w > width && cur_width > 0;

        if cursor_pos.is_none() && cursor == byte_idx {
            cursor_pos = if will_wrap {
                Some((0, rows.len() + 1))
            } else {
                Some((cur_width, rows.len()))
            };
        }

        if ch == '\n' {
            rows.push(std::mem::take(&mut cur));
            row_byte_starts.push(cur_start);
            cur_width = 0;
            cur_start = byte_idx + 1;
            continue;
        }

        if will_wrap {
            rows.push(std::mem::take(&mut cur));
            row_byte_starts.push(cur_start);
            cur_width = 0;
            cur_start = byte_idx;
        }

        cur.push(ch);
        cur_width += ch_w;
    }

    let ends_with_newline = input.as_bytes().last() == Some(&b'\n');
    if !cur.is_empty() || rows.is_empty() || ends_with_newline {
        rows.push(std::mem::take(&mut cur));
        row_byte_starts.push(cur_start);
    }

    if cursor_pos.is_none() && cursor == bytes_total {
        let last = rows.len() - 1;
        cursor_pos = Some((UnicodeWidthStr::width(rows[last].as_str()), last));
    }

    WrapResult {
        rows,
        row_byte_starts,
        cursor: cursor_pos,
    }
}

fn land_on_row(
    rows: &[String],
    row_byte_starts: &[usize],
    target_row: usize,
    desired_col: usize,
) -> usize {
    let target_line = &rows[target_row];
    let target_start = row_byte_starts[target_row];
    let target_bytes = target_line.len();

    let mut byte_off = 0usize;
    let mut col = 0usize;
    for ch in target_line.chars() {
        let ch_w = UnicodeWidthChar::width(ch).unwrap_or(0);
        if col + ch_w > desired_col {
            break;
        }
        col += ch_w;
        byte_off += ch.len_utf8();
    }

    if byte_off == target_bytes && byte_off > 0 {
        if let Some(&next_start) = row_byte_starts.get(target_row + 1) {
            if next_start == target_start + target_bytes {
                if let Some(last_ch) = target_line.chars().last() {
                    byte_off -= last_ch.len_utf8();
                }
            }
        }
    }

    target_start + byte_off
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PromptPartKind {
    Text(String),
    File {
        mime: String,
        filename: String,
        filepath: String,
        base64: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromptPart {
    pub kind: PromptPartKind,
    pub source_start: usize,
    pub source_end: usize,
    pub placeholder: String,
}

impl PromptPart {
    pub fn len(&self) -> usize {
        self.source_end - self.source_start
    }
    pub fn is_empty(&self) -> bool {
        self.source_end == self.source_start
    }
}

pub struct InputState {
    pub input: String,
    pub cursor_position: usize,
    pub cursor_visible: bool,

    pub completion: CompletionManager,

    pub input_history: Vec<(String, Vec<PromptPart>)>,
    pub history_index: usize,
    pub saved_input: String,
    pub saved_parts: Vec<PromptPart>,

    pub parts: Vec<PromptPart>,
}

impl InputState {
    pub fn new() -> Self {
        Self {
            input: String::new(),
            cursor_position: 0,
            cursor_visible: true,
            completion: CompletionManager::new(),
            input_history: Vec::new(),
            history_index: 0,
            saved_input: String::new(),
            saved_parts: Vec::new(),
            parts: Vec::new(),
        }
    }

    pub fn has_text_parts(&self) -> bool {
        self.parts
            .iter()
            .any(|p| matches!(p.kind, PromptPartKind::Text(_)))
    }

    fn drop_parts_overlapping(&mut self, start: usize, end: usize) {
        if self.parts.is_empty() || start >= end {
            return;
        }
        self.parts
            .retain(|p| p.source_end <= start || p.source_start >= end);
    }

    fn shift_parts_at(&mut self, from: usize, delta: isize) {
        if delta == 0 || self.parts.is_empty() {
            return;
        }
        for p in self.parts.iter_mut() {
            if p.source_start >= from {
                if delta < 0 {
                    let d = (-delta) as usize;
                    p.source_start = p.source_start.saturating_sub(d);
                    p.source_end = p.source_end.saturating_sub(d);
                } else {
                    p.source_start += delta as usize;
                    p.source_end += delta as usize;
                }
            }
        }
    }

    pub fn insert_prompt_part(&mut self, kind: PromptPartKind, placeholder: String) {
        let cursor = self.cursor_position.min(self.input.len());
        let cursor = if self.input.is_char_boundary(cursor) {
            cursor
        } else {
            let mut p = cursor;
            while p > 0 && !self.input.is_char_boundary(p) {
                p -= 1;
            }
            p
        };
        let text_to_insert = format!("{placeholder} ");
        let inserted_len = text_to_insert.len();
        self.shift_parts_at(cursor, inserted_len as isize);
        self.input.insert_str(cursor, &text_to_insert);
        let part = PromptPart {
            kind,
            source_start: cursor,
            source_end: cursor + placeholder.len(),
            placeholder,
        };
        self.parts.push(part);
        self.parts.sort_by_key(|p| p.source_start);
        self.cursor_position = cursor + inserted_len;
        self.completion.update(&self.input);
    }

    pub fn expand_for_submit(&self) -> String {
        if self.parts.is_empty() {
            return self.input.clone();
        }
        let mut parts_sorted: Vec<&PromptPart> = self.parts.iter().collect();
        parts_sorted.sort_by_key(|p| p.source_start);
        let mut out = String::with_capacity(self.input.len());
        let mut last = 0usize;
        for p in parts_sorted {
            if p.source_start < last {
                continue;
            }
            out.push_str(&self.input[last..p.source_start]);
            match &p.kind {
                PromptPartKind::Text(real) => out.push_str(real),
                PromptPartKind::File { .. } => out.push_str(&p.placeholder),
            }
            last = p.source_end;
        }
        if last < self.input.len() {
            out.push_str(&self.input[last..]);
        }
        out
    }

    pub fn take_file_parts(&mut self) -> Vec<PromptPart> {
        let mut out = Vec::new();
        let mut keep = Vec::new();
        for p in self.parts.drain(..) {
            match &p.kind {
                PromptPartKind::File { .. } => out.push(p),
                _ => keep.push(p),
            }
        }
        self.parts = keep;
        out
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor_position == 0 {
            return;
        }
        let mut p = self.cursor_position - 1;
        while p > 0 && !self.input.is_char_boundary(p) {
            p -= 1;
        }
        self.cursor_position = p;
    }

    pub fn move_cursor_right(&mut self) {
        if self.cursor_position >= self.input.len() {
            return;
        }
        let mut p = self.cursor_position + 1;
        while p < self.input.len() && !self.input.is_char_boundary(p) {
            p += 1;
        }
        self.cursor_position = p;
    }

    pub fn move_cursor_home(&mut self) {
        let before = &self.input[..self.cursor_position];
        self.cursor_position = before.rfind('\n').map(|i| i + 1).unwrap_or(0);
    }

    pub fn move_cursor_end(&mut self) {
        if let Some(rel) = self.input[self.cursor_position..].find('\n') {
            self.cursor_position += rel;
        } else {
            self.cursor_position = self.input.len();
        }
    }

    pub fn move_cursor_word_left(&mut self) {
        if self.cursor_position == 0 {
            return;
        }
        let bytes = self.input.as_bytes();
        let mut p = self.cursor_position;
        while p > 0 && bytes[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        while p > 0 && !bytes[p - 1].is_ascii_whitespace() {
            p -= 1;
        }
        self.cursor_position = p;
    }

    pub fn move_cursor_word_right(&mut self) {
        let bytes = self.input.as_bytes();
        let len = bytes.len();
        if self.cursor_position >= len {
            return;
        }
        let mut p = self.cursor_position;
        while p < len && !bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        while p < len && bytes[p].is_ascii_whitespace() {
            p += 1;
        }
        self.cursor_position = p;
    }

    pub fn insert_char(&mut self, c: char) {
        let at = self.cursor_position;
        self.drop_parts_overlapping(at, at + 1);
        let n = c.len_utf8();
        self.shift_parts_at(at, n as isize);
        self.input.insert(at, c);
        self.cursor_position += n;
        self.completion.update(&self.input);
    }

    pub fn paste_text(&mut self, text: &str) {
        for ch in text.chars() {
            self.insert_char(ch);
        }
    }

    pub fn insert_newline(&mut self) {
        let at = self.cursor_position;
        self.drop_parts_overlapping(at, at + 1);
        self.shift_parts_at(at, 1);
        self.input.insert(at, '\n');
        self.cursor_position += 1;
    }

    pub fn backspace(&mut self) {
        if self.cursor_position == 0 {
            return;
        }
        let prev = self.cursor_position;
        self.move_cursor_left();
        if self.cursor_position < self.input.len() {
            let removed = self.input.remove(self.cursor_position);
            self.drop_parts_overlapping(self.cursor_position, prev);
            self.shift_parts_at(self.cursor_position, -(removed.len_utf8() as isize));
        }
        self.completion.update(&self.input);
    }

    pub fn delete_forward(&mut self) {
        if self.cursor_position < self.input.len() {
            let at = self.cursor_position;
            let removed = self.input.remove(at);
            self.drop_parts_overlapping(at, at + removed.len_utf8());
            self.shift_parts_at(at, -(removed.len_utf8() as isize));
            self.completion.update(&self.input);
        }
    }

    pub fn delete_word_back(&mut self) {
        if self.cursor_position == 0 {
            return;
        }
        let bytes = self.input.as_bytes();
        let mut end = self.cursor_position;
        while end > 0 && bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        while end > 0 && !bytes[end - 1].is_ascii_whitespace() {
            end -= 1;
        }
        let start = end;
        let cursor = self.cursor_position;
        self.input.drain(start..cursor);
        let removed = cursor - start;
        self.drop_parts_overlapping(start, cursor);
        self.shift_parts_at(cursor, -(removed as isize));
        self.cursor_position = start;
        self.completion.update(&self.input);
    }

    pub fn kill_to_end(&mut self) {
        if self.cursor_position < self.input.len() {
            let cut = self.cursor_position;
            let len = self.input.len();
            self.input.truncate(cut);
            self.drop_parts_overlapping(cut, len);
            self.completion.update(&self.input);
        }
    }

    pub fn clear(&mut self) {
        if !self.input.is_empty() {
            self.input.clear();
            self.cursor_position = 0;
            self.parts.clear();
            self.completion.update(&self.input);
        }
    }

    fn visual_layout(&self, width: usize) -> (Vec<String>, Vec<usize>, usize, usize) {
        let WrapResult {
            rows,
            row_byte_starts,
            cursor,
        } = wrap_for_display(&self.input, self.cursor_position, width);
        let (col, row) = cursor.unwrap_or((0, 0));
        (rows, row_byte_starts, row, col)
    }

    pub fn cursor_visual_row(&self, width: usize) -> usize {
        let (_, _, row, _) = self.visual_layout(width);
        row
    }

    pub fn total_visual_rows(&self, width: usize) -> usize {
        let (rows, _, _, _) = self.visual_layout(width);
        rows.len()
    }

    pub fn move_cursor_up(&mut self, width: usize) -> bool {
        let (rows, row_byte_starts, cur_row, cur_col) = self.visual_layout(width);
        if cur_row == 0 {
            return false;
        }
        let target_row = cur_row - 1;
        self.cursor_position = land_on_row(&rows, &row_byte_starts, target_row, cur_col);
        true
    }

    pub fn move_cursor_down(&mut self, width: usize) -> bool {
        let (rows, row_byte_starts, cur_row, cur_col) = self.visual_layout(width);
        if cur_row >= rows.len().saturating_sub(1) {
            return false;
        }
        let target_row = cur_row + 1;
        self.cursor_position = land_on_row(&rows, &row_byte_starts, target_row, cur_col);
        true
    }

    pub fn history_prev(&mut self) {
        if self.history_index == 0 {
            return;
        }
        if self.history_index == self.input_history.len() {
            self.saved_input = self.input.clone();
            self.saved_parts = self.parts.clone();
        }
        self.history_index -= 1;
        let (text, parts) = &self.input_history[self.history_index];
        self.input = text.clone();
        self.parts = parts.clone();
        self.cursor_position = self.input.len();
    }

    pub fn history_next(&mut self) {
        if self.history_index >= self.input_history.len() {
            return;
        }
        self.history_index += 1;
        if self.history_index == self.input_history.len() {
            self.input = self.saved_input.clone();
            self.parts = self.saved_parts.clone();
        } else {
            let (text, parts) = &self.input_history[self.history_index];
            self.input = text.clone();
            self.parts = parts.clone();
        }
        self.cursor_position = self.input.len();
    }

    pub fn push_history(&mut self, raw: &str, parts: Vec<PromptPart>) {
        if self.input_history.last().map(|(s, _)| s.as_str()) != Some(raw) {
            self.input_history.push((raw.to_string(), parts));
        }
        self.history_index = self.input_history.len();
        self.saved_input.clear();
        self.saved_parts.clear();
    }

    pub fn handle_completion_key(
        &mut self,
        code: KeyCode,
        mods: KeyModifiers,
    ) -> CompletionOutcome {
        match code {
            KeyCode::Up | KeyCode::BackTab => {
                self.completion.previous();
                self.apply_completion();
                CompletionOutcome::Consumed
            }
            KeyCode::Down | KeyCode::Tab => {
                self.completion.next();
                self.apply_completion();
                CompletionOutcome::Consumed
            }
            KeyCode::Enter => {
                if mods.contains(KeyModifiers::ALT) || mods.contains(KeyModifiers::SHIFT) {
                    return CompletionOutcome::NotConsumed;
                }
                let current = self.input.clone();
                let replacement = self.completion.confirm(&current).or_else(|| {
                    if self.completion.input_matches_top(&current) {
                        None
                    } else {
                        self.completion.top_replacement(&current)
                    }
                });
                if let Some(text) = replacement {
                    self.input = text;
                    self.cursor_position = self.input.len();
                }
                self.completion.active = false;
                CompletionOutcome::SubmitNow
            }
            KeyCode::Esc => {
                self.completion.active = false;
                CompletionOutcome::Consumed
            }
            _ => CompletionOutcome::NotConsumed,
        }
    }

    fn apply_completion(&mut self) {
        let current = self.input.clone();
        if let Some(replacement) = self.completion.confirm(&current) {
            self.input = replacement;
            self.cursor_position = self.input.len();
        }
    }
}

impl Default for InputState {
    fn default() -> Self {
        Self::new()
    }
}

pub enum CompletionOutcome {
    Consumed,
    NotConsumed,
    SubmitNow,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wrap(input: &str, cursor: usize, width: usize) -> WrapResult {
        wrap_for_display(input, cursor, width)
    }

    #[test]
    fn wrap_preserves_every_byte_of_pure_whitespace() {
        let input: String = " ".repeat(100);
        let r = wrap(&input, 0, 10);
        assert_eq!(r.rows.len(), 10);
        for row in &r.rows {
            assert_eq!(row.len(), 10);
            assert!(row.chars().all(|c| c == ' '));
        }
        let total: usize = r.rows.iter().map(|s| s.len()).sum();
        assert_eq!(total, input.len());
    }

    #[test]
    fn wrap_cursor_in_middle_of_whitespace_run_is_found() {
        let input: String = " ".repeat(40);
        let r = wrap(&input, 20, 10);
        assert_eq!(r.cursor, Some((0, 2)));
        assert_eq!(r.rows.len(), 4);
        let r = wrap(&input, 15, 10);
        assert_eq!(r.cursor, Some((5, 1)));
    }

    #[test]
    fn wrap_cursor_at_end_of_whitespace_is_found() {
        let input: String = " ".repeat(40);
        let r = wrap(&input, input.len(), 10);
        assert_eq!(r.rows.len(), 4);
        assert_eq!(r.cursor, Some((10, 3)));
    }

    #[test]
    fn wrap_breaks_long_word_at_width() {
        let input = "abcdefghij".repeat(3);
        let r = wrap(&input, input.len(), 10);
        assert_eq!(r.rows.len(), 3);
        for row in &r.rows {
            assert_eq!(row.len(), 10);
        }
        assert_eq!(r.cursor, Some((10, 2)));
    }

    #[test]
    fn wrap_long_word_cursor_at_each_byte_maps_correctly() {
        let input = "abcdefghijklmnop";
        for byte_pos in 0..=input.len() {
            let r = wrap(input, byte_pos, 5);
            let (col, row) = r.cursor.expect("cursor must be found");
            let expected_row = byte_pos / 5;
            let expected_col = byte_pos % 5;
            let on_boundary = expected_col == 0 && byte_pos > 0 && byte_pos < input.len();
            if on_boundary {
                assert!(
                    (col, row) == (0, expected_row) || (col, row) == (5, expected_row - 1),
                    "byte {byte_pos}: got ({col}, {row})"
                );
            } else if byte_pos == input.len() {
                let last_row = (input.len() - 1) / 5;
                let last_col = input.len() - last_row * 5;
                assert_eq!((col, row), (last_col, last_row));
            } else {
                assert_eq!((col, row), (expected_col, expected_row));
            }
        }
    }

    #[test]
    fn wrap_keeps_space_at_wrap_boundary() {
        let input = "abcde     fghij";
        let r = wrap(input, 0, 5);
        assert_eq!(r.rows, vec!["abcde", "     ", "fghij"]);
        let total: usize = r.rows.iter().map(|s| s.len()).sum();
        assert_eq!(total, input.len());
    }

    #[test]
    fn wrap_cursor_on_space_at_boundary() {
        let input = "abcde     fghij";
        let r = wrap(input, 5, 5);
        assert_eq!(r.cursor, Some((0, 1)));
        let r = wrap(input, 8, 5);
        assert_eq!(r.cursor, Some((3, 1)));
    }

    #[test]
    fn wrap_newline_starts_new_row() {
        let r = wrap("foo\nbar", 4, 80);
        assert_eq!(r.rows, vec!["foo".to_string(), "bar".to_string()]);
        assert_eq!(r.cursor, Some((0, 1)));
    }

    #[test]
    fn wrap_consecutive_newlines_produce_empty_rows() {
        let r = wrap("a\n\nb", 4, 80);
        assert_eq!(
            r.rows,
            vec!["a".to_string(), "".to_string(), "b".to_string()]
        );
        assert_eq!(r.cursor, Some((1, 2)));
    }

    #[test]
    fn wrap_empty_input_produces_single_empty_row() {
        let r = wrap("", 0, 10);
        assert_eq!(r.rows, vec![String::new()]);
        assert_eq!(r.cursor, Some((0, 0)));
    }

    #[test]
    fn wrap_trailing_newline_opens_new_empty_row_for_cursor() {
        let r = wrap("abc\n", 4, 80);
        assert_eq!(r.rows, vec!["abc".to_string(), String::new()]);
        assert_eq!(r.cursor, Some((0, 1)));
    }

    #[test]
    fn wrap_lone_newline_produces_two_empty_rows() {
        let r = wrap("\n", 1, 80);
        assert_eq!(r.rows, vec![String::new(), String::new()]);
        assert_eq!(r.cursor, Some((0, 1)));
    }

    #[test]
    fn wrap_input_ending_in_two_newlines_opens_two_blank_rows_below() {
        let r = wrap("abc\n\n", 5, 80);
        assert_eq!(
            r.rows,
            vec!["abc".to_string(), String::new(), String::new()]
        );
        assert_eq!(r.cursor, Some((0, 2)));
    }

    #[test]
    fn input_state_enter_at_end_of_input_makes_cursor_appear_on_new_row() {
        let mut s = InputState::new();
        for c in "abc".chars() {
            s.insert_char(c);
        }
        s.insert_newline();
        let r = wrap_for_display(&s.input, s.cursor_position, 80);
        assert_eq!(r.rows, vec!["abc".to_string(), String::new()]);
        assert_eq!(r.cursor, Some((0, 1)));
    }

    #[test]
    fn wrap_does_not_split_multibyte_codepoint() {
        let input = "café";
        let r = wrap(input, input.len(), 4);
        assert_eq!(r.rows, vec!["café".to_string()]);
        assert_eq!(r.cursor, Some((4, 0)));
    }

    #[test]
    fn wrap_wide_cjk_chars_count_two_cells() {
        let input = "中文测试";
        let r = wrap(input, input.len(), 4);
        assert_eq!(r.rows.len(), 2);
        assert_eq!(r.rows[0].chars().count(), 2);
        assert_eq!(r.rows[1].chars().count(), 2);
        assert_eq!(r.cursor, Some((4, 1)));
    }

    #[test]
    fn wrap_wide_char_overflow_wraps_to_next_row() {
        let input = "abc中";
        let r = wrap(input, input.len(), 4);
        assert_eq!(r.rows, vec!["abc".to_string(), "中".to_string()]);
    }

    #[test]
    fn wrap_zero_width_char_attaches_to_previous_row() {
        let input = "abcde\u{301}";
        let r = wrap(input, input.len(), 5);
        assert_eq!(r.rows.len(), 1);
        assert_eq!(r.rows[0], "abcde\u{301}");
    }

    #[test]
    fn input_state_move_up_then_down_returns_to_same_column() {
        let mut s = InputState::new();
        for c in "abcdefghij\nklmnop".chars() {
            s.insert_char(c);
        }
        let original = s.cursor_position;
        let moved = s.move_cursor_up(80);
        assert!(moved);
        let moved_back = s.move_cursor_down(80);
        assert!(moved_back);
        assert_eq!(s.cursor_position, original);
    }

    #[test]
    fn input_state_move_up_in_wrapped_whitespace_lands_somewhere_valid() {
        let mut s = InputState::new();
        for _ in 0..40 {
            s.insert_char(' ');
        }
        let moved = s.move_cursor_up(10);
        assert!(moved);
        assert!(s.cursor_position <= s.input.len());
        let r = wrap_for_display(&s.input, s.cursor_position, 10);
        let (_, row) = r.cursor.expect("cursor must be found after move_up");
        assert_eq!(row, 2);
    }

    #[test]
    fn input_state_type_after_move_up_inserts_at_visible_location() {
        let mut s = InputState::new();
        for _ in 0..40 {
            s.insert_char(' ');
        }
        let moved = s.move_cursor_up(10);
        assert!(moved);
        let before = wrap_for_display(&s.input, s.cursor_position, 10)
            .cursor
            .expect("cursor visible after move_up");
        let (col_before, row_before) = before;
        assert_eq!(
            row_before, 2,
            "after move_up from end of full row 3, cursor must be on row 2"
        );

        s.insert_char('X');
        let after = wrap_for_display(&s.input, s.cursor_position, 10);
        let row_str = &after.rows[row_before];
        let mut col_walk = 0usize;
        let mut found = None;
        for ch in row_str.chars() {
            if col_walk == col_before {
                found = Some(ch);
                break;
            }
            col_walk += UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        assert_eq!(
            found,
            Some('X'),
            "typed char must land at the visible cursor position"
        );
    }
}
