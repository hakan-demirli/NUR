use raider_opencode::types::{
    diff::FileDiff,
    lsp::LspStatus,
    mcp::McpRegistry,
    message::{MessageRole, MessageWithParts},
    session::Session,
    todo::Todo,
};
use raider_tui::{
    sidebar::{FileChange, LspEntry, McpEntry, TodoEntry},
    Action, HostAction, ModelCatalog,
};

#[allow(clippy::too_many_arguments)]
pub fn sidebar_actions_for_session(
    session: &Session,
    catalog: Option<&ModelCatalog>,
    messages: &[MessageWithParts],
    diff: &[FileDiff],
    todos: &[Todo],
    mcp: &McpRegistry,
    lsps: &[LspStatus],
    lsp_enabled: bool,
) -> Vec<Action> {
    use raider_tui::sidebar::{slot, SidebarSection};

    let title = if session.title.is_empty() {
        session.id.as_str().to_string()
    } else {
        session.title.clone()
    };

    let mut sections: Vec<SidebarSection> = Vec::new();

    let last_assistant_context = last_assistant_context_window(messages);
    let cost = session.extra.get("cost").and_then(|v| v.as_f64());
    let (total_tokens, msg_provider_id, msg_model_id) = match &last_assistant_context {
        Some(ctx) => (
            Some(ctx.tokens),
            ctx.provider_id.clone(),
            ctx.model_id.clone(),
        ),
        None => (None, None, None),
    };

    let context_limit = match (catalog, &msg_provider_id, &msg_model_id) {
        (Some(c), Some(pid), Some(mid)) => c
            .find_provider(pid)
            .and_then(|p| p.find_model(mid))
            .map(|m| m.context_limit)
            .filter(|&n| n > 0),
        _ => None,
    };
    let percent_used: Option<u32> = match (total_tokens, context_limit) {
        (Some(t), Some(limit)) if limit > 0 => {
            let pct = ((t as f64) / (limit as f64) * 100.0 + 0.5) as u32;
            Some(pct)
        }
        _ => None,
    };

    if total_tokens.is_some() || cost.is_some() {
        let mut lines: Vec<String> = Vec::new();
        let total_display = total_tokens.unwrap_or(0);
        lines.push(format!("{} tokens", format_thousands(total_display)));
        if let Some(p) = percent_used {
            lines.push(format!("{p}% used"));
        }
        if let Some(c) = cost {
            lines.push(format!("${c:.2} spent"));
        }
        sections.push(SidebarSection::new("Context", lines).with_order(slot::CONTEXT));
    }

    if !mcp.is_empty() {
        let entries: Vec<McpEntry> = mcp
            .iter()
            .map(|(name, status)| McpEntry::new(name, &status.status, &status.error))
            .collect();
        sections.push(SidebarSection::mcps("MCP", entries).with_order(slot::MCP));
    }

    let lsp_entries: Vec<LspEntry> = lsps
        .iter()
        .map(|s| LspEntry::new(s.id.clone(), s.root.clone(), s.status.clone()))
        .collect();
    let lsp_placeholder = if lsp_enabled {
        "LSPs will activate as files are read"
    } else {
        "LSPs are disabled"
    };
    sections.push(SidebarSection::lsps("LSP", lsp_entries, lsp_placeholder).with_order(slot::LSP));

    let has_uncompleted = todos.iter().any(|t| t.status != "completed");
    if !todos.is_empty() && has_uncompleted {
        let entries: Vec<TodoEntry> = todos
            .iter()
            .map(|t| TodoEntry::new(t.content.clone(), t.status.clone()))
            .collect();
        sections.push(SidebarSection::todos("Todo", entries).with_order(slot::TODO));
    }

    if !diff.is_empty() {
        let entries: Vec<FileChange> = diff
            .iter()
            .map(|d| FileChange::new(d.file.clone(), d.additions, d.deletions))
            .collect();
        sections.push(
            SidebarSection::files("Modified Files", entries).with_order(slot::MODIFIED_FILES),
        );
    }

    sections.sort_by_key(|s| s.order);

    let tokens_str = total_tokens.map(|t| match percent_used {
        Some(p) => format!("{} ({p}%)", format_tokens_compact(t)),
        None => format_tokens_compact(t),
    });
    let usage = match (tokens_str, cost) {
        (Some(t), Some(c)) => Some(format!("{t} · ${c:.2}")),
        (Some(t), None) => Some(t),
        (None, Some(c)) => Some(format!("${c:.2}")),
        (None, None) => None,
    };

    let mut actions = vec![
        Action::Host(HostAction::SetSidebarTitle(title)),
        Action::Host(HostAction::SetSidebarSubtitle(Some(
            session.id.as_str().to_string(),
        ))),
        Action::Host(HostAction::SetSidebarSections(sections)),
        Action::Host(HostAction::SetUsage(usage)),
    ];
    actions.shrink_to_fit();
    actions
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LastAssistantContext {
    pub(super) tokens: u64,
    pub(super) provider_id: Option<String>,
    pub(super) model_id: Option<String>,
}

pub(super) fn last_assistant_context_window(
    messages: &[MessageWithParts],
) -> Option<LastAssistantContext> {
    for entry in messages.iter().rev() {
        if !matches!(entry.info.role, MessageRole::Assistant) {
            continue;
        }
        let tokens_obj = match entry.info.extra.get("tokens").and_then(|v| v.as_object()) {
            Some(o) => o,
            None => continue,
        };
        let output = tokens_obj
            .get("output")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if output == 0 {
            continue;
        }
        let input = tokens_obj
            .get("input")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let reasoning = tokens_obj
            .get("reasoning")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_obj = tokens_obj.get("cache").and_then(|v| v.as_object());
        let cache_read = cache_obj
            .and_then(|c| c.get("read"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let cache_write = cache_obj
            .and_then(|c| c.get("write"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let tokens = input + output + reasoning + cache_read + cache_write;
        let provider_id = entry
            .info
            .extra
            .get("providerID")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let model_id = entry
            .info
            .extra
            .get("modelID")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        return Some(LastAssistantContext {
            tokens,
            provider_id,
            model_id,
        });
    }
    None
}

pub(super) fn message_output_tokens(
    extra: &serde_json::Map<String, serde_json::Value>,
) -> Option<u64> {
    let tokens_obj = extra.get("tokens").and_then(|v| v.as_object())?;
    let output = tokens_obj
        .get("output")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let reasoning = tokens_obj
        .get("reasoning")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let total = output.saturating_add(reasoning);
    if total == 0 {
        None
    } else {
        Some(total)
    }
}

pub(crate) fn format_tokens_compact(n: u64) -> String {
    if n < 1_000 {
        return format!("{n}");
    }
    if n < 1_000_000 {
        let k = n as f64 / 1_000.0;
        return strip_trailing_zero(format!("{k:.1}K"));
    }
    let m = n as f64 / 1_000_000.0;
    strip_trailing_zero(format!("{m:.1}M"))
}

pub(super) fn strip_trailing_zero(s: String) -> String {
    if let Some(stripped) = s.strip_suffix(".0K") {
        return format!("{stripped}K");
    }
    if let Some(stripped) = s.strip_suffix(".0M") {
        return format!("{stripped}M");
    }
    s
}

pub(crate) fn format_thousands(n: u64) -> String {
    let s = n.to_string();
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + s.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i > 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}
