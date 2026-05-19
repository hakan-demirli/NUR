use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::event::Event;
use crate::prompt::QuestionPrompt;

pub struct QuestionModalState {
    pub question_active: Option<QuestionPrompt>,
    pub question_queue: VecDeque<QuestionPrompt>,
}

impl QuestionModalState {
    pub fn new() -> Self {
        Self {
            question_active: None,
            question_queue: VecDeque::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.question_active.is_some()
    }

    pub fn clear(&mut self) {
        self.question_active = None;
        self.question_queue.clear();
    }

    pub fn host_asked(&mut self, prompt: QuestionPrompt) {
        if self.question_active.as_ref().map(|p| &p.id) == Some(&prompt.id) {
            return;
        }
        if self.question_queue.iter().any(|p| p.id == prompt.id) {
            return;
        }
        if self.question_active.is_none() {
            self.question_active = Some(prompt);
        } else {
            self.question_queue.push_back(prompt);
        }
    }

    pub fn host_dismissed(&mut self, request_id: String) {
        if let Some(active) = self.question_active.as_ref() {
            if active.id == request_id {
                self.question_active = self.question_queue.pop_front();
                return;
            }
        }
        self.question_queue.retain(|p| p.id != request_id);
    }

    pub fn submit_question(&mut self, sink: &mut Vec<Event>) {
        let Some(active) = self.question_active.take() else {
            return;
        };
        let mut answers = active.answers.clone();
        answers.resize(active.questions.len(), Vec::new());
        sink.push(Event::QuestionReply {
            request_id: active.id.clone(),
            answers,
        });
        self.question_active = self.question_queue.pop_front();
    }

    pub fn reject_question(&mut self, sink: &mut Vec<Event>) {
        let Some(active) = self.question_active.take() else {
            return;
        };
        sink.push(Event::QuestionReject {
            request_id: active.id.clone(),
        });
        self.question_active = self.question_queue.pop_front();
    }

    pub fn handle_key(&mut self, sink: &mut Vec<Event>, code: KeyCode, mods: KeyModifiers) -> bool {
        if self.question_active.is_none() {
            return false;
        }
        let editing = self
            .question_active
            .as_ref()
            .map(|q| q.editing)
            .unwrap_or(false);
        if editing {
            return self.handle_editing(sink, code, mods);
        }
        match code {
            KeyCode::Left | KeyCode::Char('h') => {
                self.tab_step(-1);
                true
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Tab => {
                let dir = if mods.contains(KeyModifiers::SHIFT) {
                    -1
                } else {
                    1
                };
                self.tab_step(dir);
                true
            }
            KeyCode::BackTab => {
                self.tab_step(-1);
                true
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.select_step(-1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.select_step(1);
                true
            }
            KeyCode::Char(c @ '1'..='9') => {
                let idx = (c as u8 - b'1') as usize;
                self.select_index(idx);
                self.select_option(sink);
                true
            }
            KeyCode::Enter => {
                if self.on_confirm() {
                    self.submit_question(sink);
                } else {
                    self.select_option(sink);
                }
                true
            }
            KeyCode::Esc => {
                self.reject_question(sink);
                true
            }
            _ => true,
        }
    }

    fn handle_editing(
        &mut self,
        sink: &mut Vec<Event>,
        code: KeyCode,
        _mods: KeyModifiers,
    ) -> bool {
        let Some(q) = self.question_active.as_mut() else {
            return false;
        };
        match code {
            KeyCode::Enter => {
                let text = q.edit_buffer.trim().to_string();
                let tab = q.tab;
                let multi = q.questions.get(tab).map(|x| x.multiple).unwrap_or(false);
                let prev = q.custom.get(tab).cloned().unwrap_or_default();
                if text.is_empty() {
                    if !prev.is_empty() {
                        if let Some(slot) = q.custom.get_mut(tab) {
                            slot.clear();
                        }
                        if let Some(ans) = q.answers.get_mut(tab) {
                            ans.retain(|s| s != &prev);
                        }
                    }
                    q.editing = false;
                    q.edit_buffer.clear();
                    return true;
                }
                if multi {
                    if let Some(slot) = q.custom.get_mut(tab) {
                        *slot = text.clone();
                    }
                    if let Some(ans) = q.answers.get_mut(tab) {
                        if !prev.is_empty() {
                            ans.retain(|s| s != &prev);
                        }
                        if !ans.contains(&text) {
                            ans.push(text);
                        }
                    }
                    q.editing = false;
                    q.edit_buffer.clear();
                    return true;
                }
                if let Some(slot) = q.custom.get_mut(tab) {
                    *slot = text.clone();
                }
                if let Some(ans) = q.answers.get_mut(tab) {
                    *ans = vec![text.clone()];
                }
                q.editing = false;
                q.edit_buffer.clear();
                if q.is_single() {
                    self.submit_question(sink);
                } else {
                    self.tab_step(1);
                }
                true
            }
            KeyCode::Esc => {
                q.editing = false;
                q.edit_buffer.clear();
                true
            }
            KeyCode::Backspace => {
                q.edit_buffer.pop();
                true
            }
            KeyCode::Char(c) => {
                q.edit_buffer.push(c);
                true
            }
            _ => true,
        }
    }

    fn tab_step(&mut self, delta: i32) {
        let Some(q) = self.question_active.as_mut() else {
            return;
        };
        let total = q.tab_count() as i32;
        if total == 0 {
            return;
        }
        let next = ((q.tab as i32 + delta).rem_euclid(total)) as usize;
        q.tab = next;
        q.selected = 0;
    }

    fn select_step(&mut self, delta: i32) {
        let Some(q) = self.question_active.as_mut() else {
            return;
        };
        if q.on_confirm() {
            return;
        }
        let total = q.current_row_count() as i32;
        if total == 0 {
            return;
        }
        let next = ((q.selected as i32 + delta).rem_euclid(total)) as usize;
        q.selected = next;
    }

    fn select_index(&mut self, idx: usize) {
        let Some(q) = self.question_active.as_mut() else {
            return;
        };
        let total = q.current_row_count();
        if idx >= total {
            return;
        }
        q.selected = idx;
    }

    fn on_confirm(&self) -> bool {
        self.question_active
            .as_ref()
            .map(|q| q.on_confirm())
            .unwrap_or(false)
    }

    fn select_option(&mut self, sink: &mut Vec<Event>) {
        let Some(q) = self.question_active.as_mut() else {
            return;
        };
        if q.on_confirm() {
            return;
        }
        let tab = q.tab;
        let Some(info) = q.questions.get(tab).cloned() else {
            return;
        };
        let is_custom = q.on_custom_row();
        if is_custom {
            if !info.multiple {
                q.editing = true;
                q.edit_buffer = q.custom.get(tab).cloned().unwrap_or_default();
                return;
            }
            let value = q.custom.get(tab).cloned().unwrap_or_default();
            if !value.is_empty() && q.custom_picked() {
                if let Some(ans) = q.answers.get_mut(tab) {
                    ans.retain(|s| s != &value);
                }
                return;
            }
            q.editing = true;
            q.edit_buffer = value;
            return;
        }
        let label = match info.options.get(q.selected) {
            Some(o) => o.label.clone(),
            None => return,
        };
        if info.multiple {
            if let Some(ans) = q.answers.get_mut(tab) {
                if let Some(pos) = ans.iter().position(|s| s == &label) {
                    ans.remove(pos);
                } else {
                    ans.push(label);
                }
            }
            return;
        }
        if let Some(ans) = q.answers.get_mut(tab) {
            *ans = vec![label];
        }
        if q.is_single() {
            drop(info);
            self.submit_question(sink);
        } else {
            self.tab_step(1);
        }
    }
}

impl Default for QuestionModalState {
    fn default() -> Self {
        Self::new()
    }
}
