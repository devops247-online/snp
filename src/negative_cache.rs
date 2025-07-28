// Negative caching implementation with Bloom filter for efficient cache miss detection
// Implements the specifications from GitHub issue #67

use bloom::{BloomFilter, ASMS};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Configuration for negative caching with Bloom filter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NegativeCacheConfig {
    /// Enable negative caching (default: true)
    pub enabled: bool,
    /// False positive rate for Bloom filter (default: 0.01 = 1%)
    pub false_positive_rate: f64,
    /// Expected number of entries (default: 10000)
    pub expected_entries: usize,
    /// Maximum confirmed entries in definitive cache (default: 1000)
    pub max_confirmed_entries: usize,
    /// Interval for resetting Bloom filter (default: 1 hour)
    pub reset_interval: Duration,
    /// Enable persistence of negative cache (default: false)
    pub persistence_enabled: bool,
}

impl Default for NegativeCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            false_positive_rate: 0.01,
            expected_entries: 10000,
            max_confirmed_entries: 1000,
            reset_interval: Duration::from_secs(3600),
            persistence_enabled: false,
        }
    }
}

/// Metrics for negative cache performance monitoring
#[derive(Debug, Clone, Default)]
pub struct NegativeCacheMetrics {
    /// Number of Bloom filter checks performed
    pub bloom_checks: u64,
    /// Number of confirmed cache checks performed
    pub confirmed_checks: u64,
    /// Number of misses recorded in the cache
    pub recorded_misses: u64,
    /// Number of confirmed misses (definitive negative cache)
    pub confirmed_misses: usize,
    /// Current false positive rate estimate
    pub false_positive_rate: f64,
    /// Memory usage in bytes
    pub memory_usage_bytes: u64,
    /// Number of cache resets performed
    pub cache_resets: u64,
}

impl NegativeCacheMetrics {
    /// Calculate cache efficiency (hit rate for negative lookups)
    pub fn cache_efficiency(&self) -> f64 {
        let total_checks = self.bloom_checks + self.confirmed_checks;
        if total_checks == 0 {
            0.0
        } else {
            // Efficiency is the percentage of quick negative determinations
            self.bloom_checks as f64 / total_checks as f64
        }
    }
}

/// Negative cache implementation using Bloom filter for probabilistic checking
/// and a definitive cache for confirmed misses
pub struct NegativeCache {
    /// Probabilistic filter for quick negative checks (never false negatives)
    bloom_filter: BloomFilter,

    /// Definitive negative cache for confirmed misses
    confirmed_misses: HashSet<String>,

    /// Configuration settings
    config: NegativeCacheConfig,

    /// Performance metrics (using atomics for thread-safety)
    bloom_checks: AtomicU64,
    confirmed_checks: AtomicU64,
    recorded_misses: AtomicU64,
    cache_resets: AtomicU64,
}

impl NegativeCache {
    /// Create a new negative cache with specified parameters
    pub fn new(expected_entries: usize, false_positive_rate: f64) -> Self {
        let config = NegativeCacheConfig {
            expected_entries: expected_entries.max(1), // Ensure minimum of 1
            false_positive_rate: false_positive_rate.max(f64::EPSILON), // Ensure positive rate
            ..NegativeCacheConfig::default()
        };

        Self::with_config(config)
    }

    /// Create a new negative cache with custom configuration
    pub fn with_config(config: NegativeCacheConfig) -> Self {
        let bloom_filter = BloomFilter::with_rate(
            config.false_positive_rate as f32,
            config.expected_entries as u32,
        );

        Self {
            bloom_filter,
            confirmed_misses: HashSet::with_capacity(config.max_confirmed_entries),
            config,
            bloom_checks: AtomicU64::new(0),
            confirmed_checks: AtomicU64::new(0),
            recorded_misses: AtomicU64::new(0),
            cache_resets: AtomicU64::new(0),
        }
    }

    /// Quick probabilistic check - returns true if key is definitely not present
    /// Returns false if key might be present (no false negatives, possible false positives)
    pub fn probably_absent(&self, key: &str) -> bool {
        self.bloom_checks.fetch_add(1, Ordering::Relaxed);

        // Bloom filter check: if returns false, key was definitely not recorded as a miss
        // If returns true, key was recorded as a miss (so it's probably absent from actual cache)
        self.bloom_filter.contains(&key)
    }

    /// Definitive check for confirmed misses
    /// Returns true if key is definitively known to be absent
    pub fn definitely_absent(&self, key: &str) -> bool {
        self.confirmed_checks.fetch_add(1, Ordering::Relaxed);
        self.confirmed_misses.contains(key)
    }

    /// Record a cache miss for the given key
    pub fn record_miss(&mut self, key: String) {
        self.recorded_misses.fetch_add(1, Ordering::Relaxed);

        // Add to Bloom filter (probabilistic)
        self.bloom_filter.insert(&key);

        // Add to confirmed misses if there's space
        if self.confirmed_misses.len() < self.config.max_confirmed_entries {
            self.confirmed_misses.insert(key);
        }
        // Note: If confirmed cache is full, we only rely on Bloom filter
        // This is acceptable as Bloom filter provides the primary negative caching benefit
    }

    /// Reset the negative cache, clearing all entries
    pub fn reset(&mut self) {
        self.cache_resets.fetch_add(1, Ordering::Relaxed);

        // Create new Bloom filter
        self.bloom_filter = BloomFilter::with_rate(
            self.config.false_positive_rate as f32,
            self.config.expected_entries as u32,
        );

        // Clear confirmed misses
        self.confirmed_misses.clear();

        // Reset metrics counters
        self.bloom_checks.store(0, Ordering::Relaxed);
        self.confirmed_checks.store(0, Ordering::Relaxed);
        self.recorded_misses.store(0, Ordering::Relaxed);
        // Note: cache_resets counter is not reset as it tracks total resets
    }

    /// Get current cache metrics
    pub fn get_metrics(&self) -> NegativeCacheMetrics {
        NegativeCacheMetrics {
            bloom_checks: self.bloom_checks.load(Ordering::Relaxed),
            confirmed_checks: self.confirmed_checks.load(Ordering::Relaxed),
            recorded_misses: self.recorded_misses.load(Ordering::Relaxed),
            confirmed_misses: self.confirmed_misses.len(),
            false_positive_rate: self.config.false_positive_rate,
            memory_usage_bytes: self.estimate_memory_usage(),
            cache_resets: self.cache_resets.load(Ordering::Relaxed),
        }
    }

    /// Get the current configuration
    pub fn get_config(&self) -> &NegativeCacheConfig {
        &self.config
    }

    /// Estimate current memory usage in bytes
    fn estimate_memory_usage(&self) -> u64 {
        // Rough estimation:
        // - Bloom filter size depends on expected entries and false positive rate
        // - Confirmed misses: string data + HashSet overhead

        let bloom_filter_bits = self.bloom_filter.num_bits() as u64;
        let bloom_filter_bytes = bloom_filter_bits.div_ceil(8); // Convert bits to bytes

        let confirmed_cache_bytes: u64 = self
            .confirmed_misses
            .iter()
            .map(|key| key.len() as u64 + 24) // String data + overhead estimate
            .sum();

        bloom_filter_bytes + confirmed_cache_bytes + 1024 // Add struct overhead
    }

    /// Check if negative caching is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the number of hash functions used by the Bloom filter
    pub fn hash_functions(&self) -> u32 {
        self.bloom_filter.num_hashes()
    }

    /// Get the total number of bits in the Bloom filter
    pub fn bloom_filter_bits(&self) -> u64 {
        self.bloom_filter.num_bits() as u64
    }

    /// Check if the confirmed cache is at capacity
    pub fn confirmed_cache_full(&self) -> bool {
        self.confirmed_misses.len() >= self.config.max_confirmed_entries
    }
}

impl Default for NegativeCache {
    fn default() -> Self {
        Self::new(10000, 0.01)
    }
}

// Note: The Bloom filter crate may not be Send/Sync by default
// We'll need to verify this and potentially add unsafe impl if needed for our use case
// For now, implementing basic thread safety through external synchronization

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_negative_cache_basic_functionality() {
        let mut cache = NegativeCache::new(100, 0.01);
        let key = "test_key";

        // Initially should not be absent
        assert!(!cache.probably_absent(key));
        assert!(!cache.definitely_absent(key));

        // Record miss
        cache.record_miss(key.to_string());

        // Now should be probably absent
        assert!(cache.probably_absent(key));

        // Might be definitely absent (depends on confirmed cache capacity)
        // In this case with small cache, it should be definitely absent
        assert!(cache.definitely_absent(key));
    }

    #[test]
    fn test_negative_cache_no_false_negatives() {
        let mut cache = NegativeCache::new(100, 0.01);
        let keys = vec!["key1", "key2", "key3", "key4", "key5"];

        // Record all keys
        for key in &keys {
            cache.record_miss(key.to_string());
        }

        // Verify no false negatives
        for key in &keys {
            assert!(cache.probably_absent(key), "False negative for key: {key}");
        }
    }

    #[test]
    fn test_negative_cache_metrics() {
        let mut cache = NegativeCache::new(100, 0.01);

        // Perform some operations
        cache.record_miss("key1".to_string());
        cache.probably_absent("key1");
        cache.probably_absent("key2");
        cache.definitely_absent("key1");

        let metrics = cache.get_metrics();
        assert_eq!(metrics.recorded_misses, 1);
        assert_eq!(metrics.bloom_checks, 2);
        assert_eq!(metrics.confirmed_checks, 1);
        assert!(metrics.memory_usage_bytes > 0);
    }

    #[test]
    fn test_negative_cache_reset() {
        let mut cache = NegativeCache::new(100, 0.01);

        cache.record_miss("key1".to_string());
        assert!(cache.probably_absent("key1"));

        cache.reset();

        // After reset, should not be probably absent
        assert!(!cache.probably_absent("key1"));

        let metrics = cache.get_metrics();
        assert_eq!(metrics.cache_resets, 1);
    }

    #[test]
    fn test_negative_cache_config() {
        let config = NegativeCacheConfig {
            false_positive_rate: 0.005,
            expected_entries: 5000,
            ..NegativeCacheConfig::default()
        };

        let cache = NegativeCache::with_config(config);
        assert_eq!(cache.get_config().false_positive_rate, 0.005);
        assert_eq!(cache.get_config().expected_entries, 5000);
    }

    #[test]
    fn test_negative_cache_confirmed_limit() {
        let config = NegativeCacheConfig {
            max_confirmed_entries: 2,
            ..NegativeCacheConfig::default()
        };

        let mut cache = NegativeCache::with_config(config);

        // Add more entries than the limit
        cache.record_miss("key1".to_string());
        cache.record_miss("key2".to_string());
        cache.record_miss("key3".to_string());

        let metrics = cache.get_metrics();
        assert_eq!(metrics.confirmed_misses, 2); // Should respect limit
        assert_eq!(metrics.recorded_misses, 3); // But all should be recorded in bloom filter
    }
}
