use crate::dialog::PluginInstallScope;
use crate::provider::ModelRef;
use crate::ui::theme::Mode as ThemeMode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UserFileAttachment {
    pub mime: String,
    pub filename: String,
    pub filepath: String,
    pub base64: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    UserMessage(String),

    UserMessageWithFiles {
        text: String,
        files: Vec<UserFileAttachment>,
    },

    Command {
        name: String,
        args: String,
    },

    PluginCommand {
        name: String,
        args: String,
    },

    PluginDialogSelected {
        callback_id: u64,
        value: String,
    },

    PluginDialogDismissed {
        callback_id: u64,
    },

    ThemeChanged(String),

    ThemeModeChanged(ThemeMode),

    AgentChanged(String),

    ModelChanged {
        model: ModelRef,
        variant: Option<String>,
    },

    VariantChanged(Option<String>),

    Export {
        suggested_filename: String,
        markdown: String,
    },

    SessionSwitched(String),

    SubagentNavigate(String),

    Interrupt,

    PermissionReply {
        request_id: String,
        reply: PermissionReplyChoice,
        message: Option<String>,
    },

    QuestionReply {
        request_id: String,
        answers: Vec<Vec<String>>,
    },

    QuestionReject {
        request_id: String,
    },

    Undo {
        message_id: String,
    },

    Redo,

    ForkSession {
        message_id: Option<String>,
    },

    RenameSession {
        session_id: String,
        title: String,
    },

    DeleteSession {
        session_id: String,
    },

    CopyToClipboard {
        text: String,
        success_message: String,
        error_message: String,
    },

    OpenUrl(String),

    TogglePlugin(String),

    ReloadPlugin(String),

    InstallPluginPath {
        path: String,
        scope: PluginInstallScope,
    },

    Quit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PermissionReplyChoice {
    Once,
    Always,
    Reject,
}
