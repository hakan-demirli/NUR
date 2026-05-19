use ratatui::widgets::ListState;

pub struct ScrollState {
    pub list_state: ListState,
    pub scroll_stick_to_bottom: bool,
    pub total_visual_lines: usize,
    pub last_messages_viewport_rows: usize,
}

impl ScrollState {
    pub fn new() -> Self {
        Self {
            list_state: ListState::default(),
            scroll_stick_to_bottom: true,
            total_visual_lines: 0,
            last_messages_viewport_rows: 0,
        }
    }

    pub fn stick_to_bottom(&mut self) {
        self.scroll_stick_to_bottom = true;
    }

    pub fn scroll_messages(&mut self, amount: isize) {
        let max_offset = self
            .total_visual_lines
            .saturating_sub(self.last_messages_viewport_rows.max(1));
        let cur = self.list_state.offset() as isize;
        let new = (cur + amount).clamp(0, max_offset as isize) as usize;
        self.list_state = ListState::default().with_offset(new);
        self.scroll_stick_to_bottom = new >= max_offset;
    }

    pub fn on_resize(&mut self, _cols: u16, _rows: u16) {
        self.scroll_stick_to_bottom = true;
    }

    pub fn on_mouse_scroll(&mut self, lines: i32) {
        let delta = -(lines as isize);
        self.scroll_messages(delta);
    }

    pub fn reset(&mut self) {
        self.list_state.select(None);
        self.scroll_stick_to_bottom = true;
    }
}

impl Default for ScrollState {
    fn default() -> Self {
        Self::new()
    }
}
