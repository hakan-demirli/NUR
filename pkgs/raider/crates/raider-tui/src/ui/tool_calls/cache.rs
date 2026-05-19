use crate::action::ToolCall;
use crate::model::ToolCacheKey;

pub(crate) fn compute_tool_cache_key(
    tool: &ToolCall,
    width: usize,
    theme_mode: crate::ui::theme::Mode,
) -> ToolCacheKey {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    tool.name.hash(&mut h);
    tool.title.hash(&mut h);
    tool.command.hash(&mut h);
    tool.output.hash(&mut h);
    tool.error.hash(&mut h);
    tool.file_path.hash(&mut h);
    tool.diff.hash(&mut h);
    tool.loaded.hash(&mut h);
    for p in &tool.patches {
        (p.kind as u8).hash(&mut h);
        p.path.hash(&mut h);
        p.new_path.hash(&mut h);
        p.diff.hash(&mut h);
    }
    for t in &tool.todos {
        t.content.hash(&mut h);
        t.status.hash(&mut h);
    }
    for q in &tool.questions {
        q.text.hash(&mut h);
        q.options.hash(&mut h);
    }
    tool.answers.hash(&mut h);
    if let Some(child) = &tool.current_child {
        child.part_id.hash(&mut h);
        child.name.hash(&mut h);
        child.title.hash(&mut h);
        child.file_path.hash(&mut h);
        child.command.hash(&mut h);
        (child.status as u8).hash(&mut h);
    }
    tool.child_tool_count.hash(&mut h);
    tool.started_at_ms.hash(&mut h);
    tool.completed_at_ms.hash(&mut h);
    let content_hash = h.finish();
    ToolCacheKey {
        width,
        theme_mode,
        status: tool.status,
        expanded: tool.expanded,
        content_hash,
    }
}
