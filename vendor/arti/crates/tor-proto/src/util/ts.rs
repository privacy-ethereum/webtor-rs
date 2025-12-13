//! Implement a fast 'timestamp' for determining when an event last
//! happened.

use std::sync::atomic::{AtomicU64, Ordering};

/// An object for determining whether an event happened,
/// and if yes, when it happened.
///
/// Every `Timestamp` has internal mutability.  A timestamp can move
/// forward in time, but never backwards.
///
/// Internally, it uses the `coarsetime` crate to represent times in a way
/// that lets us do atomic updates. On WASM, it uses web_time with millisecond
/// precision since coarsetime doesn't support WASM.
#[derive(Default, Debug)]
pub(crate) struct AtomicOptTimestamp {
    /// A timestamp describing when this timestamp was last updated.
    ///
    /// On native: coarsetime ticks
    /// On WASM: milliseconds since some epoch (from performance.now())
    latest: AtomicU64,
}

#[cfg(not(target_arch = "wasm32"))]
impl AtomicOptTimestamp {
    /// Construct a new timestamp that has never been updated.
    pub(crate) const fn new() -> Self {
        AtomicOptTimestamp {
            latest: AtomicU64::new(0),
        }
    }

    /// Update this timestamp to (at least) the current time.
    pub(crate) fn update(&self) {
        self.update_to(coarsetime::Instant::now());
    }

    /// If the timestamp is currently unset, then set it to the current time.
    /// Otherwise leave it alone.
    pub(crate) fn update_if_none(&self) {
        let now = coarsetime::Instant::now().as_ticks();

        let _ignore = self
            .latest
            .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    }

    /// Clear the timestamp and make it not updated again.
    pub(crate) fn clear(&self) {
        self.latest.store(0, Ordering::Relaxed);
    }

    /// Return the time since `update` was last called.
    ///
    /// Return `None` if update was never called.
    pub(crate) fn time_since_update(&self) -> Option<coarsetime::Duration> {
        self.time_since_update_at(coarsetime::Instant::now())
    }

    /// Return the time between the time when `update` was last
    /// called, and the time `now`.
    ///
    /// Return `None` if `update` was never called, or `now` is before
    /// that time.
    #[inline]
    pub(crate) fn time_since_update_at(
        &self,
        now: coarsetime::Instant,
    ) -> Option<coarsetime::Duration> {
        let earlier = self.latest.load(Ordering::Relaxed);
        let now = now.as_ticks();
        if now >= earlier && earlier != 0 {
            Some(coarsetime::Duration::from_ticks(now - earlier))
        } else {
            None
        }
    }

    /// Update this timestamp to (at least) the time `now`.
    #[inline]
    pub(crate) fn update_to(&self, now: coarsetime::Instant) {
        self.latest.fetch_max(now.as_ticks(), Ordering::Relaxed);
    }
}

// WASM implementation using web_time instead of coarsetime
#[cfg(target_arch = "wasm32")]
impl AtomicOptTimestamp {
    /// Construct a new timestamp that has never been updated.
    pub(crate) const fn new() -> Self {
        AtomicOptTimestamp {
            latest: AtomicU64::new(0),
        }
    }

    /// Get current time as milliseconds (using web_time which uses performance.now())
    fn now_ms() -> u64 {
        use web_time::Instant;
        // web_time::Instant doesn't expose raw time, so we use elapsed from a base
        // We store milliseconds since the first call (lazy static would be complex)
        // Instead, use js_sys::Date::now() for absolute time
        js_sys::Date::now() as u64
    }

    /// Update this timestamp to (at least) the current time.
    pub(crate) fn update(&self) {
        let now = Self::now_ms();
        self.latest.fetch_max(now, Ordering::Relaxed);
    }

    /// If the timestamp is currently unset, then set it to the current time.
    /// Otherwise leave it alone.
    pub(crate) fn update_if_none(&self) {
        let now = Self::now_ms();
        let _ignore = self
            .latest
            .compare_exchange(0, now, Ordering::Relaxed, Ordering::Relaxed);
    }

    /// Clear the timestamp and make it not updated again.
    pub(crate) fn clear(&self) {
        self.latest.store(0, Ordering::Relaxed);
    }

    /// Return the time since `update` was last called.
    ///
    /// Return `None` if update was never called.
    pub(crate) fn time_since_update(&self) -> Option<std::time::Duration> {
        let earlier = self.latest.load(Ordering::Relaxed);
        if earlier == 0 {
            return None;
        }
        let now = Self::now_ms();
        if now >= earlier {
            Some(std::time::Duration::from_millis(now - earlier))
        } else {
            None
        }
    }
}

#[cfg(not(miri))]
#[cfg(test)]
mod test {
    // @@ begin test lint list maintained by maint/add_warning @@
    #![allow(clippy::bool_assert_comparison)]
    #![allow(clippy::clone_on_copy)]
    #![allow(clippy::dbg_macro)]
    #![allow(clippy::mixed_attributes_style)]
    #![allow(clippy::print_stderr)]
    #![allow(clippy::print_stdout)]
    #![allow(clippy::single_char_pattern)]
    #![allow(clippy::unwrap_used)]
    #![allow(clippy::unchecked_time_subtraction)]
    #![allow(clippy::useless_vec)]
    #![allow(clippy::needless_pass_by_value)]
    //! <!-- @@ end test lint list maintained by maint/add_warning @@ -->

    use super::*;

    #[test]
    fn opt_timestamp() {
        use coarsetime::{Duration, Instant};

        let ts = AtomicOptTimestamp::new();
        assert!(ts.time_since_update().is_none());

        let zero = Duration::from_secs(0);
        let one_sec = Duration::from_secs(1);

        let first = Instant::now();
        let in_a_bit = first + one_sec * 10;
        let even_later = first + one_sec * 25;

        assert!(ts.time_since_update_at(first).is_none());

        ts.update_to(first);
        assert_eq!(ts.time_since_update_at(first), Some(zero));
        assert!(ts.time_since_update_at(in_a_bit) >= Some(one_sec * 10));

        ts.update_to(in_a_bit);
        assert!(ts.time_since_update_at(first).is_none()); // Clock ran backwards.
        assert!(ts.time_since_update_at(in_a_bit) <= Some(zero));
        assert!(ts.time_since_update_at(even_later) >= Some(one_sec * 15));

        ts.clear();
        assert!(ts.time_since_update_at(even_later).is_none());

        ts.update_if_none();
        assert!(ts.time_since_update().is_some());
        ts.update_if_none();
        assert!(ts.time_since_update().is_some());
    }
}
