// Integration tests for lock-free data structures
// Tests concurrent access, performance characteristics, and correctness

use serial_test::serial;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use snp::{
    concurrency::{ConcurrencyExecutor, ResourceLimits, TaskConfig},
    language::LanguageRegistry,
    lock_free_cache::LockFreeCache,
    lock_free_scheduler::{LockFreeTaskScheduler, WorkerState},
    RegistryConfig,
};

#[cfg(test)]
mod language_registry_tests {
    use super::*;

    #[test]
    fn test_registry_atomic_statistics() {
        let registry = LanguageRegistry::new();

        // Initial stats should be zero
        let stats = registry.get_stats();
        assert_eq!(stats.total_registrations, 0);
        assert_eq!(stats.active_plugins, 0);
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.detection_requests, 0);
        assert_eq!(stats.cache_hit_rate, 0.0);
    }

    #[test]
    fn test_registry_configuration_updates() {
        let registry = LanguageRegistry::new();

        // Get default config
        let default_config = registry.get_config();
        assert_eq!(default_config.max_cache_size, 1000);
        assert!(default_config.enable_detection_cache);

        // Update config
        let new_config = RegistryConfig {
            max_cache_size: 2000,
            enable_detection_cache: false,
            cache_ttl_seconds: 7200,
        };
        registry.update_config(new_config.clone());

        // Verify update
        let updated_config = registry.get_config();
        assert_eq!(updated_config.max_cache_size, 2000);
        assert!(!updated_config.enable_detection_cache);
        assert_eq!(updated_config.cache_ttl_seconds, 7200);
    }

    #[test]
    fn test_registry_cache_management() {
        let registry = LanguageRegistry::new();

        // Initially cache should be empty
        assert_eq!(registry.cache_size(), 0);

        // Clear cache (should not error on empty cache)
        registry.clear_cache();
        assert_eq!(registry.cache_size(), 0);

        // Reset stats
        registry.reset_stats();
        let stats = registry.get_stats();
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
    }

    #[test]
    fn test_registry_with_custom_config() {
        let config = RegistryConfig {
            max_cache_size: 500,
            enable_detection_cache: false,
            cache_ttl_seconds: 1800,
        };
        let registry = LanguageRegistry::with_config(config.clone());

        let loaded_config = registry.get_config();
        assert_eq!(loaded_config.max_cache_size, 500);
        assert!(!loaded_config.enable_detection_cache);
        assert_eq!(loaded_config.cache_ttl_seconds, 1800);
    }
}

#[cfg(test)]
mod lock_free_cache_tests {
    use super::*;

    #[test]
    fn test_cache_basic_operations() {
        let cache = LockFreeCache::new(100);

        // Test insert and get
        assert_eq!(cache.insert("key1".to_string(), "value1".to_string()), None);
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));

        // Test miss
        assert_eq!(cache.get(&"nonexistent".to_string()), None);

        // Test contains_key
        assert!(cache.contains_key(&"key1".to_string()));
        assert!(!cache.contains_key(&"nonexistent".to_string()));

        // Test update
        assert_eq!(
            cache.insert("key1".to_string(), "value2".to_string()),
            Some("value1".to_string())
        );
        assert_eq!(cache.get(&"key1".to_string()), Some("value2".to_string()));

        // Test remove
        assert_eq!(
            cache.remove(&"key1".to_string()),
            Some("value2".to_string())
        );
        assert_eq!(cache.get(&"key1".to_string()), None);
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_eviction() {
        let cache = LockFreeCache::new(2);

        cache.insert("key1".to_string(), "value1".to_string());
        thread::sleep(Duration::from_millis(1)); // Ensure distinct timestamps
        cache.insert("key2".to_string(), "value2".to_string());
        assert_eq!(cache.len(), 2);

        // Access key1 to make it more recently used
        thread::sleep(Duration::from_millis(1)); // Ensure distinct timestamps
        cache.get(&"key1".to_string());

        // This should trigger eviction of key2 (LRU)
        cache.insert("key3".to_string(), "value3".to_string());
        assert_eq!(cache.len(), 2);

        // key2 should have been evicted
        assert_eq!(cache.get(&"key2".to_string()), None);
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        assert_eq!(cache.get(&"key3".to_string()), Some("value3".to_string()));
    }

    #[test]
    fn test_cache_concurrent_access() {
        let cache = Arc::new(LockFreeCache::new(1000));
        let mut handles = vec![];

        // Spawn multiple threads to access the cache concurrently
        for i in 0..10 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    let key = format!("key_{i}_{j}");
                    let value = format!("value_{i}_{j}");
                    cache_clone.insert(key.clone(), value.clone());
                    assert_eq!(cache_clone.get(&key), Some(value));
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final state
        assert_eq!(cache.len(), 1000);
        let stats = cache.stats();
        assert_eq!(stats.hits, 1000);
        assert_eq!(stats.inserts, 1000);
        assert_eq!(stats.hit_rate, 1.0);
    }

    #[test]
    fn test_cache_get_or_compute() {
        let cache = LockFreeCache::new(100);

        // First call should compute
        let value = cache.get_or_compute("key1".to_string(), || "computed_value".to_string());
        assert_eq!(value, "computed_value");

        // Second call should use cached value
        let value = cache.get_or_compute("key1".to_string(), || "should_not_compute".to_string());
        assert_eq!(value, "computed_value");

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.hit_rate, 0.5);
    }

    #[tokio::test]
    async fn test_cache_async_get_or_compute() {
        let cache = LockFreeCache::new(100);

        // First call should compute
        let value = cache
            .get_or_compute_async("key1".to_string(), || async {
                tokio::time::sleep(Duration::from_millis(1)).await;
                "computed_value".to_string()
            })
            .await;
        assert_eq!(value, "computed_value");

        // Second call should use cached value (no delay)
        let start = std::time::Instant::now();
        let value = cache
            .get_or_compute_async("key1".to_string(), || async {
                tokio::time::sleep(Duration::from_millis(100)).await; // Should not execute
                "should_not_compute".to_string()
            })
            .await;
        let duration = start.elapsed();

        assert_eq!(value, "computed_value");
        assert!(duration < Duration::from_millis(50)); // Should be much faster than 100ms
    }

    #[test]
    fn test_cache_bulk_operations() {
        let cache = LockFreeCache::new(100);

        let entries = (0..50)
            .map(|i| (format!("key_{i}"), format!("value_{i}")))
            .collect::<Vec<_>>();
        cache.insert_bulk(entries.clone());

        assert_eq!(cache.len(), 50);

        for (key, value) in entries {
            assert_eq!(cache.get(&key), Some(value));
        }
    }

    #[test]
    fn test_cache_utilization() {
        let cache = LockFreeCache::new(100);

        // Empty cache
        assert_eq!(cache.utilization(), 0.0);

        // Half full
        for i in 0..50 {
            cache.insert(format!("key_{i}"), format!("value_{i}"));
        }
        assert_eq!(cache.utilization(), 50.0);

        // Full cache
        for i in 50..100 {
            cache.insert(format!("key_{i}"), format!("value_{i}"));
        }
        assert_eq!(cache.utilization(), 100.0);
    }

    #[test]
    fn test_cache_expired_eviction() {
        let cache = LockFreeCache::new(100);

        cache.insert("key1".to_string(), "value1".to_string());
        cache.insert("key2".to_string(), "value2".to_string());
        assert_eq!(cache.len(), 2);

        // Sleep to ensure time passes
        thread::sleep(Duration::from_millis(100));

        // Evict entries older than 50 milliseconds
        cache.evict_expired(50);
        assert_eq!(cache.len(), 0);

        let stats = cache.stats();
        assert_eq!(stats.evictions, 2);
    }

    #[test]
    fn test_cache_stats() {
        let cache = LockFreeCache::new(100);

        // Initial stats
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.evictions, 0);
        assert_eq!(stats.inserts, 0);
        assert_eq!(stats.current_size, 0);
        assert_eq!(stats.max_size, 100);
        assert_eq!(stats.hit_rate, 0.0);

        // Perform operations
        cache.insert("key1".to_string(), "value1".to_string());
        cache.get(&"key1".to_string());
        cache.get(&"nonexistent".to_string());

        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.inserts, 1);
        assert_eq!(stats.current_size, 1);
        assert_eq!(stats.hit_rate, 0.5);
        assert_eq!(stats.total_requests(), 2);
        assert_eq!(stats.miss_rate(), 0.5);
        assert!(!stats.is_near_capacity());

        // Reset stats
        cache.reset_stats();
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 0);
    }
}

#[cfg(test)]
mod lock_free_scheduler_tests {
    use super::*;
    use futures::future::BoxFuture;

    #[tokio::test]
    async fn test_scheduler_basic_task_submission() {
        let scheduler = LockFreeTaskScheduler::new(4, 1024);

        let config = TaskConfig::new("test_task");
        let _receiver = scheduler
            .submit_task(config, || {
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    Ok(())
                }) as BoxFuture<'static, snp::error::Result<()>>
            })
            .unwrap();

        // Simulate worker stealing and executing the task
        let task = scheduler.steal_task(0).unwrap();
        assert_eq!(task.id, "test_task");

        // Execute the task
        if let Some(task_future) = task.task {
            let start = std::time::SystemTime::now();
            let result = task_future.await;
            let duration = start.elapsed().unwrap_or_default();

            assert!(result.is_ok());
            scheduler.mark_task_completed(0, &task.id, duration);
        }

        let stats = scheduler.get_stats();
        assert_eq!(stats.total_tasks_submitted, 1);
        assert_eq!(stats.total_tasks_completed, 1);
        assert_eq!(stats.pending_tasks, 0);
    }

    #[test]
    fn test_scheduler_worker_state_tracking() {
        let scheduler = LockFreeTaskScheduler::new(2, 1024);

        // Initially, no worker states
        assert!(scheduler.get_worker_state(0).is_none());

        // Submit a task and steal it
        let config = TaskConfig::new("test_task");
        scheduler
            .submit_task(config, || {
                Box::pin(async move { Ok(()) }) as BoxFuture<'static, snp::error::Result<()>>
            })
            .unwrap();

        let task = scheduler.steal_task(0).unwrap();

        // Worker should be in working state
        if let Some(WorkerState::Working { task_id, .. }) = scheduler.get_worker_state(0) {
            assert_eq!(task_id, "test_task");
        } else {
            panic!("Expected worker to be in working state");
        }

        // Mark task as completed
        scheduler.mark_task_completed(0, &task.id, Duration::from_millis(10));

        // Worker should be idle
        assert_eq!(scheduler.get_worker_state(0), Some(WorkerState::Idle));
    }

    #[test]
    fn test_scheduler_load_balance_metrics() {
        let scheduler = LockFreeTaskScheduler::new(3, 1024);

        // Submit tasks
        for i in 0..6 {
            let config = TaskConfig::new(format!("task_{i}"));
            scheduler
                .submit_task(config, || {
                    Box::pin(async move { Ok(()) }) as BoxFuture<'static, snp::error::Result<()>>
                })
                .unwrap();
        }

        let metrics = scheduler.get_load_balance_metrics();
        assert_eq!(metrics.total_queued_tasks, 6);
        assert!(!metrics.queue_lengths.is_empty());
    }

    #[tokio::test]
    async fn test_scheduler_batch_submission() {
        let scheduler = LockFreeTaskScheduler::new(2, 1024);

        let tasks = (0..5)
            .map(|i| {
                let config = TaskConfig::new(format!("batch_task_{i}"));
                (config, move || {
                    Box::pin(async move {
                        tokio::time::sleep(Duration::from_millis(1)).await;
                        Ok(())
                    }) as BoxFuture<'static, snp::error::Result<()>>
                })
            })
            .collect();

        let receivers = scheduler.submit_batch(tasks).unwrap();
        assert_eq!(receivers.len(), 5);
        assert_eq!(scheduler.pending_task_count(), 5);
    }

    #[tokio::test]
    #[serial] // Run this test in isolation to avoid interference
    async fn test_scheduler_shutdown_timeout() {
        let scheduler = LockFreeTaskScheduler::new(1, 1024);

        // Submit a task but don't process it
        let config = TaskConfig::new("unprocessed_task");
        scheduler
            .submit_task(config, || {
                Box::pin(async move { Ok(()) }) as BoxFuture<'static, snp::error::Result<()>>
            })
            .unwrap();

        // Shutdown should timeout
        let result = scheduler.shutdown(Duration::from_millis(10)).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_scheduler_statistics() {
        let scheduler = LockFreeTaskScheduler::new(4, 1024);

        // Initial stats
        let stats = scheduler.get_stats();
        assert_eq!(stats.total_tasks_submitted, 0);
        assert_eq!(stats.total_tasks_completed, 0);
        assert_eq!(stats.total_tasks_failed, 0);
        assert_eq!(stats.total_tasks_stolen, 0);
        assert_eq!(stats.pending_tasks, 0);
        assert_eq!(stats.active_workers, 0);
        assert_eq!(stats.average_task_duration_ms, 0.0);

        // Submit tasks
        for i in 0..3 {
            let config = TaskConfig::new(format!("task_{i}"));
            scheduler
                .submit_task(config, || {
                    Box::pin(async move { Ok(()) }) as BoxFuture<'static, snp::error::Result<()>>
                })
                .unwrap();
        }

        let stats = scheduler.get_stats();
        assert_eq!(stats.total_tasks_submitted, 3);
        assert_eq!(stats.pending_tasks, 3);

        // Reset stats
        scheduler.reset_stats();
        let stats = scheduler.get_stats();
        assert_eq!(stats.total_tasks_completed, 0);
        assert_eq!(stats.total_tasks_failed, 0);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_concurrency_executor_lock_free_integration() {
        let executor = ConcurrencyExecutor::with_lock_free_scheduler(4, ResourceLimits::default());

        assert!(executor.is_lock_free_enabled());

        // Should be able to get scheduler stats
        let stats = executor.get_scheduler_stats();
        assert!(stats.is_some());

        let stats = stats.unwrap();
        assert_eq!(stats.total_tasks_submitted, 0);
        assert_eq!(stats.pending_tasks, 0);
    }

    #[test]
    fn test_concurrency_executor_without_lock_free() {
        let executor = ConcurrencyExecutor::new(4, ResourceLimits::default());

        assert!(!executor.is_lock_free_enabled());
        assert!(executor.get_scheduler_stats().is_none());
    }

    #[tokio::test]
    async fn test_performance_comparison_setup() {
        // This test verifies the setup for performance comparisons
        // Actual benchmarks would be in benches/ directory

        let lock_free_executor =
            ConcurrencyExecutor::with_lock_free_scheduler(4, ResourceLimits::default());

        let regular_executor = ConcurrencyExecutor::new(4, ResourceLimits::default());

        // Both should be functional, just using different scheduling strategies
        assert!(lock_free_executor.is_lock_free_enabled());
        assert!(!regular_executor.is_lock_free_enabled());

        // Both should report resource usage
        let lf_usage = lock_free_executor.get_resource_usage();
        let reg_usage = regular_executor.get_resource_usage();

        assert_eq!(lf_usage.active_tasks, 0);
        assert_eq!(reg_usage.active_tasks, 0);
    }
}

#[cfg(test)]
mod stress_tests {
    use super::*;

    #[test]
    fn test_cache_high_concurrency_stress() {
        let cache = Arc::new(LockFreeCache::new(1000));
        let mut handles = vec![];

        // Spawn many threads for stress testing
        for i in 0..20 {
            let cache_clone = Arc::clone(&cache);
            let handle = thread::spawn(move || {
                for j in 0..500 {
                    let key = format!("key_{i}_{j}");
                    let value = format!("value_{i}_{j}");

                    // Mix of operations
                    cache_clone.insert(key.clone(), value.clone());
                    cache_clone.get(&key);
                    if j % 10 == 0 {
                        cache_clone.remove(&key);
                    }
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        let stats = cache.stats();
        assert!(stats.total_requests() > 0);
        assert!(stats.inserts > 0);
    }

    #[test]
    fn test_registry_concurrent_plugin_operations() {
        let registry = Arc::new(LanguageRegistry::new());
        let mut handles = vec![];

        // Spawn multiple threads to perform registry operations
        for i in 0..10 {
            let registry_clone = Arc::clone(&registry);
            let handle = thread::spawn(move || {
                for j in 0..100 {
                    // Update configuration
                    let config = RegistryConfig {
                        max_cache_size: 1000 + (i * 100),
                        enable_detection_cache: j % 2 == 0,
                        cache_ttl_seconds: 3600 + j,
                    };
                    registry_clone.update_config(config);

                    // Get configuration
                    let _ = registry_clone.get_config();

                    // Get stats
                    let _ = registry_clone.get_stats();

                    // Cache operations
                    registry_clone.clear_cache();
                    let _ = registry_clone.cache_size();
                }
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify final state is consistent
        let _stats = registry.get_stats();
        let config = registry.get_config();

        // Should not panic or have corrupted state
        assert!(config.max_cache_size >= 1000);
    }
}
