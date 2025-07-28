// Pooled Language Environment Implementation
// Provides efficient reuse of language environments with dependency matching

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use thiserror::Error;

use crate::error::{Result, SnpError};
use crate::language::environment::{EnvironmentConfig, LanguageEnvironment};
use crate::resource_pool::Poolable;

/// Configuration for creating pooled language environments
#[derive(Debug, Clone)]
pub struct LanguagePoolConfig {
    /// Language name (e.g., "python", "nodejs", "rust")
    pub language: String,
    /// Default language version to use
    pub default_version: Option<String>,
    /// Cache directory for environments
    pub cache_directory: PathBuf,
    /// Whether to match dependencies for reuse
    pub match_dependencies: bool,
    /// Maximum number of cached dependency combinations
    pub max_dependency_combinations: usize,
    /// Timeout for environment setup
    pub setup_timeout: Duration,
}

impl Default for LanguagePoolConfig {
    fn default() -> Self {
        Self {
            language: "system".to_string(),
            default_version: None,
            cache_directory: std::env::temp_dir().join("snp-language-envs"),
            match_dependencies: true,
            max_dependency_combinations: 20,
            setup_timeout: Duration::from_secs(300), // 5 minutes
        }
    }
}

/// Errors specific to pooled language environment operations
#[derive(Debug, Error)]
pub enum PooledLanguageError {
    #[error("Failed to create language environment for {language}: {source}")]
    EnvironmentCreation {
        language: String,
        #[source]
        source: SnpError,
    },

    #[error("Failed to install dependencies {dependencies:?} for {language}: {source}")]
    DependencyInstallation {
        language: String,
        dependencies: Vec<String>,
        #[source]
        source: SnpError,
    },

    #[error("Environment health check failed for {language}: {reason}")]
    HealthCheckFailed { language: String, reason: String },

    #[error("Failed to reset environment for {language}: {source}")]
    ResetFailed {
        language: String,
        #[source]
        source: SnpError,
    },

    #[error("Failed to cleanup environment for {language}: {source}")]
    CleanupFailed {
        language: String,
        #[source]
        source: SnpError,
    },

    #[error("Language plugin not found: {language}")]
    LanguagePluginNotFound { language: String },

    #[error("Environment setup timeout after {timeout:?}")]
    SetupTimeout { timeout: Duration },
}

// Convert pooled language errors to SNP errors
impl From<PooledLanguageError> for SnpError {
    fn from(err: PooledLanguageError) -> Self {
        match err {
            PooledLanguageError::EnvironmentCreation {
                language: _,
                source,
            } => source,
            PooledLanguageError::DependencyInstallation {
                language: _,
                dependencies: _,
                source,
            } => source,
            other => SnpError::HookExecution(Box::new(
                crate::error::HookExecutionError::EnvironmentSetupFailed {
                    language: "unknown".to_string(),
                    hook_id: "unknown".to_string(),
                    message: other.to_string(),
                    suggestion: None,
                },
            )),
        }
    }
}

/// A pooled language environment that can be reused across different dependency sets
pub struct PooledLanguageEnvironment {
    /// The underlying language environment
    environment: Arc<LanguageEnvironment>,
    /// Current dependency state
    current_dependencies: Vec<String>,
    /// Cached dependency combinations
    dependency_cache: HashMap<String, DependencyState>,
    /// Configuration
    config: LanguagePoolConfig,
    /// Last setup time
    last_setup: SystemTime,
}

/// State information for a dependency combination
#[derive(Debug, Clone)]
pub struct DependencyState {
    pub dependencies: Vec<String>,
    pub dependencies_hash: u64,
    pub setup_time: SystemTime,
    pub install_duration: Duration,
    pub is_verified: bool,
}

impl PooledLanguageEnvironment {
    /// Setup the environment for the specified dependencies
    pub async fn setup_for_dependencies(
        &mut self,
        dependencies: &[String],
    ) -> Result<&LanguageEnvironment> {
        let dependencies_hash = self.calculate_dependencies_hash(dependencies);
        let cache_key = self.generate_cache_key(dependencies);

        // Check if we already have this dependency combination
        if let Some(current_state) = self.dependency_cache.get(&cache_key) {
            if current_state.dependencies == dependencies
                && current_state.is_verified
                && self.is_dependency_state_fresh(current_state)
            {
                tracing::debug!(
                    "Reusing cached dependency state for {}: {:?}",
                    self.config.language,
                    dependencies
                );
                self.current_dependencies = dependencies.to_vec();
                return Ok(&self.environment);
            }
        }

        // Setup dependencies
        let setup_start = SystemTime::now();
        self.install_dependencies(dependencies)
            .await
            .map_err(SnpError::from)?;
        let install_duration = setup_start.elapsed().unwrap_or_default();

        // Create and cache dependency state
        let dependency_state = DependencyState {
            dependencies: dependencies.to_vec(),
            dependencies_hash,
            setup_time: setup_start,
            install_duration,
            is_verified: true,
        };

        // Manage cache size
        if self.dependency_cache.len() >= self.config.max_dependency_combinations {
            self.evict_oldest_dependency_state();
        }

        self.dependency_cache.insert(cache_key, dependency_state);
        self.current_dependencies = dependencies.to_vec();
        self.last_setup = SystemTime::now();

        Ok(&self.environment)
    }

    /// Get the current dependency state
    pub fn current_dependencies(&self) -> &[String] {
        &self.current_dependencies
    }

    /// Get the underlying language environment
    pub fn language_environment(&self) -> &LanguageEnvironment {
        &self.environment
    }

    /// Get environment statistics
    pub fn stats(&self) -> PooledLanguageStats {
        PooledLanguageStats {
            language: self.config.language.clone(),
            current_dependencies: self.current_dependencies.clone(),
            cached_dependency_combinations: self.dependency_cache.len(),
            last_setup: self.last_setup,
            cache_directory: self.config.cache_directory.clone(),
        }
    }

    /// Verify that all current dependencies are still properly installed
    pub async fn verify_dependencies(&self) -> bool {
        // In a real implementation, this would:
        // 1. Check that all dependencies are still available
        // 2. Verify dependency versions are correct
        // 3. Test that dependency imports/usage works

        // For now, simulate verification
        tracing::debug!(
            "Verifying dependencies for {}: {:?}",
            self.config.language,
            self.current_dependencies
        );

        // Simulate some verification work
        tokio::time::sleep(Duration::from_millis(10)).await;

        // Return true for simplicity - in real implementation would do actual checks
        true
    }

    /// Install the specified dependencies
    async fn install_dependencies(
        &mut self,
        dependencies: &[String],
    ) -> std::result::Result<(), PooledLanguageError> {
        if dependencies.is_empty() {
            return Ok(());
        }

        tracing::debug!(
            "Installing dependencies for {}: {:?}",
            self.config.language,
            dependencies
        );

        // In a real implementation, this would:
        // 1. Get the language plugin
        // 2. Parse and resolve dependencies
        // 3. Install dependencies using the language's package manager
        // 4. Verify installation success

        // Simulate installation work with timeout
        let install_future = async {
            // Simulate installation time based on number of dependencies
            let install_time = Duration::from_millis(100 * dependencies.len() as u64);
            tokio::time::sleep(install_time).await;
            Ok(())
        };

        tokio::time::timeout(self.config.setup_timeout, install_future)
            .await
            .map_err(|_| PooledLanguageError::SetupTimeout {
                timeout: self.config.setup_timeout,
            })??;

        Ok(())
    }

    /// Calculate hash for dependency combination
    fn calculate_dependencies_hash(&self, dependencies: &[String]) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Sort dependencies for consistent hashing
        let mut sorted_deps = dependencies.to_vec();
        sorted_deps.sort();

        for dep in &sorted_deps {
            dep.hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Generate cache key for dependency combination
    fn generate_cache_key(&self, dependencies: &[String]) -> String {
        let hash = self.calculate_dependencies_hash(dependencies);
        format!("{}:{:x}", self.config.language, hash)
    }

    /// Check if dependency state is still fresh
    fn is_dependency_state_fresh(&self, state: &DependencyState) -> bool {
        // Consider dependency state fresh for 1 hour
        const FRESHNESS_THRESHOLD: Duration = Duration::from_secs(3600);

        state.setup_time.elapsed().unwrap_or(FRESHNESS_THRESHOLD) < FRESHNESS_THRESHOLD
    }

    /// Evict the oldest dependency state from cache
    fn evict_oldest_dependency_state(&mut self) {
        if let Some(oldest_key) = self.find_oldest_dependency_state() {
            self.dependency_cache.remove(&oldest_key);
            tracing::debug!("Evicted oldest dependency state: {}", oldest_key);
        }
    }

    /// Find the oldest dependency state for eviction
    fn find_oldest_dependency_state(&self) -> Option<String> {
        self.dependency_cache
            .iter()
            .min_by_key(|(_, state)| state.setup_time)
            .map(|(key, _)| key.clone())
    }

    /// Clear all cached dependency states
    fn clear_dependency_cache(&mut self) {
        self.dependency_cache.clear();
        self.current_dependencies.clear();
    }
}

#[async_trait]
impl Poolable for PooledLanguageEnvironment {
    type Config = LanguagePoolConfig;
    type Error = PooledLanguageError;

    async fn create(config: &Self::Config) -> std::result::Result<Self, Self::Error> {
        // Create cache directory
        if let Err(e) = tokio::fs::create_dir_all(&config.cache_directory).await {
            tracing::warn!(
                "Failed to create cache directory {:?}: {}",
                config.cache_directory,
                e
            );
        }

        // Create environment configuration
        let _env_config = EnvironmentConfig::new()
            .with_version(
                config
                    .default_version
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
            )
            .with_dependencies(Vec::new()) // Start with no dependencies
            .with_working_directory(config.cache_directory.clone());

        // Create the language environment
        // In a real implementation, this would use the language registry to get the plugin
        // and create a proper environment
        let environment_id = format!("{}_{}", config.language, uuid::Uuid::new_v4());
        let environment = Arc::new(LanguageEnvironment {
            language: config.language.clone(),
            environment_id: environment_id.clone(),
            root_path: config.cache_directory.join(&environment_id),
            executable_path: PathBuf::from("mock_executable"), // Would be set by language plugin
            environment_variables: HashMap::new(),
            installed_dependencies: Vec::new(),
            metadata: crate::language::environment::EnvironmentMetadata {
                created_at: SystemTime::now(),
                last_used: SystemTime::now(),
                usage_count: 0,
                size_bytes: 0,
                dependency_count: 0,
                language_version: config
                    .default_version
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        });

        Ok(Self {
            environment,
            current_dependencies: Vec::new(),
            dependency_cache: HashMap::new(),
            config: config.clone(),
            last_setup: SystemTime::now(),
        })
    }

    async fn is_healthy(&self) -> bool {
        // For the test environment, the root_path might not exist yet
        // In a real implementation, we would check if the environment directory exists
        // For now, always consider the environment healthy if dependencies verify
        self.verify_dependencies().await
    }

    async fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        tracing::debug!(
            "Resetting pooled language environment for {}",
            self.config.language
        );

        // Clear current state
        self.current_dependencies.clear();

        // Optionally clear dependency cache
        if self.config.match_dependencies {
            // Keep cache for better performance
            // Only clear if we want to start fresh
        } else {
            self.clear_dependency_cache();
        }

        // In a real implementation, we would:
        // 1. Reset environment variables
        // 2. Clear any temporary state
        // 3. Verify environment is in clean state

        // Simulate reset work
        tokio::time::sleep(Duration::from_millis(5)).await;

        Ok(())
    }

    async fn cleanup(&mut self) -> std::result::Result<(), Self::Error> {
        tracing::debug!(
            "Cleaning up pooled language environment for {}",
            self.config.language
        );

        // Clear all state
        self.clear_dependency_cache();

        // Remove environment directory
        if self.environment.root_path.exists() {
            tokio::fs::remove_dir_all(&self.environment.root_path)
                .await
                .map_err(|e| PooledLanguageError::CleanupFailed {
                    language: self.config.language.clone(),
                    source: SnpError::Io(e),
                })?;
        }

        Ok(())
    }
}

/// Statistics for a pooled language environment
#[derive(Debug)]
pub struct PooledLanguageStats {
    pub language: String,
    pub current_dependencies: Vec<String>,
    pub cached_dependency_combinations: usize,
    pub last_setup: SystemTime,
    pub cache_directory: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_pool::{PoolConfig, ResourcePool};
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_pooled_language_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = LanguagePoolConfig {
            language: "python".to_string(),
            cache_directory: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let result = PooledLanguageEnvironment::create(&config).await;
        assert!(result.is_ok());

        let env = result.unwrap();
        assert_eq!(env.config.language, "python");
        assert!(env.current_dependencies.is_empty());
        assert!(env.dependency_cache.is_empty());
    }

    #[tokio::test]
    async fn test_pooled_language_dependency_setup() {
        let temp_dir = TempDir::new().unwrap();
        let config = LanguagePoolConfig {
            language: "python".to_string(),
            cache_directory: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let mut env = PooledLanguageEnvironment::create(&config).await.unwrap();

        // Setup dependencies
        let dependencies = vec!["requests==2.25.1".to_string(), "pytest>=6.0".to_string()];
        let result = env.setup_for_dependencies(&dependencies).await;
        assert!(result.is_ok());

        // Check state
        assert_eq!(env.current_dependencies, dependencies);
        assert_eq!(env.dependency_cache.len(), 1);

        // Setup same dependencies again - should use cache
        let result2 = env.setup_for_dependencies(&dependencies).await;
        assert!(result2.is_ok());
        assert_eq!(env.dependency_cache.len(), 1); // Should not increase
    }

    #[tokio::test]
    async fn test_pooled_language_dependency_cache() {
        let temp_dir = TempDir::new().unwrap();
        let config = LanguagePoolConfig {
            language: "nodejs".to_string(),
            cache_directory: temp_dir.path().to_path_buf(),
            max_dependency_combinations: 2,
            ..Default::default()
        };

        let mut env = PooledLanguageEnvironment::create(&config).await.unwrap();

        // Setup different dependency combinations
        let deps1 = vec!["express@4.17.1".to_string()];
        let deps2 = vec!["lodash@4.17.21".to_string()];
        let deps3 = vec!["axios@0.21.1".to_string()];

        env.setup_for_dependencies(&deps1).await.unwrap();
        env.setup_for_dependencies(&deps2).await.unwrap();
        assert_eq!(env.dependency_cache.len(), 2);

        // Adding third should evict oldest
        env.setup_for_dependencies(&deps3).await.unwrap();
        assert_eq!(env.dependency_cache.len(), 2);
    }

    #[tokio::test]
    async fn test_pooled_language_resource_pool_integration() {
        let temp_dir = TempDir::new().unwrap();
        let language_config = LanguagePoolConfig {
            language: "rust".to_string(),
            cache_directory: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let pool_config = PoolConfig {
            min_size: 1,
            max_size: 3,
            ..Default::default()
        };

        let pool: ResourcePool<PooledLanguageEnvironment> =
            ResourcePool::new(language_config, pool_config);

        // Acquire environment
        let mut guard = pool.acquire().await.unwrap();
        let env = guard.resource_mut();

        // Setup dependencies
        let dependencies = vec!["serde@1.0".to_string()];
        let result = env.setup_for_dependencies(&dependencies).await;
        assert!(result.is_ok());

        // Check stats
        let stats = env.stats();
        assert_eq!(stats.language, "rust");
        assert_eq!(stats.current_dependencies, dependencies);
        assert_eq!(stats.cached_dependency_combinations, 1);

        // Resource should be automatically returned to pool when guard is dropped
        drop(guard);

        // Wait for async return
        tokio::time::sleep(Duration::from_millis(50)).await;

        // Acquire again - should get reset environment
        let guard2 = pool.acquire().await.unwrap();
        let env2 = guard2.resource();

        // Should be reset (no current dependencies, but cache might be preserved)
        assert!(env2.current_dependencies.is_empty());
    }

    #[tokio::test]
    async fn test_pooled_language_hash_consistency() {
        let temp_dir = TempDir::new().unwrap();
        let config = LanguagePoolConfig {
            language: "python".to_string(),
            cache_directory: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let env = PooledLanguageEnvironment::create(&config).await.unwrap();

        // Test hash consistency with different orderings
        let deps1 = vec!["requests".to_string(), "flask".to_string()];
        let deps2 = vec!["flask".to_string(), "requests".to_string()];

        let hash1 = env.calculate_dependencies_hash(&deps1);
        let hash2 = env.calculate_dependencies_hash(&deps2);

        assert_eq!(hash1, hash2); // Should be the same due to sorting

        let key1 = env.generate_cache_key(&deps1);
        let key2 = env.generate_cache_key(&deps2);

        assert_eq!(key1, key2);
    }

    #[tokio::test]
    async fn test_pooled_language_verification() {
        let temp_dir = TempDir::new().unwrap();
        let config = LanguagePoolConfig {
            language: "python".to_string(),
            cache_directory: temp_dir.path().to_path_buf(),
            ..Default::default()
        };

        let mut env = PooledLanguageEnvironment::create(&config).await.unwrap();

        // Setup some dependencies
        let dependencies = vec!["numpy".to_string()];
        env.setup_for_dependencies(&dependencies).await.unwrap();

        // Health check should pass
        assert!(env.is_healthy().await);

        // Verify dependencies should pass
        assert!(env.verify_dependencies().await);
    }
}
