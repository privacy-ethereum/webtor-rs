//! Platform-independent time utilities for WASM and native builds.

use std::time::Duration;

#[cfg(target_arch = "wasm32")]
mod platform {
    use super::*;

    fn get_performance_now_ms() -> f64 {
        web_sys::window()
            .and_then(|w| w.performance())
            .map(|p| p.now())
            .unwrap_or(0.0)
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Instant(f64);

    impl Instant {
        pub fn now() -> Self {
            Instant(get_performance_now_ms())
        }

        pub fn elapsed(&self) -> Duration {
            let now = get_performance_now_ms();
            Duration::from_secs_f64((now - self.0) / 1000.0)
        }

        pub fn duration_since(&self, earlier: Instant) -> Duration {
            Duration::from_secs_f64((self.0 - earlier.0).max(0.0) / 1000.0)
        }
    }

    impl std::ops::Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, other: Duration) -> Instant {
            Instant(self.0 + other.as_secs_f64() * 1000.0)
        }
    }

    impl std::ops::Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, other: Duration) -> Instant {
            Instant((self.0 - other.as_secs_f64() * 1000.0).max(0.0))
        }
    }

    pub fn system_time_now() -> std::time::SystemTime {
        let ms = js_sys::Date::now();
        std::time::UNIX_EPOCH + Duration::from_millis(ms as u64)
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod platform {
    use super::*;

    #[derive(Clone, Copy, Debug)]
    pub struct Instant(std::time::Instant);

    impl Instant {
        pub fn now() -> Self {
            Instant(std::time::Instant::now())
        }

        pub fn elapsed(&self) -> Duration {
            self.0.elapsed()
        }

        pub fn duration_since(&self, earlier: Instant) -> Duration {
            self.0.duration_since(earlier.0)
        }
    }

    impl std::ops::Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, other: Duration) -> Instant {
            Instant(self.0 + other)
        }
    }

    impl std::ops::Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, other: Duration) -> Instant {
            Instant(self.0 - other)
        }
    }

    pub fn system_time_now() -> std::time::SystemTime {
        std::time::SystemTime::now()
    }
}

pub use platform::system_time_now;
pub use platform::Instant;
