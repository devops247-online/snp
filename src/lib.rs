// SNP (Shell Not Pass) - Library module
// This file contains the core library functionality

pub mod cache;
pub mod classification;
pub mod cli;
pub mod commands;
pub mod concurrency;
pub mod config;
pub mod core;
pub mod enhanced_regex_processor;
pub mod error;
pub mod events;
pub mod execution;
pub mod file_change_detector;
pub mod file_lock;
pub mod filesystem;
pub mod git;
pub mod hook_chaining;
pub mod install;
pub mod language;
pub mod lock_free_cache;
pub mod lock_free_scheduler;
pub mod logging;
pub mod migration;
pub mod negative_cache;
pub mod output;
pub mod pooled_git;
pub mod pooled_language;
pub mod process;
pub mod regex_processor;
pub mod resource_pool;
pub mod resource_pool_manager;
pub mod storage;
pub mod user_output;
pub mod validation;

// Re-export main types for easier access
pub use cache::{CacheConfig, CacheMetrics, CacheTier, MultiTierCache};
pub use classification::{
    ClassificationError, DetectionMethod, FileClassification, FileClassifier, FileType,
    LanguageDefinitions, LanguageDetection,
};
pub use concurrency::{
    BatchResult, ConcurrencyExecutor, ErrorAggregator, ResourceGuard, ResourceLimits,
    ResourceRequirements, ResourceUsage, TaskConfig, TaskPriority, TaskResult, TaskState,
};
pub use config::{Config, IncrementalConfig};
pub use core::{ExecutionContext, Hook, Repository, Stage};
pub use enhanced_regex_processor::{CachePerformanceReport, EnhancedRegexProcessor};
pub use error::{
    exit_codes, CliError, ConfigError, GitError, HookChainingError, HookExecutionError, LockError,
    ProcessError, Result, SnpError, StorageError,
};
pub use events::{
    EventBus, EventConfig, EventFilter, EventHandler, EventHandlerPriority, EventMetrics,
    HookEvent, HookEventType, LoggingEventHandler, MetricsEventHandler,
};
pub use execution::{ExecutionConfig, ExecutionResult, HookExecutionEngine, HookExecutionResult};
pub use file_change_detector::{FileChangeDetector, FileChangeDetectorConfig};
pub use file_lock::{
    ConfigFileLock, FileLock, FileLockManager, LockBehavior, LockConfig, LockHierarchy, LockInfo,
    LockMetrics, LockOrdering, LockStatus, LockType, StaleLockDetector, TempFileLock,
};
pub use filesystem::{AsyncConfig, FileFilter, FileSystem};
pub use git::GitRepository;
pub use hook_chaining::{
    ChainExecutionResult, ChainedHook, ConditionContext, ConditionEvaluator,
    CustomConditionEvaluator, DependencyGraph, DependencyResolver, ExecutionCondition,
    ExecutionPlan, ExecutionStrategy, FailureBehavior, FailureStrategy, HookChain,
    HookChainExecutor, InterHookCommunication,
};
pub use install::{
    BackupInfo, CleanupResult, GitHookManager, HookBackupManager, HookConfig,
    HookTemplateGenerator, HookType, InstallConfig, InstallResult, RestoreInfo, UninstallConfig,
    UninstallResult,
};
pub use language::{
    registry::{RegistryConfig, RegistryStats},
    BaseLanguagePlugin, CommandBuilder, Dependency, DependencyConflict, DependencyManager,
    DependencyManagerConfig, DependencySource, EnvironmentConfig, EnvironmentInfo,
    EnvironmentManager, EnvironmentMetadata, ExecutionCache, InstallationResult, InstalledPackage,
    IsolationLevel, Language, LanguageEnvironment, LanguageError, LanguageHookExecutor,
    LanguageRegistry, PluginMetadata, ResolvedDependency, UpdateResult, ValidationIssue,
    ValidationReport, VersionSpec,
};
pub use lock_free_cache::{CacheStats, LockFreeCache};
pub use lock_free_scheduler::{
    LoadBalanceMetrics, LockFreeTaskScheduler, ScheduledTask, SchedulerStats, WorkerState,
};
pub use logging::{ColorConfig, LogConfig, LogFormat};
pub use migration::{
    AppliedMigration, ConfigFormat, ConfigMigrator, FieldMigration, FieldTransformation,
    MigrationComplexity, MigrationConfig, MigrationError, MigrationPlan, MigrationResult,
    MigrationRule, MigrationWarning, RepositoryMigrationResult, StructuralChange,
    StructuralChangeType, TransformationType,
};
pub use negative_cache::{NegativeCache, NegativeCacheConfig, NegativeCacheMetrics};
pub use output::{
    AggregatedOutput, BufferEntry, CacheStatistics, CollectedOutput, ColorMode, ExecutionSummary,
    GitHubFormatter, HumanFormatter, JsonFormatter, JunitFormatter, OutputAggregator, OutputBuffer,
    OutputCollector, OutputConfig, OutputFormat, OutputStream, OutputWriter,
    PerformanceMetrics as OutputPerformanceMetrics, ProgressConfig, ProgressRenderer,
    ProgressReporter, ProgressState, ResourceUsageMetrics, ResultFormatter, StdoutWriter,
    TapFormatter, TeamCityFormatter, TerminalProgressRenderer, ThroughputMetrics, VerbosityLevel,
};
pub use pooled_git::{
    GitPoolConfig, PooledGitError, PooledGitRepository, PooledGitStats, RepositoryState,
};
pub use pooled_language::{
    DependencyState, LanguagePoolConfig, PooledLanguageEnvironment, PooledLanguageError,
    PooledLanguageStats,
};
pub use process::{
    OutputHandler, ProcessConfig, ProcessEnvironment, ProcessManager, ProcessResult,
};
pub use regex_processor::{
    BatchRegexProcessor, CompiledRegex, MatchMatrix, PatternAnalysis, PatternAnalyzer,
    PatternIssue, PerformanceClass, RegexConfig, RegexError, RegexProcessor, SecurityWarning,
};
pub use resource_pool::{
    PoolConfig, PoolError, PoolGuard, PoolMaintenance, PoolStats, Poolable, PooledResource,
    PooledResourceMetadata, ResourcePool,
};
pub use resource_pool_manager::{
    MaintenanceResults, ResourcePoolManager, ResourcePoolManagerConfig, ResourcePoolStats,
};
pub use storage::{ConfigInfo, EnvironmentInfo as StorageEnvironmentInfo, RepositoryInfo, Store};
pub use validation::{
    PerformanceImpact, PerformanceIssue, PerformanceMetrics, PerformanceWarning, SchemaValidator,
    ValidationConfig, ValidationError, ValidationErrorType, ValidationResult, ValidationWarning,
};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

// Build information (set by build script)
pub const BUILD_DATE: &str = env!("BUILD_DATE");
pub const GIT_COMMIT: &str = env!("GIT_COMMIT");
pub const GIT_BRANCH: &str = env!("GIT_BRANCH");
pub const RUST_VERSION: &str = env!("RUST_VERSION");

/// Get formatted version string with build information
pub fn version_info() -> String {
    format!(
        "{NAME} {VERSION} (commit: {GIT_COMMIT}, branch: {GIT_BRANCH}, built: {BUILD_DATE}, rustc: {RUST_VERSION})"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constant() {
        // Verify VERSION follows semantic versioning format (X.Y.Z or X.Y.Z-suffix)
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert!(
            parts.len() >= 3,
            "VERSION '{VERSION}' should have at least 3 parts separated by dots (X.Y.Z)"
        );

        // Check that first three parts are numbers
        for (i, part) in parts.iter().take(3).enumerate() {
            let number_part = if i == 2 {
                // Third part might have a suffix (e.g., "0-alpha1")
                part.split('-').next().unwrap_or(part)
            } else {
                part
            };

            assert!(
                number_part.chars().all(|c| c.is_ascii_digit()),
                "VERSION '{VERSION}' part '{number_part}' should be a number"
            );
        }
    }

    #[test]
    fn test_name_constant() {
        assert_eq!(NAME, "snp");
    }

    #[test]
    fn test_description_exists() {
        // DESCRIPTION is a const string that's never empty
        assert!(DESCRIPTION.contains("pre-commit framework"));
        assert!(DESCRIPTION.contains("Rust"));
    }
}
