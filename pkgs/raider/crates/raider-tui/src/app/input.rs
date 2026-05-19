use crossterm::event::{KeyCode, KeyModifiers};
use unicode_width::UnicodeWidthStr;

use crate::completion::CompletionManager;

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
        let w = width.max(1);
        let opts = textwrap::Options::new(w).break_words(true);
        let mut rows: Vec<String> = Vec::new();
        let mut row_byte_starts: Vec<usize> = Vec::new();
        let mut cursor_row: usize = 0;
        let mut cursor_col: usize = 0;
        let mut cursor_found = false;
        let mut current_byte_idx: usize = 0;

        let parts: Vec<&str> = self.input.split('\n').collect();
        let parts_count = parts.len();

        for (i, part) in parts.iter().enumerate() {
            let part_start = current_byte_idx;
            let part_len = part.len();

            let mut lines_for_part = Vec::new();
            if part.is_empty() {
                lines_for_part.push(String::new());
            } else {
                let s = format!("{}\u{200B}", part);
                let wrapped = textwrap::wrap(&s, &opts);
                let last_idx = wrapped.len().saturating_sub(1);
                for (w_i, wl) in wrapped.iter().enumerate() {
                    let mut s = wl.to_string();
                    if w_i == last_idx && s.ends_with('\u{200B}') {
                        s.pop();
                    }
                    lines_for_part.push(s);
                }
            }

            let mut local = 0usize;
            for (li, line_str) in lines_for_part.iter().enumerate() {
                let line_bytes = line_str.len();
                let g_start = part_start + local;
                let g_end = g_start + line_bytes;
                let is_last_visual = li == lines_for_part.len() - 1;

                row_byte_starts.push(g_start);

                if !cursor_found {
                    if self.cursor_position >= g_start && self.cursor_position < g_end {
                        let off = self.cursor_position - g_start;
                        cursor_row = rows.len();
                        cursor_col = UnicodeWidthStr::width(&line_str[..off]);
                        cursor_found = true;
                    } else if self.cursor_position == g_end && is_last_visual {
                        cursor_row = rows.len();
                        cursor_col = UnicodeWidthStr::width(line_str.as_str());
                        cursor_found = true;
                    }
                }
                rows.push(line_str.clone());
                local += line_bytes;
            }

            current_byte_idx += part_len;
            if i < parts_count - 1 {
                current_byte_idx += 1;
            }
        }

        if !cursor_found && self.cursor_position == current_byte_idx {
            if rows.is_empty() {
                rows.push(String::new());
                row_byte_starts.push(0);
            }
            cursor_row = rows.len() - 1;
            cursor_col = UnicodeWidthStr::width(rows.last().unwrap().as_str());
        }

        (rows, row_byte_starts, cursor_row, cursor_col)
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
        let target_line = &rows[target_row];
        let target_start = row_byte_starts[target_row];
        let mut byte_off = 0usize;
        let mut col = 0usize;
        for ch in target_line.chars() {
            let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if col + ch_w > cur_col {
                break;
            }
            col += ch_w;
            byte_off += ch.len_utf8();
        }
        self.cursor_position = target_start + byte_off;
        true
    }

    pub fn move_cursor_down(&mut self, width: usize) -> bool {
        let (rows, row_byte_starts, cur_row, cur_col) = self.visual_layout(width);
        if cur_row >= rows.len().saturating_sub(1) {
            return false;
        }
        let target_row = cur_row + 1;
        let target_line = &rows[target_row];
        let target_start = row_byte_starts[target_row];
        let mut byte_off = 0usize;
        let mut col = 0usize;
        for ch in target_line.chars() {
            let ch_w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
            if col + ch_w > cur_col {
                break;
            }
            col += ch_w;
            byte_off += ch.len_utf8();
        }
        self.cursor_position = target_start + byte_off;
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
