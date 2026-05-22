use raider_opencode::{
    events::{
        MessagePartDeltaProps, MessagePartUpdatedProps, MessageUpdatedProps, ServerEvent,
        SessionDeletedProps, SessionIdleProps, SessionUpdatedProps,
    },
    types::{
        common::{MessageId, SessionId},
        message::{MessagePart, MessageRole},
    },
};
use raider_tui::{Action, HostAction};

use super::extra::{
    extract_agent, extract_model_display, extract_provider, is_message_aborted_error,
    unwrap_error_message,
};
use super::mirror::{PartKind, PartMirror};
use super::permission::permission_to_prompt;
use super::question::question_to_prompt;
use super::session_map::{session_status_to_tui, session_to_entry};
use super::tool::{child_tool_ref_from_part, tool_part_to_call};

#[derive(Debug, Default)]
pub struct Translation {
    pub actions: Vec<Action>,
    pub log: Vec<String>,
}

impl Translation {
    fn push(&mut self, a: Action) {
        self.actions.push(a);
    }

    fn log(&mut self, s: impl Into<String>) {
        self.log.push(s.into());
    }
}

pub fn translate(
    ev: ServerEvent,
    active_session: Option<&SessionId>,
    mirror: &mut PartMirror,
) -> Translation {
    let mut out = Translation::default();
    match ev {
        ServerEvent::SessionUpdated(SessionUpdatedProps { info, .. }) => {
            out.log(format!("session.updated id={}", info.id));
            if let Some(parent) = info.parent_id.clone() {
                mirror.remember_child_parent(info.id.clone(), parent);
            }
            let current = active_session.map(|s| s.as_str());
            let active_match = current == Some(info.id.as_str());
            let entry = session_to_entry(&info, current);
            out.push(Action::Host(HostAction::UpsertSession(entry)));
            if active_match {
                let title = if info.title.trim().is_empty() {
                    info.id.as_str().to_string()
                } else {
                    info.title.clone()
                };
                out.push(Action::Host(HostAction::SetSidebarTitle(title)));
            }
        }
        ServerEvent::SessionDeleted(SessionDeletedProps { session_id, info }) => {
            let id = info
                .as_ref()
                .map(|s| s.id.as_str().to_string())
                .or_else(|| session_id.as_ref().map(|s| s.as_str().to_string()))
                .unwrap_or_default();
            out.log(format!("session.deleted id={id}"));
            if !id.is_empty() {
                if let Some(sid) = info
                    .as_ref()
                    .map(|s| s.id.clone())
                    .or_else(|| session_id.clone())
                {
                    mirror.forget_session(&sid);
                }
                out.push(Action::Host(HostAction::RemoveSession(id)));
            }
        }
        ServerEvent::SessionIdle(SessionIdleProps { session_id }) => {
            out.push(Action::Host(HostAction::SetSessionStatus {
                session_id: session_id.as_str().to_string(),
                status: raider_tui::SessionStatus::Idle,
            }));
            out.push(Action::Host(HostAction::SetSessionBusy {
                session_id: session_id.as_str().to_string(),
                busy: false,
            }));
            if Some(&session_id) == active_session {
                out.push(Action::Host(HostAction::AssistantDone { message_id: None }));
                out.push(Action::Host(HostAction::SetBusy(false)));
            }
        }
        ServerEvent::SessionError(p) => {
            // BUGFIX: the prior implementation only checked the
            let is_active = active_session
                .map(|sid| p.session_id.as_ref() == Some(sid))
                .unwrap_or(false);
            if is_active {
                if let Some(session_id) = &p.session_id {
                    out.push(Action::Host(HostAction::SetSessionStatus {
                        session_id: session_id.as_str().to_string(),
                        status: raider_tui::SessionStatus::Idle,
                    }));
                    out.push(Action::Host(HostAction::SetSessionBusy {
                        session_id: session_id.as_str().to_string(),
                        busy: false,
                    }));
                }
                if is_message_aborted_error(&p.error) {
                    out.push(Action::Host(HostAction::AssistantDone { message_id: None }));
                    out.push(Action::Host(HostAction::SetBusy(false)));
                } else {
                    let msg = unwrap_error_message(&p.error)
                        .unwrap_or_else(|| "Session error".to_string());
                    out.push(Action::View(raider_tui::ViewAction::ShowToast(
                        raider_tui::Toast::new(msg.clone(), raider_tui::ToastVariant::Error),
                    )));
                    out.push(Action::Host(HostAction::SetLastAssistantError(msg)));
                    out.push(Action::Host(HostAction::AssistantDone { message_id: None }));
                    out.push(Action::Host(HostAction::SetBusy(false)));
                }
            }
        }
        ServerEvent::MessageUpdated(MessageUpdatedProps { info }) => {
            mirror.remember_role(info.info.id.clone(), info.info.role);
            if let Some(sid) = info.info.session_id.as_ref() {
                mirror.associate_message_with_session(info.info.id.clone(), sid.clone());
            }

            let session_match = active_session
                .and_then(|active| info.info.session_id.as_ref().map(|s| s == active))
                .unwrap_or(false);
            if session_match && matches!(info.info.role, MessageRole::Assistant) {
                if info.info.time.completed.is_some() {
                    mirror.mark_message_complete(&info.info.id);
                    let agent = extract_agent(&info.info.extra);
                    let model = extract_model_display(&info.info.extra);
                    let provider_id = extract_provider(&info.info.extra);
                    let duration = match (info.info.time.created, info.info.time.completed) {
                        (Some(start), Some(end)) if end >= start => {
                            Some(std::time::Duration::from_millis((end - start) as u64))
                        }
                        _ => None,
                    };
                    let output_tokens = super::sidebar::message_output_tokens(&info.info.extra);
                    if agent.is_some()
                        || model.is_some()
                        || provider_id.is_some()
                        || duration.is_some()
                        || output_tokens.is_some()
                    {
                        out.push(Action::Host(HostAction::UpdateLastAssistantMeta {
                            agent,
                            model,
                            provider_id,
                            duration,
                            output_tokens,
                        }));
                    }
                    if let Some(raw) = info.info.extra.get("error") {
                        if is_message_aborted_error(raw) {
                            out.push(Action::Host(HostAction::MarkAssistantInterrupted {
                                message_id: info.info.id.as_str().to_string(),
                            }));
                        }
                    }
                    if let Some(session_id) = info.info.session_id.as_ref() {
                        out.push(Action::Host(HostAction::SetSessionStatus {
                            session_id: session_id.as_str().to_string(),
                            status: raider_tui::SessionStatus::Idle,
                        }));
                        out.push(Action::Host(HostAction::SetSessionBusy {
                            session_id: session_id.as_str().to_string(),
                            busy: false,
                        }));
                    }
                    out.push(Action::Host(HostAction::AssistantDone {
                        message_id: Some(info.info.id.as_str().to_string()),
                    }));
                    out.push(Action::Host(HostAction::SetBusy(false)));
                } else {
                    out.push(Action::Host(HostAction::SetBusy(true)));
                }
            }
        }
        ServerEvent::MessagePartUpdated(MessagePartUpdatedProps {
            session_id,
            message_id,
            part,
            part_id: _,
        }) => {
            let resolved_message_id = message_id.clone().or_else(|| part.message_id().cloned());
            let part_id = part.part_id().cloned();
            if let (Some(mid), Some(pid)) = (resolved_message_id.clone(), part_id) {
                mirror.remember_kind(mid, pid, PartKind::from_part(&part));
            }
            if Some(&session_id) != active_session {
                if let Some(parent_part) = mirror.parent_part_for_child(&session_id) {
                    if let MessagePart::Tool(child_tool) = &part {
                        let child_ref = child_tool_ref_from_part(child_tool);
                        mirror.note_child_session_current_tool(
                            session_id.clone(),
                            child_tool.id.clone(),
                        );
                        let count =
                            mirror.record_child_tool(parent_part.clone(), child_tool.id.clone());
                        out.push(Action::Host(HostAction::UpdateTaskChild {
                            parent_tool_id: parent_part.as_str().to_string(),
                            child: Some(child_ref),
                            child_tool_count: count,
                        }));
                    }
                }
                return out;
            }
            let Some(mid) = resolved_message_id else {
                return out;
            };
            if mirror.is_user_message(&mid) && !matches!(part, MessagePart::Compaction(_)) {
                return out;
            }
            handle_part(&mut out, mirror, mid, part);
        }
        ServerEvent::VcsBranchUpdated(p) => {
            out.push(Action::Host(HostAction::SetVcsBranch(p.branch.clone())));
        }
        ServerEvent::SessionStatus(p) => {
            let status = session_status_to_tui(&p.status);
            out.push(Action::Host(HostAction::SetSessionStatus {
                session_id: p.session_id.as_str().to_string(),
                status,
            }));
            out.push(Action::Host(HostAction::SetSessionBusy {
                session_id: p.session_id.as_str().to_string(),
                busy: p.status.is_working(),
            }));
        }
        ServerEvent::MessageRemoved(p) => {
            if Some(&p.session_id) != active_session {
                out.log(format!(
                    "message.removed id={} session={} (inactive — local drop only)",
                    p.message_id.as_str(),
                    p.session_id.as_str(),
                ));
                return out;
            }
            out.push(Action::Host(HostAction::RemoveMessage(
                p.message_id.as_str().to_string(),
            )));
        }
        ServerEvent::MessagePartRemoved(p) => {
            if Some(&p.session_id) != active_session {
                return out;
            }
            out.push(Action::Host(HostAction::RemoveToolCall(
                p.part_id.as_str().to_string(),
            )));
        }
        ServerEvent::MessagePartDelta(MessagePartDeltaProps {
            session_id,
            message_id,
            part_id,
            field,
            delta,
        }) => {
            if Some(&session_id) != active_session {
                return out;
            }
            if delta.is_empty() {
                return out;
            }
            let kind = mirror
                .kind_of(&message_id, &part_id)
                .unwrap_or(PartKind::Text);
            let thoughts = matches!(kind, PartKind::Reasoning) || field.as_str() == "reasoning";
            let recognised_field = matches!(field.as_str(), "text" | "reasoning")
                || matches!(kind, PartKind::Reasoning);
            if !recognised_field {
                let delta_chars = delta.chars().count();
                if delta_chars > 0 {
                    mirror.note_tool_input_chars(message_id.clone(), part_id.clone(), delta_chars);
                    let approx = raider_tui::model::approx_tokens_from_chars(delta_chars as u64);
                    if approx > 0 {
                        out.push(Action::Host(HostAction::AssistantTokenProgress {
                            tokens: approx,
                            message_id: Some(message_id.as_str().to_string()),
                        }));
                    }
                }
                out.log(format!(
                    "message.part.delta with unknown field={field} (credited to live token counter)"
                ));
                return out;
            }
            let mirror_kind = if thoughts {
                PartKind::Reasoning
            } else {
                PartKind::Text
            };
            mirror.note_streamed_part(message_id.clone(), part_id.clone(), mirror_kind, &delta);
            out.push(Action::Host(HostAction::AssistantDelta {
                text: delta,
                thoughts,
                message_id: Some(message_id.as_str().to_string()),
            }));
            out.push(Action::Host(HostAction::SetBusy(true)));
        }
        ServerEvent::PermissionAsked(req) => {
            if Some(&req.session_id) == active_session {
                out.push(Action::Host(HostAction::PermissionAsked(
                    permission_to_prompt(&req),
                )));
            } else {
                out.log(format!(
                    "permission.asked id={} session={} (inactive — modal suppressed)",
                    req.id,
                    req.session_id.as_str(),
                ));
            }
        }
        ServerEvent::PermissionReplied(p) => {
            out.push(Action::Host(HostAction::PermissionDismissed(
                p.request_id.clone(),
            )));
            out.log(format!(
                "permission.replied id={} reply={:?}",
                p.request_id, p.reply,
            ));
        }
        ServerEvent::QuestionAsked(req) => {
            if Some(&req.session_id) == active_session {
                out.push(Action::Host(HostAction::QuestionAsked(question_to_prompt(
                    &req,
                ))));
            } else {
                out.log(format!(
                    "question.asked id={} session={} (inactive — modal suppressed)",
                    req.id,
                    req.session_id.as_str(),
                ));
            }
        }
        ServerEvent::QuestionReplied(p) => {
            out.push(Action::Host(HostAction::QuestionDismissed(
                p.request_id.clone(),
            )));
            out.log(format!(
                "question.replied id={} answers={}",
                p.request_id,
                p.answers.len(),
            ));
        }
        ServerEvent::QuestionRejected(p) => {
            out.push(Action::Host(HostAction::QuestionDismissed(
                p.request_id.clone(),
            )));
            out.log(format!("question.rejected id={}", p.request_id));
        }
        ServerEvent::Unknown(ty) => {
            out.log(format!(
                "unknown server event type={ty} (preserved as no-op)"
            ));
        }
    }
    out
}

fn handle_part(
    out: &mut Translation,
    mirror: &mut PartMirror,
    message_id: MessageId,
    part: MessagePart,
) {
    let mid_str = Some(message_id.as_str().to_string());
    match part {
        MessagePart::Text(t) => {
            if let Some(delta) = mirror.diff_text(message_id, t.id, &t.text) {
                out.actions.push(Action::Host(HostAction::AssistantDelta {
                    text: delta,
                    thoughts: false,
                    message_id: mid_str,
                }));
                out.actions.push(Action::Host(HostAction::SetBusy(true)));
            }
        }
        MessagePart::Reasoning(r) => {
            if let Some(delta) = mirror.diff_reasoning(message_id, r.id, &r.text) {
                out.actions.push(Action::Host(HostAction::AssistantDelta {
                    text: delta,
                    thoughts: true,
                    message_id: mid_str,
                }));
                out.actions.push(Action::Host(HostAction::SetBusy(true)));
            }
        }
        MessagePart::Tool(t) => {
            // BUG7 fix: tool parts arrive via `message.part.updated`
            if t.tool_name == "task" {
                let meta = t.state.metadata.as_object();
                let child_sid = meta
                    .and_then(|m| m.get("sessionId").or_else(|| m.get("sessionID")))
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(SessionId::new);
                if let Some(child_sid) = child_sid {
                    mirror.remember_task_child_session(child_sid, t.id.clone());
                }
            }
            let new_chars = tool_llm_char_count(&t);
            let delta_chars =
                mirror.diff_tool_input_chars(message_id.clone(), t.id.clone(), new_chars);
            if delta_chars > 0 {
                let approx = raider_tui::model::approx_tokens_from_chars(delta_chars as u64);
                if approx > 0 {
                    out.actions
                        .push(Action::Host(HostAction::AssistantTokenProgress {
                            tokens: approx,
                            message_id: mid_str.clone(),
                        }));
                }
            }
            out.actions
                .push(Action::Host(HostAction::UpsertToolCall(Box::new(
                    tool_part_to_call(&t),
                ))));
            out.actions.push(Action::Host(HostAction::SetBusy(true)));
        }
        MessagePart::Compaction(c) => {
            out.actions.push(Action::Host(HostAction::MarkCompaction {
                message_id: message_id.as_str().to_string(),
                marker: raider_tui::model::CompactionMarker { auto: c.auto },
            }));
        }
        MessagePart::StepStart(_) | MessagePart::StepFinish(_) | MessagePart::Other => {}
    }
}

fn tool_llm_char_count(t: &raider_opencode::types::message::ToolPart) -> usize {
    let title_chars = t.state.title.chars().count();
    let input_chars = t.state.input.to_string().chars().count();
    title_chars.saturating_add(input_chars)
}
