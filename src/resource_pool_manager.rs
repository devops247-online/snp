// Resource Pool Manager for coordinating multiple resource pools

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::task::JoinHandle;

use crate::error::Result;
use crate::pooled_git::{GitPoolConfig, PooledGitRepository};
use crate::pooled_language::{LanguagePoolConfig, PooledLanguageEnvironment};
use crate::resource_pool::{PoolConfig, ResourcePool};

/// Configuration for the resource pool manager
#[derive(Debug, Clone)]
pub struct ResourcePoolManagerConfig {
    /// Git repository pool configuration
    pub git_pool: GitPoolConfig,
    /// Language environment pool configuration
    pub language_pool: LanguagePoolConfig,
    /// Pool maintenance interval
    pub maintenance_interval: Duration,
    /// Whether to enable health checks
    pub enable_health_checks: bool,
    /// Base cache directory for all pools
    pub cache_directory: PathBuf,
}

impl Default for ResourcePoolManagerConfig {
    fn default() -> Self {
        let cache_dir = std::env::temp_dir().join("snp-resource-pools");

        Self {
            git_pool: GitPoolConfig {
                work_dir_template: cache_dir.join("git-{}").to_string_lossy().to_string(),
                ..Default::default()
            },
            language_pool: LanguagePoolConfig {
                cache_directory: cache_dir.join("languages"),
                ..Default::default()
            },
            maintenance_interval: Duration::from_secs(60), // 1 minute
            enable_health_checks: true,
            cache_directory: cache_dir,
        }
    }
}

impl ResourcePoolManagerConfig {
    /// Create a minimal configuration for basic operation
    /// Used as a fallback when full initialization fails
    pub fn minimal() -> Self {
        let cache_dir = std::env::temp_dir().join("snp-resource-pools-minimal");

        Self {
            git_pool: GitPoolConfig {
                work_dir_template: cache_dir.join("git-{}").to_string_lossy().to_string(),
                cleanup_on_reset: false, // Minimal cleanup
                verify_health: false,    // Disable health checks for fallback
                max_cached_states: 1,    // Minimal cache
            },
            language_pool: LanguagePoolConfig {
                language: "system".to_string(),
                default_version: None,
                cache_directory: cache_dir.join("languages"),
                match_dependencies: false, // Simplified dependency matching
                max_dependency_combinations: 1, // Minimal combinations
                setup_timeout: Duration::from_secs(30), // Shorter timeout
            },
            maintenance_interval: Duration::from_secs(300), // 5 minutes
            enable_health_checks: false,
            cache_directory: cache_dir,
        }
    }
}

/// Manager for coordinating multiple resource pools
pub struct ResourcePoolManager {
    /// Git repository pools keyed by configuration hash
    git_pools: HashMap<String, Arc<ResourcePool<PooledGitRepository>>>,
    /// Language environment pools keyed by language name
    language_pools: HashMap<String, Arc<ResourcePool<PooledLanguageEnvironment>>>,
    /// Configuration
    config: ResourcePoolManagerConfig,
    /// Background maintenance task handle
    maintenance_task: Option<JoinHandle<()>>,
}

impl ResourcePoolManager {
    /// Create a new resource pool manager
    pub async fn new(config: ResourcePoolManagerConfig) -> Result<Self> {
        // Ensure cache directory exists
        tokio::fs::create_dir_all(&config.cache_directory).await?;

        let mut manager = Self {
            git_pools: HashMap::new(),
            language_pools: HashMap::new(),
            config,
            maintenance_task: None,
        };

        // Start maintenance task if enabled
        if manager.config.enable_health_checks {
            manager.start_maintenance_task().await;
        }

        Ok(manager)
    }

    /// Get or create a Git repository pool for the given configuration
    pub async fn get_git_pool(
        &mut self,
        config: &GitPoolConfig,
    ) -> Arc<ResourcePool<PooledGitRepository>> {
        let pool_key = self.generate_git_pool_key(config);

        if let Some(pool) = self.git_pools.get(&pool_key) {
            return Arc::clone(pool);
        }

        // Create new pool
        let pool_config = PoolConfig {
            min_size: 1,
            max_size: 10,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(300),
            health_check_interval: Duration::from_secs(60),
        };

        let pool = Arc::new(ResourcePool::new(config.clone(), pool_config));
        self.git_pools.insert(pool_key, Arc::clone(&pool));

        pool
    }

    /// Get or create a language environment pool for the given language
    pub async fn get_language_pool(
        &mut self,
        language: &str,
        version: Option<&str>,
    ) -> Arc<ResourcePool<PooledLanguageEnvironment>> {
        let pool_key = format!("{}:{}", language, version.unwrap_or("default"));

        if let Some(pool) = self.language_pools.get(&pool_key) {
            return Arc::clone(pool);
        }

        // Create language-specific configuration
        let language_config = LanguagePoolConfig {
            language: language.to_string(),
            default_version: version.map(|v| v.to_string()),
            cache_directory: self.config.cache_directory.join("languages").join(language),
            match_dependencies: true,
            max_dependency_combinations: 20,
            setup_timeout: Duration::from_secs(300),
        };

        let pool_config = PoolConfig {
            min_size: 1,
            max_size: 5,
            acquire_timeout: Duration::from_secs(30),
            idle_timeout: Duration::from_secs(600), // Language environments are more expensive
            health_check_interval: Duration::from_secs(120),
        };

        let pool = Arc::new(ResourcePool::new(language_config, pool_config));
        self.language_pools.insert(pool_key, Arc::clone(&pool));

        pool
    }

    /// Get statistics for all managed pools
    pub async fn get_pool_stats(&self) -> ResourcePoolStats {
        let mut stats = ResourcePoolStats {
            git_pools: HashMap::new(),
            language_pools: HashMap::new(),
            total_git_resources: 0,
            total_language_resources: 0,
        };

        // Collect Git pool stats
        for (key, pool) in &self.git_pools {
            let pool_stats = pool.stats().await;
            stats.total_git_resources += pool_stats.total_resources;
            stats.git_pools.insert(key.clone(), pool_stats);
        }

        // Collect language pool stats
        for (key, pool) in &self.language_pools {
            let pool_stats = pool.stats().await;
            stats.total_language_resources += pool_stats.total_resources;
            stats.language_pools.insert(key.clone(), pool_stats);
        }

        stats
    }

    /// Perform maintenance on all pools
    pub async fn maintain_all_pools(&self) -> Result<MaintenanceResults> {
        let mut results = MaintenanceResults {
            git_pool_maintenance: HashMap::new(),
            language_pool_maintenance: HashMap::new(),
            total_resources_created: 0,
            total_resources_removed: 0,
            maintenance_errors: Vec::new(),
        };

        // Maintain Git pools
        for (key, pool) in &self.git_pools {
            match pool.maintain_pool().await {
                Ok(maintenance) => {
                    results.total_resources_created += maintenance.resources_created;
                    results.total_resources_removed += maintenance.resources_removed;
                    results
                        .git_pool_maintenance
                        .insert(key.clone(), maintenance);
                }
                Err(e) => {
                    results
                        .maintenance_errors
                        .push(format!("Git pool {key}: {e}"));
                }
            }
        }

        // Maintain language pools
        for (key, pool) in &self.language_pools {
            match pool.maintain_pool().await {
                Ok(maintenance) => {
                    results.total_resources_created += maintenance.resources_created;
                    results.total_resources_removed += maintenance.resources_removed;
                    results
                        .language_pool_maintenance
                        .insert(key.clone(), maintenance);
                }
                Err(e) => {
                    results
                        .maintenance_errors
                        .push(format!("Language pool {key}: {e}"));
                }
            }
        }

        Ok(results)
    }

    /// Shutdown all pools and cleanup resources
    pub async fn shutdown(&mut self) -> Result<()> {
        // Stop maintenance task
        if let Some(task) = self.maintenance_task.take() {
            task.abort();
        }

        // Shutdown all Git pools
        for (key, pool) in &self.git_pools {
            if let Err(e) = pool.shutdown().await {
                tracing::warn!("Error shutting down Git pool {}: {}", key, e);
            }
        }

        // Shutdown all language pools
        for (key, pool) in &self.language_pools {
            if let Err(e) = pool.shutdown().await {
                tracing::warn!("Error shutting down language pool {}: {}", key, e);
            }
        }

        // Clear pool references
        self.git_pools.clear();
        self.language_pools.clear();

        // Cleanup cache directory
        if self.config.cache_directory.exists() {
            if let Err(e) = tokio::fs::remove_dir_all(&self.config.cache_directory).await {
                tracing::warn!("Error cleaning up cache directory: {}", e);
            }
        }

        Ok(())
    }

    /// Start the background maintenance task
    async fn start_maintenance_task(&mut self) {
        // For now, skip the maintenance task due to complexity of sharing self
        // In a production implementation, we'd use Arc<Self> or a different pattern
        tracing::info!("Skipping background maintenance task for now - would be implemented with proper async patterns");
    }

    /// Generate a unique key for a Git pool configuration
    fn generate_git_pool_key(&self, config: &GitPoolConfig) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        config.work_dir_template.hash(&mut hasher);
        config.cleanup_on_reset.hash(&mut hasher);
        config.verify_health.hash(&mut hasher);
        config.max_cached_states.hash(&mut hasher);

        format!("git_{:x}", hasher.finish())
    }
}

impl Drop for ResourcePoolManager {
    fn drop(&mut self) {
        // Cancel maintenance task
        if let Some(task) = self.maintenance_task.take() {
            task.abort();
        }
    }
}

/// Statistics for all managed resource pools
#[derive(Debug)]
pub struct ResourcePoolStats {
    pub git_pools: HashMap<String, crate::resource_pool::PoolStats>,
    pub language_pools: HashMap<String, crate::resource_pool::PoolStats>,
    pub total_git_resources: usize,
    pub total_language_resources: usize,
}

/// Results from pool maintenance operations
#[derive(Debug)]
pub struct MaintenanceResults {
    pub git_pool_maintenance: HashMap<String, crate::resource_pool::PoolMaintenance>,
    pub language_pool_maintenance: HashMap<String, crate::resource_pool::PoolMaintenance>,
    pub total_resources_created: usize,
    pub total_resources_removed: usize,
    pub maintenance_errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn create_test_manager() -> (TempDir, ResourcePoolManager) {
        let temp_dir = TempDir::new().unwrap();

        let config = ResourcePoolManagerConfig {
            cache_directory: temp_dir.path().to_path_buf(),
            enable_health_checks: false, // Disable for testing
            ..Default::default()
        };

        let manager = ResourcePoolManager::new(config).await.unwrap();
        (temp_dir, manager)
    }

    #[tokio::test]
    async fn test_resource_pool_manager_creation() {
        let (_temp_dir, manager) = create_test_manager().await;

        // Should start with no pools
        let stats = manager.get_pool_stats().await;
        assert_eq!(stats.git_pools.len(), 0);
        assert_eq!(stats.language_pools.len(), 0);
        assert_eq!(stats.total_git_resources, 0);
        assert_eq!(stats.total_language_resources, 0);
    }

    #[tokio::test]
    async fn test_git_pool_creation() {
        let (_temp_dir, mut manager) = create_test_manager().await;

        let git_config = GitPoolConfig::default();
        let pool = manager.get_git_pool(&git_config).await;

        // Pool should be created and cached
        let stats = manager.get_pool_stats().await;
        assert_eq!(stats.git_pools.len(), 1);

        // Getting same config should return same pool
        let pool2 = manager.get_git_pool(&git_config).await;
        assert!(Arc::ptr_eq(&pool, &pool2));
    }

    #[tokio::test]
    async fn test_language_pool_creation() {
        let (_temp_dir, mut manager) = create_test_manager().await;

        let pool = manager.get_language_pool("python", Some("3.9")).await;

        // Pool should be created and cached
        let stats = manager.get_pool_stats().await;
        assert_eq!(stats.language_pools.len(), 1);

        // Getting same language should return same pool
        let pool2 = manager.get_language_pool("python", Some("3.9")).await;
        assert!(Arc::ptr_eq(&pool, &pool2));

        // Different version should create different pool
        let pool3 = manager.get_language_pool("python", Some("3.10")).await;
        assert!(!Arc::ptr_eq(&pool, &pool3));

        let stats = manager.get_pool_stats().await;
        assert_eq!(stats.language_pools.len(), 2);
    }

    #[tokio::test]
    async fn test_pool_usage() {
        let (_temp_dir, mut manager) = create_test_manager().await;

        // Get pools
        let git_pool = manager.get_git_pool(&GitPoolConfig::default()).await;
        let lang_pool = manager.get_language_pool("nodejs", None).await;

        // Use Git pool
        let git_guard = git_pool.acquire().await.unwrap();
        let git_repo = git_guard.resource();
        let result = git_repo.current_state();
        assert!(result.is_none()); // Should start with no state
        drop(git_guard);

        // Use language pool
        let mut lang_guard = lang_pool.acquire().await.unwrap();
        let lang_env = lang_guard.resource_mut();
        let deps = vec!["express@4.17.1".to_string()];
        let result = lang_env.setup_for_dependencies(&deps).await;
        assert!(result.is_ok());
        drop(lang_guard);

        // Check that resources were used
        let stats = manager.get_pool_stats().await;
        assert!(stats.total_git_resources > 0 || stats.total_language_resources > 0);
    }

    #[tokio::test]
    async fn test_manager_shutdown() {
        let (_temp_dir, mut manager) = create_test_manager().await;

        // Create some pools
        let _git_pool = manager.get_git_pool(&GitPoolConfig::default()).await;
        let _lang_pool = manager.get_language_pool("rust", None).await;

        // Shutdown should succeed
        let result = manager.shutdown().await;
        assert!(result.is_ok());

        // Pools should be cleared
        assert_eq!(manager.git_pools.len(), 0);
        assert_eq!(manager.language_pools.len(), 0);
    }
}
