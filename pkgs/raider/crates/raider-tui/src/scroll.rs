use std::time::{Duration, Instant};

const A: f64 = 0.8;
const TAU: f64 = 3.0;
const MAX_MULTIPLIER: f64 = 6.0;
const HISTORY_SIZE: usize = 3;
const STREAK_TIMEOUT: Duration = Duration::from_millis(150);
const MIN_TICK_INTERVAL: Duration = Duration::from_millis(6);
const REFERENCE_INTERVAL_MS: f64 = 100.0;

#[derive(Debug, Default)]
pub struct ScrollAccel {
    last_tick: Option<Instant>,
    history_ms: Vec<f64>,
}

impl ScrollAccel {
    pub fn new() -> Self {
        Self {
            last_tick: None,
            history_ms: Vec::with_capacity(HISTORY_SIZE),
        }
    }

    pub fn tick(&mut self) -> f64 {
        self.tick_at(Instant::now())
    }

    pub fn tick_at(&mut self, now: Instant) -> f64 {
        let dt = match self.last_tick {
            None => {
                self.last_tick = Some(now);
                self.history_ms.clear();
                return 1.0;
            }
            Some(prev) => now.saturating_duration_since(prev),
        };

        if dt > STREAK_TIMEOUT {
            self.last_tick = Some(now);
            self.history_ms.clear();
            return 1.0;
        }

        if dt < MIN_TICK_INTERVAL {
            return 1.0;
        }

        self.last_tick = Some(now);
        self.history_ms.push(dt.as_secs_f64() * 1000.0);
        if self.history_ms.len() > HISTORY_SIZE {
            self.history_ms.remove(0);
        }

        let avg: f64 = self.history_ms.iter().sum::<f64>() / self.history_ms.len() as f64;
        if avg <= 0.0 {
            return 1.0;
        }
        let velocity = REFERENCE_INTERVAL_MS / avg;
        let multiplier = 1.0 + A * ((velocity / TAU).exp() - 1.0);
        multiplier.clamp(1.0, MAX_MULTIPLIER)
    }

    pub fn reset(&mut self) {
        self.last_tick = None;
        self.history_ms.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn first_tick_returns_unit_multiplier() {
        let mut a = ScrollAccel::new();
        assert_eq!(a.tick_at(Instant::now()), 1.0);
    }

    #[test]
    fn idle_between_bursts_resets_streak() {
        let mut a = ScrollAccel::new();
        let t0 = Instant::now();
        a.tick_at(t0);
        let t1 = t0 + Duration::from_millis(200);
        assert_eq!(a.tick_at(t1), 1.0);
    }

    #[test]
    fn sub_tick_coalescing_returns_unit() {
        let mut a = ScrollAccel::new();
        let t0 = Instant::now();
        a.tick_at(t0);
        let t1 = t0 + Duration::from_millis(4);
        assert_eq!(a.tick_at(t1), 1.0);
    }

    #[test]
    fn rapid_bursts_accelerate_above_unit() {
        let mut a = ScrollAccel::new();
        let mut t = Instant::now();
        a.tick_at(t);
        let mut last_mul = 1.0;
        for _ in 0..6 {
            t += Duration::from_millis(20);
            last_mul = a.tick_at(t);
        }
        assert!(
            last_mul > 2.0,
            "expected multiplier > 2.0 under burst; got {last_mul}"
        );
        assert!(last_mul <= MAX_MULTIPLIER + 1e-9);
    }

    #[test]
    fn very_fast_bursts_clamp_at_max_multiplier() {
        let mut a = ScrollAccel::new();
        let mut t = Instant::now();
        a.tick_at(t);
        for _ in 0..20 {
            t += Duration::from_millis(7);
            a.tick_at(t);
        }
        assert!((a.tick_at(t + Duration::from_millis(7)) - MAX_MULTIPLIER).abs() < 1e-6);
    }
}
