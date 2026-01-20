use std::future::Future;
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

/// Configuration for retry behavior
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of attempts (including the first one)
    pub max_attempts: u32,
    /// Initial delay before the first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff (e.g., 2.0 doubles the delay each time)
    pub backoff_multiplier: f64,
}

impl RetryConfig {
    /// Create a new retry configuration
    pub fn new(max_attempts: u32, initial_delay: Duration) -> Self {
        Self {
            max_attempts,
            initial_delay,
            max_delay: Duration::from_secs(30),
            backoff_multiplier: 2.0,
        }
    }

    /// Set the maximum delay between retries
    pub fn with_max_delay(mut self, max_delay: Duration) -> Self {
        self.max_delay = max_delay;
        self
    }

    /// Set the backoff multiplier
    pub fn with_backoff_multiplier(mut self, multiplier: f64) -> Self {
        self.backoff_multiplier = multiplier;
        self
    }

    /// Preset: Fast retries for health checks (4 attempts, ~7s total)
    /// Delays: 1s, 2s, 4s = 7s total wait time
    pub fn health_check() -> Self {
        Self::new(4, Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(4))
            .with_backoff_multiplier(2.0)
    }

    /// Preset: Standard retries for API calls (3 attempts)
    /// Delays: 1s, 2s = 3s total wait time
    pub fn api_call() -> Self {
        Self::new(3, Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(5))
            .with_backoff_multiplier(2.0)
    }

    /// Preset: RSS feed retries (3 attempts with shorter delays)
    /// Delays: 500ms, 1s = 1.5s total wait time
    pub fn rss_feed() -> Self {
        Self::new(3, Duration::from_millis(500))
            .with_max_delay(Duration::from_secs(2))
            .with_backoff_multiplier(2.0)
    }

    /// Calculate the delay for a given attempt number (0-indexed)
    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        if attempt == 0 {
            return Duration::ZERO;
        }

        let delay_ms = self.initial_delay.as_millis() as f64
            * self.backoff_multiplier.powi((attempt - 1) as i32);

        let delay = Duration::from_millis(delay_ms as u64);
        delay.min(self.max_delay)
    }
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self::api_call()
    }
}

/// Execute an async operation with retries
///
/// # Arguments
/// * `config` - Retry configuration (max_attempts must be >= 1)
/// * `operation_name` - Name of the operation for logging
/// * `operation` - Async closure that returns Result<T, E>
///
/// # Returns
/// The result of the operation, or the last error if all retries failed
///
/// # Panics
/// Panics if `config.max_attempts` is 0
pub async fn with_retry<T, E, F, Fut>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
{
    assert!(
        config.max_attempts >= 1,
        "RetryConfig.max_attempts must be >= 1, got {}",
        config.max_attempts
    );

    let mut last_error: Option<E> = None;

    for attempt in 0..config.max_attempts {
        // Wait before retry (except for first attempt)
        let delay = config.delay_for_attempt(attempt);
        if !delay.is_zero() {
            debug!(
                "{}: Retry attempt {}/{} after {:?}",
                operation_name,
                attempt + 1,
                config.max_attempts,
                delay
            );
            sleep(delay).await;
        }

        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(
                        "{}: Succeeded on attempt {}/{}",
                        operation_name,
                        attempt + 1,
                        config.max_attempts
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                let remaining = config.max_attempts - attempt - 1;
                if remaining > 0 {
                    warn!(
                        "{}: Attempt {}/{} failed ({}), {} retries remaining",
                        operation_name,
                        attempt + 1,
                        config.max_attempts,
                        e,
                        remaining
                    );
                } else {
                    warn!(
                        "{}: All {} attempts failed. Last error: {}",
                        operation_name, config.max_attempts, e
                    );
                }
                last_error = Some(e);
            }
        }
    }

    Err(last_error.expect("At least one attempt should have been made"))
}

/// Execute an async operation with retries, using a predicate to determine if retry is appropriate
///
/// Some errors (like 4xx client errors) should not be retried, while others (5xx, network) should.
///
/// # Panics
/// Panics if `config.max_attempts` is 0
pub async fn with_retry_if<T, E, F, Fut, P>(
    config: &RetryConfig,
    operation_name: &str,
    mut operation: F,
    should_retry: P,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: std::fmt::Display,
    P: Fn(&E) -> bool,
{
    assert!(
        config.max_attempts >= 1,
        "RetryConfig.max_attempts must be >= 1, got {}",
        config.max_attempts
    );

    let mut last_error: Option<E> = None;

    for attempt in 0..config.max_attempts {
        // Wait before retry (except for first attempt)
        let delay = config.delay_for_attempt(attempt);
        if !delay.is_zero() {
            debug!(
                "{}: Retry attempt {}/{} after {:?}",
                operation_name,
                attempt + 1,
                config.max_attempts,
                delay
            );
            sleep(delay).await;
        }

        match operation().await {
            Ok(result) => {
                if attempt > 0 {
                    debug!(
                        "{}: Succeeded on attempt {}/{}",
                        operation_name,
                        attempt + 1,
                        config.max_attempts
                    );
                }
                return Ok(result);
            }
            Err(e) => {
                // Check if we should retry this error
                if !should_retry(&e) {
                    debug!(
                        "{}: Error is not retryable, failing immediately: {}",
                        operation_name, e
                    );
                    return Err(e);
                }

                let remaining = config.max_attempts - attempt - 1;
                if remaining > 0 {
                    warn!(
                        "{}: Attempt {}/{} failed ({}), {} retries remaining",
                        operation_name,
                        attempt + 1,
                        config.max_attempts,
                        e,
                        remaining
                    );
                } else {
                    warn!(
                        "{}: All {} attempts failed. Last error: {}",
                        operation_name, config.max_attempts, e
                    );
                }
                last_error = Some(e);
            }
        }
    }

    Err(last_error.expect("At least one attempt should have been made"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    #[test]
    fn test_retry_config_defaults() {
        let config = RetryConfig::default();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
    }

    #[test]
    fn test_retry_config_health_check() {
        let config = RetryConfig::health_check();
        assert_eq!(config.max_attempts, 4);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
    }

    #[test]
    fn test_retry_config_api_call() {
        let config = RetryConfig::api_call();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
    }

    #[test]
    fn test_retry_config_rss_feed() {
        let config = RetryConfig::rss_feed();
        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
    }

    #[test]
    fn test_delay_calculation() {
        let config = RetryConfig::new(4, Duration::from_secs(1)).with_backoff_multiplier(2.0);

        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(config.delay_for_attempt(1), Duration::from_secs(1));
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(2));
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(4));
    }

    #[test]
    fn test_delay_respects_max() {
        let config = RetryConfig::new(10, Duration::from_secs(1))
            .with_max_delay(Duration::from_secs(3))
            .with_backoff_multiplier(2.0);

        // Attempt 4 would be 8 seconds, but max is 3
        assert_eq!(config.delay_for_attempt(4), Duration::from_secs(3));
    }

    #[tokio::test]
    async fn test_with_retry_succeeds_first_attempt() {
        let config = RetryConfig::new(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, &str> = with_retry(&config, "test", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(42)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_succeeds_after_failures() {
        let config = RetryConfig::new(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, &str> = with_retry(&config, "test", || {
            let c = counter_clone.clone();
            async move {
                let attempt = c.fetch_add(1, Ordering::SeqCst);
                if attempt < 2 {
                    Err("temporary failure")
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_all_attempts_fail() {
        let config = RetryConfig::new(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, &str> = with_retry(&config, "test", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err("permanent failure")
            }
        })
        .await;

        assert_eq!(result.unwrap_err(), "permanent failure");
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_with_retry_if_non_retryable_error() {
        let config = RetryConfig::new(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, &str> = with_retry_if(
            &config,
            "test",
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("client error 400")
                }
            },
            |e: &&str| !e.contains("400"), // Don't retry 400 errors
        )
        .await;

        assert_eq!(result.unwrap_err(), "client error 400");
        // Should only have tried once since 400 is not retryable
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_if_retryable_error() {
        let config = RetryConfig::new(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<u32, &str> = with_retry_if(
            &config,
            "test",
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("server error 500")
                }
            },
            |e: &&str| e.contains("500"), // Retry 500 errors
        )
        .await;

        assert_eq!(result.unwrap_err(), "server error 500");
        // Should have tried all 3 times since 500 is retryable
        assert_eq!(counter.load(Ordering::SeqCst), 3);
    }

    // ==================== Additional Edge Case Tests ====================

    #[test]
    fn test_retry_config_new_sets_defaults() {
        let config = RetryConfig::new(5, Duration::from_millis(100));

        assert_eq!(config.max_attempts, 5);
        assert_eq!(config.initial_delay, Duration::from_millis(100));
        assert_eq!(config.max_delay, Duration::from_secs(30)); // default
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON); // default
    }

    #[test]
    fn test_retry_config_builder_pattern() {
        let config = RetryConfig::new(2, Duration::from_millis(50))
            .with_max_delay(Duration::from_secs(10))
            .with_backoff_multiplier(1.5);

        assert_eq!(config.max_attempts, 2);
        assert_eq!(config.initial_delay, Duration::from_millis(50));
        assert_eq!(config.max_delay, Duration::from_secs(10));
        assert!((config.backoff_multiplier - 1.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_health_check_complete() {
        let config = RetryConfig::health_check();

        assert_eq!(config.max_attempts, 4);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(4));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_api_call_complete() {
        let config = RetryConfig::api_call();

        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_secs(1));
        assert_eq!(config.max_delay, Duration::from_secs(5));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_rss_feed_complete() {
        let config = RetryConfig::rss_feed();

        assert_eq!(config.max_attempts, 3);
        assert_eq!(config.initial_delay, Duration::from_millis(500));
        assert_eq!(config.max_delay, Duration::from_secs(2));
        assert!((config.backoff_multiplier - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_clone() {
        let config1 = RetryConfig::api_call();
        let config2 = config1.clone();

        assert_eq!(config1.max_attempts, config2.max_attempts);
        assert_eq!(config1.initial_delay, config2.initial_delay);
        assert_eq!(config1.max_delay, config2.max_delay);
        assert!((config1.backoff_multiplier - config2.backoff_multiplier).abs() < f64::EPSILON);
    }

    #[test]
    fn test_retry_config_debug() {
        let config = RetryConfig::api_call();
        let debug_str = format!("{:?}", config);

        assert!(debug_str.contains("RetryConfig"));
        assert!(debug_str.contains("max_attempts"));
        assert!(debug_str.contains("initial_delay"));
    }

    // ==================== Delay Calculation Edge Cases ====================

    #[test]
    fn test_delay_first_attempt_always_zero() {
        let configs = vec![
            RetryConfig::health_check(),
            RetryConfig::api_call(),
            RetryConfig::rss_feed(),
            RetryConfig::new(10, Duration::from_secs(5)),
        ];

        for config in configs {
            assert_eq!(
                config.delay_for_attempt(0),
                Duration::ZERO,
                "First attempt should always have zero delay"
            );
        }
    }

    #[test]
    fn test_delay_calculation_with_multiplier_1() {
        let config = RetryConfig::new(5, Duration::from_secs(1)).with_backoff_multiplier(1.0);

        // With multiplier 1.0, all delays should be the initial delay
        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(config.delay_for_attempt(1), Duration::from_secs(1));
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(1));
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(1));
    }

    #[test]
    fn test_delay_calculation_with_multiplier_3() {
        let config = RetryConfig::new(5, Duration::from_millis(100))
            .with_backoff_multiplier(3.0)
            .with_max_delay(Duration::from_secs(60));

        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(100)); // 100ms * 3^0
        assert_eq!(config.delay_for_attempt(2), Duration::from_millis(300)); // 100ms * 3^1
        assert_eq!(config.delay_for_attempt(3), Duration::from_millis(900)); // 100ms * 3^2
    }

    #[test]
    fn test_delay_calculation_health_check_preset() {
        let config = RetryConfig::health_check();

        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(config.delay_for_attempt(1), Duration::from_secs(1)); // 1s * 2^0
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(2)); // 1s * 2^1
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(4)); // 1s * 2^2, capped at max 4s
    }

    #[test]
    fn test_delay_calculation_rss_feed_preset() {
        let config = RetryConfig::rss_feed();

        assert_eq!(config.delay_for_attempt(0), Duration::ZERO);
        assert_eq!(config.delay_for_attempt(1), Duration::from_millis(500)); // 500ms * 2^0
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(1)); // 500ms * 2^1
    }

    #[test]
    fn test_delay_max_capping_strict() {
        let config = RetryConfig::new(10, Duration::from_secs(2))
            .with_max_delay(Duration::from_secs(5))
            .with_backoff_multiplier(2.0);

        // attempt 1: 2s, attempt 2: 4s, attempt 3: 8s (capped to 5s)
        assert_eq!(config.delay_for_attempt(1), Duration::from_secs(2));
        assert_eq!(config.delay_for_attempt(2), Duration::from_secs(4));
        assert_eq!(config.delay_for_attempt(3), Duration::from_secs(5)); // capped
        assert_eq!(config.delay_for_attempt(4), Duration::from_secs(5)); // still capped
        assert_eq!(config.delay_for_attempt(9), Duration::from_secs(5)); // always capped after
    }

    // ==================== Single Attempt Configuration ====================

    #[tokio::test]
    async fn test_with_retry_single_attempt_success() {
        let config = RetryConfig::new(1, Duration::from_millis(100));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, &str> = with_retry(&config, "single_attempt", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(100)
            }
        })
        .await;

        assert_eq!(result.unwrap(), 100);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_with_retry_single_attempt_failure() {
        let config = RetryConfig::new(1, Duration::from_millis(100));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, &str> = with_retry(&config, "single_attempt", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err("failure on first attempt")
            }
        })
        .await;

        assert_eq!(result.unwrap_err(), "failure on first attempt");
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    // ==================== Zero Attempts Panic Tests ====================

    #[tokio::test]
    #[should_panic(expected = "max_attempts must be >= 1")]
    async fn test_with_retry_panics_on_zero_attempts() {
        let config = RetryConfig::new(0, Duration::from_millis(100));

        let _result: Result<(), &str> =
            with_retry(&config, "zero_attempts", || async { Ok(()) }).await;
    }

    #[tokio::test]
    #[should_panic(expected = "max_attempts must be >= 1")]
    async fn test_with_retry_if_panics_on_zero_attempts() {
        let config = RetryConfig::new(0, Duration::from_millis(100));

        let _result: Result<(), &str> =
            with_retry_if(&config, "zero_attempts", || async { Ok(()) }, |_| true).await;
    }

    // ==================== with_retry_if Advanced Tests ====================

    #[tokio::test]
    async fn test_with_retry_if_succeeds_after_retryable_then_non_retryable() {
        // Test scenario: first error is retryable, then succeeds
        let config = RetryConfig::new(3, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<&str, &str> = with_retry_if(
            &config,
            "test",
            || {
                let c = counter_clone.clone();
                async move {
                    let attempt = c.fetch_add(1, Ordering::SeqCst);
                    match attempt {
                        0 => Err("500 server error"),
                        _ => Ok("success"),
                    }
                }
            },
            |e: &&str| e.contains("500"),
        )
        .await;

        assert_eq!(result.unwrap(), "success");
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_with_retry_if_retryable_then_non_retryable() {
        // Test scenario: first error is retryable, second is not
        let config = RetryConfig::new(5, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<&str, &str> = with_retry_if(
            &config,
            "test",
            || {
                let c = counter_clone.clone();
                async move {
                    let attempt = c.fetch_add(1, Ordering::SeqCst);
                    match attempt {
                        0 => Err("500 server error"),
                        _ => Err("400 bad request"),
                    }
                }
            },
            |e: &&str| e.contains("500"), // Only retry 500 errors
        )
        .await;

        // Should fail immediately on the 400 error
        assert_eq!(result.unwrap_err(), "400 bad request");
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_with_retry_if_all_retryable_errors_exhaust_attempts() {
        let config = RetryConfig::new(4, Duration::from_millis(5));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), &str> = with_retry_if(
            &config,
            "exhaust_test",
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("retryable error")
                }
            },
            |_: &&str| true, // Always retry
        )
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 4);
    }

    #[tokio::test]
    async fn test_with_retry_if_non_retryable_on_first_attempt() {
        let config = RetryConfig::new(5, Duration::from_millis(100));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), &str> = with_retry_if(
            &config,
            "first_attempt_non_retryable",
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err("non-retryable error")
                }
            },
            |_: &&str| false, // Never retry
        )
        .await;

        assert!(result.is_err());
        assert_eq!(counter.load(Ordering::SeqCst), 1); // Only one attempt
    }

    // ==================== Timing Verification Tests ====================

    #[tokio::test]
    async fn test_exponential_backoff_timing() {
        // Verify that delays actually occur (approximately)
        let config = RetryConfig::new(3, Duration::from_millis(50)).with_backoff_multiplier(2.0);

        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let start = std::time::Instant::now();

        let _result: Result<(), &str> = with_retry(&config, "timing_test", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err("always fails")
            }
        })
        .await;

        let elapsed = start.elapsed();

        // Should have waited: 0ms + 50ms + 100ms = 150ms minimum
        // Allow some tolerance for test execution overhead
        assert!(
            elapsed >= Duration::from_millis(100),
            "Expected at least 100ms delay, got {:?}",
            elapsed
        );
        assert!(
            elapsed < Duration::from_millis(500),
            "Expected less than 500ms total, got {:?}",
            elapsed
        );
    }

    #[tokio::test]
    async fn test_no_delay_on_immediate_success() {
        let config = RetryConfig::new(3, Duration::from_secs(10)); // Long delay if retry happens

        let start = std::time::Instant::now();

        let result: Result<i32, &str> = with_retry(&config, "immediate_success", || async {
            Ok(42) // Succeeds immediately
        })
        .await;

        let elapsed = start.elapsed();

        assert!(result.is_ok());
        // Should complete very quickly since no retries were needed
        assert!(
            elapsed < Duration::from_millis(100),
            "Expected quick completion, got {:?}",
            elapsed
        );
    }

    // ==================== Error Message Preservation Tests ====================

    #[tokio::test]
    async fn test_last_error_is_returned() {
        let config = RetryConfig::new(3, Duration::from_millis(5));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), String> = with_retry(&config, "error_test", || {
            let c = counter_clone.clone();
            async move {
                let attempt = c.fetch_add(1, Ordering::SeqCst);
                Err(format!("error on attempt {}", attempt + 1))
            }
        })
        .await;

        // Should return the error from the last attempt
        assert_eq!(result.unwrap_err(), "error on attempt 3");
    }

    #[tokio::test]
    async fn test_with_retry_if_returns_correct_error() {
        let config = RetryConfig::new(3, Duration::from_millis(5));

        let result: Result<(), &str> = with_retry_if(
            &config,
            "error_test",
            || async { Err("non-retryable error") },
            |_: &&str| false,
        )
        .await;

        assert_eq!(result.unwrap_err(), "non-retryable error");
    }

    // ==================== Concurrent Safety Tests ====================

    #[tokio::test]
    async fn test_retry_with_shared_state() {
        let config = RetryConfig::new(3, Duration::from_millis(10));
        let shared_state = Arc::new(std::sync::Mutex::new(Vec::new()));
        let state_clone = shared_state.clone();

        let _result: Result<(), &str> = with_retry(&config, "shared_state", || {
            let state = state_clone.clone();
            async move {
                state.lock().unwrap().push("attempt");
                Err("failure")
            }
        })
        .await;

        let attempts = shared_state.lock().unwrap().len();
        assert_eq!(attempts, 3, "Should have recorded 3 attempts");
    }

    // ==================== Different Error Types ====================

    #[tokio::test]
    async fn test_with_retry_custom_error_type() {
        #[derive(Debug, Clone)]
        struct CustomError {
            code: u32,
            message: String,
        }

        impl std::fmt::Display for CustomError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "Error {}: {}", self.code, self.message)
            }
        }

        let config = RetryConfig::new(2, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), CustomError> = with_retry(&config, "custom_error", || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err(CustomError {
                    code: 500,
                    message: "Internal error".to_string(),
                })
            }
        })
        .await;

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.code, 500);
        assert_eq!(err.message, "Internal error");
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn test_with_retry_if_custom_error_type() {
        #[derive(Debug, Clone)]
        struct ApiError {
            status: u16,
        }

        impl std::fmt::Display for ApiError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "HTTP {}", self.status)
            }
        }

        let config = RetryConfig::new(5, Duration::from_millis(10));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<(), ApiError> = with_retry_if(
            &config,
            "api_error",
            || {
                let c = counter_clone.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err(ApiError { status: 401 })
                }
            },
            |e: &ApiError| e.status >= 500, // Only retry 5xx errors
        )
        .await;

        assert!(result.is_err());
        assert_eq!(result.unwrap_err().status, 401);
        assert_eq!(counter.load(Ordering::SeqCst), 1); // No retries for 4xx
    }

    // ==================== Max Attempts Edge Cases ====================

    #[tokio::test]
    async fn test_many_attempts_config() {
        let config = RetryConfig::new(10, Duration::from_millis(1));
        let counter = Arc::new(AtomicU32::new(0));
        let counter_clone = counter.clone();

        let result: Result<i32, &str> = with_retry(&config, "many_attempts", || {
            let c = counter_clone.clone();
            async move {
                let attempt = c.fetch_add(1, Ordering::SeqCst);
                if attempt < 7 {
                    Err("not yet")
                } else {
                    Ok(42)
                }
            }
        })
        .await;

        assert_eq!(result.unwrap(), 42);
        assert_eq!(counter.load(Ordering::SeqCst), 8); // Succeeded on 8th attempt
    }
}
