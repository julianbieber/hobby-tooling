//! A repeating period driven by the time that passed since the caller's last pass.

use std::time::Duration;

/// A repeating period. [`Interval::tick`] is fed the time since the previous pass and answers
/// whether a period completed.
#[derive(Clone, Debug)]
pub struct Interval {
    period: Duration,
    elapsed: Duration,
}

impl Interval {
    /// A period of `period`; when `first_now`, the first tick already completes one.
    pub fn new(period: Duration, first_now: bool) -> Self {
        let mut interval = Self {
            period,
            elapsed: Duration::ZERO,
        };
        if first_now {
            interval.fire_next();
        }
        interval
    }

    /// Add `delta` and answer whether a period completed, starting the next one.
    pub fn tick(&mut self, delta: Duration) -> bool {
        self.elapsed += delta;
        if self.elapsed < self.period {
            return false;
        }
        self.elapsed = Duration::ZERO;
        true
    }

    /// Make the next tick complete a period.
    pub fn fire_next(&mut self) {
        self.elapsed = self.period;
    }
}
