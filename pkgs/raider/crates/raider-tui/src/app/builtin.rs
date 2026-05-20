use crate::action::{Action, UserAction, ViewAction};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PromptInfo {
    pub right: Option<String>,
    pub usage: Option<String>,
    pub build_label: Option<String>,
    pub busy: bool,
}

#[derive(Clone, Debug)]
pub struct Agent {
    pub name: String,
    pub title: String,
}

impl Agent {
    pub fn new(name: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            title: title.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmptyAgentsError;

impl std::fmt::Display for EmptyAgentsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("agents list must be non-empty")
    }
}

impl std::error::Error for EmptyAgentsError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct AgentIndex(usize);

impl AgentIndex {
    pub fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug)]
pub struct Agents {
    agents: Vec<Agent>,
    cursor: AgentIndex,
}

impl Agents {
    pub fn new(first: Agent, rest: impl IntoIterator<Item = Agent>) -> Self {
        let mut agents = Vec::with_capacity(1);
        agents.push(first);
        agents.extend(rest);
        Self {
            agents,
            cursor: AgentIndex(0),
        }
    }

    pub fn try_from_vec(agents: Vec<Agent>) -> Result<Self, EmptyAgentsError> {
        if agents.is_empty() {
            return Err(EmptyAgentsError);
        }
        Ok(Self {
            agents,
            cursor: AgentIndex(0),
        })
    }

    pub fn current(&self) -> &Agent {
        &self.agents[self.cursor.0]
    }

    pub fn current_index(&self) -> AgentIndex {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.agents.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Agent> {
        self.agents.iter()
    }

    pub fn as_slice(&self) -> &[Agent] {
        &self.agents
    }

    pub fn cycle(&mut self, delta: i32) -> Option<&Agent> {
        let len = self.agents.len() as i32;
        let next = ((self.cursor.0 as i32 + delta).rem_euclid(len)) as usize;
        if next == self.cursor.0 {
            return None;
        }
        self.cursor = AgentIndex(next);
        Some(&self.agents[self.cursor.0])
    }

    pub fn try_replace(&mut self, new_agents: Vec<Agent>) -> Result<(), EmptyAgentsError> {
        if new_agents.is_empty() {
            return Err(EmptyAgentsError);
        }
        let prior_name = self.current().name.clone();
        let next_cursor = new_agents
            .iter()
            .position(|a| a.name == prior_name)
            .unwrap_or(0);
        self.agents = new_agents;
        self.cursor = AgentIndex(next_cursor);
        Ok(())
    }
}

#[derive(Clone, Debug)]
pub struct Command {
    pub name: String,
    pub title: String,
    pub slash_name: Option<String>,
    pub action: Action,
}

fn cmd(name: &str) -> Action {
    Action::View(ViewAction::Command(name.into()))
}

fn view(action: ViewAction) -> Action {
    Action::View(action)
}

fn user(action: UserAction) -> Action {
    Action::User(action)
}

pub fn builtin_commands() -> Vec<Command> {
    vec![
        Command {
            name: "session.new".into(),
            title: "New session".into(),
            slash_name: Some("new".into()),
            action: cmd("new"),
        },
        Command {
            name: "session.list".into(),
            title: "Switch session".into(),
            slash_name: Some("sessions".into()),
            action: view(ViewAction::OpenSessionPicker),
        },
        Command {
            name: "session.share".into(),
            title: "Share session".into(),
            slash_name: Some("share".into()),
            action: cmd("share"),
        },
        Command {
            name: "session.unshare".into(),
            title: "Unshare session".into(),
            slash_name: Some("unshare".into()),
            action: cmd("unshare"),
        },
        Command {
            name: "session.rename".into(),
            title: "Rename session".into(),
            slash_name: Some("rename".into()),
            action: view(ViewAction::OpenSessionRename(None)),
        },
        Command {
            name: "session.fork".into(),
            title: "Fork session".into(),
            slash_name: Some("fork".into()),
            action: view(ViewAction::OpenForkPicker),
        },
        Command {
            name: "session.compact".into(),
            title: "Compact session".into(),
            slash_name: Some("compact".into()),
            action: cmd("compact"),
        },
        Command {
            name: "session.undo".into(),
            title: "Undo previous message".into(),
            slash_name: Some("undo".into()),
            action: cmd("undo"),
        },
        Command {
            name: "session.redo".into(),
            title: "Redo".into(),
            slash_name: Some("redo".into()),
            action: cmd("redo"),
        },
        Command {
            name: "session.copy".into(),
            title: "Copy session transcript".into(),
            slash_name: Some("copy".into()),
            action: view(ViewAction::CopySessionTranscript),
        },
        Command {
            name: "messages.copy".into(),
            title: "Copy last assistant message".into(),
            slash_name: None,
            action: view(ViewAction::CopyLastAssistantMessage),
        },
        Command {
            name: "session.export".into(),
            title: "Export session transcript".into(),
            slash_name: Some("export".into()),
            action: view(ViewAction::ExportSession),
        },
        Command {
            name: "model.list".into(),
            title: "Switch model".into(),
            slash_name: Some("models".into()),
            action: view(ViewAction::OpenModelPicker),
        },
        Command {
            name: "model.cycle_recent".into(),
            title: "Model cycle".into(),
            slash_name: None,
            action: view(ViewAction::CycleModelRecent(1)),
        },
        Command {
            name: "model.cycle_recent_reverse".into(),
            title: "Model cycle reverse".into(),
            slash_name: None,
            action: view(ViewAction::CycleModelRecent(-1)),
        },
        Command {
            name: "model.cycle_favorite".into(),
            title: "Favorite cycle".into(),
            slash_name: None,
            action: view(ViewAction::CycleModelFavorite(1)),
        },
        Command {
            name: "model.cycle_favorite_reverse".into(),
            title: "Favorite cycle reverse".into(),
            slash_name: None,
            action: view(ViewAction::CycleModelFavorite(-1)),
        },
        Command {
            name: "agent.list".into(),
            title: "Switch agent".into(),
            slash_name: Some("agents".into()),
            action: view(ViewAction::OpenAgentPicker),
        },
        Command {
            name: "agent.cycle".into(),
            title: "Agent cycle".into(),
            slash_name: None,
            action: view(ViewAction::CycleAgent(1)),
        },
        Command {
            name: "agent.cycle.reverse".into(),
            title: "Agent cycle reverse".into(),
            slash_name: None,
            action: view(ViewAction::CycleAgent(-1)),
        },
        Command {
            name: "variant.list".into(),
            title: "Switch model variant".into(),
            slash_name: Some("variants".into()),
            action: view(ViewAction::OpenVariantPicker),
        },
        Command {
            name: "variant.cycle".into(),
            title: "Variant cycle".into(),
            slash_name: None,
            action: view(ViewAction::CycleVariant),
        },
        Command {
            name: "mcp.list".into(),
            title: "Toggle MCPs".into(),
            slash_name: Some("mcps".into()),
            action: cmd("mcps"),
        },
        Command {
            name: "provider.connect".into(),
            title: "Connect provider".into(),
            slash_name: Some("connect".into()),
            action: cmd("connect"),
        },
        Command {
            name: "prompt.editor".into(),
            title: "Open editor".into(),
            slash_name: Some("editor".into()),
            action: cmd("editor"),
        },
        Command {
            name: "command.init".into(),
            title: "guided AGENTS.md setup".into(),
            slash_name: Some("init".into()),
            action: cmd("init"),
        },
        Command {
            name: "command.review".into(),
            title: "Review changes".into(),
            slash_name: Some("review".into()),
            action: cmd("review"),
        },
        Command {
            name: "session.toggle.thinking".into(),
            title: "Toggle thinking".into(),
            slash_name: Some("thinking".into()),
            action: cmd("thinking"),
        },
        Command {
            name: "session.toggle.timestamps".into(),
            title: "Toggle timestamps".into(),
            slash_name: Some("timestamps".into()),
            action: view(ViewAction::ToggleTimestamps),
        },
        Command {
            name: "opencode.status".into(),
            title: "View status".into(),
            slash_name: Some("status".into()),
            action: cmd("status"),
        },
        Command {
            name: "theme.switch".into(),
            title: "Switch theme".into(),
            slash_name: Some("themes".into()),
            action: view(ViewAction::OpenThemePicker),
        },
        Command {
            name: "theme.switch_mode".into(),
            title: "Toggle dark/light mode".into(),
            slash_name: None,
            action: view(ViewAction::ToggleThemeMode),
        },
        Command {
            name: "docs.open".into(),
            title: "Open docs".into(),
            slash_name: None,
            action: view(ViewAction::OpenDocs),
        },
        Command {
            name: "help.show".into(),
            title: "Help".into(),
            slash_name: Some("help".into()),
            action: view(ViewAction::OpenHelp),
        },
        Command {
            name: "app.exit".into(),
            title: "Exit the app".into(),
            slash_name: Some("exit".into()),
            action: Action::Lifecycle(crate::action::Lifecycle::Quit),
        },
        Command {
            name: "session.clear".into(),
            title: "Clear chat".into(),
            slash_name: None,
            action: view(ViewAction::ClearMessages),
        },
        Command {
            name: "session.sidebar.toggle".into(),
            title: "Toggle sidebar".into(),
            slash_name: None,
            action: view(ViewAction::ToggleSidebar),
        },
        Command {
            name: "session.interrupt".into(),
            title: "Interrupt session".into(),
            slash_name: None,
            action: user(UserAction::Interrupt),
        },
        Command {
            name: "prompt.clear".into(),
            title: "Clear prompt".into(),
            slash_name: None,
            action: user(UserAction::ClearInput),
        },
    ]
}

pub fn default_agents() -> Agents {
    Agents::new(Agent::new("build", "Build"), [Agent::new("plan", "Plan")])
}
