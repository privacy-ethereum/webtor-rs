//! WASM-compatible time utilities that replace coarsetime.
//!
//! On native platforms, this delegates to coarsetime.
//! On WASM, it uses web_sys::Performance for timing.

use std::ops::{Add, Sub};

/// A monotonic instant in time, compatible with both WASM and native.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Instant(u64);

/// A duration of time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Duration(u64);

// One tick = 1 microsecond for compatibility with coarsetime's tick rate
const TICKS_PER_SEC: u64 = 1_000_000;

impl Instant {
    /// Returns the current instant.
    #[cfg(target_arch = "wasm32")]
    pub fn now() -> Self {
        // Use performance.now() which returns milliseconds as f64
        let ms = web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0);
        // Convert to microseconds (ticks)
        Instant((ms * 1000.0) as u64)
    }

    /// Returns the current instant.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn now() -> Self {
        let ct = coarsetime::Instant::now();
        Instant(ct.as_ticks())
    }

    /// Returns this instant as ticks (microseconds).
    #[inline]
    pub fn as_ticks(&self) -> u64 {
        self.0
    }

    /// Creates an Instant from ticks.
    #[inline]
    pub fn from_ticks(ticks: u64) -> Self {
        Instant(ticks)
    }

    /// Returns the duration since an earlier instant.
    pub fn duration_since(&self, earlier: Instant) -> Duration {
        Duration(self.0.saturating_sub(earlier.0))
    }

    /// Returns the duration since an earlier instant, saturating to zero on underflow.
    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        Duration(self.0.saturating_sub(earlier.0))
    }

    /// Returns the time elapsed since this instant.
    pub fn elapsed(&self) -> Duration {
        Self::now().duration_since(*self)
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;
    fn add(self, rhs: Duration) -> Instant {
        Instant(self.0.saturating_add(rhs.0))
    }
}

impl Sub<Duration> for Instant {
    type Output = Instant;
    fn sub(self, rhs: Duration) -> Instant {
        Instant(self.0.saturating_sub(rhs.0))
    }
}

impl Sub<Instant> for Instant {
    type Output = Duration;
    fn sub(self, rhs: Instant) -> Duration {
        Duration(self.0.saturating_sub(rhs.0))
    }
}

impl Add<std::time::Duration> for Instant {
    type Output = Instant;
    fn add(self, rhs: std::time::Duration) -> Instant {
        Instant(self.0.saturating_add(rhs.as_micros() as u64))
    }
}

impl Sub<std::time::Duration> for Instant {
    type Output = Instant;
    fn sub(self, rhs: std::time::Duration) -> Instant {
        Instant(self.0.saturating_sub(rhs.as_micros() as u64))
    }
}

impl Duration {
    /// Creates a zero duration.
    pub const fn new() -> Self {
        Duration(0)
    }

    /// Returns true if this duration spans no time.
    pub const fn is_zero(&self) -> bool {
        self.0 == 0
    }

    /// Creates a duration from seconds.
    pub const fn from_secs(secs: u64) -> Self {
        Duration(secs * TICKS_PER_SEC)
    }

    /// Creates a duration from milliseconds.
    pub const fn from_millis(millis: u64) -> Self {
        Duration(millis * 1000)
    }

    /// Creates a duration from microseconds.
    pub const fn from_micros(micros: u64) -> Self {
        Duration(micros)
    }

    /// Creates a duration from ticks (microseconds).
    #[inline]
    pub const fn from_ticks(ticks: u64) -> Self {
        Duration(ticks)
    }

    /// Returns this duration as ticks (microseconds).
    #[inline]
    pub const fn as_ticks(&self) -> u64 {
        self.0
    }

    /// Returns this duration as microseconds.
    pub const fn as_micros(&self) -> u128 {
        self.0 as u128
    }

    /// Returns this duration as seconds.
    pub const fn as_secs(&self) -> u64 {
        self.0 / TICKS_PER_SEC
    }

    /// Returns this duration as a standard library Duration.
    pub fn as_std_duration(&self) -> std::time::Duration {
        std::time::Duration::from_micros(self.0)
    }
}

impl Default for Duration {
    fn default() -> Self {
        Duration::new()
    }
}

impl From<Duration> for std::time::Duration {
    fn from(d: Duration) -> std::time::Duration {
        std::time::Duration::from_micros(d.0)
    }
}

impl Add for Duration {
    type Output = Duration;
    fn add(self, rhs: Duration) -> Duration {
        Duration(self.0.saturating_add(rhs.0))
    }
}

impl Sub for Duration {
    type Output = Duration;
    fn sub(self, rhs: Duration) -> Duration {
        Duration(self.0.saturating_sub(rhs.0))
    }
}

impl std::ops::Mul<u32> for Duration {
    type Output = Duration;
    fn mul(self, rhs: u32) -> Duration {
        Duration(self.0.saturating_mul(rhs as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_instant_now() {
        let i1 = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let i2 = Instant::now();
        assert!(i2 > i1);
    }

    #[test]
    fn test_duration_from_secs() {
        let d = Duration::from_secs(5);
        assert_eq!(d.as_secs(), 5);
    }
}
