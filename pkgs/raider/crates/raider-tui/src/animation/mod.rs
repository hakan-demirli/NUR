use std::time::{Duration, Instant};

pub const SPINNER_FRAME: Duration = Duration::from_millis(80);
pub const IDLE_POLL: Duration = Duration::from_millis(250);

#[derive(Copy, Clone, Debug, Default)]
pub struct AnimationDemand {
    pub streaming: bool,
    pub tool_running: bool,
    pub toast_active: bool,
    pub retry_pending: bool,
}

impl AnimationDemand {
    pub fn any(&self) -> bool {
        self.streaming || self.tool_running || self.toast_active || self.retry_pending
    }

    pub fn next_wake(&self) -> Option<Instant> {
        if self.any() {
            Some(Instant::now() + SPINNER_FRAME)
        } else {
            None
        }
    }

    pub fn idle_poll_or_animation(&self) -> Duration {
        if self.any() {
            SPINNER_FRAME
        } else {
            IDLE_POLL
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_demand_means_no_animation() {
        let d = AnimationDemand::default();
        assert!(!d.any());
        assert!(d.next_wake().is_none());
        assert_eq!(d.idle_poll_or_animation(), IDLE_POLL);
    }

    #[test]
    fn any_field_demands_animation() {
        let d = AnimationDemand {
            streaming: true,
            ..Default::default()
        };
        assert!(d.any());
        assert_eq!(d.idle_poll_or_animation(), SPINNER_FRAME);

        let d = AnimationDemand {
            tool_running: true,
            ..Default::default()
        };
        assert!(d.any());

        let d = AnimationDemand {
            toast_active: true,
            ..Default::default()
        };
        assert!(d.any());

        let d = AnimationDemand {
            retry_pending: true,
            ..Default::default()
        };
        assert!(d.any());
    }

    #[test]
    fn next_wake_is_within_spinner_frame_when_animating() {
        let d = AnimationDemand {
            streaming: true,
            ..Default::default()
        };
        let now = Instant::now();
        let wake = d.next_wake().unwrap();
        assert!(wake > now);
        assert!(wake <= now + SPINNER_FRAME + Duration::from_millis(2));
    }
}
