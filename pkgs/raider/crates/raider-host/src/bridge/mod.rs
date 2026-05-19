//! Test helpers live in [`tests`] (only compiled with `cfg(test)`).

pub mod extra;
pub mod helpers;
pub mod message_map;
pub mod mirror;
pub mod permission;
pub mod provider;
pub mod question;
pub mod session_map;
pub mod sidebar;
pub mod tool;
pub mod translate;

pub const MAX_TOOL_OUTPUT_BYTES: usize = 30_000;
pub const MAX_TOOL_OUTPUT_LINES: usize = 256;

pub use message_map::{message_to_host, messages_refresh_actions};
pub use mirror::PartMirror;
pub use permission::permission_to_prompt;
pub use provider::provider_refresh_actions;
pub use question::question_to_prompt;
pub(crate) use session_map::session_status_to_tui;
pub use session_map::{session_to_entry, sessions_refresh_actions};
pub use sidebar::sidebar_actions_for_session;
pub use translate::{translate, Translation};

#[cfg(test)]
pub(crate) use extra::{extract_agent, extract_model_display, extract_provider};
#[cfg(test)]
pub(crate) use helpers::tail_bytes;
#[cfg(test)]
pub(crate) use sidebar::{format_thousands, format_tokens_compact};
#[cfg(test)]
pub(crate) use tool::{synthesize_tool_title, tool_part_to_call};

#[cfg(test)]
mod tests;
