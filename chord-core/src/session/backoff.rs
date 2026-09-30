//! The delay between two login attempts (RFC 6120 has no rule, but a busy loop hurts the
//! server).
//!
//! The delay starts at 1 s and doubles up to 30 s. Each wait gets random jitter of up to 50
//! percent, so that many clients that lost one server do not come back at the same time.
//! A server can accept the login and then drop the stream. So a stream that lived less than
//! `STABLE_STREAM` does not reset the delay: the next login waits, and the delay keeps
//! growing.

use core::time::Duration;

use tokio::time::Instant;

/// First and largest delay between two login attempts. Same values as `new_c2s`.
pub(super) const FIRST_DELAY: Duration = Duration::from_secs(1);
pub(super) const MAX_DELAY: Duration = Duration::from_secs(30);

/// A stream that lived this long was a good one: the delay starts again at `FIRST_DELAY`.
pub(super) const STABLE_STREAM: Duration = Duration::from_secs(30);

/// The state that the login loops of one session share.
#[derive(Debug)]
pub(super) struct Backoff {
    /// The delay for the next wait, if the last stream was short.
    delay: Duration,
    /// When the last login succeeded.
    connected_at: Option<Instant>,
    /// Add jitter to each wait. Tests with exact times turn it off.
    jitter: bool,
}

impl Backoff {
    pub fn new() -> Self {
        Self {
            delay: FIRST_DELAY,
            connected_at: None,
            jitter: true,
        }
    }

    /// No jitter, for tests that check exact times.
    #[cfg(test)]
    pub fn exact() -> Self {
        Self {
            jitter: false,
            ..Self::new()
        }
    }

    /// A login loop starts. Returns the first delay of the loop, and whether the loop must
    /// wait before its first attempt (the last stream was short).
    pub fn start(&self) -> (Duration, bool) {
        match self.connected_at {
            Some(at) if at.elapsed() < STABLE_STREAM => (self.delay, true),
            _ => (FIRST_DELAY, false),
        }
    }

    /// A login succeeded. `next` is the delay that a new loop uses if this stream is short.
    pub fn connected(&mut self, next: Duration) {
        self.delay = next;
        self.connected_at = Some(Instant::now());
    }

    /// The time to wait for `delay`, with jitter.
    pub fn wait(&self, delay: Duration) -> Duration {
        if self.jitter {
            with_jitter(delay, random_unit())
        } else {
            delay
        }
    }
}

/// The delay after `delay`.
pub(super) fn next_delay(delay: Duration) -> Duration {
    (delay * 2).min(MAX_DELAY)
}

/// `delay` plus up to 50 percent. `unit` is in `0.0..1.0`.
fn with_jitter(delay: Duration, unit: f64) -> Duration {
    delay + delay.mul_f64(0.5 * unit.clamp(0.0, 1.0))
}

/// A random number in `0.0..1.0`. A UUID v4 is a source of random bits that the crate has.
fn random_unit() -> f64 {
    let bytes = uuid::Uuid::new_v4().into_bytes();
    let n = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    f64::from(n) / (f64::from(u32::MAX) + 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_adds_up_to_half() {
        let d = Duration::from_secs(10);
        assert_eq!(with_jitter(d, 0.0), d);
        assert_eq!(with_jitter(d, 0.5), Duration::from_millis(12_500));
        assert_eq!(with_jitter(d, 1.0), Duration::from_secs(15));
        assert_eq!(with_jitter(d, 7.0), Duration::from_secs(15), "clamped");
    }

    #[test]
    fn random_waits_stay_in_range() {
        let backoff = Backoff::new();
        let d = Duration::from_secs(4);
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..50 {
            let w = backoff.wait(d);
            assert!(w >= d && w <= d + d / 2, "{w:?}");
            seen.insert(w);
        }
        assert!(seen.len() > 1, "the wait is not always the same");
    }

    #[test]
    fn delays_double_up_to_the_limit() {
        let mut d = FIRST_DELAY;
        let mut all = vec![d.as_secs()];
        for _ in 0..6 {
            d = next_delay(d);
            all.push(d.as_secs());
        }
        assert_eq!(all, [1, 2, 4, 8, 16, 30, 30]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_short_stream_keeps_the_delay_and_a_long_one_resets_it() {
        let mut b = Backoff::exact();
        assert_eq!(b.start(), (FIRST_DELAY, false), "no stream yet");
        b.connected(Duration::from_secs(8));
        tokio::time::advance(Duration::from_secs(5)).await;
        assert_eq!(b.start(), (Duration::from_secs(8), true), "short stream");
        tokio::time::advance(Duration::from_secs(30)).await;
        assert_eq!(b.start(), (FIRST_DELAY, false), "long stream");
    }
}
