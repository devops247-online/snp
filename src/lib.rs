// SNP (Shell Not Pass) - Library module
// This file contains the core library functionality

pub mod classification;
pub mod cli;
pub mod concurrency;
pub mod config;
pub mod core;
pub mod error;
pub mod execution;
pub mod file_lock;
pub mod filesystem;
pub mod git;
pub mod hook_chaining;
pub mod logging;
pub mod migration;
pub mod output;
pub mod process;
pub mod regex_processor;
pub mod storage;
pub mod validation;

// Re-export main types for easier access
pub use classification::{
    ClassificationError, DetectionMethod, FileClassification, FileClassifier, FileType,
    LanguageDefinitions, LanguageDetection,
};
pub use concurrency::{
    BatchResult, ConcurrencyExecutor, ErrorAggregator, ResourceGuard, ResourceLimits,
    ResourceRequirements, ResourceUsage, TaskConfig, TaskPriority, TaskResult, TaskState,
};
pub use config::Config;
pub use core::{ExecutionContext, Hook, Repository, Stage};
pub use error::{
    exit_codes, CliError, ConfigError, GitError, HookChainingError, HookExecutionError, LockError,
    ProcessError, Result, SnpError, StorageError,
};
pub use execution::{ExecutionConfig, ExecutionResult, HookExecutionEngine, HookExecutionResult};
pub use file_lock::{
    ConfigFileLock, FileLock, FileLockManager, LockBehavior, LockConfig, LockHierarchy, LockInfo,
    LockMetrics, LockOrdering, LockStatus, LockType, StaleLockDetector, TempFileLock,
};
pub use filesystem::{FileFilter, FileSystem};
pub use git::GitRepository;
pub use hook_chaining::{
    ChainExecutionResult, ChainedHook, ConditionContext, ConditionEvaluator,
    CustomConditionEvaluator, DependencyGraph, DependencyResolver, ExecutionCondition,
    ExecutionPlan, ExecutionStrategy, FailureBehavior, FailureStrategy, HookChain,
    HookChainExecutor, InterHookCommunication,
};
pub use logging::{ColorConfig, LogConfig, LogFormat};
pub use migration::{
    AppliedMigration, ConfigFormat, ConfigMigrator, FieldMigration, FieldTransformation,
    MigrationComplexity, MigrationConfig, MigrationError, MigrationPlan, MigrationResult,
    MigrationRule, MigrationWarning, RepositoryMigrationResult, StructuralChange,
    StructuralChangeType, TransformationType,
};
pub use output::{
    AggregatedOutput, BufferEntry, CacheStatistics, CollectedOutput, ColorMode, ExecutionSummary,
    GitHubFormatter, HumanFormatter, JsonFormatter, JunitFormatter, OutputAggregator, OutputBuffer,
    OutputCollector, OutputConfig, OutputFormat, OutputStream, OutputWriter,
    PerformanceMetrics as OutputPerformanceMetrics, ProgressConfig, ProgressRenderer,
    ProgressReporter, ProgressState, ResourceUsageMetrics, ResultFormatter, StdoutWriter,
    TapFormatter, TeamCityFormatter, TerminalProgressRenderer, ThroughputMetrics, VerbosityLevel,
};
pub use process::{
    OutputHandler, ProcessConfig, ProcessEnvironment, ProcessManager, ProcessResult,
};
pub use regex_processor::{
    BatchRegexProcessor, CompiledRegex, MatchMatrix, PatternAnalysis, PatternAnalyzer,
    PatternIssue, PerformanceClass, RegexConfig, RegexError, RegexProcessor, SecurityWarning,
};
pub use storage::{ConfigInfo, EnvironmentInfo, RepositoryInfo, Store};
pub use validation::{
    PerformanceImpact, PerformanceIssue, PerformanceMetrics, PerformanceWarning, SchemaValidator,
    ValidationConfig, ValidationError, ValidationErrorType, ValidationResult, ValidationWarning,
};

// Version information
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NAME: &str = env!("CARGO_PKG_NAME");
pub const DESCRIPTION: &str = env!("CARGO_PKG_DESCRIPTION");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_constant() {
        assert_eq!(VERSION, "0.1.0");
    }

    #[test]
    fn test_name_constant() {
        assert_eq!(NAME, "snp");
    }

    #[test]
    fn test_description_exists() {
        // DESCRIPTION is a const string that's never empty
        assert!(DESCRIPTION.contains("Shell Not Pass"));
    }
}
