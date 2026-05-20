use crate::action::{Action, HostAction, Lifecycle, UserAction, ViewAction};
use crate::provider::ModelRef;
use crate::ui::theme::Mode as ThemeMode;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SlashCommand {
    Action(Box<Action>),
    InvalidArg { slash: &'static str, detail: String },
    Unknown { name: String, args: String },
    Empty,
}

impl SlashCommand {
    fn action(action: impl Into<Action>) -> Self {
        Self::Action(Box::new(action.into()))
    }

    pub fn parse(input: &str) -> Self {
        let trimmed = input.trim();
        if trimmed.is_empty() || trimmed == "/" {
            return Self::Empty;
        }
        let payload = trimmed.strip_prefix('/').unwrap_or(trimmed);
        let (name, args) = match payload.split_once(' ') {
            Some((n, a)) => (n, a.trim()),
            None => (payload, ""),
        };
        Self::from_parts(name, args)
    }

    pub(crate) fn from_parts(name: &str, args: &str) -> Self {
        match name {
            "exit" | "quit" | "q" => Self::action(Lifecycle::Quit),
            "clear" => Self::action(ViewAction::ClearMessages),
            "summarize" => Self::action(ViewAction::Command("compact".into())),
            "resume" | "continue" if args.is_empty() => Self::action(ViewAction::OpenSessionPicker),
            "toggle-thinking" => Self::action(ViewAction::Command("thinking".into())),
            "themes" => Self::action(ViewAction::OpenThemePicker),
            "theme" if args.is_empty() => Self::action(ViewAction::OpenThemePicker),
            "theme" => Self::action(ViewAction::SetTheme(args.to_string())),
            "dark" => Self::action(ViewAction::SetThemeMode(ThemeMode::Dark)),
            "light" => Self::action(ViewAction::SetThemeMode(ThemeMode::Light)),
            "agents" if args.is_empty() => Self::action(ViewAction::OpenAgentPicker),
            "agents" => Self::action(ViewAction::SetAgent(args.to_string())),
            "models" => Self::action(ViewAction::OpenModelPicker),
            "model" if args.is_empty() => Self::action(ViewAction::OpenModelPicker),
            "model" => match ModelRef::parse(args) {
                Some(m) => Self::action(ViewAction::SetModel(m)),
                None => Self::InvalidArg {
                    slash: "model",
                    detail: format!("model must be provider/id (got '{args}')"),
                },
            },
            "variants" => Self::action(ViewAction::OpenVariantPicker),
            "variant" if args.is_empty() => Self::action(ViewAction::OpenVariantPicker),
            "variant" => Self::action(ViewAction::SetVariant(Some(args.to_string()))),
            "sidebar" => Self::action(ViewAction::ToggleSidebar),
            "sessions" if args.is_empty() => Self::action(ViewAction::OpenSessionPicker),
            "sessions" => Self::action(ViewAction::SwitchSession(args.to_string())),
            "rename" if args.is_empty() => Self::action(ViewAction::OpenSessionRename(None)),
            "timestamps" | "toggle-timestamps" => Self::action(ViewAction::ToggleTimestamps),
            "copy" => Self::action(ViewAction::CopySessionTranscript),
            "export" => Self::action(ViewAction::ExportSession),
            "help" => Self::action(ViewAction::OpenHelp),
            "fork" => Self::action(ViewAction::OpenForkPicker),
            _ => Self::Unknown {
                name: name.to_string(),
                args: args.to_string(),
            },
        }
    }
}

#[allow(dead_code)]
fn _force_imports(a: HostAction, b: UserAction) {
    let _ = (a, b);
}
