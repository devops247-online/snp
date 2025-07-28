// Multi-tier caching architecture for improved cache efficiency
// Implements L1 (hot), L2 (warm), and L3 (cold) storage tiers with promotion/demotion algorithms

use crate::error::Result;
use crate::storage::Store;
use dashmap::DashMap;
use lru::LruCache;
use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock as AsyncRwLock;
use tracing::debug;

/// Cache configuration for multi-tier system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    /// Maximum entries in L1 cache (fast HashMap)
    pub l1_max_entries: usize,
    /// Maximum entries in L2 cache (LRU)
    pub l2_max_entries: usize,
    /// Number of accesses required for promotion
    pub promotion_threshold: usize,
    /// Enable L3 persistent storage
    pub enable_l3_persistence: bool,
    /// Enable cache warming strategies
    pub cache_warming: bool,
    /// Enable metrics collection
    pub metrics_enabled: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            l1_max_entries: 1000,
            l2_max_entries: 10000,
            promotion_threshold: 3,
            enable_l3_persistence: true,
            cache_warming: true,
            metrics_enabled: true,
        }
    }
}

/// Cache tier enum for tracking value locations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheTier {
    L1,
    L2,
    L3,
}

/// Cache entry with metadata
#[derive(Debug, Clone)]
struct CacheEntry<V> {
    value: V,
    access_count: usize,
    last_accessed: Instant,
    created_at: Instant,
}

impl<V> CacheEntry<V> {
    fn new(value: V) -> Self {
        let now = Instant::now();
        Self {
            value,
            access_count: 1,
            last_accessed: now,
            created_at: now,
        }
    }

    fn accessed(&mut self) {
        self.access_count += 1;
        self.last_accessed = Instant::now();
    }
}

/// Comprehensive cache metrics
#[derive(Debug, Clone, Default)]
pub struct CacheMetrics {
    // Size metrics
    pub l1_size: usize,
    pub l2_size: usize,
    pub l3_size: usize,

    // Hit metrics
    pub l1_hits: u64,
    pub l2_hits: u64,
    pub l3_hits: u64,
    pub total_hits: u64,
    pub total_misses: u64,

    // Promotion metrics
    pub l2_to_l1_promotions: u64,
    pub l3_to_l2_promotions: u64,

    // Eviction metrics
    pub l1_evictions: u64,
    pub l2_evictions: u64,

    // Memory usage
    pub memory_usage_bytes: u64,
}

impl CacheMetrics {
    pub fn hit_rate(&self) -> f64 {
        let total_requests = self.total_hits + self.total_misses;
        if total_requests == 0 {
            0.0
        } else {
            self.total_hits as f64 / total_requests as f64
        }
    }

    pub fn l1_hit_rate(&self) -> f64 {
        let total_requests = self.total_hits + self.total_misses;
        if total_requests == 0 {
            0.0
        } else {
            self.l1_hits as f64 / total_requests as f64
        }
    }

    pub fn l2_hit_rate(&self) -> f64 {
        let total_requests = self.total_hits + self.total_misses;
        if total_requests == 0 {
            0.0
        } else {
            self.l2_hits as f64 / total_requests as f64
        }
    }

    pub fn l3_hit_rate(&self) -> f64 {
        let total_requests = self.total_hits + self.total_misses;
        if total_requests == 0 {
            0.0
        } else {
            self.l3_hits as f64 / total_requests as f64
        }
    }
}

/// Multi-tier cache implementation
pub struct MultiTierCache<K, V>
where
    K: Clone + Eq + Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    // L1: Hot data in fast DashMap for concurrent access
    l1_cache: Arc<DashMap<K, CacheEntry<V>>>,

    // L2: Warm data with LRU eviction
    l2_cache: Arc<AsyncRwLock<LruCache<K, CacheEntry<V>>>>,

    // L3: Cold data in persistent storage (SQLite)
    l3_cache: Option<Arc<Store>>,

    // Configuration
    config: CacheConfig,

    // Access tracking for promotion decisions
    access_counts: Arc<DashMap<K, usize>>,

    // Metrics
    metrics: Arc<AsyncRwLock<CacheMetrics>>,
}

impl<K, V> MultiTierCache<K, V>
where
    K: Clone + Eq + Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    /// Create a new multi-tier cache with default configuration
    pub async fn new() -> Result<Self> {
        Self::with_config(CacheConfig::default()).await
    }

    /// Create a new multi-tier cache with custom configuration
    pub async fn with_config(config: CacheConfig) -> Result<Self> {
        let l1_cache = Arc::new(DashMap::new());

        let l2_cache = Arc::new(AsyncRwLock::new(LruCache::new(
            NonZeroUsize::new(config.l2_max_entries).unwrap(),
        )));

        let l3_cache = if config.enable_l3_persistence {
            Some(Arc::new(Store::new()?))
        } else {
            None
        };

        Ok(Self {
            l1_cache,
            l2_cache,
            l3_cache,
            config,
            access_counts: Arc::new(DashMap::new()),
            metrics: Arc::new(AsyncRwLock::new(CacheMetrics::default())),
        })
    }

    /// Get a value from the cache, checking all tiers
    pub async fn get(&self, key: &K) -> Result<Option<V>> {
        // Check L1 first (fastest)
        if let Some(mut entry) = self.l1_cache.get_mut(key) {
            entry.accessed();
            if self.config.metrics_enabled {
                let mut metrics = self.metrics.write().await;
                metrics.l1_hits += 1;
                metrics.total_hits += 1;
            }
            debug!("Cache hit in L1 for key");
            return Ok(Some(entry.value.clone()));
        }

        // Check L2 (fast)
        {
            let mut l2_cache = self.l2_cache.write().await;
            if let Some(entry) = l2_cache.get_mut(key) {
                entry.accessed();
                let value = entry.value.clone();

                // Check if should promote to L1
                if entry.access_count >= self.config.promotion_threshold {
                    self.promote_to_l1(key.clone(), entry.clone()).await?;
                    l2_cache.pop(key); // Remove from L2
                }

                if self.config.metrics_enabled {
                    let mut metrics = self.metrics.write().await;
                    metrics.l2_hits += 1;
                    metrics.total_hits += 1;
                }
                debug!("Cache hit in L2 for key");
                return Ok(Some(value));
            }
        }

        // Check L3 (persistent)
        if let Some(l3_cache) = &self.l3_cache {
            if let Ok(Some(value)) = self.get_from_l3(key, l3_cache).await {
                // Promote to L2
                self.promote_to_l2(key.clone(), value.clone()).await?;

                if self.config.metrics_enabled {
                    let mut metrics = self.metrics.write().await;
                    metrics.l3_hits += 1;
                    metrics.total_hits += 1;
                    metrics.l3_to_l2_promotions += 1;
                }
                debug!("Cache hit in L3 for key, promoted to L2");
                return Ok(Some(value));
            }
        }

        // Cache miss
        if self.config.metrics_enabled {
            let mut metrics = self.metrics.write().await;
            metrics.total_misses += 1;
        }
        debug!("Cache miss for key");
        Ok(None)
    }

    /// Put a value into the cache (starts in L1)
    pub async fn put(&mut self, key: K, value: V) -> Result<()> {
        let entry = CacheEntry::new(value);

        // Check if L1 is at capacity
        if self.l1_cache.len() >= self.config.l1_max_entries {
            // Evict least recently used from L1 to L2
            if let Some(lru_key) = self.find_lru_in_l1().await {
                if let Some((evicted_key, evicted_entry)) = self.l1_cache.remove(&lru_key) {
                    self.demote_to_l2(evicted_key, evicted_entry).await?;
                }
            }
        }

        // Insert into L1
        self.l1_cache.insert(key, entry);

        if self.config.metrics_enabled {
            self.update_size_metrics().await;
        }

        Ok(())
    }

    /// Invalidate a key from all cache tiers
    pub async fn invalidate(&self, key: &K) -> Result<()> {
        // Remove from L1
        self.l1_cache.remove(key);

        // Remove from L2
        {
            let mut l2_cache = self.l2_cache.write().await;
            l2_cache.pop(key);
        }

        // Remove from L3
        if let Some(l3_cache) = &self.l3_cache {
            self.remove_from_l3(key, l3_cache).await?;
        }

        // Remove access tracking
        self.access_counts.remove(key);

        if self.config.metrics_enabled {
            self.update_size_metrics().await;
        }

        Ok(())
    }

    /// Clear all cache tiers
    pub async fn clear(&mut self) -> Result<()> {
        // Clear L1
        self.l1_cache.clear();

        // Clear L2
        {
            let mut l2_cache = self.l2_cache.write().await;
            l2_cache.clear();
        }

        // Clear L3
        if let Some(l3_cache) = &self.l3_cache {
            self.clear_l3(l3_cache).await?;
        }

        // Clear access tracking
        self.access_counts.clear();

        // Reset metrics
        if self.config.metrics_enabled {
            *self.metrics.write().await = CacheMetrics::default();
        }

        Ok(())
    }

    /// Get current cache metrics
    pub async fn get_metrics(&self) -> CacheMetrics {
        if self.config.metrics_enabled {
            let mut metrics = self.metrics.write().await;

            // Update size metrics
            metrics.l1_size = self.l1_cache.len();
            metrics.l2_size = {
                let l2_cache = self.l2_cache.read().await;
                l2_cache.len()
            };

            if let Some(_l3_cache) = &self.l3_cache {
                // This is a simplified size calculation - in a real implementation,
                // you'd query the actual L3 storage
                metrics.l3_size = 0; // Placeholder
            }

            // Estimate memory usage
            metrics.memory_usage_bytes = self.estimate_memory_usage().await;

            metrics.clone()
        } else {
            CacheMetrics::default()
        }
    }

    /// Get the cache configuration
    pub async fn get_config(&self) -> CacheConfig {
        self.config.clone()
    }

    /// Warm the cache by promoting frequently accessed keys
    pub async fn warm_cache(&mut self, keys: &[K]) -> Result<()> {
        if !self.config.cache_warming {
            return Ok(());
        }

        for key in keys {
            if let Some(l3_cache) = &self.l3_cache {
                if let Ok(Some(value)) = self.get_from_l3(key, l3_cache).await {
                    self.promote_to_l2(key.clone(), value).await?;
                }
            }
        }

        Ok(())
    }

    /// Evict entries older than the specified duration
    pub async fn evict_older_than(&mut self, max_age: Duration) -> Result<()> {
        let cutoff = Instant::now() - max_age;

        // Evict from L1
        let l1_keys_to_remove: Vec<K> = self
            .l1_cache
            .iter()
            .filter_map(|entry| {
                if entry.value().created_at < cutoff {
                    Some(entry.key().clone())
                } else {
                    None
                }
            })
            .collect();

        for key in l1_keys_to_remove {
            self.l1_cache.remove(&key);
        }

        // Evict from L2 (more complex due to LRU structure)
        {
            let mut l2_cache = self.l2_cache.write().await;
            let keys_to_remove: Vec<K> = l2_cache
                .iter()
                .filter_map(|(k, v)| {
                    if v.created_at < cutoff {
                        Some(k.clone())
                    } else {
                        None
                    }
                })
                .collect();

            for key in keys_to_remove {
                l2_cache.pop(&key);
            }
        }

        if self.config.metrics_enabled {
            self.update_size_metrics().await;
        }

        Ok(())
    }

    // Internal helper methods

    /// Get the tier where a value is currently stored
    pub async fn get_value_tier(&self, key: &K) -> Result<Option<CacheTier>> {
        if self.l1_cache.contains_key(key) {
            return Ok(Some(CacheTier::L1));
        }

        {
            let l2_cache = self.l2_cache.read().await;
            if l2_cache.contains(key) {
                return Ok(Some(CacheTier::L2));
            }
        }

        if let Some(l3_cache) = &self.l3_cache {
            if self.exists_in_l3(key, l3_cache).await? {
                return Ok(Some(CacheTier::L3));
            }
        }

        Ok(None)
    }

    /// Move a key from L1 to L2 (for testing)
    pub async fn move_to_l2(&mut self, key: K) -> Result<()> {
        if let Some((_, entry)) = self.l1_cache.remove(&key) {
            self.demote_to_l2(key, entry).await?;
        }
        Ok(())
    }

    /// Put a value directly in L3 (for testing)
    pub async fn put_in_l3(&mut self, key: K, value: V) -> Result<()> {
        if let Some(l3_cache) = &self.l3_cache {
            self.store_in_l3(&key, &value, l3_cache).await?;
        }
        Ok(())
    }

    async fn promote_to_l1(&self, key: K, entry: CacheEntry<V>) -> Result<()> {
        // Check if L1 has space
        if self.l1_cache.len() >= self.config.l1_max_entries {
            // Evict from L1 first
            if let Some(lru_key) = self.find_lru_in_l1().await {
                if let Some((evicted_key, evicted_entry)) = self.l1_cache.remove(&lru_key) {
                    self.demote_to_l2(evicted_key, evicted_entry).await?;
                }
            }
        }

        self.l1_cache.insert(key, entry);

        if self.config.metrics_enabled {
            let mut metrics = self.metrics.write().await;
            metrics.l2_to_l1_promotions += 1;
        }

        Ok(())
    }

    async fn promote_to_l2(&self, key: K, value: V) -> Result<()> {
        let entry = CacheEntry::new(value);

        {
            let mut l2_cache = self.l2_cache.write().await;

            // LruCache automatically handles eviction when at capacity
            if let Some((evicted_key, evicted_entry)) = l2_cache.push(key, entry) {
                // Evicted entry goes to L3
                if let Some(l3_cache) = &self.l3_cache {
                    self.store_in_l3(&evicted_key, &evicted_entry.value, l3_cache)
                        .await?;
                }

                if self.config.metrics_enabled {
                    let mut metrics = self.metrics.write().await;
                    metrics.l2_evictions += 1;
                }
            }
        }

        Ok(())
    }

    async fn demote_to_l2(&self, key: K, entry: CacheEntry<V>) -> Result<()> {
        {
            let mut l2_cache = self.l2_cache.write().await;

            if let Some((evicted_key, evicted_entry)) = l2_cache.push(key, entry) {
                // Evicted entry goes to L3
                if let Some(l3_cache) = &self.l3_cache {
                    self.store_in_l3(&evicted_key, &evicted_entry.value, l3_cache)
                        .await?;
                }

                if self.config.metrics_enabled {
                    let mut metrics = self.metrics.write().await;
                    metrics.l2_evictions += 1;
                }
            }
        }

        if self.config.metrics_enabled {
            let mut metrics = self.metrics.write().await;
            metrics.l1_evictions += 1;
        }

        Ok(())
    }

    async fn find_lru_in_l1(&self) -> Option<K> {
        let mut oldest_key = None;
        let mut oldest_time = Instant::now();

        for entry in self.l1_cache.iter() {
            if entry.value().last_accessed < oldest_time {
                oldest_time = entry.value().last_accessed;
                oldest_key = Some(entry.key().clone());
            }
        }

        oldest_key
    }

    async fn update_size_metrics(&self) {
        if self.config.metrics_enabled {
            let mut metrics = self.metrics.write().await;
            metrics.l1_size = self.l1_cache.len();

            {
                let l2_cache = self.l2_cache.read().await;
                metrics.l2_size = l2_cache.len();
            }

            // L3 size would be queried from storage in a real implementation
            metrics.l3_size = 0; // Placeholder
        }
    }

    async fn estimate_memory_usage(&self) -> u64 {
        // This is a rough estimation - in a real implementation,
        // you'd use more sophisticated memory tracking
        let l1_size = self.l1_cache.len() as u64;
        let l2_size = {
            let l2_cache = self.l2_cache.read().await;
            l2_cache.len() as u64
        };

        // Rough estimate: each entry uses ~100 bytes on average
        (l1_size + l2_size) * 100
    }

    // L3 storage operations (simplified interface)
    async fn get_from_l3(&self, _key: &K, _store: &Store) -> Result<Option<V>> {
        // In a real implementation, this would serialize the key and query the store
        // For now, return None to indicate no L3 data
        Ok(None)
    }

    async fn store_in_l3(&self, _key: &K, _value: &V, _store: &Store) -> Result<()> {
        // In a real implementation, this would serialize and store the key-value pair
        Ok(())
    }

    async fn exists_in_l3(&self, _key: &K, _store: &Store) -> Result<bool> {
        // In a real implementation, this would check if the key exists in storage
        Ok(false)
    }

    async fn remove_from_l3(&self, _key: &K, _store: &Store) -> Result<()> {
        // In a real implementation, this would remove the key from storage
        Ok(())
    }

    async fn clear_l3(&self, _store: &Store) -> Result<()> {
        // In a real implementation, this would clear all cache data from storage
        Ok(())
    }
}

// Thread-safe clone implementation
impl<K, V> Clone for MultiTierCache<K, V>
where
    K: Clone + Eq + Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn clone(&self) -> Self {
        Self {
            l1_cache: Arc::clone(&self.l1_cache),
            l2_cache: Arc::clone(&self.l2_cache),
            l3_cache: self.l3_cache.clone(),
            config: self.config.clone(),
            access_counts: Arc::clone(&self.access_counts),
            metrics: Arc::clone(&self.metrics),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cache_creation() {
        let cache = MultiTierCache::<String, String>::new().await;
        assert!(cache.is_ok());
    }

    #[tokio::test]
    async fn test_basic_operations() {
        let mut cache = MultiTierCache::<String, String>::new().await.unwrap();

        // Test put and get
        cache
            .put("test_key".to_string(), "test_value".to_string())
            .await
            .unwrap();
        let result = cache.get(&"test_key".to_string()).await.unwrap();
        assert_eq!(result, Some("test_value".to_string()));

        // Test cache miss
        let result = cache.get(&"missing_key".to_string()).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_metrics() {
        let mut cache = MultiTierCache::<String, String>::new().await.unwrap();

        cache
            .put("key1".to_string(), "value1".to_string())
            .await
            .unwrap();
        cache.get(&"key1".to_string()).await.unwrap(); // Hit
        cache.get(&"missing".to_string()).await.unwrap(); // Miss

        let metrics = cache.get_metrics().await;
        assert_eq!(metrics.total_hits, 1);
        assert_eq!(metrics.total_misses, 1);
        assert_eq!(metrics.l1_hits, 1);
    }
}
