//! Portable Instant type that works on both WASM and native platforms.
//!
//! On WASM, `std::time::Instant` panics when created, so we provide a
//! platform-specific implementation using `web_sys::Performance`.

use std::ops::{Add, AddAssign, Sub, SubAssign};
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::*;

    /// A portable instant in time, wrapping `std::time::Instant` on native platforms.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant(std::time::Instant);

    impl Instant {
        /// Returns the current instant.
        pub fn now() -> Self {
            Instant(std::time::Instant::now())
        }

        /// Returns the duration since this instant was created.
        pub fn elapsed(&self) -> Duration {
            self.0.elapsed()
        }

        /// Returns the duration from `earlier` to `self`.
        pub fn duration_since(&self, earlier: Instant) -> Duration {
            self.0.duration_since(earlier.0)
        }

        /// Returns the duration from `earlier` to `self`, or zero if `earlier` is later.
        pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
            self.0.saturating_duration_since(earlier.0)
        }

        /// Returns `Some(self + duration)` or `None` if overflow would occur.
        pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
            self.0.checked_add(duration).map(Instant)
        }

        /// Returns `Some(self - duration)` or `None` if underflow would occur.
        pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
            self.0.checked_sub(duration).map(Instant)
        }

        /// Returns `Some(duration)` from `earlier` to `self`, or `None` if `earlier` is later.
        pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
            self.0.checked_duration_since(earlier.0)
        }
    }

    impl Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, other: Duration) -> Instant {
            Instant(self.0 + other)
        }
    }

    impl Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, other: Duration) -> Instant {
            Instant(self.0 - other)
        }
    }

    impl Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, other: Instant) -> Duration {
            self.0 - other.0
        }
    }

    impl AddAssign<Duration> for Instant {
        fn add_assign(&mut self, other: Duration) {
            self.0 = self.0 + other;
        }
    }

    impl SubAssign<Duration> for Instant {
        fn sub_assign(&mut self, other: Duration) {
            self.0 = self.0 - other;
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::*;

    fn get_performance_now_ms() -> f64 {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    /// A portable instant in time, using `web_sys::Performance` on WASM.
    #[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
    pub struct Instant(f64); // milliseconds since page load

    impl Eq for Instant {}

    impl std::hash::Hash for Instant {
        fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
            self.0.to_bits().hash(state);
        }
    }

    impl Ord for Instant {
        fn cmp(&self, other: &Self) -> std::cmp::Ordering {
            self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
        }
    }

    impl Instant {
        /// Returns the current instant.
        pub fn now() -> Self {
            Instant(get_performance_now_ms())
        }

        /// Returns the duration since this instant was created.
        pub fn elapsed(&self) -> Duration {
            let now = get_performance_now_ms();
            Duration::from_secs_f64((now - self.0).max(0.0) / 1000.0)
        }

        /// Returns the duration from `earlier` to `self`.
        pub fn duration_since(&self, earlier: Instant) -> Duration {
            Duration::from_secs_f64((self.0 - earlier.0).max(0.0) / 1000.0)
        }

        /// Returns the duration from `earlier` to `self`, or zero if `earlier` is later.
        pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
            Duration::from_secs_f64((self.0 - earlier.0).max(0.0) / 1000.0)
        }

        /// Returns `Some(self + duration)` or `None` if overflow would occur.
        pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
            let ms = self.0 + duration.as_secs_f64() * 1000.0;
            if ms.is_finite() {
                Some(Instant(ms))
            } else {
                None
            }
        }

        /// Returns `Some(self - duration)` or `None` if underflow would occur.
        pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
            let ms = self.0 - duration.as_secs_f64() * 1000.0;
            if ms >= 0.0 && ms.is_finite() {
                Some(Instant(ms))
            } else {
                None
            }
        }

        /// Returns `Some(duration)` from `earlier` to `self`, or `None` if `earlier` is later.
        pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
            let diff = self.0 - earlier.0;
            if diff >= 0.0 {
                Some(Duration::from_secs_f64(diff / 1000.0))
            } else {
                None
            }
        }
    }

    impl Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, other: Duration) -> Instant {
            Instant(self.0 + other.as_secs_f64() * 1000.0)
        }
    }

    impl Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, other: Duration) -> Instant {
            Instant((self.0 - other.as_secs_f64() * 1000.0).max(0.0))
        }
    }

    impl Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, other: Instant) -> Duration {
            Duration::from_secs_f64((self.0 - other.0).max(0.0) / 1000.0)
        }
    }

    impl AddAssign<Duration> for Instant {
        fn add_assign(&mut self, other: Duration) {
            self.0 += other.as_secs_f64() * 1000.0;
        }
    }

    impl SubAssign<Duration> for Instant {
        fn sub_assign(&mut self, other: Duration) {
            self.0 = (self.0 - other.as_secs_f64() * 1000.0).max(0.0);
        }
    }
}

pub use platform::Instant;
