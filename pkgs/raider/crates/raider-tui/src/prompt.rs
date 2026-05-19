#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PermissionPrompt {
    pub id: String,
    pub session_id: String,
    pub permission: String,
    pub patterns: Vec<String>,
    pub metadata: serde_json::Map<String, serde_json::Value>,
    pub always: Vec<String>,
    pub view: PermissionView,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PermissionView {
    pub icon: String,
    pub title: String,
    pub detail: Vec<String>,
}

impl PermissionPrompt {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        permission: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            session_id: session_id.into(),
            permission: permission.into(),
            patterns: Vec::new(),
            metadata: serde_json::Map::new(),
            always: Vec::new(),
            view: PermissionView::default(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct QuestionPrompt {
    pub id: String,
    pub session_id: String,
    pub questions: Vec<QuestionInfo>,
    pub tab: usize,
    pub selected: usize,
    pub answers: Vec<Vec<String>>,
    pub custom: Vec<String>,
    pub editing: bool,
    pub edit_buffer: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct QuestionInfo {
    pub question: String,
    pub header: String,
    pub options: Vec<QuestionOption>,
    pub multiple: bool,
    pub custom_allowed: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct QuestionOption {
    pub label: String,
    pub description: String,
}

impl QuestionPrompt {
    pub fn new(
        id: impl Into<String>,
        session_id: impl Into<String>,
        questions: Vec<QuestionInfo>,
    ) -> Self {
        let len = questions.len();
        Self {
            id: id.into(),
            session_id: session_id.into(),
            questions,
            tab: 0,
            selected: 0,
            answers: vec![Vec::new(); len],
            custom: vec![String::new(); len],
            editing: false,
            edit_buffer: String::new(),
        }
    }

    pub fn is_single(&self) -> bool {
        self.questions.len() == 1 && !self.questions[0].multiple
    }

    pub fn confirm_tab(&self) -> usize {
        self.questions.len()
    }

    pub fn tab_count(&self) -> usize {
        if self.is_single() {
            1
        } else {
            self.questions.len() + 1
        }
    }

    pub fn on_confirm(&self) -> bool {
        !self.is_single() && self.tab == self.confirm_tab()
    }

    pub fn current(&self) -> Option<&QuestionInfo> {
        self.questions.get(self.tab)
    }

    pub fn current_row_count(&self) -> usize {
        match self.current() {
            None => 0,
            Some(q) => q.options.len() + if q.custom_allowed { 1 } else { 0 },
        }
    }

    pub fn on_custom_row(&self) -> bool {
        match self.current() {
            None => false,
            Some(q) => q.custom_allowed && self.selected == q.options.len(),
        }
    }

    pub fn custom_picked(&self) -> bool {
        let value = self.custom.get(self.tab).cloned().unwrap_or_default();
        if value.is_empty() {
            return false;
        }
        self.answers
            .get(self.tab)
            .map(|a| a.contains(&value))
            .unwrap_or(false)
    }

    pub fn option_picked(&self, label: &str) -> bool {
        self.answers
            .get(self.tab)
            .map(|a| a.iter().any(|s| s == label))
            .unwrap_or(false)
    }
}
