#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct FileChange {
    pub file: String,
    pub additions: u64,
    pub deletions: u64,
}

impl FileChange {
    pub fn new(file: impl Into<String>, additions: u64, deletions: u64) -> Self {
        Self {
            file: file.into(),
            additions,
            deletions,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct TodoEntry {
    pub content: String,
    pub status: String,
}

impl TodoEntry {
    pub fn new(content: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            status: status.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LspEntry {
    pub id: String,
    pub root: String,
    pub status: String,
}

impl LspEntry {
    pub fn new(id: impl Into<String>, root: impl Into<String>, status: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            root: root.into(),
            status: status.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct McpEntry {
    pub name: String,
    pub status: String,
    pub error: String,
}

impl McpEntry {
    pub fn new(
        name: impl Into<String>,
        status: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            name: name.into(),
            status: status.into(),
            error: error.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarBody {
    Lines(Vec<String>),
    Files {
        entries: Vec<FileChange>,
        collapsed: bool,
    },
    Todos {
        entries: Vec<TodoEntry>,
        collapsed: bool,
    },
    Mcps {
        entries: Vec<McpEntry>,
        collapsed: bool,
    },
    Lsps {
        entries: Vec<LspEntry>,
        placeholder: String,
        collapsed: bool,
    },
}

impl Default for SidebarBody {
    fn default() -> Self {
        Self::Lines(Vec::new())
    }
}

pub mod slot {
    pub const CONTEXT: u32 = 100;
    pub const MCP: u32 = 200;
    pub const LSP: u32 = 300;
    pub const TODO: u32 = 400;
    pub const MODIFIED_FILES: u32 = 500;
    pub const DEFAULT: u32 = 10_000;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarSection {
    pub title: String,
    pub body: SidebarBody,
    pub order: u32,
}

impl Default for SidebarSection {
    fn default() -> Self {
        Self {
            title: String::new(),
            body: SidebarBody::default(),
            order: slot::DEFAULT,
        }
    }
}

impl SidebarSection {
    pub fn new(
        title: impl Into<String>,
        lines: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            title: title.into(),
            body: SidebarBody::Lines(lines.into_iter().map(Into::into).collect()),
            order: slot::DEFAULT,
        }
    }

    pub fn files(title: impl Into<String>, entries: Vec<FileChange>) -> Self {
        Self {
            title: title.into(),
            body: SidebarBody::Files {
                entries,
                collapsed: false,
            },
            order: slot::DEFAULT,
        }
    }

    pub fn todos(title: impl Into<String>, entries: Vec<TodoEntry>) -> Self {
        Self {
            title: title.into(),
            body: SidebarBody::Todos {
                entries,
                collapsed: false,
            },
            order: slot::DEFAULT,
        }
    }

    pub fn mcps(title: impl Into<String>, entries: Vec<McpEntry>) -> Self {
        Self {
            title: title.into(),
            body: SidebarBody::Mcps {
                entries,
                collapsed: false,
            },
            order: slot::DEFAULT,
        }
    }

    pub fn lsps(
        title: impl Into<String>,
        entries: Vec<LspEntry>,
        placeholder: impl Into<String>,
    ) -> Self {
        Self {
            title: title.into(),
            body: SidebarBody::Lsps {
                entries,
                placeholder: placeholder.into(),
                collapsed: false,
            },
            order: slot::DEFAULT,
        }
    }

    pub fn with_order(mut self, order: u32) -> Self {
        self.order = order;
        self
    }

    pub fn lines(&self) -> &[String] {
        match &self.body {
            SidebarBody::Lines(v) => v,
            _ => &[],
        }
    }

    pub fn files_entries(&self) -> &[FileChange] {
        match &self.body {
            SidebarBody::Files { entries, .. } => entries,
            _ => &[],
        }
    }

    pub fn todo_entries(&self) -> &[TodoEntry] {
        match &self.body {
            SidebarBody::Todos { entries, .. } => entries,
            _ => &[],
        }
    }

    pub fn mcp_entries(&self) -> &[McpEntry] {
        match &self.body {
            SidebarBody::Mcps { entries, .. } => entries,
            _ => &[],
        }
    }

    pub fn lsp_entries(&self) -> &[LspEntry] {
        match &self.body {
            SidebarBody::Lsps { entries, .. } => entries,
            _ => &[],
        }
    }
}

#[derive(Clone, Debug)]
pub struct SidebarState {
    pub visible: bool,
    pub width: u16,
    pub title: String,
    pub subtitle: Option<String>,
    pub sections: Vec<SidebarSection>,
    pub footer: String,
    pub footer_cwd: Option<String>,

    pub footer_branch: Option<String>,

    pub footer_path: Option<String>,
    pub scroll_offset: usize,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            visible: false,
            width: 42,
            title: "Session".to_string(),
            subtitle: None,
            sections: Vec::new(),
            footer: format!("raider v{}", env!("CARGO_PKG_VERSION")),
            footer_path: None,
            footer_cwd: None,
            footer_branch: None,
            scroll_offset: 0,
        }
    }
}
