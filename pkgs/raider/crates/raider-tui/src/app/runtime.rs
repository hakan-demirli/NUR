use crate::event::Event;

pub trait Clock: Send + Sync {
    fn now_hhmm(&self) -> String;
}

pub struct SystemClock;
impl Clock for SystemClock {
    fn now_hhmm(&self) -> String {
        chrono::Local::now().format("%H:%M").to_string()
    }
}

pub struct FixedClock(pub String);
impl Clock for FixedClock {
    fn now_hhmm(&self) -> String {
        self.0.clone()
    }
}

pub struct RuntimeState {
    pub events: Vec<Event>,
    pub clock: Box<dyn Clock>,
    pub quit_requested: bool,
    pub leader_armed_at_ms: Option<u128>,
}

impl RuntimeState {
    pub fn new(clock: Box<dyn Clock>) -> Self {
        Self {
            events: Vec::new(),
            clock,
            quit_requested: false,
            leader_armed_at_ms: None,
        }
    }

    pub fn arm_leader(&mut self) {
        self.leader_armed_at_ms = Some(now_epoch_ms());
    }

    pub fn take_leader_armed(&mut self) -> bool {
        self.leader_armed_at_ms.take().is_some()
    }

    pub fn is_leader_armed(&self) -> bool {
        self.leader_armed_at_ms.is_some()
    }

    pub fn now_hhmm(&self) -> String {
        self.clock.now_hhmm()
    }

    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    pub fn should_quit(&self) -> bool {
        self.quit_requested
    }

    pub fn request_quit(&mut self) {
        self.quit_requested = true;
        self.events.push(Event::Quit);
    }
}

fn now_epoch_ms() -> u128 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
