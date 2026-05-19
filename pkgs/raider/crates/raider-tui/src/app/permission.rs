use std::collections::VecDeque;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::event::{Event, PermissionReplyChoice};
use crate::prompt::PermissionPrompt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PermissionStage {
    #[default]
    Permission,
    Always,
    Reject,
}

pub struct PermissionModalState {
    pub permission_active: Option<PermissionPrompt>,
    pub permission_queue: VecDeque<PermissionPrompt>,
    pub permission_stage: PermissionStage,
    pub permission_reject_buffer: String,
}

impl PermissionModalState {
    pub fn new() -> Self {
        Self {
            permission_active: None,
            permission_queue: VecDeque::new(),
            permission_stage: PermissionStage::default(),
            permission_reject_buffer: String::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.permission_active.is_some()
    }

    pub fn clear(&mut self) {
        self.permission_active = None;
        self.permission_queue.clear();
        self.permission_stage = PermissionStage::default();
        self.permission_reject_buffer.clear();
    }

    pub fn host_asked(&mut self, prompt: PermissionPrompt) {
        if self.permission_active.as_ref().map(|p| &p.id) == Some(&prompt.id) {
            return;
        }
        if self.permission_queue.iter().any(|p| p.id == prompt.id) {
            return;
        }
        if self.permission_active.is_none() {
            self.permission_active = Some(prompt);
            self.permission_stage = PermissionStage::Permission;
            self.permission_reject_buffer.clear();
        } else {
            self.permission_queue.push_back(prompt);
        }
    }

    pub fn host_dismissed(&mut self, request_id: String) {
        if let Some(active) = self.permission_active.as_ref() {
            if active.id == request_id {
                self.permission_active = self.permission_queue.pop_front();
                self.permission_stage = PermissionStage::Permission;
                self.permission_reject_buffer.clear();
                return;
            }
        }
        self.permission_queue.retain(|p| p.id != request_id);
    }

    fn emit_reply(
        &mut self,
        sink: &mut Vec<Event>,
        choice: PermissionReplyChoice,
        message: Option<String>,
    ) {
        let Some(active) = self.permission_active.take() else {
            return;
        };
        sink.push(Event::PermissionReply {
            request_id: active.id.clone(),
            reply: choice,
            message,
        });
        self.permission_active = self.permission_queue.pop_front();
        self.permission_stage = PermissionStage::Permission;
        self.permission_reject_buffer.clear();
    }

    pub fn handle_key(
        &mut self,
        sink: &mut Vec<Event>,
        code: KeyCode,
        _mods: KeyModifiers,
    ) -> bool {
        if self.permission_active.is_none() {
            return false;
        }
        match self.permission_stage {
            PermissionStage::Permission => self.handle_main(sink, code),
            PermissionStage::Always => self.handle_always(sink, code),
            PermissionStage::Reject => self.handle_reject(sink, code),
        }
    }

    fn handle_main(&mut self, sink: &mut Vec<Event>, code: KeyCode) -> bool {
        match code {
            KeyCode::Char('1') => {
                self.emit_reply(sink, PermissionReplyChoice::Once, None);
                true
            }
            KeyCode::Char('2') => {
                self.permission_stage = PermissionStage::Always;
                true
            }
            KeyCode::Char('3') => {
                self.permission_stage = PermissionStage::Reject;
                self.permission_reject_buffer.clear();
                true
            }
            KeyCode::Enter => {
                self.emit_reply(sink, PermissionReplyChoice::Once, None);
                true
            }
            KeyCode::Esc => {
                self.emit_reply(sink, PermissionReplyChoice::Reject, None);
                true
            }
            _ => true,
        }
    }

    fn handle_always(&mut self, sink: &mut Vec<Event>, code: KeyCode) -> bool {
        match code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.emit_reply(sink, PermissionReplyChoice::Always, None);
                true
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.permission_stage = PermissionStage::Permission;
                true
            }
            _ => true,
        }
    }

    fn handle_reject(&mut self, sink: &mut Vec<Event>, code: KeyCode) -> bool {
        match code {
            KeyCode::Enter => {
                let msg = if self.permission_reject_buffer.trim().is_empty() {
                    None
                } else {
                    Some(self.permission_reject_buffer.clone())
                };
                self.emit_reply(sink, PermissionReplyChoice::Reject, msg);
                true
            }
            KeyCode::Esc => {
                self.permission_stage = PermissionStage::Permission;
                self.permission_reject_buffer.clear();
                true
            }
            KeyCode::Backspace => {
                self.permission_reject_buffer.pop();
                true
            }
            KeyCode::Char(c) => {
                self.permission_reject_buffer.push(c);
                true
            }
            _ => true,
        }
    }
}

impl Default for PermissionModalState {
    fn default() -> Self {
        Self::new()
    }
}
