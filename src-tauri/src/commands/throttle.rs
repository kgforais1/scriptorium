//! Rate limit for progress events, so a fast ingest does not flood the webview.

use std::time::{Duration, Instant};

/// Least time between two `ingest-progress` events.
pub const EMIT_INTERVAL: Duration = Duration::from_millis(100);

/// Decides which progress ticks to emit: the first, one per interval, and always the last.
#[derive(Debug)]
pub struct EmitThrottle {
    interval: Duration,
    last: Option<Instant>,
}

impl EmitThrottle {
    /// A throttle that lets one tick through per `interval`.
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last: None,
        }
    }

    /// True when the tick `(done, total)` at `now` should be emitted. The final tick
    /// (`done >= total`) always is.
    pub fn should_emit(&mut self, done: u32, total: u32, now: Instant) -> bool {
        let due = done >= total
            || self
                .last
                .is_none_or(|last| now.saturating_duration_since(last) >= self.interval);
        if due {
            self.last = Some(now);
        }
        due
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_the_rate_and_always_emits_the_last_tick() {
        let t0 = Instant::now();
        let ms = Duration::from_millis;
        let mut t = EmitThrottle::new(ms(100));
        assert!(t.should_emit(1, 100, t0), "first tick");
        assert!(!t.should_emit(2, 100, t0 + ms(10)));
        assert!(!t.should_emit(3, 100, t0 + ms(99)));
        assert!(t.should_emit(4, 100, t0 + ms(100)));
        assert!(!t.should_emit(5, 100, t0 + ms(150)));
        assert!(
            t.should_emit(100, 100, t0 + ms(151)),
            "final tick is never dropped"
        );
    }

    #[test]
    fn a_burst_emits_about_ten_per_second() {
        let t0 = Instant::now();
        let mut t = EmitThrottle::new(EMIT_INTERVAL);
        // 1000 ticks over one second, 1 ms apart.
        let emitted = (1..=1000u32)
            .filter(|&i| t.should_emit(i, 1000, t0 + Duration::from_millis(u64::from(i))))
            .count();
        assert!((10..=12).contains(&emitted), "{emitted}");
    }
}
