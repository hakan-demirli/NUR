use std::collections::HashMap;
use std::sync::Arc;

use ratatui::prelude::Line;

use crate::sidebar::{SidebarBody, SidebarSection, SidebarState};
use crate::state::Version;
use crate::ui::theme::Mode as ThemeMode;

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct SidebarCacheKey {
    pub version: Version,
    pub width: u16,
    pub theme_mode: ThemeMode,
}

pub struct SidebarRender {
    pub lines: Vec<Line<'static>>,
    pub header_line_indices: Vec<(u32, usize)>,
}

pub struct SidebarUiState {
    pub sidebar: SidebarState,

    pub sidebar_collapsed_preferences: HashMap<u32, bool>,

    pub total_sidebar_content_lines: usize,
    pub sidebar_body_height: u16,
    pub last_sidebar_rect: Option<ratatui::layout::Rect>,
    pub sidebar_header_rects: Vec<(u32, ratatui::layout::Rect)>,

    version: Version,
    pub render_cache: Option<(SidebarCacheKey, Arc<SidebarRender>)>,
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
            version: Version::default(),
            render_cache: None,
        }
    }

    pub fn version(&self) -> Version {
        self.version
    }

    fn bump_version(&mut self) {
        self.version.bump();
    }

    pub fn set_title(&mut self, title: impl Into<String>) {
        let title = title.into();
        if self.sidebar.title != title {
            self.sidebar.title = title;
            self.bump_version();
        }
    }

    pub fn set_subtitle(&mut self, subtitle: Option<String>) {
        if self.sidebar.subtitle != subtitle {
            self.sidebar.subtitle = subtitle;
            self.bump_version();
        }
    }

    pub fn set_sections(&mut self, mut sections: Vec<SidebarSection>) {
        for section in &mut sections {
            if let Some(collapsed) = self.sidebar_collapsed_preferences.get(&section.order) {
                Self::apply_collapsed(section, *collapsed);
            }
        }
        if self.sidebar.sections != sections {
            self.sidebar.sections = sections;
            self.bump_version();
        }
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.sidebar.visible = visible;
    }

    pub fn toggle_visible(&mut self) {
        self.sidebar.visible = !self.sidebar.visible;
    }

    pub fn set_footer(&mut self, footer: impl Into<String>) {
        let footer = footer.into();
        if self.sidebar.footer != footer {
            self.sidebar.footer = footer;
            self.bump_version();
        }
    }

    pub fn set_footer_path(&mut self, path: Option<String>) {
        if self.sidebar.footer_path != path {
            self.sidebar.footer_path = path;
            self.bump_version();
        }
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
        if self.sidebar.footer_path != composed {
            self.sidebar.footer_path = composed.clone();
            self.bump_version();
        }
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
                self.bump_version();
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sidebar::SidebarSection;

    #[test]
    fn version_bumps_only_on_visual_state_changes() {
        let mut s = SidebarUiState::new();
        let v0 = s.version();

        s.set_title("a");
        let v1 = s.version();
        assert!(v1 > v0, "set_title must bump");

        s.set_title("a");
        assert_eq!(s.version(), v1, "no-op set_title must not bump");

        s.set_subtitle(Some("sub".into()));
        let v2 = s.version();
        assert!(v2 > v1);

        s.set_sections(vec![SidebarSection::new("section", ["a", "b"])]);
        let v3 = s.version();
        assert!(v3 > v2);

        s.scroll_sidebar(2);
        assert_eq!(s.version(), v3, "scroll must not bump version");
    }

    #[test]
    fn toggle_section_bumps_version_only_when_collapsible() {
        let mut s = SidebarUiState::new();
        s.set_sections(vec![SidebarSection::files("Files", vec![]).with_order(500)]);
        let v_before = s.version();
        s.toggle_section(500);
        assert!(s.version() > v_before, "collapsible section toggle bumps");
    }
}
