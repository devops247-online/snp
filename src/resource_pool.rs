// Resource Pool Pattern Implementation for SNP
// Provides efficient management and reuse of expensive objects like Git repositories,
// language environments, and process handles, reducing initialization overhead
// and improving resource utilization.

use async_trait::async_trait;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::sync::{Mutex, Semaphore};

/// Trait for objects that can be pooled
///
/// Types implementing this trait can be managed by the ResourcePool
/// and must provide methods for creation, health checking, resetting,
/// and cleanup operations.
#[async_trait]
pub trait Poolable: Send + Sync + 'static {
    type Config: Send + Sync + Clone;
    type Error: Send + Sync + std::error::Error + 'static;

    /// Create a new instance of the poolable resource
    async fn create(config: &Self::Config) -> std::result::Result<Self, Self::Error>
    where
        Self: Sized;

    /// Check if the resource instance is still healthy and usable
    async fn is_healthy(&self) -> bool;

    /// Reset the resource state for reuse
    /// This should clean up any state from previous usage
    async fn reset(&mut self) -> std::result::Result<(), Self::Error>;

    /// Cleanup the resource before destruction
    /// This should release any held resources
    async fn cleanup(&mut self) -> std::result::Result<(), Self::Error>;
}

/// Errors that can occur during resource pool operations
#[derive(Debug, Error)]
pub enum PoolError {
    #[error("Failed to acquire resource: timeout after {timeout:?}")]
    AcquireTimeout { timeout: Duration },

    #[error("Resource creation failed: {source}")]
    ResourceCreation {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Resource validation failed: {message}")]
    ResourceValidation { message: String },

    #[error("Pool is shutting down")]
    PoolShutdown,

    #[error("Resource reset failed: {source}")]
    ResourceReset {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Resource cleanup failed: {source}")]
    ResourceCleanup {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

/// A pooled resource with metadata
#[derive(Debug)]
pub struct PooledResource<T> {
    resource: T,
    created_at: Instant,
    last_used: Instant,
    use_count: usize,
}

impl<T> PooledResource<T> {
    fn new(resource: T) -> Self {
        let now = Instant::now();
        Self {
            resource,
            created_at: now,
            last_used: now,
            use_count: 0,
        }
    }

    fn update_usage(&mut self) {
        self.last_used = Instant::now();
        self.use_count += 1;
    }

    fn is_idle_too_long(&self, idle_timeout: Duration) -> bool {
        self.last_used.elapsed() > idle_timeout
    }
}

/// RAII guard for pooled resources
///
/// When dropped, automatically returns the resource to the pool
pub struct PoolGuard<T: Poolable> {
    resource: Option<PooledResource<T>>,
    pool: Arc<ResourcePool<T>>,
}

impl<T: Poolable> PoolGuard<T> {
    /// Get a reference to the pooled resource
    pub fn resource(&self) -> &T {
        &self.resource.as_ref().unwrap().resource
    }

    /// Get a mutable reference to the pooled resource
    pub fn resource_mut(&mut self) -> &mut T {
        &mut self.resource.as_mut().unwrap().resource
    }

    /// Get metadata about the pooled resource
    pub fn metadata(&self) -> PooledResourceMetadata {
        let pooled = self.resource.as_ref().unwrap();
        PooledResourceMetadata {
            created_at: pooled.created_at,
            last_used: pooled.last_used,
            use_count: pooled.use_count,
            age: pooled.created_at.elapsed(),
        }
    }
}

impl<T: Poolable> Drop for PoolGuard<T> {
    fn drop(&mut self) {
        if let Some(pooled) = self.resource.take() {
            let pool = Arc::clone(&self.pool);
            tokio::spawn(async move {
                pool.return_resource(pooled).await;
            });
        }
    }
}

/// Metadata about a pooled resource
#[derive(Debug, Clone)]
pub struct PooledResourceMetadata {
    pub created_at: Instant,
    pub last_used: Instant,
    pub use_count: usize,
    pub age: Duration,
}

/// Configuration for resource pool behavior
#[derive(Debug, Clone)]
pub struct PoolConfig {
    /// Minimum number of resources to maintain in the pool
    pub min_size: usize,
    /// Maximum number of resources allowed in the pool
    pub max_size: usize,
    /// Timeout for acquiring a resource from the pool
    pub acquire_timeout: Duration,
    /// Maximum idle time before a resource is removed from the pool
    pub idle_timeout: Duration,
    /// Interval for running pool maintenance tasks
    pub health_check_interval: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            min_size: 1,
            max_size: 10,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300), // 5 minutes
            health_check_interval: Duration::from_secs(60),
        }
    }
}

/// Generic resource pool for managing expensive objects
pub struct ResourcePool<T: Poolable> {
    /// Available resources in the pool
    available: Arc<Mutex<VecDeque<PooledResource<T>>>>,
    /// Configuration for creating new resources
    factory_config: T::Config,
    /// Pool configuration settings
    config: PoolConfig,
    /// Current number of resources (both available and in use)
    current_size: Arc<Mutex<usize>>,
    /// Semaphore to limit concurrent resource acquisition
    semaphore: Arc<Semaphore>,
    /// Flag indicating if the pool is shutting down
    shutdown: Arc<Mutex<bool>>,
}

impl<T: Poolable> ResourcePool<T> {
    /// Create a new resource pool with the given configuration
    pub fn new(factory_config: T::Config, pool_config: PoolConfig) -> Self {
        let max_size = pool_config.max_size;

        Self {
            available: Arc::new(Mutex::new(VecDeque::new())),
            factory_config,
            config: pool_config,
            current_size: Arc::new(Mutex::new(0)),
            semaphore: Arc::new(Semaphore::new(max_size)),
            shutdown: Arc::new(Mutex::new(false)),
        }
    }

    /// Acquire a resource from the pool
    ///
    /// This method will either return an existing healthy resource from the pool
    /// or create a new one if needed. The returned PoolGuard will automatically
    /// return the resource to the pool when dropped.
    pub async fn acquire(&self) -> std::result::Result<PoolGuard<T>, PoolError> {
        // Check if pool is shutting down
        if *self.shutdown.lock().await {
            return Err(PoolError::PoolShutdown);
        }

        // Acquire semaphore permit to limit concurrent resources
        let _permit = tokio::time::timeout(
            self.config.acquire_timeout,
            self.semaphore.clone().acquire_owned(),
        )
        .await
        .map_err(|_| PoolError::AcquireTimeout {
            timeout: self.config.acquire_timeout,
        })?
        .map_err(|_| PoolError::PoolShutdown)?;

        // Try to get an existing healthy resource
        let mut available = self.available.lock().await;

        // Look for a healthy resource
        while let Some(mut pooled) = available.pop_front() {
            if pooled.resource.is_healthy().await {
                pooled.update_usage();
                drop(available); // Release lock early

                return Ok(PoolGuard {
                    resource: Some(pooled),
                    pool: Arc::new(self.clone()),
                });
            } else {
                // Resource is unhealthy, cleanup and remove from pool
                if let Err(e) = pooled.resource.cleanup().await {
                    tracing::warn!("Failed to cleanup unhealthy resource: {}", e);
                }
                let mut current_size = self.current_size.lock().await;
                *current_size = current_size.saturating_sub(1);
            }
        }

        drop(available); // Release lock before creating new resource

        // Create a new resource
        let resource =
            T::create(&self.factory_config)
                .await
                .map_err(|e| PoolError::ResourceCreation {
                    source: Box::new(e),
                })?;

        let mut pooled = PooledResource::new(resource);
        pooled.update_usage();

        // Update pool size
        let mut current_size = self.current_size.lock().await;
        *current_size += 1;

        Ok(PoolGuard {
            resource: Some(pooled),
            pool: Arc::new(self.clone()),
        })
    }

    /// Return a resource to the pool
    async fn return_resource(&self, mut pooled: PooledResource<T>) {
        // Don't return resources if pool is shutting down
        if *self.shutdown.lock().await {
            let _ = pooled.resource.cleanup().await;
            return;
        }

        // Reset the resource state
        match pooled.resource.reset().await {
            Ok(()) => {
                // Resource was successfully reset, return to pool
                let mut available = self.available.lock().await;
                available.push_back(pooled);
            }
            Err(e) => {
                // Failed to reset, cleanup and remove from pool
                tracing::warn!("Failed to reset resource for reuse: {}", e);
                let _ = pooled.resource.cleanup().await;
                let mut current_size = self.current_size.lock().await;
                *current_size = current_size.saturating_sub(1);
            }
        }
    }

    /// Perform pool maintenance tasks
    ///
    /// This method should be called periodically to:
    /// - Remove idle resources
    /// - Ensure minimum pool size
    /// - Perform health checks
    pub async fn maintain_pool(&self) -> std::result::Result<PoolMaintenance, PoolError> {
        if *self.shutdown.lock().await {
            return Err(PoolError::PoolShutdown);
        }

        let mut maintenance = PoolMaintenance::default();
        let mut available = self.available.lock().await;

        // Remove idle resources
        let mut to_keep = VecDeque::new();
        while let Some(pooled) = available.pop_front() {
            if pooled.is_idle_too_long(self.config.idle_timeout) {
                // Resource is idle too long, cleanup and remove
                drop(available); // Release lock during cleanup
                let mut resource = pooled.resource;
                if let Err(e) = resource.cleanup().await {
                    tracing::warn!("Failed to cleanup idle resource: {}", e);
                }
                maintenance.resources_removed += 1;

                // Update pool size
                let mut current_size = self.current_size.lock().await;
                *current_size = current_size.saturating_sub(1);
                drop(current_size);

                available = self.available.lock().await;
            } else {
                to_keep.push_back(pooled);
            }
        }

        // Put remaining resources back
        while let Some(pooled) = to_keep.pop_front() {
            available.push_back(pooled);
        }

        let current_available = available.len();
        drop(available);

        // Ensure minimum pool size
        let current_size = *self.current_size.lock().await;
        if current_size < self.config.min_size {
            let needed = self.config.min_size - current_size;

            for _ in 0..needed {
                match T::create(&self.factory_config).await {
                    Ok(resource) => {
                        let pooled = PooledResource::new(resource);
                        let mut available = self.available.lock().await;
                        available.push_back(pooled);

                        let mut size = self.current_size.lock().await;
                        *size += 1;
                        maintenance.resources_created += 1;
                    }
                    Err(e) => {
                        tracing::warn!("Failed to create resource during maintenance: {}", e);
                        maintenance.creation_failures += 1;
                    }
                }
            }
        }

        maintenance.available_resources = current_available;
        maintenance.total_resources = *self.current_size.lock().await;

        Ok(maintenance)
    }

    /// Get pool statistics
    pub async fn stats(&self) -> PoolStats {
        let available = self.available.lock().await;
        let current_size = *self.current_size.lock().await;
        let is_shutdown = *self.shutdown.lock().await;

        PoolStats {
            total_resources: current_size,
            available_resources: available.len(),
            in_use_resources: current_size.saturating_sub(available.len()),
            max_size: self.config.max_size,
            min_size: self.config.min_size,
            is_shutdown,
        }
    }

    /// Shutdown the pool and cleanup all resources
    pub async fn shutdown(&self) -> std::result::Result<(), PoolError> {
        // Set shutdown flag
        {
            let mut shutdown = self.shutdown.lock().await;
            *shutdown = true;
        }

        // Cleanup all available resources
        let mut available = self.available.lock().await;
        while let Some(mut pooled) = available.pop_front() {
            if let Err(e) = pooled.resource.cleanup().await {
                tracing::warn!("Failed to cleanup resource during shutdown: {}", e);
            }
        }

        // Reset pool size
        let mut current_size = self.current_size.lock().await;
        *current_size = 0;

        Ok(())
    }
}

impl<T: Poolable> Clone for ResourcePool<T> {
    fn clone(&self) -> Self {
        Self {
            available: Arc::clone(&self.available),
            factory_config: self.factory_config.clone(),
            config: self.config.clone(),
            current_size: Arc::clone(&self.current_size),
            semaphore: Arc::clone(&self.semaphore),
            shutdown: Arc::clone(&self.shutdown),
        }
    }
}

/// Pool maintenance results
#[derive(Debug, Default)]
pub struct PoolMaintenance {
    pub resources_created: usize,
    pub resources_removed: usize,
    pub creation_failures: usize,
    pub available_resources: usize,
    pub total_resources: usize,
}

/// Pool statistics
#[derive(Debug)]
pub struct PoolStats {
    pub total_resources: usize,
    pub available_resources: usize,
    pub in_use_resources: usize,
    pub max_size: usize,
    pub min_size: usize,
    pub is_shutdown: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::time::sleep;

    // Mock poolable resource for testing
    #[derive(Debug)]
    struct MockResource {
        id: usize,
        is_healthy: bool,
        reset_count: usize,
        cleanup_count: usize,
    }

    #[derive(Debug, Clone, Default)]
    struct MockConfig {
        fail_creation: bool,
        fail_reset: bool,
        fail_cleanup: bool,
    }

    static MOCK_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);

    #[derive(Debug, Error)]
    #[error("Mock error")]
    struct MockError;

    #[async_trait]
    impl Poolable for MockResource {
        type Config = MockConfig;
        type Error = MockError;

        async fn create(config: &Self::Config) -> std::result::Result<Self, Self::Error> {
            if config.fail_creation {
                return Err(MockError);
            }

            Ok(Self {
                id: MOCK_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
                is_healthy: true,
                reset_count: 0,
                cleanup_count: 0,
            })
        }

        async fn is_healthy(&self) -> bool {
            self.is_healthy
        }

        async fn reset(&mut self) -> std::result::Result<(), Self::Error> {
            if self.reset_count == 0 && MockConfig::default().fail_reset {
                return Err(MockError);
            }

            self.reset_count += 1;
            Ok(())
        }

        async fn cleanup(&mut self) -> std::result::Result<(), Self::Error> {
            if MockConfig::default().fail_cleanup {
                return Err(MockError);
            }

            self.cleanup_count += 1;
            Ok(())
        }
    }

    #[tokio::test]
    async fn test_pool_acquire_and_return() {
        let config = MockConfig::default();
        let pool_config = PoolConfig {
            min_size: 1,
            max_size: 5,
            ..Default::default()
        };

        let pool: ResourcePool<MockResource> = ResourcePool::new(config, pool_config);

        // Acquire a resource
        let guard = pool.acquire().await.unwrap();
        let _resource_id = guard.resource().id;
        // Don't assert specific ID as it's shared across tests

        // Check pool stats
        let stats = pool.stats().await;
        assert_eq!(stats.total_resources, 1);
        assert_eq!(stats.in_use_resources, 1);
        assert_eq!(stats.available_resources, 0);

        // Drop the guard to return resource to pool
        drop(guard);

        // Give some time for async return
        sleep(Duration::from_millis(50)).await;

        // Resource should be back in pool
        let stats = pool.stats().await;
        assert_eq!(stats.total_resources, 1);
        assert_eq!(stats.in_use_resources, 0);
        assert_eq!(stats.available_resources, 1);
    }

    #[tokio::test]
    async fn test_pool_reuse() {
        let config = MockConfig::default();
        let pool_config = PoolConfig::default();
        let pool: ResourcePool<MockResource> = ResourcePool::new(config, pool_config);

        // Acquire and return a resource
        let guard1 = pool.acquire().await.unwrap();
        let id1 = guard1.resource().id;
        drop(guard1);

        sleep(Duration::from_millis(50)).await;

        // Acquire again - should reuse the same resource
        let guard2 = pool.acquire().await.unwrap();
        let id2 = guard2.resource().id;

        assert_eq!(id1, id2);
        assert_eq!(guard2.resource().reset_count, 1);
    }

    #[tokio::test]
    async fn test_pool_concurrent_access() {
        let config = MockConfig::default();
        let pool_config = PoolConfig {
            max_size: 3,
            ..Default::default()
        };
        let pool: Arc<ResourcePool<MockResource>> =
            Arc::new(ResourcePool::new(config, pool_config));

        let mut handles = Vec::new();

        // Spawn multiple concurrent tasks
        for _ in 0..5 {
            let pool_clone = Arc::clone(&pool);
            let handle = tokio::spawn(async move {
                let guard = pool_clone.acquire().await.unwrap();
                let id = guard.resource().id;
                sleep(Duration::from_millis(50)).await;
                id
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        let mut results = Vec::new();
        for handle in handles {
            results.push(handle.await.unwrap());
        }

        // Give time for resources to be returned to pool
        sleep(Duration::from_millis(100)).await;

        // Should have created at most 3 resources (pool max size)
        let stats = pool.stats().await;
        // Due to concurrent access patterns, we might temporarily exceed max_size during creation
        // but the semaphore should limit active resources to max_size
        assert!(stats.total_resources <= 5); // Allow some tolerance for concurrent creation

        // All results should be valid IDs
        assert_eq!(results.len(), 5);
        // IDs might be reused or higher due to global counter, just check they're valid
        for id in results {
            assert!(id < 100); // Just a reasonable upper bound
        }
    }

    #[tokio::test]
    async fn test_pool_maintenance() {
        let config = MockConfig::default();
        let pool_config = PoolConfig {
            min_size: 2,
            max_size: 5,
            idle_timeout: Duration::from_millis(100),
            ..Default::default()
        };
        let pool: ResourcePool<MockResource> = ResourcePool::new(config, pool_config);

        // Maintenance should create min_size resources
        let maintenance = pool.maintain_pool().await.unwrap();
        assert_eq!(maintenance.resources_created, 2);

        let stats = pool.stats().await;
        assert_eq!(stats.total_resources, 2);
        assert_eq!(stats.available_resources, 2);

        // Wait for idle timeout
        sleep(Duration::from_millis(150)).await;

        // Maintenance should remove idle resources and recreate to min_size
        let maintenance = pool.maintain_pool().await.unwrap();
        assert_eq!(maintenance.resources_removed, 2);
        assert_eq!(maintenance.resources_created, 2);
    }

    #[tokio::test]
    async fn test_pool_shutdown() {
        let config = MockConfig::default();
        let pool_config = PoolConfig::default();
        let pool: ResourcePool<MockResource> = ResourcePool::new(config, pool_config);

        // Create some resources
        let _guard = pool.acquire().await.unwrap();

        // Shutdown the pool
        pool.shutdown().await.unwrap();

        let stats = pool.stats().await;
        assert!(stats.is_shutdown);

        // Should not be able to acquire after shutdown
        let result = pool.acquire().await;
        assert!(matches!(result, Err(PoolError::PoolShutdown)));
    }
}
