// Environment management system for language plugins

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use crate::error::Result;
use crate::storage::Store;

use super::dependency::Dependency;

/// Language environment representation
#[derive(Debug, Clone)]
pub struct LanguageEnvironment {
    pub language: String,
    pub environment_id: String,
    pub root_path: PathBuf,
    pub executable_path: PathBuf,
    pub environment_variables: HashMap<String, String>,
    pub installed_dependencies: Vec<Dependency>,
    pub metadata: EnvironmentMetadata,
}

/// Environment configuration for setup
#[derive(Debug, Clone)]
pub struct EnvironmentConfig {
    pub language_version: Option<String>,
    pub additional_dependencies: Vec<String>,
    pub environment_variables: HashMap<String, String>,
    pub cache_strategy: CacheStrategy,
    pub isolation_level: IsolationLevel,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IsolationLevel {
    None,     // Use system environment
    Partial,  // Isolated PATH and language-specific vars
    Complete, // Fully isolated environment
}

#[derive(Debug, Clone)]
pub enum CacheStrategy {
    None,
    Memory,
    Disk,
    Both,
}

/// Environment metadata and statistics
#[derive(Debug, Clone)]
pub struct EnvironmentMetadata {
    pub created_at: SystemTime,
    pub last_used: SystemTime,
    pub usage_count: u64,
    pub size_bytes: u64,
    pub dependency_count: usize,
    pub language_version: String,
    pub platform: String,
}

/// Environment manager for language-specific environments
pub struct EnvironmentManager {
    storage: Arc<Store>,
    cache_root: PathBuf,
    active_environments: HashMap<String, Arc<LanguageEnvironment>>,
}

impl EnvironmentManager {
    pub fn new(storage: Arc<Store>, cache_root: PathBuf) -> Self {
        Self {
            storage,
            cache_root,
            active_environments: HashMap::new(),
        }
    }

    // Environment lifecycle
    pub async fn create_environment(
        &mut self,
        language: &str,
        config: &EnvironmentConfig,
    ) -> Result<Arc<LanguageEnvironment>> {
        let env_id = self.generate_environment_id(language, config);
        let env_path = self.cache_root.join(&env_id);

        std::fs::create_dir_all(&env_path)?;

        let environment = Arc::new(LanguageEnvironment {
            language: language.to_string(),
            environment_id: env_id.clone(),
            root_path: env_path,
            executable_path: PathBuf::new(), // Will be set by language plugin
            environment_variables: config.environment_variables.clone(),
            installed_dependencies: Vec::new(),
            metadata: EnvironmentMetadata {
                created_at: SystemTime::now(),
                last_used: SystemTime::now(),
                usage_count: 0,
                size_bytes: 0,
                dependency_count: 0,
                language_version: config
                    .language_version
                    .clone()
                    .unwrap_or_else(|| "default".to_string()),
                platform: self.get_platform_string(),
            },
        });

        self.active_environments.insert(env_id, environment.clone());
        Ok(environment)
    }

    pub async fn get_or_create_environment(
        &mut self,
        language: &str,
        config: &EnvironmentConfig,
    ) -> Result<Arc<LanguageEnvironment>> {
        let env_id = self.generate_environment_id(language, config);

        if let Some(env) = self.active_environments.get(&env_id) {
            return Ok(env.clone());
        }

        // Try to load from storage first
        if let Ok(env_info) = self
            .storage
            .get_environment_info(&env_id, &config.additional_dependencies)
        {
            // Environment exists in storage, load it
            let env_path = self.cache_root.join(&env_id);
            if env_path.exists() {
                let environment = Arc::new(LanguageEnvironment {
                    language: language.to_string(),
                    environment_id: env_id.clone(),
                    root_path: env_path,
                    executable_path: PathBuf::new(),
                    environment_variables: config.environment_variables.clone(),
                    installed_dependencies: Vec::new(), // Will be loaded from storage
                    metadata: EnvironmentMetadata {
                        created_at: SystemTime::now(), // Use current time as fallback
                        last_used: env_info.last_used,
                        usage_count: 1, // Reset usage count
                        size_bytes: 0,  // Will be calculated separately
                        dependency_count: env_info.dependencies.len(),
                        language_version: config
                            .language_version
                            .clone()
                            .unwrap_or_else(|| "default".to_string()),
                        platform: self.get_platform_string(),
                    },
                });

                self.active_environments.insert(env_id, environment.clone());
                return Ok(environment);
            }
        }

        // Create new environment
        self.create_environment(language, config).await
    }

    pub async fn destroy_environment(&mut self, env_id: &str) -> Result<()> {
        // Remove from active environments
        self.active_environments.remove(env_id);

        // Remove from storage - we'll implement this method or handle it differently
        // For now, just clean up filesystem since storage doesn't have remove_environment

        // Remove from filesystem
        let env_path = self.cache_root.join(env_id);
        if env_path.exists() {
            std::fs::remove_dir_all(env_path)?;
        }

        Ok(())
    }

    // Environment management
    pub fn list_environments(&self, language: Option<&str>) -> Vec<&LanguageEnvironment> {
        self.active_environments
            .values()
            .filter_map(|env| {
                if let Some(lang) = language {
                    if env.language == lang {
                        Some(env.as_ref())
                    } else {
                        None
                    }
                } else {
                    Some(env.as_ref())
                }
            })
            .collect()
    }

    pub async fn cleanup_unused_environments(&mut self, max_age: Duration) -> Result<usize> {
        let cutoff_time = SystemTime::now() - max_age;
        let mut cleaned_count = 0;

        let mut to_remove = Vec::new();
        for (env_id, env) in &self.active_environments {
            if env.metadata.last_used < cutoff_time {
                to_remove.push(env_id.clone());
            }
        }

        for env_id in to_remove {
            self.destroy_environment(&env_id).await?;
            cleaned_count += 1;
        }

        Ok(cleaned_count)
    }

    pub fn get_environment_size(&self, env_id: &str) -> Result<u64> {
        let env_path = self.cache_root.join(env_id);
        if !env_path.exists() {
            return Ok(0);
        }

        // Calculate directory size recursively
        fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
            let mut size = 0;
            if path.is_dir() {
                for entry in std::fs::read_dir(path)? {
                    let entry = entry?;
                    let entry_path = entry.path();
                    if entry_path.is_dir() {
                        size += dir_size(&entry_path)?;
                    } else {
                        size += entry.metadata()?.len();
                    }
                }
            }
            Ok(size)
        }
        Ok(dir_size(&env_path)?)
    }

    // Caching and optimization
    pub fn environment_cache_key(language: &str, config: &EnvironmentConfig) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        language.hash(&mut hasher);
        config.language_version.hash(&mut hasher);
        config.additional_dependencies.hash(&mut hasher);
        // Don't hash environment variables or they'll make cache useless

        format!("{}-{:x}", language, hasher.finish())
    }

    pub async fn warm_environment_cache(&mut self, languages: &[String]) -> Result<()> {
        for language in languages {
            let config = EnvironmentConfig::default();
            let _ = self.get_or_create_environment(language, &config).await;
        }
        Ok(())
    }

    // Helper methods
    fn generate_environment_id(&self, language: &str, config: &EnvironmentConfig) -> String {
        Self::environment_cache_key(language, config)
    }

    fn get_platform_string(&self) -> String {
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

/// Environment information for storage and retrieval
#[derive(Debug, Clone)]
pub struct EnvironmentInfo {
    pub environment_id: String,
    pub language: String,
    pub language_version: String,
    pub created_at: SystemTime,
    pub last_used: SystemTime,
    pub usage_count: u64,
    pub size_bytes: u64,
    pub dependency_count: usize,
    pub is_healthy: bool,
}

/// Environment validation and health checking
#[derive(Debug)]
pub struct ValidationReport {
    pub is_healthy: bool,
    pub issues: Vec<ValidationIssue>,
    pub recommendations: Vec<String>,
    pub performance_score: f64,
}

#[derive(Debug)]
pub struct ValidationIssue {
    pub severity: IssueSeverity,
    pub category: IssueCategory,
    pub description: String,
    pub fix_suggestion: Option<String>,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum IssueSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

#[derive(Debug)]
pub enum IssueCategory {
    MissingDependency,
    VersionMismatch,
    PermissionIssue,
    PathConfiguration,
    Performance,
    Security,
}

impl Default for EnvironmentConfig {
    fn default() -> Self {
        Self {
            language_version: None,
            additional_dependencies: Vec::new(),
            environment_variables: HashMap::new(),
            cache_strategy: CacheStrategy::Both,
            isolation_level: IsolationLevel::Partial,
        }
    }
}

impl EnvironmentConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.language_version = Some(version.into());
        self
    }

    pub fn with_dependencies(mut self, deps: Vec<String>) -> Self {
        self.additional_dependencies = deps;
        self
    }

    pub fn with_env_var(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment_variables.insert(key.into(), value.into());
        self
    }

    pub fn with_isolation(mut self, level: IsolationLevel) -> Self {
        self.isolation_level = level;
        self
    }
}
