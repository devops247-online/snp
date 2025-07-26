// Clean and garbage collection commands for SNP
// Provides cache cleanup and storage optimization functionality

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use crate::error::{Result, SnpError, StorageError};
use crate::storage::Store;

/// Configuration for clean operations
#[derive(Debug, Clone)]
pub struct CleanConfig {
    pub repos: bool,
    pub envs: bool,
    pub temp: bool,
    pub older_than: Option<Duration>,
    pub dry_run: bool,
}

/// Configuration for garbage collection operations
#[derive(Debug, Clone)]
pub struct GcConfig {
    pub aggressive: bool,
    pub dry_run: bool,
}

/// Result of a cleanup operation
#[derive(Debug)]
pub struct CleanupResult {
    pub repos_cleaned: usize,
    pub envs_cleaned: usize,
    pub temp_files_cleaned: usize,
    pub space_reclaimed: u64,
    pub operations_performed: Vec<String>,
}

/// Analyze cache resources and identify what can be cleaned
pub struct CacheAnalyzer {
    store: Store,
}

impl CacheAnalyzer {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Analyze all cached resources and generate cleanup plan
    pub async fn analyze_cache(&self) -> Result<CacheAnalysis> {
        let repos = self.store.list_repositories()?;
        let envs = self.store.list_environments()?;
        let configs = self.store.list_configs()?;

        let cache_dir = self.store.cache_directory();
        let temp_files = self.find_temp_files(cache_dir).await?;

        let total_size = self.calculate_total_size(cache_dir).await?;

        Ok(CacheAnalysis {
            repositories: repos,
            environments: envs,
            configs,
            temp_files,
            total_size,
        })
    }

    /// Find repositories older than the specified duration
    pub async fn find_old_repositories(
        &self,
        max_age: Duration,
    ) -> Result<Vec<crate::storage::RepositoryInfo>> {
        let all_repos = self.store.list_repositories()?;
        let cutoff_time = SystemTime::now()
            .checked_sub(max_age)
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let old_repos = all_repos
            .into_iter()
            .filter(|repo| repo.last_used < cutoff_time)
            .collect();

        Ok(old_repos)
    }

    /// Find environments older than the specified duration
    pub async fn find_old_environments(
        &self,
        max_age: Duration,
    ) -> Result<Vec<crate::storage::EnvironmentInfo>> {
        let all_envs = self.store.list_environments()?;
        let cutoff_time = SystemTime::now()
            .checked_sub(max_age)
            .unwrap_or(SystemTime::UNIX_EPOCH);

        let old_envs = all_envs
            .into_iter()
            .filter(|env| env.last_used < cutoff_time)
            .collect();

        Ok(old_envs)
    }

    /// Find orphaned environments (directories that don't exist)
    pub async fn find_orphaned_environments(&self) -> Result<Vec<crate::storage::EnvironmentInfo>> {
        let all_envs = self.store.list_environments()?;
        let orphaned_envs = all_envs
            .into_iter()
            .filter(|env| !env.path.exists())
            .collect();

        Ok(orphaned_envs)
    }

    /// Find temporary files in the cache
    async fn find_temp_files(&self, cache_dir: &Path) -> Result<Vec<PathBuf>> {
        let mut temp_files = Vec::new();
        let temp_dir = cache_dir.join("temp");

        if temp_dir.exists() {
            let entries = std::fs::read_dir(&temp_dir).map_err(|e| {
                SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                    path: temp_dir.clone(),
                    error: e.to_string(),
                }))
            })?;

            for entry in entries {
                let entry = entry.map_err(|e| {
                    SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                        path: temp_dir.clone(),
                        error: e.to_string(),
                    }))
                })?;
                temp_files.push(entry.path());
            }
        }

        Ok(temp_files)
    }

    /// Calculate total cache size
    async fn calculate_total_size(&self, cache_dir: &Path) -> Result<u64> {
        fn dir_size(path: &Path) -> Result<u64> {
            let mut size = 0;
            if path.is_dir() {
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let path = entry.path();
                    if path.is_dir() {
                        size += dir_size(&path)?;
                    } else {
                        size += entry.metadata()?.len();
                    }
                }
            }
            Ok(size)
        }

        dir_size(cache_dir)
    }
}

/// Results of cache analysis
#[derive(Debug)]
pub struct CacheAnalysis {
    pub repositories: Vec<crate::storage::RepositoryInfo>,
    pub environments: Vec<crate::storage::EnvironmentInfo>,
    pub configs: Vec<crate::storage::ConfigInfo>,
    pub temp_files: Vec<PathBuf>,
    pub total_size: u64,
}

/// Manages safe cleanup operations
pub struct CleanupManager {
    store: Store,
}

impl CleanupManager {
    pub fn new(store: Store) -> Self {
        Self { store }
    }

    /// Execute a cleanup operation based on configuration
    pub async fn execute_cleanup(&self, config: &CleanConfig) -> Result<CleanupResult> {
        let mut result = CleanupResult {
            repos_cleaned: 0,
            envs_cleaned: 0,
            temp_files_cleaned: 0,
            space_reclaimed: 0,
            operations_performed: Vec::new(),
        };

        // Calculate initial size
        let initial_size = if config.dry_run {
            0
        } else {
            self.calculate_cache_size().await?
        };

        // Clean repositories if requested
        if config.repos || (!config.envs && !config.temp) {
            result.repos_cleaned = self.clean_repositories(config).await?;
            if result.repos_cleaned > 0 {
                result
                    .operations_performed
                    .push(format!("Cleaned {} repositories", result.repos_cleaned));
            }
        }

        // Clean environments if requested
        if config.envs || (!config.repos && !config.temp) {
            result.envs_cleaned = self.clean_environments(config).await?;
            if result.envs_cleaned > 0 {
                result
                    .operations_performed
                    .push(format!("Cleaned {} environments", result.envs_cleaned));
            }
        }

        // Clean temporary files if requested
        if config.temp || (!config.repos && !config.envs) {
            result.temp_files_cleaned = self.clean_temp_files(config).await?;
            if result.temp_files_cleaned > 0 {
                result.operations_performed.push(format!(
                    "Cleaned {} temporary files",
                    result.temp_files_cleaned
                ));
            }
        }

        // Calculate space reclaimed
        if !config.dry_run {
            let final_size = self.calculate_cache_size().await?;
            result.space_reclaimed = initial_size.saturating_sub(final_size);
        }

        Ok(result)
    }

    /// Execute garbage collection based on configuration
    pub async fn execute_gc(&self, config: &GcConfig) -> Result<CleanupResult> {
        let max_age = if config.aggressive {
            Duration::from_secs(7 * 24 * 60 * 60) // 7 days for aggressive
        } else {
            Duration::from_secs(30 * 24 * 60 * 60) // 30 days for normal
        };

        let clean_config = CleanConfig {
            repos: true,
            envs: true,
            temp: true,
            older_than: Some(max_age),
            dry_run: config.dry_run,
        };

        self.execute_cleanup(&clean_config).await
    }

    /// Clean repositories based on age and configuration
    async fn clean_repositories(&self, config: &CleanConfig) -> Result<usize> {
        if config.dry_run {
            // In dry run mode, just count what would be cleaned
            let analyzer = CacheAnalyzer::new(self.store.clone());

            if let Some(max_age) = config.older_than {
                let old_repos = analyzer.find_old_repositories(max_age).await?;
                println!("Would clean {} old repositories:", old_repos.len());
                for repo in &old_repos {
                    println!(
                        "  - {} (rev: {}, last used: {:?})",
                        repo.url, repo.revision, repo.last_used
                    );
                }
                return Ok(old_repos.len());
            } else {
                let all_repos = self.store.list_repositories()?;
                println!("Would clean {} repositories:", all_repos.len());
                for repo in &all_repos {
                    println!("  - {} (rev: {})", repo.url, repo.revision);
                }
                return Ok(all_repos.len());
            }
        }

        // Actual cleanup
        if let Some(max_age) = config.older_than {
            self.store.cleanup_old_repositories(max_age)
        } else {
            // Clean all repositories
            let repos = self.store.list_repositories()?;
            let count = repos.len();

            // Use garbage collection to clean all
            self.store
                .cleanup_old_repositories(Duration::from_secs(0))?;
            Ok(count)
        }
    }

    /// Clean environments based on age and configuration
    async fn clean_environments(&self, config: &CleanConfig) -> Result<usize> {
        if config.dry_run {
            // In dry run mode, just count what would be cleaned
            let analyzer = CacheAnalyzer::new(self.store.clone());

            if let Some(max_age) = config.older_than {
                let old_envs = analyzer.find_old_environments(max_age).await?;
                println!("Would clean {} old environments:", old_envs.len());
                for env in &old_envs {
                    println!(
                        "  - {} with deps: {:?} (last used: {:?})",
                        env.language, env.dependencies, env.last_used
                    );
                }
                return Ok(old_envs.len());
            } else {
                let all_envs = self.store.list_environments()?;
                println!("Would clean {} environments:", all_envs.len());
                for env in &all_envs {
                    println!("  - {} with deps: {:?}", env.language, env.dependencies);
                }
                return Ok(all_envs.len());
            }
        }

        // Actual cleanup
        if let Some(max_age) = config.older_than {
            self.store.cleanup_environments(max_age)
        } else {
            // Clean all environments
            let envs = self.store.list_environments()?;
            let count = envs.len();

            // Use cleanup with zero age to clean all
            self.store.cleanup_environments(Duration::from_secs(0))?;
            Ok(count)
        }
    }

    /// Clean temporary files
    async fn clean_temp_files(&self, config: &CleanConfig) -> Result<usize> {
        let cache_dir = self.store.cache_directory();
        let temp_dir = cache_dir.join("temp");

        if !temp_dir.exists() {
            return Ok(0);
        }

        let entries = std::fs::read_dir(&temp_dir).map_err(|e| {
            SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                path: temp_dir.clone(),
                error: e.to_string(),
            }))
        })?;

        let mut temp_files = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| {
                SnpError::Storage(Box::new(StorageError::CacheDirectoryFailed {
                    path: temp_dir.clone(),
                    error: e.to_string(),
                }))
            })?;

            let path = entry.path();
            let should_clean = if let Some(max_age) = config.older_than {
                if let Ok(metadata) = entry.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let cutoff_time = SystemTime::now()
                            .checked_sub(max_age)
                            .unwrap_or(SystemTime::UNIX_EPOCH);
                        modified < cutoff_time
                    } else {
                        true // If we can't get modified time, assume it should be cleaned
                    }
                } else {
                    true
                }
            } else {
                true // Clean all temp files if no age limit
            };

            if should_clean {
                temp_files.push(path);
            }
        }

        if config.dry_run {
            println!("Would clean {} temporary files:", temp_files.len());
            for temp_file in &temp_files {
                println!("  - {}", temp_file.display());
            }
            return Ok(temp_files.len());
        }

        // Actually remove temp files
        let mut cleaned_count = 0;
        for temp_file in temp_files {
            if let Err(e) = std::fs::remove_file(&temp_file) {
                tracing::warn!("Failed to remove temp file {}: {}", temp_file.display(), e);
            } else {
                cleaned_count += 1;
            }
        }

        Ok(cleaned_count)
    }

    /// Calculate total cache size
    async fn calculate_cache_size(&self) -> Result<u64> {
        let analyzer = CacheAnalyzer::new(self.store.clone());
        let analysis = analyzer.analyze_cache().await?;
        Ok(analysis.total_size)
    }
}

/// Main entry point for clean command
pub async fn execute_clean_command(config: &CleanConfig) -> Result<CleanupResult> {
    let store = Store::new()?;
    let cleanup_manager = CleanupManager::new(store);

    let result = cleanup_manager.execute_cleanup(config).await?;

    // Report results
    if config.dry_run {
        println!("\nDry run summary:");
        println!("Would clean {} repositories", result.repos_cleaned);
        println!("Would clean {} environments", result.envs_cleaned);
        println!("Would clean {} temporary files", result.temp_files_cleaned);
    } else {
        println!("\nCleanup summary:");
        println!("Cleaned {} repositories", result.repos_cleaned);
        println!("Cleaned {} environments", result.envs_cleaned);
        println!("Cleaned {} temporary files", result.temp_files_cleaned);
        println!("Space reclaimed: {} bytes", result.space_reclaimed);

        for operation in &result.operations_performed {
            println!("✓ {operation}");
        }
    }

    Ok(result)
}

/// Main entry point for gc command
pub async fn execute_gc_command(config: &GcConfig) -> Result<CleanupResult> {
    let store = Store::new()?;
    let cleanup_manager = CleanupManager::new(store);

    let result = cleanup_manager.execute_gc(config).await?;

    // Report results
    if config.dry_run {
        println!("\nGarbage collection dry run summary:");
        println!("Would clean {} repositories", result.repos_cleaned);
        println!("Would clean {} environments", result.envs_cleaned);
        println!("Would clean {} temporary files", result.temp_files_cleaned);
    } else {
        println!("\nGarbage collection summary:");
        println!("Cleaned {} repositories", result.repos_cleaned);
        println!("Cleaned {} environments", result.envs_cleaned);
        println!("Cleaned {} temporary files", result.temp_files_cleaned);
        println!("Space reclaimed: {} bytes", result.space_reclaimed);

        for operation in &result.operations_performed {
            println!("✓ {operation}");
        }
    }

    Ok(result)
}
