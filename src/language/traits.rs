// Core Language trait definition for the SNP language plugin architecture

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use thiserror::Error;

use crate::core::Hook;
use crate::error::{HookExecutionError, Result, SnpError};
use crate::execution::HookExecutionResult;

use super::dependency::{Dependency, DependencyManager};
use super::environment::{
    EnvironmentConfig, EnvironmentInfo, LanguageEnvironment, ValidationReport,
};

/// Main trait for language plugin implementations
///
/// This trait defines the interface that all language plugins must implement
/// to provide consistent environment management, dependency installation,
/// and hook execution across different programming languages.
#[async_trait]
pub trait Language: Send + Sync {
    /// Language identification
    fn language_name(&self) -> &str;
    fn supported_extensions(&self) -> &[&str];
    fn detection_patterns(&self) -> &[&str];

    /// Environment management
    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment>;
    async fn install_dependencies(
        &self,
        env: &LanguageEnvironment,
        dependencies: &[Dependency],
    ) -> Result<()>;
    async fn cleanup_environment(&self, env: &LanguageEnvironment) -> Result<()>;

    /// Hook execution
    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult>;
    fn build_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command>;

    /// Dependency management
    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>>;
    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency>;
    fn get_dependency_manager(&self) -> &dyn DependencyManager;

    /// Environment introspection
    fn get_environment_info(&self, env: &LanguageEnvironment) -> EnvironmentInfo;
    fn validate_environment(&self, env: &LanguageEnvironment) -> Result<ValidationReport>;

    /// Configuration and customization
    fn default_config(&self) -> LanguageConfig;
    fn validate_config(&self, config: &LanguageConfig) -> Result<()>;
}

/// Language-specific configuration
#[derive(Debug, Clone)]
pub struct LanguageConfig {
    pub enabled: bool,
    pub default_version: Option<String>,
    pub environment_variables: HashMap<String, String>,
    pub dependency_manager_config: super::dependency::DependencyManagerConfig,
    pub cache_settings: CacheSettings,
    pub execution_settings: ExecutionSettings,
}

#[derive(Debug, Clone)]
pub struct CacheSettings {
    pub enable_environment_cache: bool,
    pub enable_dependency_cache: bool,
    pub cache_ttl: Duration,
    pub max_cache_size: Option<u64>,
}

#[derive(Debug, Clone)]
pub struct ExecutionSettings {
    pub default_timeout: Duration,
    pub max_memory_mb: Option<u64>,
    pub environment_isolation: super::environment::IsolationLevel,
    pub capture_output: bool,
}

/// Command construction for hook execution
#[derive(Debug)]
pub struct Command {
    pub executable: String,
    pub arguments: Vec<String>,
    pub environment: HashMap<String, String>,
    pub working_directory: Option<PathBuf>,
    pub timeout: Option<Duration>,
}

/// Language plugin errors with detailed context
#[derive(Debug, Error)]
pub enum LanguageError {
    #[error("Language plugin not found: {language}")]
    PluginNotFound {
        language: String,
        available_languages: Vec<String>,
    },

    #[error("Environment setup failed for {language}: {error}")]
    EnvironmentSetupFailed {
        language: String,
        error: String,
        recovery_suggestion: Option<String>,
    },

    #[error("Dependency installation failed: {dependency}")]
    DependencyInstallationFailed {
        dependency: String,
        language: String,
        error: String,
        installation_log: Option<String>,
    },

    #[error("Plugin initialization failed: {plugin_name}")]
    PluginInitializationFailed {
        plugin_name: String,
        error: String,
        plugin_path: Option<PathBuf>,
    },

    #[error("Language version not supported: {version}")]
    UnsupportedVersion {
        language: String,
        version: String,
        supported_versions: Vec<String>,
    },

    #[error("Command construction failed: {error}")]
    CommandConstructionFailed {
        hook_id: String,
        language: String,
        error: String,
    },

    #[error("Environment validation failed: {language}")]
    EnvironmentValidationFailed {
        language: String,
        issues: Vec<String>,
    },
}

impl Default for LanguageConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            default_version: None,
            environment_variables: HashMap::new(),
            dependency_manager_config: super::dependency::DependencyManagerConfig::default(),
            cache_settings: CacheSettings {
                enable_environment_cache: true,
                enable_dependency_cache: true,
                cache_ttl: Duration::from_secs(3600), // 1 hour
                max_cache_size: Some(1024 * 1024 * 100), // 100MB
            },
            execution_settings: ExecutionSettings {
                default_timeout: Duration::from_secs(300), // 5 minutes
                max_memory_mb: Some(512),
                environment_isolation: super::environment::IsolationLevel::Partial,
                capture_output: true,
            },
        }
    }
}

impl Command {
    /// Create a new command
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            arguments: Vec::new(),
            environment: HashMap::new(),
            working_directory: None,
            timeout: None,
        }
    }

    /// Add an argument
    pub fn arg(&mut self, arg: impl Into<String>) -> &mut Self {
        self.arguments.push(arg.into());
        self
    }

    /// Add multiple arguments
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.arguments.extend(args.into_iter().map(|s| s.into()));
        self
    }

    /// Set environment variable
    pub fn env(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    /// Set working directory
    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.working_directory = Some(dir.as_ref().to_path_buf());
        self
    }

    /// Set timeout
    pub fn timeout(&mut self, timeout: Duration) -> &mut Self {
        self.timeout = Some(timeout);
        self
    }
}

// Convert language errors to SNP errors
impl From<LanguageError> for SnpError {
    fn from(err: LanguageError) -> Self {
        match err {
            LanguageError::EnvironmentSetupFailed {
                language, error, ..
            } => SnpError::HookExecution(Box::new(HookExecutionError::EnvironmentSetupFailed {
                language,
                hook_id: "unknown".to_string(),
                message: error,
                suggestion: None,
            })),
            LanguageError::DependencyInstallationFailed {
                dependency,
                language,
                error,
                ..
            } => SnpError::HookExecution(Box::new(HookExecutionError::DependencyInstallFailed {
                dependency,
                hook_id: "unknown".to_string(),
                language,
                error,
            })),
            other => {
                SnpError::HookExecution(Box::new(HookExecutionError::EnvironmentSetupFailed {
                    language: "unknown".to_string(),
                    hook_id: "unknown".to_string(),
                    message: other.to_string(),
                    suggestion: None,
                }))
            }
        }
    }
}
