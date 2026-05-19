pub(crate) const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

pub(crate) const SPINNER_INTERVAL_MS: u128 = 80;

pub(crate) fn spinner_frame() -> &'static str {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    spinner_frame_for(now_ms)
}

pub(crate) fn spinner_frame_for(now_ms: u128) -> &'static str {
    let idx = (now_ms / SPINNER_INTERVAL_MS) as usize % SPINNER_FRAMES.len();
    SPINNER_FRAMES[idx]
}

pub(crate) fn tool_uses_running_spinner(tool_name: &str) -> bool {
    matches!(tool_name, "bash" | "read" | "task" | "question")
}
