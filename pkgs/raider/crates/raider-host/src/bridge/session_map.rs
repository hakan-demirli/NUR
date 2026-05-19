use raider_opencode::{
    events::SessionStatusKind,
    types::{common::SessionId, session::Session},
};
use raider_tui::{Action, HostAction, SessionEntry};

pub fn session_to_entry(s: &Session, current_id: Option<&str>) -> SessionEntry {
    let title = if s.title.trim().is_empty() {
        s.id.as_str().to_string()
    } else {
        s.title.clone()
    };
    let label = match s.time.archived {
        Some(_) => "(archived)".to_string(),
        None => updated_label(s.time.updated),
    };
    let mut entry = SessionEntry::new(s.id.as_str(), title, label);
    if let Some(parent) = &s.parent_id {
        entry = entry.with_parent(parent.as_str().to_string());
    }
    if let (Some(_), Some(_)) = (&s.parent_id, current_id) {
        entry = entry.with_fork("(fork)".to_string());
    }
    entry
}

pub(super) fn updated_label(ms: Option<i64>) -> String {
    let Some(ms) = ms else {
        return "—".to_string();
    };
    use chrono::{Datelike, Local, TimeZone};
    let Some(updated) = Local.timestamp_millis_opt(ms).single() else {
        return "—".to_string();
    };
    let now = Local::now();
    let is_today = updated.year() == now.year()
        && updated.month() == now.month()
        && updated.day() == now.day();
    if is_today {
        updated.format("%-I:%M %p").to_string()
    } else {
        format!(
            "{} · {}",
            updated.format("%-I:%M %p"),
            updated.format("%-m/%-d/%Y"),
        )
    }
}

pub fn sessions_refresh_actions(sessions: &[Session], current: Option<&SessionId>) -> Vec<Action> {
    let entries: Vec<SessionEntry> = sessions
        .iter()
        .map(|s| session_to_entry(s, current.map(|c| c.as_str())))
        .collect();
    vec![
        Action::Host(HostAction::SetSessions(entries)),
        Action::Host(HostAction::SetCurrentSession(
            current.map(|c| c.as_str().to_string()),
        )),
    ]
}

pub(crate) fn session_status_to_tui(status: &SessionStatusKind) -> raider_tui::SessionStatus {
    match status {
        SessionStatusKind::Idle => raider_tui::SessionStatus::Idle,
        SessionStatusKind::Busy => raider_tui::SessionStatus::Busy,
        SessionStatusKind::Retry {
            attempt,
            message,
            next,
        } => raider_tui::SessionStatus::Retry {
            attempt: *attempt,
            message: message.clone(),
            next: *next,
        },
    }
}
