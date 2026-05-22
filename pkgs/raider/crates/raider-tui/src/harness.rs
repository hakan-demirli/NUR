use ratatui::{backend::TestBackend, Terminal};

use crate::action::Action;
use crate::app::{App, FixedClock};
use crate::event::Event;
use crate::ui::ui;

pub struct Harness {
    pub app: App,
    pub terminal: Terminal<TestBackend>,
    captured_events: Vec<Event>,
}

impl Harness {
    pub fn new(width: u16, height: u16) -> Self {
        unsafe {
            std::env::set_var("RAIDER_STATE_DIR", "");
        }
        let terminal =
            Terminal::new(TestBackend::new(width, height)).expect("create test terminal");
        let mut app = App::with_clock(Box::new(FixedClock("00:00".to_string())));
        app.sidebar.set_visible(false);
        Self {
            app,
            terminal,
            captured_events: Vec::new(),
        }
    }

    pub fn dispatch(&mut self, action: Action) {
        self.app.dispatch(action);
        self.captured_events.extend(self.app.take_events());
        self.draw();
    }

    pub fn dispatch_all<I: IntoIterator<Item = Action>>(&mut self, actions: I) {
        for a in actions {
            self.dispatch(a);
        }
    }

    pub fn draw(&mut self) {
        let app = &mut self.app;
        self.terminal
            .draw(|f| ui(f, app))
            .expect("draw to test backend");
    }

    pub fn events(&self) -> &[Event] {
        &self.captured_events
    }

    pub fn clear_events(&mut self) {
        self.captured_events.clear();
    }

    pub fn snapshot(&self) -> String {
        let buf = self.terminal.backend().buffer();
        let mut out =
            String::with_capacity((buf.area.width as usize + 1) * buf.area.height as usize);
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                out.push_str(buf[(x, y)].symbol());
            }
            out.push('\n');
        }
        out
    }

    pub fn row_backgrounds(&self, y: u16) -> Vec<ratatui::style::Color> {
        let buf = self.terminal.backend().buffer();
        let mut out = Vec::with_capacity(buf.area.width as usize);
        for x in 0..buf.area.width {
            out.push(
                buf[(x, y)]
                    .style()
                    .bg
                    .unwrap_or(ratatui::style::Color::Reset),
            );
        }
        out
    }

    pub fn prompt_tray_row(&self) -> Option<u16> {
        let buf = self.terminal.backend().buffer();
        for y in 0..buf.area.height {
            for x in 0..buf.area.width {
                let s = buf[(x, y)].symbol();
                if s == "╹" {
                    return Some(y);
                }
                if s != " " && !s.is_empty() {
                    break;
                }
            }
        }
        None
    }
}
