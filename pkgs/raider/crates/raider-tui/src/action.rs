use crossterm::event::{KeyCode, KeyModifiers};

use crate::model::Sender;
use crate::prompt::{PermissionPrompt, QuestionPrompt};
use crate::provider::{ModelCatalog, ModelRef};
use crate::session::SessionEntry;
use crate::ui::theme::Mode as ThemeMode;

// =============================================================================

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ToolCall {
    pub id: Option<String>,
    pub name: String,
    pub status: ToolStatus,
    pub title: String,
    pub command: Option<String>,
    pub output: String,
    pub error: Option<String>,
    pub todos: Vec<crate::sidebar::TodoEntry>,
    pub file_path: Option<String>,
    pub diff: Option<String>,
    pub loaded: Vec<String>,
    pub patches: Vec<PatchFile>,
    pub questions: Vec<Question>,
    pub answers: Vec<Vec<String>>,
    pub expanded: bool,
    pub current_child: Option<ChildToolRef>,
    pub child_tool_count: u32,
    pub started_at_ms: Option<u128>,
    pub completed_at_ms: Option<u128>,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ChildToolRef {
    pub part_id: String,
    pub name: String,
    pub status: ToolStatus,
    pub file_path: Option<String>,
    pub command: Option<String>,
    pub title: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PatchFile {
    pub kind: PatchKind,
    pub path: String,
    pub new_path: Option<String>,
    pub diff: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PatchKind {
    Created,
    Deleted,
    Moved,
    #[default]
    Patched,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Question {
    pub text: String,
    pub options: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostMessagePart {
    Text(String),
    Thought(String),
    Tool(Box<ToolCall>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ToolStatus {
    #[default]
    Pending,
    Running,
    Completed,
    Error,
}

impl ToolStatus {
    pub fn from_wire(s: &str) -> Self {
        match s {
            "running" => Self::Running,
            "completed" => Self::Completed,
            "error" => Self::Error,
            _ => Self::Pending,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostMessage {
    pub sender: Sender,
    pub content: String,
    pub thoughts: String,
    pub server_id: Option<String>,
    pub timestamp: String,
    pub is_streaming: bool,
    pub agent: Option<String>,
    pub model: Option<String>,
    pub provider_id: Option<String>,
    pub duration: Option<std::time::Duration>,
    pub error: Option<String>,
    pub interrupted: bool,
    pub tool_calls: Vec<ToolCall>,
    pub parts: Vec<HostMessagePart>,
    pub compaction: Option<crate::model::CompactionMarker>,
}

impl HostMessage {
    pub fn user(content: impl Into<String>) -> Self {
        Self {
            sender: Sender::User,
            content: content.into(),
            thoughts: String::new(),
            server_id: None,
            timestamp: String::new(),
            is_streaming: false,
            agent: None,
            model: None,
            provider_id: None,
            duration: None,
            error: None,
            interrupted: false,
            tool_calls: Vec::new(),
            parts: Vec::new(),
            compaction: None,
        }
    }

    pub fn assistant(content: impl Into<String>, thoughts: impl Into<String>) -> Self {
        Self {
            sender: Sender::Assistant,
            content: content.into(),
            thoughts: thoughts.into(),
            server_id: None,
            timestamp: String::new(),
            is_streaming: false,
            agent: None,
            model: None,
            provider_id: None,
            duration: None,
            error: None,
            interrupted: false,
            tool_calls: Vec::new(),
            parts: Vec::new(),
            compaction: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self {
            sender: Sender::System,
            content: content.into(),
            thoughts: String::new(),
            server_id: None,
            timestamp: String::new(),
            is_streaming: false,
            agent: None,
            model: None,
            provider_id: None,
            duration: None,
            error: None,
            interrupted: false,
            tool_calls: Vec::new(),
            parts: Vec::new(),
            compaction: None,
        }
    }

    pub fn with_tool(mut self, tool: ToolCall) -> Self {
        self.tool_calls.push(tool);
        self
    }

    pub fn with_server_id(mut self, id: impl Into<String>) -> Self {
        self.server_id = Some(id.into());
        self
    }

    pub fn with_agent(mut self, agent: impl Into<String>) -> Self {
        self.agent = Some(agent.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = Some(model.into());
        self
    }

    pub fn with_provider(mut self, provider_id: impl Into<String>) -> Self {
        self.provider_id = Some(provider_id.into());
        self
    }

    pub fn with_duration(mut self, duration: std::time::Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn with_error(mut self, error: impl Into<String>) -> Self {
        self.error = Some(error.into());
        self
    }

    pub fn with_interrupted(mut self, interrupted: bool) -> Self {
        self.interrupted = interrupted;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginCommand {
    pub name: String,
    pub title: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub slash_name: Option<String>,
    pub slash_aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginDialogOption {
    pub title: String,
    pub value: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginStatus {
    Active,
    Inactive,
    Error(String),
}

impl PluginStatus {
    pub fn label(&self) -> &'static str {
        match self {
            PluginStatus::Active => "active",
            PluginStatus::Inactive => "inactive",
            PluginStatus::Error(_) => "error",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PluginKind {
    Discovered,
    Configured,
    Installed,
}

impl PluginKind {
    pub fn label(self) -> &'static str {
        match self {
            PluginKind::Discovered => "discovered",
            PluginKind::Configured => "configured",
            PluginKind::Installed => "installed",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PluginInfo {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    pub version: Option<String>,
    pub kind: PluginKind,
    pub source: String,
    pub status: PluginStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastVariant {
    Info,
    Success,
    Warning,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Toast {
    pub title: Option<String>,
    pub message: String,
    pub variant: ToastVariant,
    pub ttl_ticks: u16,
}

impl Toast {
    pub const DEFAULT_TTL_TICKS: u16 = 100;

    pub fn new(message: impl Into<String>, variant: ToastVariant) -> Self {
        Self {
            title: None,
            message: message.into(),
            variant,
            ttl_ticks: Self::DEFAULT_TTL_TICKS,
        }
    }

    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    User(UserAction),
    View(ViewAction),
    Host(HostAction),
    Lifecycle(Lifecycle),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UserAction {
    Key { code: KeyCode, mods: KeyModifiers },
    SubmitInput,
    Interrupt,
    PasteText(String),
    ClearInput,
    MouseScroll { lines: i32 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewAction {
    OpenCommandPalette,
    OpenHelp,
    OpenThemePicker,
    OpenAgentPicker,
    OpenModelPicker,
    OpenVariantPicker,
    OpenSessionPicker,
    OpenSessionRename(Option<String>),
    OpenMessageActions(String),
    OpenForkPicker,
    CloseDialog,

    SetTheme(String),
    SetThemeMode(ThemeMode),
    ToggleThemeMode,

    CycleAgent(i32),
    SetAgent(String),

    SetModel(ModelRef),
    SetVariant(Option<String>),
    CycleModelRecent(i32),
    CycleModelFavorite(i32),
    CycleVariant,

    ToggleSidebar,
    ScrollSidebar(i32),
    ToggleSidebarSection(u32),
    ToggleTimestamps,
    ToggleToolExpanded {
        id: String,
    },

    CopyLastAssistantMessage,
    CopySessionTranscript,
    ExportSession,
    OpenDocs,

    SwitchSession(String),
    PluginNavigateSession(String),

    SubagentEnterFirstChild,
    SubagentGoToParent,
    SubagentCycleSibling(i32),

    OpenPluginManager,
    OpenPluginInstallPrompt,
    TogglePlugin(String),
    ReloadPlugin(String),
    AddPluginPath(String),

    Command(String),
    ShowToast(Toast),
    CopyToClipboard {
        text: String,
        success_message: String,
        error_message: String,
    },
    ClearMessages,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostAction {
    SetSessions(Vec<SessionEntry>),
    RegisterPluginCommands(Vec<PluginCommand>),
    UnregisterPluginCommands(Vec<String>),
    SetPluginList(Vec<PluginInfo>),
    OpenPluginSelect {
        callback_id: u64,
        title: String,
        placeholder: Option<String>,
        options: Vec<PluginDialogOption>,
    },
    OpenPluginAlert {
        title: String,
        message: String,
    },
    ClearPluginDialog,
    SetCurrentSession(Option<String>),
    ReplaceMessages(Vec<HostMessage>),
    UpdateLastAssistantMeta {
        agent: Option<String>,
        model: Option<String>,
        provider_id: Option<String>,
        duration: Option<std::time::Duration>,
    },
    SetLastAssistantError(String),
    MarkAssistantInterrupted {
        message_id: String,
    },
    BindLastUserMessage {
        server_id: String,
        agent: Option<String>,
    },
    AppendMessage(HostMessage),
    UpsertToolCall(Box<ToolCall>),
    UpdateTaskChild {
        parent_tool_id: String,
        child: Option<ChildToolRef>,
        child_tool_count: u32,
    },
    MarkCompaction {
        message_id: String,
        marker: crate::model::CompactionMarker,
    },
    UpsertSession(SessionEntry),
    RemoveSession(String),
    SetSessionBusy {
        session_id: String,
        busy: bool,
    },
    SetSessionStatus {
        session_id: String,
        status: crate::session::SessionStatus,
    },
    SetVcsBranch(Option<String>),
    SetWorkspaceCwd(Option<String>),
    RemoveMessage(String),
    RemoveToolCall(String),
    SetSidebarTitle(String),
    SetSidebarSubtitle(Option<String>),
    SetSidebarSections(Vec<crate::sidebar::SidebarSection>),
    SetSidebarVisible(bool),
    SetSidebarFooterPath(Option<String>),
    SetBusy(bool),
    SetUsage(Option<String>),
    SetCatalog(ModelCatalog),
    SetCurrentModel(Option<ModelRef>),

    PermissionAsked(PermissionPrompt),
    PermissionDismissed(String),
    QuestionAsked(QuestionPrompt),
    QuestionDismissed(String),

    SystemMessage(String),
    AssistantDelta {
        text: String,
        thoughts: bool,
        message_id: Option<String>,
    },
    AssistantDone {
        message_id: Option<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Lifecycle {
    Tick,
    Resize { cols: u16, rows: u16 },
    Quit,
}

impl From<UserAction> for Action {
    fn from(action: UserAction) -> Self {
        Action::User(action)
    }
}

impl From<ViewAction> for Action {
    fn from(action: ViewAction) -> Self {
        Action::View(action)
    }
}

impl From<HostAction> for Action {
    fn from(action: HostAction) -> Self {
        Action::Host(action)
    }
}

impl From<Lifecycle> for Action {
    fn from(action: Lifecycle) -> Self {
        Action::Lifecycle(action)
    }
}
