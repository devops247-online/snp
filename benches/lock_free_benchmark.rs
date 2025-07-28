// Benchmarks comparing lock-based vs lock-free performance
// Measures concurrency improvements and throughput gains

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;

use snp::{language::LanguageRegistry, lock_free_cache::LockFreeCache, RegistryConfig};

/// Traditional mutex-based cache for comparison
struct MutexCache<K, V> {
    storage: Arc<Mutex<HashMap<K, V>>>,
    max_size: usize,
}

impl<K, V> MutexCache<K, V>
where
    K: Clone + Eq + std::hash::Hash,
    V: Clone,
{
    fn new(max_size: usize) -> Self {
        Self {
            storage: Arc::new(Mutex::new(HashMap::new())),
            max_size,
        }
    }

    fn insert(&self, key: K, value: V) -> Option<V> {
        let mut storage = self.storage.lock().unwrap();
        let old_value = storage.insert(key, value);

        // Simple eviction if over capacity
        if storage.len() > self.max_size {
            if let Some(first_key) = storage.keys().next().cloned() {
                storage.remove(&first_key);
            }
        }

        old_value
    }

    fn get(&self, key: &K) -> Option<V> {
        let storage = self.storage.lock().unwrap();
        storage.get(key).cloned()
    }
}

fn bench_cache_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("cache_operations");

    // Test different numbers of threads
    for thread_count in [1, 2, 4, 8, 16].iter() {
        group.throughput(Throughput::Elements(*thread_count as u64 * 1000));

        // Lock-free cache benchmark
        group.bench_with_input(
            BenchmarkId::new("lock_free_cache", thread_count),
            thread_count,
            |b, &thread_count| {
                b.iter(|| {
                    let cache = Arc::new(LockFreeCache::new(1000));
                    let mut handles = vec![];

                    for i in 0..thread_count {
                        let cache_clone = Arc::clone(&cache);
                        let handle = thread::spawn(move || {
                            for j in 0..1000 {
                                let key = format!("key_{i}_{j}");
                                let value = format!("value_{i}_{j}");
                                cache_clone.insert(key.clone(), value);
                                std::hint::black_box(cache_clone.get(&key));
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );

        // Mutex-based cache benchmark
        group.bench_with_input(
            BenchmarkId::new("mutex_cache", thread_count),
            thread_count,
            |b, &thread_count| {
                b.iter(|| {
                    let cache = Arc::new(MutexCache::new(1000));
                    let mut handles = vec![];

                    for i in 0..thread_count {
                        let cache_clone = Arc::clone(&cache);
                        let handle = thread::spawn(move || {
                            for j in 0..1000 {
                                let key = format!("key_{i}_{j}");
                                let value = format!("value_{i}_{j}");
                                cache_clone.insert(key.clone(), value);
                                std::hint::black_box(cache_clone.get(&key));
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_registry_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("registry_operations");

    for thread_count in [1, 2, 4, 8].iter() {
        group.throughput(Throughput::Elements(*thread_count as u64 * 500));

        // Lock-free registry (already using DashMap)
        group.bench_with_input(
            BenchmarkId::new("lock_free_registry", thread_count),
            thread_count,
            |b, &thread_count| {
                b.iter(|| {
                    let registry = Arc::new(LanguageRegistry::new());
                    let mut handles = vec![];

                    for _i in 0..thread_count {
                        let registry_clone = Arc::clone(&registry);
                        let handle = thread::spawn(move || {
                            for j in 0..500 {
                                // Configuration updates
                                let config = RegistryConfig {
                                    max_cache_size: 1000 + j,
                                    enable_detection_cache: j % 2 == 0,
                                    cache_ttl_seconds: 3600,
                                };
                                registry_clone.update_config(config);

                                // Get configuration and stats
                                std::hint::black_box(registry_clone.get_config());
                                std::hint::black_box(registry_clone.get_stats());

                                // Cache operations
                                if j % 100 == 0 {
                                    registry_clone.clear_cache();
                                }
                                std::hint::black_box(registry_clone.cache_size());
                            }
                        });
                        handles.push(handle);
                    }

                    for handle in handles {
                        handle.join().unwrap();
                    }
                });
            },
        );
    }

    group.finish();
}

fn bench_memory_usage(c: &mut Criterion) {
    let mut group = c.benchmark_group("memory_usage");

    // Compare memory efficiency between lock-free and mutex-based caches
    for entry_count in [1000, 5000, 10000].iter() {
        group.bench_with_input(
            BenchmarkId::new("lock_free_cache_memory", entry_count),
            entry_count,
            |b, &entry_count| {
                b.iter(|| {
                    let cache = LockFreeCache::new(entry_count);

                    for i in 0..entry_count {
                        cache.insert(format!("key_{i}"), format!("value_{i}"));
                    }

                    // Access all entries to ensure they're kept in memory
                    for i in 0..entry_count {
                        std::hint::black_box(cache.get(&format!("key_{i}")));
                    }

                    std::hint::black_box(cache.len());
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("mutex_cache_memory", entry_count),
            entry_count,
            |b, &entry_count| {
                b.iter(|| {
                    let cache = MutexCache::new(entry_count);

                    for i in 0..entry_count {
                        cache.insert(format!("key_{i}"), format!("value_{i}"));
                    }

                    // Access all entries
                    for i in 0..entry_count {
                        std::hint::black_box(cache.get(&format!("key_{i}")));
                    }
                });
            },
        );
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_cache_operations,
    bench_registry_operations,
    bench_memory_usage
);

criterion_main!(benches);
