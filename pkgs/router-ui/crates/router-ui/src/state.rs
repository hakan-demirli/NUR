use std::time::{Duration, Instant};

use router_auth::User;

use crate::ipc::WifiKind;

#[derive(Debug, Clone)]
pub(crate) enum Screen {
    Blank,
    Screensaver,
    Lockscreen,
    PinEntry {
        user: User,
        digits: String,
        message: String,
        locked_until: Option<Instant>,
    },
    AdminMenu {
        user: User,
    },
    GuestMenu {
        user: User,
    },
    Fan {
        user: User,
    },
    Wifi {
        user: User,
        kind: WifiKind,
    },
}

impl Screen {
    #[must_use]
    pub(crate) fn on_wake(&self) -> Self {
        match self {
            Self::Blank => Self::Screensaver,
            _ => self.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IdlePolicy {
    pub(crate) timeout: Duration,
}

impl Default for IdlePolicy {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct IdleTracker {
    policy: IdlePolicy,
    last_touch: Instant,
}

impl IdleTracker {
    pub(crate) fn new(policy: IdlePolicy) -> Self {
        Self {
            policy,
            last_touch: Instant::now(),
        }
    }
    pub(crate) fn touch(&mut self, now: Instant) {
        self.last_touch = now;
    }
    pub(crate) fn should_blank(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.last_touch) >= self.policy.timeout
    }
}
