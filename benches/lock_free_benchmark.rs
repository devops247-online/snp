// Benchmarks comparing lock-based vs lock-free performance
// Measures concurrency improvements and throughput gains

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use std::thread;
use std::time::Duration;
use tokio::sync::RwLock;

use snp::{
    language::{LanguageRegistry, RegistryConfig},
    lock_free_cache::LockFreeCache,
    lock_free_scheduler::LockFreeTaskScheduler,
    concurrency::{ConcurrencyExecutor, ResourceLimits, TaskConfig},
};

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

/// Traditional RwLock-based task registry for comparison
struct RwLockTaskRegistry {
    tasks: Arc<RwLock<HashMap<String, String>>>, // Simple string->string for benchmarking
}

impl RwLockTaskRegistry {
    fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    async fn insert(&self, key: String, value: String) {
        let mut tasks = self.tasks.write().await;
        tasks.insert(key, value);
    }
    
    async fn get(&self, key: &str) -> Option<String> {
        let tasks = self.tasks.read().await;
        tasks.get(key).cloned()
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
                                let key = format!("key_{}_{}", i, j);
                                let value = format!("value_{}_{}", i, j);
                                cache_clone.insert(key.clone(), value);
                                black_box(cache_clone.get(&key));
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
                                let key = format!("key_{}_{}", i, j);
                                let value = format!("value_{}_{}", i, j);
                                cache_clone.insert(key.clone(), value);
                                black_box(cache_clone.get(&key));
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
                    
                    for i in 0..thread_count {
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
                                black_box(registry_clone.get_config());
                                black_box(registry_clone.get_stats());
                                
                                // Cache operations
                                if j % 100 == 0 {
                                    registry_clone.clear_cache();
                                }
                                black_box(registry_clone.cache_size());
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

fn bench_task_registry_operations(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("task_registry_operations");
    
    for thread_count in [1, 2, 4, 8].iter() {
        group.throughput(Throughput::Elements(*thread_count as u64 * 1000));
        
        // RwLock-based registry
        group.bench_with_input(
            BenchmarkId::new("rwlock_registry", thread_count),
            thread_count,
            |b, &thread_count| {
                b.to_async(&rt).iter(|| async {
                    let registry = Arc::new(RwLockTaskRegistry::new());
                    let mut handles = vec![];
                    
                    for i in 0..*thread_count {
                        let registry_clone = Arc::clone(&registry);
                        let handle = tokio::spawn(async move {
                            for j in 0..1000 {
                                let key = format!("task_{}_{}", i, j);
                                let value = format!("state_{}", j);
                                registry_clone.insert(key.clone(), value).await;
                                black_box(registry_clone.get(&key).await);
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        handle.await.unwrap();
                    }
                });
            },
        );
    }
    
    group.finish();
}

fn bench_scheduler_task_submission(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("scheduler_task_submission");
    
    for task_count in [100, 500, 1000, 2000].iter() {
        group.throughput(Throughput::Elements(*task_count as u64));
        
        // Lock-free scheduler
        group.bench_with_input(
            BenchmarkId::new("lock_free_scheduler", task_count),
            task_count,
            |b, &task_count| {
                b.to_async(&rt).iter(|| async {
                    let scheduler = Arc::new(LockFreeTaskScheduler::new(4, 1024));
                    
                    // Submit tasks
                    let mut receivers = vec![];
                    for i in 0..task_count {
                        let config = TaskConfig::new(format!("task_{}", i));
                        let receiver = scheduler.submit_task(config, || {
                            Box::pin(async move {
                                // Minimal work
                                tokio::task::yield_now().await;
                                Ok(())
                            })
                        }).unwrap();
                        receivers.push(receiver);
                    }
                    
                    // Simulate workers processing tasks
                    let scheduler_clone = Arc::clone(&scheduler);
                    let _worker_handle = tokio::spawn(async move {
                        while scheduler_clone.has_pending_tasks() {
                            for worker_id in 0..4 {
                                if let Some(task) = scheduler_clone.steal_task(worker_id) {
                                    if let Some(mut task_future) = task.task {
                                        let start = std::time::SystemTime::now();
                                        let result = task_future.await;
                                        let duration = start.elapsed().unwrap_or_default();
                                        
                                        if result.is_ok() {
                                            scheduler_clone.mark_task_completed(worker_id, &task.id, duration);
                                        } else {
                                            scheduler_clone.mark_task_failed(worker_id, &task.id);
                                        }
                                    }
                                }
                            }
                            tokio::task::yield_now().await;
                        }
                    });
                    
                    // Wait for all tasks to complete with timeout
                    let _ = tokio::time::timeout(
                        Duration::from_secs(10),
                        scheduler.shutdown(Duration::from_secs(5))
                    ).await;
                });
            },
        );
    }
    
    group.finish();
}

fn bench_concurrent_executor_comparison(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("executor_comparison");
    
    for task_count in [50, 100, 200].iter() {
        group.throughput(Throughput::Elements(*task_count as u64));
        
        // Regular executor
        group.bench_with_input(
            BenchmarkId::new("regular_executor", task_count),
            task_count,
            |b, &task_count| {
                b.to_async(&rt).iter(|| async {
                    let executor = ConcurrencyExecutor::new(4, ResourceLimits::default());
                    let mut handles = vec![];
                    
                    for i in 0..task_count {
                        let config = TaskConfig::new(format!("task_{}", i));
                        let handle = tokio::spawn({
                            let executor = executor.clone();
                            async move {
                                executor.execute_task(config, || {
                                    Box::pin(async move {
                                        tokio::task::yield_now().await;
                                        Ok(())
                                    })
                                }).await
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        let _ = handle.await;
                    }
                });
            },
        );
        
        // Lock-free enhanced executor
        group.bench_with_input(
            BenchmarkId::new("lock_free_executor", task_count),
            task_count,
            |b, &task_count| {
                b.to_async(&rt).iter(|| async {
                    let executor = ConcurrencyExecutor::with_lock_free_scheduler(4, ResourceLimits::default());
                    let mut handles = vec![];
                    
                    for i in 0..task_count {
                        let config = TaskConfig::new(format!("task_{}", i));
                        let handle = tokio::spawn({
                            let executor = executor.clone();
                            async move {
                                executor.execute_task(config, || {
                                    Box::pin(async move {
                                        tokio::task::yield_now().await;
                                        Ok(())
                                    })
                                }).await
                            }
                        });
                        handles.push(handle);
                    }
                    
                    for handle in handles {
                        let _ = handle.await;
                    }
                });
            },
        );
    }
    
    group.finish();
}

fn bench_cache_get_or_compute(c: &mut Criterion) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let mut group = c.benchmark_group("cache_get_or_compute");
    
    for hit_rate in [0.0, 0.5, 0.9].iter() {
        group.bench_with_input(
            BenchmarkId::new("lock_free_cache_hit_rate", (hit_rate * 100.0) as u32),
            hit_rate,
            |b, &hit_rate| {
                b.to_async(&rt).iter(|| async {
                    let cache = LockFreeCache::new(1000);
                    
                    // Pre-populate cache based on hit rate
                    let total_keys = 1000;
                    let cached_keys = (total_keys as f64 * hit_rate) as usize;
                    
                    for i in 0..cached_keys {
                        cache.insert(format!("key_{}", i), format!("value_{}", i));
                    }
                    
                    // Perform get_or_compute operations
                    for i in 0..total_keys {
                        let key = format!("key_{}", i);
                        let value = cache.get_or_compute_async(key, || async {
                            // Simulate computation delay
                            tokio::task::yield_now().await;
                            format!("computed_value_{}", i)
                        }).await;
                        black_box(value);
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
                        cache.insert(format!("key_{}", i), format!("value_{}", i));
                    }
                    
                    // Access all entries to ensure they're kept in memory
                    for i in 0..entry_count {
                        black_box(cache.get(&format!("key_{}", i)));
                    }
                    
                    black_box(cache.len());
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
                        cache.insert(format!("key_{}", i), format!("value_{}", i));
                    }
                    
                    // Access all entries
                    for i in 0..entry_count {
                        black_box(cache.get(&format!("key_{}", i)));
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
    bench_task_registry_operations,
    bench_scheduler_task_submission,
    bench_concurrent_executor_comparison,
    bench_cache_get_or_compute,
    bench_memory_usage
);

criterion_main!(benches);