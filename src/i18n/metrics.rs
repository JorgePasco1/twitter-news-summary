//! Translation metrics and observability module.
//!
//! This module provides metrics tracking for translation operations,
//! including cache hit rates, API calls, and failures.

use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

/// Global translation metrics singleton.
pub struct TranslationMetrics {
    /// Number of times a translation was found in cache (memory or database)
    cache_hits: AtomicUsize,

    /// Number of times a translation was not found in cache
    cache_misses: AtomicUsize,

    /// Number of API calls made to translation service
    api_calls: AtomicUsize,

    /// Number of API calls that failed
    api_failures: AtomicUsize,
}

/// Global metrics instance (initialized lazily)
static METRICS: OnceLock<TranslationMetrics> = OnceLock::new();

impl TranslationMetrics {
    /// Get the global translation metrics instance.
    ///
    /// This method initializes the metrics on first call and returns a reference
    /// to the singleton instance on subsequent calls.
    pub fn global() -> &'static TranslationMetrics {
        METRICS.get_or_init(|| TranslationMetrics {
            cache_hits: AtomicUsize::new(0),
            cache_misses: AtomicUsize::new(0),
            api_calls: AtomicUsize::new(0),
            api_failures: AtomicUsize::new(0),
        })
    }

    /// Record a cache hit (translation found in cache).
    pub fn record_cache_hit(&self) {
        self.cache_hits.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a cache miss (translation not found in cache).
    pub fn record_cache_miss(&self) {
        self.cache_misses.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an API call to the translation service.
    pub fn record_api_call(&self) {
        self.api_calls.fetch_add(1, Ordering::Relaxed);
    }

    /// Record an API call failure.
    pub fn record_api_failure(&self) {
        self.api_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Get the current cache hit count.
    pub fn cache_hits(&self) -> usize {
        self.cache_hits.load(Ordering::Relaxed)
    }

    /// Get the current cache miss count.
    pub fn cache_misses(&self) -> usize {
        self.cache_misses.load(Ordering::Relaxed)
    }

    /// Get the current API call count.
    pub fn api_calls(&self) -> usize {
        self.api_calls.load(Ordering::Relaxed)
    }

    /// Get the current API failure count.
    pub fn api_failures(&self) -> usize {
        self.api_failures.load(Ordering::Relaxed)
    }

    /// Generate a metrics report.
    pub fn report(&self) -> MetricsReport {
        let hits = self.cache_hits();
        let misses = self.cache_misses();
        let total_cache_queries = hits + misses;
        let cache_hit_rate = if total_cache_queries > 0 {
            (hits as f64 / total_cache_queries as f64) * 100.0
        } else {
            0.0
        };

        let calls = self.api_calls();
        let failures = self.api_failures();
        let api_success_rate = if calls > 0 {
            ((calls - failures) as f64 / calls as f64) * 100.0
        } else {
            0.0
        };

        MetricsReport {
            cache_hits: hits,
            cache_misses: misses,
            cache_hit_rate,
            api_calls: calls,
            api_failures: failures,
            api_success_rate,
        }
    }

    /// Reset all metrics to zero (useful for testing).
    #[cfg(test)]
    pub fn reset(&self) {
        self.cache_hits.store(0, Ordering::Relaxed);
        self.cache_misses.store(0, Ordering::Relaxed);
        self.api_calls.store(0, Ordering::Relaxed);
        self.api_failures.store(0, Ordering::Relaxed);
    }
}

/// Metrics report containing current translation statistics.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsReport {
    /// Number of cache hits
    pub cache_hits: usize,

    /// Number of cache misses
    pub cache_misses: usize,

    /// Cache hit rate as a percentage (0-100)
    pub cache_hit_rate: f64,

    /// Number of API calls made
    pub api_calls: usize,

    /// Number of API failures
    pub api_failures: usize,

    /// API success rate as a percentage (0-100)
    pub api_success_rate: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to reset metrics before each test
    fn reset_metrics() {
        TranslationMetrics::global().reset();
    }

    // ==================== Counter Tests ====================

    #[test]
    fn test_record_cache_hit() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        assert_eq!(metrics.cache_hits(), 0);
        metrics.record_cache_hit();
        assert_eq!(metrics.cache_hits(), 1);
        metrics.record_cache_hit();
        assert_eq!(metrics.cache_hits(), 2);
    }

    #[test]
    fn test_record_cache_miss() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        assert_eq!(metrics.cache_misses(), 0);
        metrics.record_cache_miss();
        assert_eq!(metrics.cache_misses(), 1);
    }

    #[test]
    fn test_record_api_call() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        assert_eq!(metrics.api_calls(), 0);
        metrics.record_api_call();
        assert_eq!(metrics.api_calls(), 1);
    }

    #[test]
    fn test_record_api_failure() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        assert_eq!(metrics.api_failures(), 0);
        metrics.record_api_failure();
        assert_eq!(metrics.api_failures(), 1);
    }

    // ==================== Report Tests ====================

    #[test]
    fn test_report_empty() {
        reset_metrics();
        let metrics = TranslationMetrics::global();
        let report = metrics.report();

        assert_eq!(report.cache_hits, 0);
        assert_eq!(report.cache_misses, 0);
        assert_eq!(report.cache_hit_rate, 0.0);
        assert_eq!(report.api_calls, 0);
        assert_eq!(report.api_failures, 0);
        assert_eq!(report.api_success_rate, 0.0);
    }

    #[test]
    fn test_report_cache_hit_rate() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        // 3 hits, 1 miss = 75% hit rate
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_hit();
        metrics.record_cache_miss();

        let report = metrics.report();
        assert_eq!(report.cache_hits, 3);
        assert_eq!(report.cache_misses, 1);
        assert_eq!(report.cache_hit_rate, 75.0);
    }

    #[test]
    fn test_report_api_success_rate() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        // 4 calls, 1 failure = 75% success rate
        metrics.record_api_call();
        metrics.record_api_call();
        metrics.record_api_call();
        metrics.record_api_call();
        metrics.record_api_failure();

        let report = metrics.report();
        assert_eq!(report.api_calls, 4);
        assert_eq!(report.api_failures, 1);
        assert_eq!(report.api_success_rate, 75.0);
    }

    #[test]
    fn test_report_100_percent_cache_hit_rate() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        metrics.record_cache_hit();
        metrics.record_cache_hit();

        let report = metrics.report();
        assert_eq!(report.cache_hit_rate, 100.0);
    }

    #[test]
    fn test_report_0_percent_cache_hit_rate() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        metrics.record_cache_miss();
        metrics.record_cache_miss();

        let report = metrics.report();
        assert_eq!(report.cache_hit_rate, 0.0);
    }

    #[test]
    fn test_report_100_percent_api_success_rate() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        metrics.record_api_call();
        metrics.record_api_call();

        let report = metrics.report();
        assert_eq!(report.api_success_rate, 100.0);
    }

    #[test]
    fn test_report_all_api_failures() {
        reset_metrics();
        let metrics = TranslationMetrics::global();

        metrics.record_api_call();
        metrics.record_api_failure();
        metrics.record_api_call();
        metrics.record_api_failure();

        let report = metrics.report();
        assert_eq!(report.api_success_rate, 0.0);
    }

    // ==================== Singleton Tests ====================

    #[test]
    fn test_global_returns_same_instance() {
        let metrics1 = TranslationMetrics::global();
        let metrics2 = TranslationMetrics::global();

        // Should return the same instance (same memory address)
        assert!(std::ptr::eq(metrics1, metrics2));
    }

    #[test]
    fn test_metrics_persist_across_calls() {
        // Note: Don't reset here - this test verifies the singleton behavior
        // by checking that incrementing through one reference is visible through another
        let metrics1 = TranslationMetrics::global();
        let initial = metrics1.cache_hits();
        metrics1.record_cache_hit();

        let metrics2 = TranslationMetrics::global();
        // Value should have increased by 1 from the initial value
        assert_eq!(metrics2.cache_hits(), initial + 1);
    }
}
