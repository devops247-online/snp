// Tests for negative cache functionality with Bloom filter implementation
// These tests verify the negative caching behavior specified in GitHub issue #67

use snp::negative_cache::{NegativeCache, NegativeCacheConfig};
use std::time::Duration;

#[test]
fn test_negative_cache_creation() {
    let cache = NegativeCache::new(1000, 0.01);
    assert_eq!(cache.get_config().expected_entries, 1000);
    assert_eq!(cache.get_config().false_positive_rate, 0.01);
}

#[test]
fn test_negative_cache_with_config() {
    let config = NegativeCacheConfig {
        enabled: true,
        false_positive_rate: 0.005,
        expected_entries: 5000,
        max_confirmed_entries: 500,
        reset_interval: Duration::from_secs(3600),
        persistence_enabled: false,
    };

    let cache = NegativeCache::with_config(config.clone());
    assert_eq!(cache.get_config().false_positive_rate, 0.005);
    assert_eq!(cache.get_config().expected_entries, 5000);
    assert_eq!(cache.get_config().max_confirmed_entries, 500);
}

#[test]
fn test_negative_cache_basic_operations() {
    let mut cache = NegativeCache::new(100, 0.01);
    let key = "test_file.txt";

    // Initially, key should not be probably absent (not yet recorded)
    assert!(!cache.probably_absent(key));
    assert!(!cache.definitely_absent(key));

    // Record a miss
    cache.record_miss(key.to_string());

    // Now key should be probably absent (in bloom filter)
    // But not definitely absent until confirmed
    assert!(cache.probably_absent(key));
    // Might or might not be definitely absent depending on confirmed cache size
}

#[test]
fn test_negative_cache_no_false_negatives() {
    let mut cache = NegativeCache::new(100, 0.01);
    let test_keys = vec!["file1.txt", "file2.py", "file3.rs", "file4.js"];

    // Record all keys as misses
    for key in &test_keys {
        cache.record_miss(key.to_string());
    }

    // Verify no false negatives - all recorded keys should be probably absent
    for key in &test_keys {
        assert!(
            cache.probably_absent(key),
            "False negative detected for key: {key}"
        );
    }
}

#[test]
fn test_negative_cache_false_positive_rate() {
    let mut cache = NegativeCache::new(1000, 0.01);

    // Record 500 entries
    for i in 0..500 {
        cache.record_miss(format!("file_{i}.txt"));
    }

    // Test 1000 random keys that weren't recorded
    let mut false_positives = 0;
    for i in 1000..2000 {
        let key = format!("unrecorded_{i}.txt");
        if cache.probably_absent(&key) {
            false_positives += 1;
        }
    }

    let false_positive_rate = false_positives as f64 / 1000.0;
    // Should be roughly within the configured rate (allowing some variance)
    assert!(
        false_positive_rate <= 0.02,
        "False positive rate too high: {false_positive_rate}"
    );
}

#[test]
fn test_negative_cache_confirmed_entries_limit() {
    let config = NegativeCacheConfig {
        enabled: true,
        false_positive_rate: 0.01,
        expected_entries: 1000,
        max_confirmed_entries: 5, // Small limit for testing
        reset_interval: Duration::from_secs(3600),
        persistence_enabled: false,
    };

    let mut cache = NegativeCache::with_config(config);

    // Record more entries than the confirmed limit
    for i in 0..10 {
        cache.record_miss(format!("file_{i}.txt"));
    }

    // Check that confirmed cache respects the limit
    let metrics = cache.get_metrics();
    assert!(metrics.confirmed_misses <= 5);
}

#[test]
fn test_negative_cache_metrics() {
    let mut cache = NegativeCache::new(100, 0.01);

    // Initial metrics
    let initial_metrics = cache.get_metrics();
    assert_eq!(initial_metrics.bloom_checks, 0);
    assert_eq!(initial_metrics.confirmed_checks, 0);
    assert_eq!(initial_metrics.recorded_misses, 0);

    // Record some misses
    cache.record_miss("file1.txt".to_string());
    cache.record_miss("file2.txt".to_string());

    // Check some keys
    cache.probably_absent("file1.txt");
    cache.probably_absent("file3.txt");
    cache.definitely_absent("file1.txt");

    let metrics = cache.get_metrics();
    assert_eq!(metrics.recorded_misses, 2);
    assert_eq!(metrics.bloom_checks, 2);
    assert_eq!(metrics.confirmed_checks, 1);
}

#[test]
fn test_negative_cache_reset() {
    let mut cache = NegativeCache::new(100, 0.01);

    // Record some entries
    cache.record_miss("file1.txt".to_string());
    cache.record_miss("file2.txt".to_string());

    assert!(cache.probably_absent("file1.txt"));

    // Reset the cache
    cache.reset();

    // After reset, entries should not be probably absent
    assert!(!cache.probably_absent("file1.txt"));

    // Metrics should be reset
    let metrics = cache.get_metrics();
    assert_eq!(metrics.recorded_misses, 0);
    assert_eq!(metrics.bloom_checks, 1); // The check we just made
}

#[test]
fn test_negative_cache_thread_safety() {
    use std::sync::Arc;
    use std::thread;

    let cache: Arc<std::sync::Mutex<NegativeCache>> =
        Arc::new(std::sync::Mutex::new(NegativeCache::new(1000, 0.01)));
    let mut handles = vec![];

    // Spawn multiple threads to record misses
    for i in 0..10 {
        let cache_clone = Arc::clone(&cache);
        let handle = thread::spawn(move || {
            for j in 0..100 {
                let key = format!("thread_{i}_file_{j}.txt");
                cache_clone.lock().unwrap().record_miss(key);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    // Verify entries were recorded
    let cache = cache.lock().unwrap();
    let metrics = cache.get_metrics();
    assert_eq!(metrics.recorded_misses, 1000);
}

#[test]
fn test_negative_cache_memory_efficiency() {
    let cache = NegativeCache::new(10000, 0.001);
    let metrics = cache.get_metrics();

    // Memory usage should be reasonable for a bloom filter
    // This is just a sanity check - actual values depend on implementation
    assert!(metrics.memory_usage_bytes > 0);
    assert!(metrics.memory_usage_bytes < 1_000_000); // Less than 1MB for 10k entries
}

#[test]
fn test_negative_cache_config_validation() {
    // Test default config
    let default_config = NegativeCacheConfig::default();
    assert!(default_config.enabled);
    assert_eq!(default_config.false_positive_rate, 0.01);
    assert_eq!(default_config.expected_entries, 10000);
    assert_eq!(default_config.max_confirmed_entries, 1000);

    // Test config bounds
    let cache = NegativeCache::new(0, 0.01); // Edge case: zero entries
    assert!(cache.get_config().expected_entries >= 1); // Should be adjusted to minimum

    let cache = NegativeCache::new(1000, 0.0); // Edge case: zero false positive rate
    assert!(cache.get_config().false_positive_rate > 0.0); // Should be adjusted to minimum
}

#[test]
fn test_negative_cache_persistence_disabled() {
    let config = NegativeCacheConfig {
        persistence_enabled: false,
        ..NegativeCacheConfig::default()
    };

    let cache = NegativeCache::with_config(config);
    assert!(!cache.get_config().persistence_enabled);
}

// Property-based test using quickcheck would go here in a real implementation
// This is a manual property test
#[test]
fn test_negative_cache_property_no_false_negatives() {
    let mut cache = NegativeCache::new(1000, 0.01);
    let mut recorded_keys = std::collections::HashSet::new();

    // Record 100 random keys
    for i in 0..100 {
        let key = format!("property_test_key_{i}");
        cache.record_miss(key.clone());
        recorded_keys.insert(key);
    }

    // Property: No false negatives
    for key in &recorded_keys {
        assert!(
            cache.probably_absent(key),
            "Property violation: false negative for recorded key {key}"
        );
    }
}

#[test]
fn test_negative_cache_integration_with_file_filter() {
    use snp::filesystem::FileFilter;
    use std::fs;
    use tempfile::TempDir;

    // Create test files
    let temp_dir = TempDir::new().unwrap();
    let py_file = temp_dir.path().join("script.py");
    let js_file = temp_dir.path().join("app.js");
    let txt_file = temp_dir.path().join("readme.txt");

    fs::write(&py_file, "print('hello')").unwrap();
    fs::write(&js_file, "console.log('hello')").unwrap();
    fs::write(&txt_file, "Hello").unwrap();

    let files = vec![py_file.clone(), js_file.clone(), txt_file.clone()];

    // Create a filter that only matches Python files
    let filter = FileFilter::new()
        .with_include_patterns(vec![r"\.py$".to_string()])
        .unwrap();

    let mut negative_cache = NegativeCache::new(100, 0.01);

    // First filtering with negative cache
    let filtered1 = filter
        .filter_files_with_negative_cache(&files, &mut negative_cache)
        .unwrap();
    assert_eq!(filtered1.len(), 1);
    assert!(filtered1[0].file_name().unwrap() == "script.py");

    // Check that non-matching files were recorded as misses
    assert!(negative_cache.probably_absent(&js_file.to_string_lossy()));
    assert!(negative_cache.probably_absent(&txt_file.to_string_lossy()));

    // Second filtering should be faster due to negative caching
    let filtered2 = filter
        .filter_files_with_negative_cache(&files, &mut negative_cache)
        .unwrap();
    assert_eq!(filtered2.len(), 1);
    assert!(filtered2[0].file_name().unwrap() == "script.py");

    // Verify cache metrics
    let metrics = negative_cache.get_metrics();
    assert!(metrics.bloom_checks > 0);
    assert_eq!(metrics.recorded_misses, 2); // js and txt files
}
