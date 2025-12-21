//! Retry, timeout, and cancellation utilities for transient failures

use crate::error::{Result, TorError};
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, warn};

/// A cross-platform cancellation token for cooperative task cancellation.
///
/// On native, this wraps `tokio_util::sync::CancellationToken`.
/// On WASM, this uses an `Arc<AtomicBool>` with polling.
#[derive(Clone)]
pub struct CancellationToken {
    #[cfg(not(target_arch = "wasm32"))]
    inner: tokio_util::sync::CancellationToken,
    #[cfg(target_arch = "wasm32")]
    cancelled: Arc<AtomicBool>,
}

impl Default for CancellationToken {
    fn default() -> Self {
        Self::new()
    }
}

impl CancellationToken {
    /// Create a new cancellation token.
    pub fn new() -> Self {
        #[cfg(not(target_arch = "wasm32"))]
        {
            Self {
                inner: tokio_util::sync::CancellationToken::new(),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            Self {
                cancelled: Arc::new(AtomicBool::new(false)),
            }
        }
    }

    /// Cancel all operations using this token.
    pub fn cancel(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner.cancel();
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.cancelled.store(true, Ordering::SeqCst);
        }
    }

    /// Check if cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner.is_cancelled()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.cancelled.load(Ordering::SeqCst)
        }
    }

    /// Returns a future that completes when cancellation is requested.
    ///
    /// On native, this uses tokio_util's efficient notification.
    /// On WASM, this polls with a small interval.
    pub async fn cancelled(&self) {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.inner.cancelled().await;
        }
        #[cfg(target_arch = "wasm32")]
        {
            use gloo_timers::future::TimeoutFuture;
            while !self.is_cancelled() {
                TimeoutFuture::new(50).await;
            }
        }
    }
}

impl std::fmt::Debug for CancellationToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationToken")
            .field("is_cancelled", &self.is_cancelled())
            .finish()
    }
}

/// Configuration for retry behavior with exponential backoff
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    /// Maximum number of attempts (including the first one)
    pub max_attempts: u32,
    /// Initial delay between retries
    pub initial_delay: Duration,
    /// Maximum delay between retries (caps exponential growth)
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (typically 2.0)
    pub backoff_factor: f64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
        }
    }
}

impl RetryPolicy {
    /// Create a new retry policy with the given max attempts
    pub fn new(max_attempts: u32) -> Self {
        Self {
            max_attempts,
            ..Default::default()
        }
    }

    /// Policy for network operations (Snowflake, WebSocket connections)
    pub fn network() -> Self {
        Self {
            max_attempts: 5,
            initial_delay: Duration::from_millis(2000),
            max_delay: Duration::from_secs(30),
            backoff_factor: 1.5,
        }
    }

    /// Policy for circuit operations (circuit creation, extension)
    pub fn circuit() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(500),
            max_delay: Duration::from_secs(10),
            backoff_factor: 2.0,
        }
    }

    /// Policy for bootstrap operations (consensus fetch)
    pub fn bootstrap() -> Self {
        Self {
            max_attempts: 3,
            initial_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(15),
            backoff_factor: 2.0,
        }
    }

    /// Set max attempts
    pub fn with_max_attempts(mut self, max_attempts: u32) -> Self {
        self.max_attempts = max_attempts;
        self
    }

    /// Set initial delay
    pub fn with_initial_delay(mut self, delay: Duration) -> Self {
        self.initial_delay = delay;
        self
    }

    /// Set max delay
    pub fn with_max_delay(mut self, delay: Duration) -> Self {
        self.max_delay = delay;
        self
    }

    /// Calculate delay for a given attempt (1-indexed)
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt <= 1 {
            return Duration::ZERO;
        }
        let multiplier = self.backoff_factor.powi((attempt - 2) as i32);
        let delay_ms = (self.initial_delay.as_millis() as f64 * multiplier) as u64;
        Duration::from_millis(delay_ms).min(self.max_delay)
    }
}

/// Execute an async operation with retry and exponential backoff
///
/// # Arguments
/// * `operation_name` - Name for logging purposes
/// * `policy` - Retry policy configuration (max_attempts must be >= 1)
/// * `is_retryable` - Function to determine if an error is retryable
/// * `operation` - The async operation to execute, receives attempt number (1-indexed)
///
/// # Panics
/// Debug-asserts if `policy.max_attempts == 0`
///
/// # Example
/// ```ignore
/// retry_with_backoff(
///     "fetch_consensus",
///     RetryPolicy::bootstrap(),
///     |e| e.is_retryable(),
///     |attempt| async move {
///         fetch_consensus().await
///     },
/// ).await
/// ```
pub async fn retry_with_backoff<F, Fut, T>(
    operation_name: &str,
    policy: RetryPolicy,
    is_retryable: impl Fn(&TorError) -> bool,
    mut operation: F,
) -> Result<T>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<T>>,
{
    debug_assert!(
        policy.max_attempts > 0,
        "max_attempts must be >= 1"
    );
    if policy.max_attempts == 0 {
        return Err(TorError::Internal(format!(
            "{}: invalid retry policy (max_attempts = 0)",
            operation_name
        )));
    }

    for attempt in 1..=policy.max_attempts {
        let delay = policy.delay_for_attempt(attempt);
        if !delay.is_zero() {
            debug!(
                "{}: waiting {:?} before attempt {}/{}",
                operation_name, delay, attempt, policy.max_attempts
            );
            sleep(delay).await;
        }

        debug!(
            "{}: attempt {}/{}",
            operation_name, attempt, policy.max_attempts
        );

        match operation(attempt).await {
            Ok(result) => {
                if attempt > 1 {
                    debug!("{}: succeeded on attempt {}", operation_name, attempt);
                }
                return Ok(result);
            }
            Err(e) => {
                let can_retry = attempt < policy.max_attempts && is_retryable(&e);

                if can_retry {
                    warn!(
                        "{}: attempt {}/{} failed (retryable): {}",
                        operation_name, attempt, policy.max_attempts, e
                    );
                } else {
                    if attempt == policy.max_attempts && is_retryable(&e) {
                        warn!(
                            "{}: all {} attempts exhausted, last error: {}",
                            operation_name, policy.max_attempts, e
                        );
                    }
                    return Err(e);
                }
            }
        }
    }

    unreachable!("retry loop should always return from inside the loop")
}

/// Platform-agnostic sleep function
pub async fn sleep(duration: Duration) {
    #[cfg(target_arch = "wasm32")]
    {
        let ms = duration.as_millis().min(u32::MAX as u128) as u32;
        gloo_timers::future::TimeoutFuture::new(ms).await;
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::time::sleep(duration).await;
    }
}

/// Execute a future with a timeout, returning TorError::Timeout on expiry
///
/// # Arguments
/// * `duration` - Maximum time to wait
/// * `operation_name` - Name for error message
/// * `future` - The async operation to execute
///
/// # Example
/// ```ignore
/// with_timeout(
///     Duration::from_secs(30),
///     "establish_channel",
///     establish_channel_impl()
/// ).await
/// ```
pub async fn with_timeout<F, T>(
    duration: Duration,
    operation_name: &str,
    future: F,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    #[cfg(target_arch = "wasm32")]
    {
        use futures::future::{select, Either};
        use std::pin::pin;

        let ms = duration.as_millis().min(u32::MAX as u128) as u32;
        let timeout_fut = gloo_timers::future::TimeoutFuture::new(ms);
        let operation_fut = pin!(future);
        let timeout_fut = pin!(timeout_fut);

        match select(operation_fut, timeout_fut).await {
            Either::Left((result, _)) => result,
            Either::Right((_, _)) => Err(TorError::timeout(format!(
                "{} timed out after {:?}",
                operation_name, duration
            ))),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        match tokio::time::timeout(duration, future).await {
            Ok(result) => result,
            Err(_) => Err(TorError::timeout(format!(
                "{} timed out after {:?}",
                operation_name, duration
            ))),
        }
    }
}

/// Execute a future with cancellation support, returning TorError::Cancelled if cancelled
///
/// # Arguments
/// * `token` - The cancellation token to monitor
/// * `future` - The async operation to execute
///
/// # Example
/// ```ignore
/// with_cancellation(
///     &self.shutdown_token,
///     establish_channel_impl()
/// ).await
/// ```
pub async fn with_cancellation<F, T>(token: &CancellationToken, future: F) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    if token.is_cancelled() {
        return Err(TorError::Cancelled);
    }

    #[cfg(target_arch = "wasm32")]
    {
        use futures::future::{select, Either};
        use std::pin::pin;

        let operation_fut = pin!(future);
        let cancel_fut = pin!(token.cancelled());

        match select(operation_fut, cancel_fut).await {
            Either::Left((result, _)) => result,
            Either::Right((_, _)) => Err(TorError::Cancelled),
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::select! {
            result = future => result,
            _ = token.cancelled() => Err(TorError::Cancelled),
        }
    }
}

/// Execute a future with both timeout and cancellation support
///
/// Returns `TorError::Cancelled` if cancelled, `TorError::Timeout` if timed out.
pub async fn with_timeout_and_cancellation<F, T>(
    duration: Duration,
    operation_name: &str,
    token: &CancellationToken,
    future: F,
) -> Result<T>
where
    F: Future<Output = Result<T>>,
{
    if token.is_cancelled() {
        return Err(TorError::Cancelled);
    }

    #[cfg(target_arch = "wasm32")]
    {
        use futures::future::{select, Either};
        use std::pin::pin;

        let ms = duration.as_millis().min(u32::MAX as u128) as u32;
        let timeout_fut = gloo_timers::future::TimeoutFuture::new(ms);
        let operation_fut = pin!(future);
        let timeout_fut = pin!(timeout_fut);
        let cancel_fut = pin!(token.cancelled());

        let timeout_or_cancel = pin!(async {
            match select(timeout_fut, cancel_fut).await {
                Either::Left(_) => false,
                Either::Right(_) => true,
            }
        });

        match select(operation_fut, timeout_or_cancel).await {
            Either::Left((result, _)) => result,
            Either::Right((was_cancelled, _)) => {
                if was_cancelled {
                    Err(TorError::Cancelled)
                } else {
                    Err(TorError::timeout(format!(
                        "{} timed out after {:?}",
                        operation_name, duration
                    )))
                }
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        tokio::select! {
            result = tokio::time::timeout(duration, future) => {
                match result {
                    Ok(r) => r,
                    Err(_) => Err(TorError::timeout(format!(
                        "{} timed out after {:?}",
                        operation_name, duration
                    ))),
                }
            }
            _ = token.cancelled() => Err(TorError::Cancelled),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    #[test]
    fn delay_calculation_is_correct() {
        let policy = RetryPolicy {
            max_attempts: 5,
            initial_delay: Duration::from_millis(1000),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
        };

        assert_eq!(policy.delay_for_attempt(1), Duration::ZERO);
        assert_eq!(policy.delay_for_attempt(2), Duration::from_millis(1000));
        assert_eq!(policy.delay_for_attempt(3), Duration::from_millis(2000));
        assert_eq!(policy.delay_for_attempt(4), Duration::from_millis(4000));
        assert_eq!(policy.delay_for_attempt(5), Duration::from_millis(8000));
    }

    #[test]
    fn delay_respects_max_delay() {
        let policy = RetryPolicy {
            max_attempts: 10,
            initial_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
        };

        assert_eq!(policy.delay_for_attempt(5), Duration::from_secs(30));
        assert_eq!(policy.delay_for_attempt(10), Duration::from_secs(30));
    }

    #[test]
    fn preset_policies_are_reasonable() {
        let network = RetryPolicy::network();
        assert_eq!(network.max_attempts, 5);
        assert!(network.initial_delay >= Duration::from_millis(1000));

        let circuit = RetryPolicy::circuit();
        assert_eq!(circuit.max_attempts, 3);
        assert!(circuit.initial_delay >= Duration::from_millis(100));

        let bootstrap = RetryPolicy::bootstrap();
        assert_eq!(bootstrap.max_attempts, 3);
    }

    #[tokio::test]
    async fn retry_succeeds_on_first_attempt() {
        let result = retry_with_backoff(
            "test_op",
            RetryPolicy::new(3),
            |_| true,
            |_attempt| async { Ok::<_, TorError>(42) },
        )
        .await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn retry_succeeds_after_failures() {
        let attempts = AtomicU32::new(0);

        let result = retry_with_backoff(
            "test_op",
            RetryPolicy::new(3).with_initial_delay(Duration::from_millis(1)),
            |_| true,
            |_attempt| {
                let count = attempts.fetch_add(1, AtomicOrdering::SeqCst) + 1;
                async move {
                    if count < 3 {
                        Err(TorError::network("transient failure"))
                    } else {
                        Ok(count)
                    }
                }
            },
        )
        .await;

        assert_eq!(result.unwrap(), 3);
        assert_eq!(attempts.load(AtomicOrdering::SeqCst), 3);
    }

    #[tokio::test]
    async fn retry_fails_on_non_retryable_error() {
        let attempts = AtomicU32::new(0);

        let result = retry_with_backoff(
            "test_op",
            RetryPolicy::new(5).with_initial_delay(Duration::from_millis(1)),
            |e| e.is_retryable(),
            |_attempt| {
                attempts.fetch_add(1, AtomicOrdering::SeqCst);
                async { Err::<u32, _>(TorError::configuration("bad config")) }
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(AtomicOrdering::SeqCst), 1);
    }

    #[tokio::test]
    async fn retry_exhausts_all_attempts() {
        let attempts = AtomicU32::new(0);

        let result = retry_with_backoff(
            "test_op",
            RetryPolicy::new(3).with_initial_delay(Duration::from_millis(1)),
            |_| true,
            |_attempt| {
                attempts.fetch_add(1, AtomicOrdering::SeqCst);
                async { Err::<u32, _>(TorError::network("always fails")) }
            },
        )
        .await;

        assert!(result.is_err());
        assert_eq!(attempts.load(AtomicOrdering::SeqCst), 3);
    }

    #[test]
    fn zero_attempts_policy_has_zero_max_attempts() {
        let policy = RetryPolicy::new(0);
        assert_eq!(policy.max_attempts, 0);
    }

    #[tokio::test]
    async fn timeout_succeeds_when_operation_completes_in_time() {
        let result = with_timeout(
            Duration::from_secs(5),
            "test_op",
            async { Ok::<_, TorError>(42) },
        )
        .await;

        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn timeout_fails_when_operation_exceeds_time() {
        let result = with_timeout(
            Duration::from_millis(10),
            "slow_op",
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, TorError>(42)
            },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("timed out"));
        assert!(err.to_string().contains("slow_op"));
    }

    #[tokio::test]
    async fn timeout_propagates_inner_error() {
        let result = with_timeout(
            Duration::from_secs(5),
            "test_op",
            async { Err::<u32, _>(TorError::network("inner error")) },
        )
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("inner error"));
    }

    #[test]
    fn cancellation_token_starts_uncancelled() {
        let token = CancellationToken::new();
        assert!(!token.is_cancelled());
    }

    #[test]
    fn cancellation_token_cancel_sets_flag() {
        let token = CancellationToken::new();
        token.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn cancellation_token_clone_shares_state() {
        let token1 = CancellationToken::new();
        let token2 = token1.clone();
        assert!(!token1.is_cancelled());
        assert!(!token2.is_cancelled());
        token1.cancel();
        assert!(token1.is_cancelled());
        assert!(token2.is_cancelled());
    }

    #[tokio::test]
    async fn with_cancellation_succeeds_when_not_cancelled() {
        let token = CancellationToken::new();
        let result = with_cancellation(&token, async { Ok::<_, TorError>(42) }).await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn with_cancellation_fails_when_already_cancelled() {
        let token = CancellationToken::new();
        token.cancel();
        let result = with_cancellation(&token, async { Ok::<_, TorError>(42) }).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TorError::Cancelled));
    }

    #[tokio::test]
    async fn with_cancellation_cancels_during_operation() {
        let token = CancellationToken::new();
        let token_clone = token.clone();

        let result = tokio::select! {
            r = with_cancellation(&token, async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok::<_, TorError>(42)
            }) => r,
            _ = async {
                tokio::time::sleep(Duration::from_millis(10)).await;
                token_clone.cancel();
            } => Err(TorError::Cancelled),
        };

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TorError::Cancelled));
    }

    #[tokio::test]
    async fn with_timeout_and_cancellation_succeeds() {
        let token = CancellationToken::new();
        let result =
            with_timeout_and_cancellation(Duration::from_secs(5), "test", &token, async {
                Ok::<_, TorError>(42)
            })
            .await;
        assert_eq!(result.unwrap(), 42);
    }

    #[tokio::test]
    async fn with_timeout_and_cancellation_times_out() {
        let token = CancellationToken::new();
        let result = with_timeout_and_cancellation(
            Duration::from_millis(10),
            "slow_op",
            &token,
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, TorError>(42)
            },
        )
        .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TorError::Timeout(_)));
    }

    #[tokio::test]
    async fn with_timeout_and_cancellation_cancels() {
        let token = CancellationToken::new();
        token.cancel();
        let result =
            with_timeout_and_cancellation(Duration::from_secs(5), "test", &token, async {
                Ok::<_, TorError>(42)
            })
            .await;

        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), TorError::Cancelled));
    }
}
