pub mod events;
pub mod message;
pub mod opencode;
pub mod permission;
pub mod prompt;
pub mod provider;
pub mod question;
pub mod session;
pub mod tooling;

pub use events::EventBackend;
pub use message::MessageBackend;
pub use opencode::OpencodeBackend;
pub use permission::PermissionBackend;
pub use prompt::PromptBackend;
pub use provider::ProviderBackend;
pub use question::QuestionBackend;
pub use session::SessionBackend;
pub use tooling::ToolingBackend;

pub trait Backend:
    SessionBackend
    + MessageBackend
    + PromptBackend
    + ProviderBackend
    + ToolingBackend
    + PermissionBackend
    + QuestionBackend
    + EventBackend
{
}

impl<T> Backend for T where
    T: SessionBackend
        + MessageBackend
        + PromptBackend
        + ProviderBackend
        + ToolingBackend
        + PermissionBackend
        + QuestionBackend
        + EventBackend
{
}
