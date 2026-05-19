use std::collections::HashMap;

use crate::sidebar::{SidebarBody, SidebarSection, SidebarState};

pub struct SidebarUiState {
    pub sidebar: SidebarState,

    pub sidebar_collapsed_preferences: HashMap<u32, bool>,

    pub total_sidebar_content_lines: usize,
    pub sidebar_body_height: u16,
    pub last_sidebar_rect: Option<ratatui::layout::Rect>,
    pub sidebar_header_rects: Vec<(u32, ratatui::layout::Rect)>,
}

impl SidebarUiState {
    pub fn new() -> Self {
        Self {
            sidebar: SidebarState::default(),
            sidebar_collapsed_preferences: HashMap::new(),
            total_sidebar_content_lines: 0,
            sidebar_body_height: 0,
            last_sidebar_rect: None,
            sidebar_header_rects: Vec::new(),
        }
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        self.sidebar.title = title.into();
    }

    pub fn set_subtitle(&mut self, subtitle: Option<String>) {
        self.sidebar.subtitle = subtitle;
    }

    pub fn set_sections(&mut self, mut sections: Vec<SidebarSection>) {
        for section in &mut sections {
            if let Some(collapsed) = self.sidebar_collapsed_preferences.get(&section.order) {
                Self::apply_collapsed(section, *collapsed);
            }
        }
        self.sidebar.sections = sections;
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.sidebar.visible = visible;
    }

    pub fn toggle_visible(&mut self) {
        self.sidebar.visible = !self.sidebar.visible;
    }

    pub fn set_footer(&mut self, footer: impl Into<String>) {
        self.sidebar.footer = footer.into();
    }

    pub fn set_footer_path(&mut self, path: Option<String>) {
        self.sidebar.footer_path = path;
    }

    pub fn set_workspace_cwd(&mut self, cwd: Option<String>) {
        self.sidebar.footer_cwd = cwd;
    }

    pub fn set_vcs_branch(&mut self, branch: Option<String>) {
        self.sidebar.footer_branch = branch;
    }

    pub fn recompose_workspace_footer(&mut self) -> Option<String> {
        let cwd = self.sidebar.footer_cwd.clone();
        let branch = self.sidebar.footer_branch.clone();
        let composed = match (cwd, branch) {
            (Some(c), Some(b)) if !c.is_empty() && !b.is_empty() => Some(format!("{c}:{b}")),
            (Some(c), _) if !c.is_empty() => Some(c),
            _ => None,
        };
        self.sidebar.footer_path = composed.clone();
        composed
    }

    pub fn scroll_sidebar(&mut self, delta: isize) {
        let max_offset = self
            .total_sidebar_content_lines
            .saturating_sub(self.sidebar_body_height as usize);
        let cur = self.sidebar.scroll_offset as isize;
        let new = (cur + delta).clamp(0, max_offset as isize);
        self.sidebar.scroll_offset = new as usize;
    }

    pub fn toggle_section(&mut self, slot: u32) {
        for section in self.sidebar.sections.iter_mut() {
            if section.order != slot {
                continue;
            }
            let collapsed = match &mut section.body {
                SidebarBody::Files { collapsed, .. }
                | SidebarBody::Todos { collapsed, .. }
                | SidebarBody::Mcps { collapsed, .. }
                | SidebarBody::Lsps { collapsed, .. } => {
                    *collapsed = !*collapsed;
                    Some(*collapsed)
                }
                SidebarBody::Lines(_) => None,
            };
            if let Some(collapsed) = collapsed {
                self.sidebar_collapsed_preferences.insert(slot, collapsed);
            }
            return;
        }
    }

    fn apply_collapsed(section: &mut SidebarSection, collapsed: bool) {
        match &mut section.body {
            SidebarBody::Files { collapsed: c, .. }
            | SidebarBody::Todos { collapsed: c, .. }
            | SidebarBody::Mcps { collapsed: c, .. }
            | SidebarBody::Lsps { collapsed: c, .. } => *c = collapsed,
            SidebarBody::Lines(_) => {}
        }
    }
}

impl Default for SidebarUiState {
    fn default() -> Self {
        Self::new()
    }
}
