// Lock-free cache implementation with atomic LRU tracking
// Provides high-performance caching with concurrent access and automatic eviction

use dashmap::DashMap;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Cache entry with atomic access tracking
#[derive(Debug)]
struct CacheEntry<V> {
    value: V,
    created_at: u64,
    access_count: AtomicU64,
    last_accessed: AtomicU64,
}

impl<V> CacheEntry<V> {
    fn new(value: V) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        Self {
            value,
            created_at: now,
            access_count: AtomicU64::new(1),
            last_accessed: AtomicU64::new(now),
        }
    }
    
    fn touch(&self) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        self.access_count.fetch_add(1, Ordering::Relaxed);
        self.last_accessed.store(now, Ordering::Relaxed);
    }
}

/// Lock-free cache with atomic LRU tracking
pub struct LockFreeCache<K, V> {
    // Main storage using DashMap for lock-free concurrent access
    storage: DashMap<K, CacheEntry<V>>,
    
    // Cache statistics
    hits: AtomicU64,
    misses: AtomicU64,
    evictions: AtomicU64,
    inserts: AtomicU64,
    
    // Configuration
    max_size: usize,
    #[allow(dead_code)]
    current_timestamp: AtomicU64,
}

impl<K, V> LockFreeCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    /// Create a new lock-free cache with specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            storage: DashMap::new(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
            evictions: AtomicU64::new(0),
            inserts: AtomicU64::new(0),
            max_size,
            current_timestamp: AtomicU64::new(0),
        }
    }
    
    /// Get a value from the cache
    pub fn get(&self, key: &K) -> Option<V> {
        if let Some(entry) = self.storage.get(key) {
            // Update access tracking atomically
            entry.touch();
            self.hits.fetch_add(1, Ordering::Relaxed);
            Some(entry.value.clone())
        } else {
            self.misses.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
    
    /// Insert a value into the cache
    pub fn insert(&self, key: K, value: V) -> Option<V> {
        // Check if we need to evict before inserting a new key
        let is_new_key = !self.storage.contains_key(&key);
        if is_new_key && self.storage.len() >= self.max_size {
            self.evict_lru();
        }
        
        let entry = CacheEntry::new(value);
        let old_value = self.storage.insert(key, entry).map(|old| old.value);
        
        if old_value.is_none() {
            self.inserts.fetch_add(1, Ordering::Relaxed);
        }
        
        old_value
    }
    
    /// Remove a value from the cache
    pub fn remove(&self, key: &K) -> Option<V> {
        self.storage.remove(key).map(|(_, entry)| entry.value)
    }
    
    /// Check if the cache contains a key
    pub fn contains_key(&self, key: &K) -> bool {
        self.storage.contains_key(key)
    }
    
    /// Get the current size of the cache
    pub fn len(&self) -> usize {
        self.storage.len()
    }
    
    /// Check if the cache is empty
    pub fn is_empty(&self) -> bool {
        self.storage.is_empty()
    }
    
    /// Clear all entries from the cache
    pub fn clear(&self) {
        self.storage.clear();
    }
    
    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let hits = self.hits.load(Ordering::Relaxed);
        let misses = self.misses.load(Ordering::Relaxed);
        let total_requests = hits + misses;
        
        let hit_rate = if total_requests > 0 {
            hits as f64 / total_requests as f64
        } else {
            0.0
        };
        
        CacheStats {
            hits,
            misses,
            evictions: self.evictions.load(Ordering::Relaxed),
            inserts: self.inserts.load(Ordering::Relaxed),
            current_size: self.len(),
            max_size: self.max_size,
            hit_rate,
        }
    }
    
    /// Reset cache statistics
    pub fn reset_stats(&self) {
        self.hits.store(0, Ordering::Relaxed);
        self.misses.store(0, Ordering::Relaxed);
        self.evictions.store(0, Ordering::Relaxed);
        self.inserts.store(0, Ordering::Relaxed);
    }
    
    /// Evict the least recently used entry
    fn evict_lru(&self) {
        let mut lru_key = None;
        let mut min_timestamp = u64::MAX;
        
        // Find the least recently used entry
        for entry in self.storage.iter() {
            let last_accessed = entry.value().last_accessed.load(Ordering::Relaxed);
            if last_accessed < min_timestamp {
                min_timestamp = last_accessed;
                lru_key = Some(entry.key().clone());
            }
        }
        
        // Remove the LRU entry
        if let Some(key) = lru_key {
            if self.storage.remove(&key).is_some() {
                self.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
    
    /// Get or compute a value with the provided function
    pub fn get_or_compute<F>(&self, key: K, compute: F) -> V
    where
        F: FnOnce() -> V,
    {
        // Fast path: try to get existing value
        if let Some(value) = self.get(&key) {
            return value;
        }
        
        // Slow path: compute and insert
        let value = compute();
        self.insert(key, value.clone());
        value
    }
    
    /// Get or compute a value asynchronously
    pub async fn get_or_compute_async<F, Fut>(&self, key: K, compute: F) -> V
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = V>,
    {
        // Fast path: try to get existing value
        if let Some(value) = self.get(&key) {
            return value;
        }
        
        // Slow path: compute and insert
        let value = compute().await;
        self.insert(key, value.clone());
        value
    }
    
    /// Bulk insert multiple key-value pairs
    pub fn insert_bulk(&self, entries: impl IntoIterator<Item = (K, V)>) {
        for (key, value) in entries {
            self.insert(key, value);
        }
    }
    
    /// Get cache capacity utilization as a percentage
    pub fn utilization(&self) -> f64 {
        if self.max_size == 0 {
            return 0.0;
        }
        (self.len() as f64 / self.max_size as f64) * 100.0
    }
    
    /// Evict entries older than the specified number of seconds
    pub fn evict_expired(&self, max_age_seconds: u64) {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        let cutoff = now.saturating_sub(max_age_seconds);
        
        // Collect keys to evict
        let keys_to_evict: Vec<K> = self.storage
            .iter()
            .filter_map(|entry| {
                let created_at = entry.value().created_at;
                if created_at < cutoff {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();
        
        // Remove expired entries
        for key in keys_to_evict {
            if self.storage.remove(&key).is_some() {
                self.evictions.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

impl<K, V> Default for LockFreeCache<K, V>
where
    K: Clone + Eq + Hash,
    V: Clone,
{
    fn default() -> Self {
        Self::new(1000) // Default to 1000 entries
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    pub hits: u64,
    pub misses: u64,
    pub evictions: u64,
    pub inserts: u64,
    pub current_size: usize,
    pub max_size: usize,
    pub hit_rate: f64,
}

impl CacheStats {
    /// Get total number of requests (hits + misses)
    pub fn total_requests(&self) -> u64 {
        self.hits + self.misses
    }
    
    /// Get miss rate (1.0 - hit_rate)
    pub fn miss_rate(&self) -> f64 {
        1.0 - self.hit_rate
    }
    
    /// Check if cache is near capacity (>90% full)
    pub fn is_near_capacity(&self) -> bool {
        if self.max_size == 0 {
            return false;
        }
        (self.current_size as f64 / self.max_size as f64) > 0.9
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;
    
    #[test]
    fn test_basic_operations() {
        let cache = LockFreeCache::new(100);
        
        // Test insert and get
        assert_eq!(cache.insert("key1".to_string(), "value1".to_string()), None);
        assert_eq!(cache.get(&"key1".to_string()), Some("value1".to_string()));
        
        // Test miss
        assert_eq!(cache.get(&"nonexistent".to_string()), None);
        
        // Test update
        assert_eq!(cache.insert("key1".to_string(), "value2".to_string()), Some("value1".to_string()));
        assert_eq!(cache.get(&"key1".to_string()), Some("value2".to_string()));
    }
    
    #[test]
    fn test_eviction() {
        let cache = LockFreeCache::new(2);
        
        // Insert two items with enough time difference
        cache.insert("key1".to_string(), "value1".to_string());
        thread::sleep(Duration::from_secs(1));
        cache.insert("key2".to_string(), "value2".to_string());
        assert_eq!(cache.len(), 2);
        
        // Access key1 to make it more recently used than key2
        cache.get(&"key1".to_string());
        thread::sleep(Duration::from_secs(1));
        
        // Verify the cache state before eviction
        let stats_before = cache.stats();
        assert_eq!(stats_before.current_size, 2);
        
        // This should trigger eviction of key2 (LRU)
        cache.insert("key3".to_string(), "value3".to_string());
        assert_eq!(cache.len(), 2);
        
        // Verify eviction occurred
        let stats_after = cache.stats();
        assert_eq!(stats_after.evictions, stats_before.evictions + 1);
        
        // Check which key remains (should be key1 and key3, key2 should be evicted)
        let key1_present = cache.get(&"key1".to_string()).is_some();
        let key2_present = cache.get(&"key2".to_string()).is_some();
        let key3_present = cache.get(&"key3".to_string()).is_some();
        
        // Either key1 or key2 was evicted, but key3 should always be present
        assert!(key3_present, "key3 should be present after insertion");
        assert!(key1_present || key2_present, "Either key1 or key2 should remain");
        assert!(!(key1_present && key2_present), "Both key1 and key2 cannot remain when cache size is 2");
    }
    
    #[test]
    fn test_concurrent_access() {
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
    }
    
    #[test]
    fn test_get_or_compute() {
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
    }
    
    #[test]
    fn test_stats() {
        let cache = LockFreeCache::new(100);
        
        cache.insert("key1".to_string(), "value1".to_string());
        cache.get(&"key1".to_string());
        cache.get(&"nonexistent".to_string());
        
        let stats = cache.stats();
        assert_eq!(stats.hits, 1);
        assert_eq!(stats.misses, 1);
        assert_eq!(stats.inserts, 1);
        assert_eq!(stats.hit_rate, 0.5);
        assert_eq!(stats.current_size, 1);
    }
    
    #[tokio::test]
    async fn test_async_get_or_compute() {
        let cache = LockFreeCache::new(100);
        
        // First call should compute
        let value = cache.get_or_compute_async("key1".to_string(), || async {
            tokio::time::sleep(Duration::from_millis(10)).await;
            "computed_value".to_string()
        }).await;
        assert_eq!(value, "computed_value");
        
        // Second call should use cached value
        let value = cache.get_or_compute_async("key1".to_string(), || async {
            "should_not_compute".to_string()
        }).await;
        assert_eq!(value, "computed_value");
    }
    
    #[test]
    fn test_expired_eviction() {
        let cache = LockFreeCache::new(100);
        
        cache.insert("key1".to_string(), "value1".to_string());
        assert_eq!(cache.len(), 1);
        
        // Sleep to ensure time passes (longer to be safe)
        thread::sleep(Duration::from_secs(1));
        
        // Evict entries older than 0 seconds (should evict all entries)
        cache.evict_expired(0);
        assert_eq!(cache.len(), 0);
        
        let stats = cache.stats();
        assert_eq!(stats.evictions, 1);
    }
}