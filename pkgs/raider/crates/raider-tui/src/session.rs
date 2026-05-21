#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SessionStatus {
    #[default]
    Idle,
    Busy,
    Retry {
        attempt: Option<u32>,
        message: Option<String>,
        next: Option<i64>,
    },
}

impl SessionStatus {
    pub fn is_working(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    pub fn is_retry(&self) -> bool {
        matches!(self, Self::Retry { .. })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionEntry {
    pub id: String,
    pub title: String,
    pub updated_label: String,
    pub fork_label: Option<String>,
    pub busy: bool,
    pub status: SessionStatus,
    pub parent_id: Option<String>,
    pub created_ms: Option<i64>,
}

impl SessionEntry {
    pub fn new(
        id: impl Into<String>,
        title: impl Into<String>,
        updated_label: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            title: title.into(),
            updated_label: updated_label.into(),
            fork_label: None,
            busy: false,
            status: SessionStatus::Idle,
            parent_id: None,
            created_ms: None,
        }
    }

    pub fn with_fork(mut self, fork_label: impl Into<String>) -> Self {
        self.fork_label = Some(fork_label.into());
        self
    }

    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        let p = parent_id.into();
        if !p.is_empty() {
            self.parent_id = Some(p);
        }
        self
    }

    pub fn with_created_ms(mut self, created_ms: i64) -> Self {
        self.created_ms = Some(created_ms);
        self
    }

    pub fn display_title(&self) -> String {
        match &self.fork_label {
            Some(f) => format!("{} {}", self.title, f),
            None => self.title.clone(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct SessionList {
    pub entries: Vec<SessionEntry>,
    pub current: Option<String>,
}

impl SessionList {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, id: &str) -> Option<&SessionEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    pub fn current_parent_id(&self) -> Option<&str> {
        let cur = self.current.as_deref()?;
        self.get(cur).and_then(|e| e.parent_id.as_deref())
    }

    pub fn current_is_child(&self) -> bool {
        self.current_parent_id().is_some()
    }

    pub fn children_of(&self, parent_id: &str) -> Vec<&SessionEntry> {
        let mut out: Vec<&SessionEntry> = self
            .entries
            .iter()
            .filter(|e| e.parent_id.as_deref() == Some(parent_id))
            .collect();
        out.sort_by(|a, b| {
            a.created_ms
                .unwrap_or(i64::MAX)
                .cmp(&b.created_ms.unwrap_or(i64::MAX))
                .then_with(|| a.id.cmp(&b.id))
        });
        out
    }

    pub fn child_index(&self, parent_id: &str, child_id: &str) -> Option<usize> {
        self.children_of(parent_id)
            .iter()
            .position(|e| e.id == child_id)
    }
}
