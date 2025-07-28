// Pooled Git Repository Implementation
// Provides efficient reuse of Git repository instances with state management

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use thiserror::Error;

use crate::error::{Result, SnpError};
use crate::resource_pool::Poolable;

/// Configuration for creating pooled Git repositories
#[derive(Debug, Clone)]
pub struct GitPoolConfig {
    /// Template for temporary working directories
    pub work_dir_template: String,
    /// Whether to cleanup working directories on reset
    pub cleanup_on_reset: bool,
    /// Whether to verify repository health on checkout
    pub verify_health: bool,
    /// Maximum number of cached repository states
    pub max_cached_states: usize,
}

impl Default for GitPoolConfig {
    fn default() -> Self {
        Self {
            work_dir_template: "/tmp/snp-git-{}".to_string(),
            cleanup_on_reset: true,
            verify_health: true,
            max_cached_states: 10,
        }
    }
}

/// Errors specific to pooled Git repository operations
#[derive(Debug, Error)]
pub enum PooledGitError {
    #[error("Failed to create Git repository: {source}")]
    RepositoryCreation {
        #[source]
        source: SnpError,
    },

    #[error("Failed to checkout repository {url} at {reference}: {source}")]
    CheckoutFailed {
        url: String,
        reference: String,
        #[source]
        source: SnpError,
    },

    #[error("Repository health check failed: {reason}")]
    HealthCheckFailed { reason: String },

    #[error("Failed to reset repository state: {source}")]
    ResetFailed {
        #[source]
        source: SnpError,
    },

    #[error("Failed to cleanup repository: {source}")]
    CleanupFailed {
        #[source]
        source: SnpError,
    },

    #[error("Working directory creation failed: {path}")]
    WorkingDirectoryCreation { path: PathBuf },
}

/// A pooled Git repository that can be reused across different checkouts
pub struct PooledGitRepository {
    /// Working directory path
    work_dir: PathBuf,
    /// Current checkout state
    current_state: Option<RepositoryState>,
    /// Cache of previously checked out states
    cached_states: HashMap<String, RepositoryState>,
    /// Configuration
    config: GitPoolConfig,
    /// Repository metadata
    repo_metadata: RepositoryMetadata,
}

/// Metadata about the Git repository
#[derive(Debug, Clone)]
pub struct RepositoryMetadata {
    pub initialized: bool,
    pub remote_urls: Vec<String>,
    pub branches: Vec<String>,
}

/// State information for a Git repository checkout
#[derive(Debug, Clone)]
pub struct RepositoryState {
    pub url: String,
    pub reference: String,
    pub commit_hash: String,
    pub checkout_time: std::time::SystemTime,
}

impl PooledGitRepository {
    /// Checkout a repository at the specified URL and reference
    pub async fn checkout_repository(
        &mut self,
        url: &str,
        reference: Option<&str>,
    ) -> Result<&RepositoryMetadata> {
        let reference = reference.unwrap_or("HEAD");
        let state_key = format!("{url}:{reference}");

        // Check if we already have this state
        if let Some(current) = &self.current_state {
            if current.url == url && current.reference == reference {
                // Already in the desired state
                return Ok(&self.repo_metadata);
            }
        }

        // Check cached states
        if let Some(cached_state) = self.cached_states.get(&state_key).cloned() {
            // Restore from cache
            self.restore_state(&cached_state).await?;
            self.current_state = Some(cached_state);
            return Ok(&self.repo_metadata);
        }

        // Perform fresh checkout
        let state = self.perform_checkout(url, reference).await?;

        // Cache the state
        if self.cached_states.len() >= self.config.max_cached_states {
            // Remove oldest cached state
            if let Some(oldest_key) = self.find_oldest_cached_state() {
                self.cached_states.remove(&oldest_key);
            }
        }

        self.cached_states.insert(state_key, state.clone());
        self.current_state = Some(state);

        Ok(&self.repo_metadata)
    }

    /// Get the current repository state
    pub fn current_state(&self) -> Option<&RepositoryState> {
        self.current_state.as_ref()
    }

    /// Get the repository metadata
    pub fn repository_metadata(&self) -> &RepositoryMetadata {
        &self.repo_metadata
    }

    /// Get repository statistics
    pub fn stats(&self) -> PooledGitStats {
        PooledGitStats {
            current_url: self.current_state.as_ref().map(|s| s.url.clone()),
            current_reference: self.current_state.as_ref().map(|s| s.reference.clone()),
            cached_states: self.cached_states.len(),
            work_dir: self.work_dir.clone(),
        }
    }

    /// Perform the actual checkout operation
    async fn perform_checkout(&mut self, url: &str, reference: &str) -> Result<RepositoryState> {
        // For now, we'll simulate checkout by creating a mock state
        // In a real implementation, this would:
        // 1. Clone or fetch the repository
        // 2. Checkout the specified reference
        // 3. Get the commit hash

        tracing::debug!("Performing checkout: {} at {}", url, reference);

        // Simulate some work
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Create repository state
        let state = RepositoryState {
            url: url.to_string(),
            reference: reference.to_string(),
            commit_hash: format!("mock_commit_hash_{}", uuid::Uuid::new_v4()),
            checkout_time: std::time::SystemTime::now(),
        };

        Ok(state)
    }

    /// Restore a previously cached repository state
    async fn restore_state(&mut self, state: &RepositoryState) -> Result<()> {
        tracing::debug!(
            "Restoring cached state: {} at {}",
            state.url,
            state.reference
        );

        // In a real implementation, this would:
        // 1. Verify the cached state is still valid
        // 2. Reset working directory to the cached commit
        // 3. Update any necessary Git state

        // Simulate restoration work
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        Ok(())
    }

    /// Find the oldest cached state for eviction
    fn find_oldest_cached_state(&self) -> Option<String> {
        self.cached_states
            .iter()
            .min_by_key(|(_, state)| state.checkout_time)
            .map(|(key, _)| key.clone())
    }

    /// Clear all cached states
    fn clear_cache(&mut self) {
        self.cached_states.clear();
    }
}

#[async_trait]
impl Poolable for PooledGitRepository {
    type Config = GitPoolConfig;
    type Error = PooledGitError;

    async fn create(config: &Self::Config) -> std::result::Result<Self, Self::Error> {
        // Create a unique working directory
        let work_dir = if config.work_dir_template.contains("{}") {
            PathBuf::from(
                config
                    .work_dir_template
                    .replace("{}", &uuid::Uuid::new_v4().to_string()),
            )
        } else {
            PathBuf::from(&config.work_dir_template).join(uuid::Uuid::new_v4().to_string())
        };

        // Create the working directory
        tokio::fs::create_dir_all(&work_dir).await.map_err(|_| {
            PooledGitError::WorkingDirectoryCreation {
                path: work_dir.clone(),
            }
        })?;

        // Initialize repository metadata
        let repo_metadata = RepositoryMetadata {
            initialized: true,
            remote_urls: Vec::new(),
            branches: vec!["main".to_string()], // Default branch
        };

        Ok(Self {
            work_dir,
            current_state: None,
            cached_states: HashMap::new(),
            config: config.clone(),
            repo_metadata,
        })
    }

    async fn is_healthy(&self) -> bool {
        if !self.config.verify_health {
            return true;
        }

        // Check if working directory exists
        if !self.work_dir.exists() {
            return false;
        }

        // Check if Git repository is still valid
        // In a real implementation, we would check:
        // - Repository integrity
        // - Working directory state
        // - Git locks

        // For now, always return true for simplicity
        true
    }

    async fn reset(&mut self) -> std::result::Result<(), Self::Error> {
        tracing::debug!("Resetting pooled Git repository");

        // Clear current state
        self.current_state = None;

        // Optionally clear cache
        if self.config.cleanup_on_reset {
            self.clear_cache();
        }

        // In a real implementation, we would:
        // 1. Reset Git working directory to clean state
        // 2. Clean untracked files
        // 3. Reset any Git state (HEAD, index, etc.)

        // Simulate reset work
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;

        Ok(())
    }

    async fn cleanup(&mut self) -> std::result::Result<(), Self::Error> {
        tracing::debug!("Cleaning up pooled Git repository at {:?}", self.work_dir);

        // Clear all state
        self.current_state = None;
        self.clear_cache();

        // Remove working directory
        if self.work_dir.exists() {
            tokio::fs::remove_dir_all(&self.work_dir)
                .await
                .map_err(|e| PooledGitError::CleanupFailed {
                    source: SnpError::Io(e),
                })?;
        }

        Ok(())
    }
}

/// Statistics for a pooled Git repository
#[derive(Debug)]
pub struct PooledGitStats {
    pub current_url: Option<String>,
    pub current_reference: Option<String>,
    pub cached_states: usize,
    pub work_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resource_pool::ResourcePool;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_pooled_git_creation() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitPoolConfig {
            work_dir_template: temp_dir.path().join("git-{}").to_string_lossy().to_string(),
            ..Default::default()
        };

        let result = PooledGitRepository::create(&config).await;
        assert!(result.is_ok());

        let repo = result.unwrap();
        assert!(repo.work_dir.exists());
        assert!(repo.cached_states.is_empty());
        assert!(repo.current_state.is_none());
    }

    #[tokio::test]
    async fn test_pooled_git_checkout() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitPoolConfig {
            work_dir_template: temp_dir.path().join("git-{}").to_string_lossy().to_string(),
            ..Default::default()
        };

        let mut repo = PooledGitRepository::create(&config).await.unwrap();

        // Perform checkout
        let result = repo
            .checkout_repository("https://github.com/example/repo", Some("main"))
            .await;
        assert!(result.is_ok());

        // Check state
        assert!(repo.current_state.is_some());
        let state = repo.current_state.as_ref().unwrap();
        assert_eq!(state.url, "https://github.com/example/repo");
        assert_eq!(state.reference, "main");

        // Check cache
        assert_eq!(repo.cached_states.len(), 1);
    }

    #[tokio::test]
    async fn test_pooled_git_state_reuse() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitPoolConfig {
            work_dir_template: temp_dir.path().join("git-{}").to_string_lossy().to_string(),
            ..Default::default()
        };

        let mut repo = PooledGitRepository::create(&config).await.unwrap();

        // First checkout
        repo.checkout_repository("https://github.com/example/repo", Some("main"))
            .await
            .unwrap();
        let first_hash = repo.current_state.as_ref().unwrap().commit_hash.clone();

        // Checkout different reference
        repo.checkout_repository("https://github.com/example/repo", Some("develop"))
            .await
            .unwrap();

        // Checkout original reference again - should use cache
        repo.checkout_repository("https://github.com/example/repo", Some("main"))
            .await
            .unwrap();
        let second_hash = repo.current_state.as_ref().unwrap().commit_hash.clone();

        assert_eq!(first_hash, second_hash);
        assert_eq!(repo.cached_states.len(), 2);
    }

    #[tokio::test]
    async fn test_pooled_git_resource_pool_integration() {
        let temp_dir = TempDir::new().unwrap();
        let git_config = GitPoolConfig {
            work_dir_template: temp_dir.path().join("git-{}").to_string_lossy().to_string(),
            ..Default::default()
        };

        let pool_config = crate::resource_pool::PoolConfig {
            min_size: 1,
            max_size: 3,
            ..Default::default()
        };

        let pool: ResourcePool<PooledGitRepository> = ResourcePool::new(git_config, pool_config);

        // Acquire repository
        let mut guard = pool.acquire().await.unwrap();
        let repo = guard.resource_mut();

        // Use repository
        let result = repo
            .checkout_repository("https://github.com/example/repo", Some("main"))
            .await;
        assert!(result.is_ok());

        // Check stats
        let stats = repo.stats();
        assert_eq!(
            stats.current_url,
            Some("https://github.com/example/repo".to_string())
        );
        assert_eq!(stats.current_reference, Some("main".to_string()));

        // Resource should be automatically returned to pool when guard is dropped
        drop(guard);

        // Wait for async return
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Acquire again - should get reset repository
        let guard2 = pool.acquire().await.unwrap();
        let repo2 = guard2.resource();

        // Should be reset (no current state)
        assert!(repo2.current_state.is_none());
    }

    #[tokio::test]
    async fn test_pooled_git_cache_eviction() {
        let temp_dir = TempDir::new().unwrap();
        let config = GitPoolConfig {
            work_dir_template: temp_dir.path().join("git-{}").to_string_lossy().to_string(),
            max_cached_states: 2,
            ..Default::default()
        };

        let mut repo = PooledGitRepository::create(&config).await.unwrap();

        // Fill cache beyond limit
        repo.checkout_repository("https://github.com/example/repo1", Some("main"))
            .await
            .unwrap();
        repo.checkout_repository("https://github.com/example/repo2", Some("main"))
            .await
            .unwrap();
        repo.checkout_repository("https://github.com/example/repo3", Some("main"))
            .await
            .unwrap();

        // Should have evicted the oldest entry
        assert_eq!(repo.cached_states.len(), 2);

        // Should not contain the first repo anymore
        assert!(!repo
            .cached_states
            .contains_key("https://github.com/example/repo1:main"));
    }
}
