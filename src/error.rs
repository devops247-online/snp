// Comprehensive error handling framework for SNP
use std::path::PathBuf;
use thiserror::Error;

pub type Result<T> = std::result::Result<T, SnpError>;

/// Main error type for SNP with comprehensive error hierarchy
#[derive(Debug, Error)]
pub enum SnpError {
    #[error("Configuration error: {0}")]
    Config(#[from] Box<ConfigError>),

    #[error("Git operation failed: {0}")]
    Git(#[from] Box<GitError>),

    #[error("Hook execution failed: {0}")]
    HookExecution(#[from] Box<HookExecutionError>),

    #[error("IO operation failed: {0}")]
    Io(#[from] std::io::Error),

    #[error("CLI argument error: {0}")]
    Cli(#[from] Box<CliError>),

    #[error("Storage operation failed: {0}")]
    Storage(#[from] Box<StorageError>),

    #[error("Process execution failed: {0}")]
    Process(#[from] Box<ProcessError>),

    #[error("File locking failed: {0}")]
    Lock(#[from] Box<LockError>),

    #[error("File classification failed: {0}")]
    Classification(crate::classification::ClassificationError),

    #[error("Regex processing failed: {0}")]
    Regex(#[from] Box<crate::regex_processor::RegexError>),
}

/// Configuration-related errors with detailed context
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Invalid YAML syntax: {message}")]
    InvalidYaml {
        message: String,
        line: Option<u32>,
        column: Option<u32>,
        file_path: Option<PathBuf>,
    },

    #[error("Missing required field: {field}")]
    MissingField {
        field: String,
        file_path: Option<PathBuf>,
        line: Option<u32>,
    },

    #[error("Configuration file not found: {path}")]
    NotFound {
        path: PathBuf,
        suggestion: Option<String>,
    },

    #[error("Invalid configuration value: {message}")]
    InvalidValue {
        message: String,
        field: String,
        value: String,
        expected: String,
        file_path: Option<PathBuf>,
        line: Option<u32>,
    },

    #[error("Configuration validation failed: {message}")]
    ValidationFailed {
        message: String,
        file_path: Option<PathBuf>,
        errors: Vec<String>,
    },

    #[error("Invalid regex pattern: {pattern}")]
    InvalidRegex {
        pattern: String,
        field: String,
        error: String,
        file_path: Option<PathBuf>,
        line: Option<u32>,
    },

    #[error("IO operation failed: {message}")]
    IOError {
        message: String,
        path: Option<PathBuf>,
    },
}

/// Git operation errors with context
#[derive(Debug, Error)]
pub enum GitError {
    #[error("Repository not found: {path}")]
    RepositoryNotFound {
        path: PathBuf,
        suggestion: Option<String>,
    },

    #[error("Git command failed: {command}")]
    CommandFailed {
        command: String,
        exit_code: Option<i32>,
        stdout: String,
        stderr: String,
        working_dir: Option<PathBuf>,
    },

    #[error("Permission denied for git operation: {operation}")]
    PermissionDenied {
        operation: String,
        path: PathBuf,
        suggestion: Option<String>,
    },

    #[error("Invalid git reference: {reference}")]
    InvalidReference {
        reference: String,
        repository: String,
        suggestion: Option<String>,
    },

    #[error("Git operation timeout: {operation}")]
    Timeout {
        operation: String,
        duration_secs: u64,
    },

    #[error("Remote repository error: {message}")]
    RemoteError {
        message: String,
        repository: String,
        operation: String,
    },
}

/// Hook execution errors with detailed context
#[derive(Debug, Error)]
pub enum HookExecutionError {
    #[error("Command not found: {command}")]
    CommandNotFound {
        command: String,
        hook_id: String,
        language: String,
        suggestion: Option<String>,
    },

    #[error("Hook execution timeout: {hook_id}")]
    ExecutionTimeout {
        hook_id: String,
        command: String,
        timeout_secs: u64,
    },

    #[error("Permission denied executing hook: {hook_id}")]
    PermissionDenied {
        hook_id: String,
        command: String,
        path: PathBuf,
        suggestion: Option<String>,
    },

    #[error("Hook failed with exit code {exit_code}: {hook_id}")]
    NonZeroExit {
        hook_id: String,
        command: String,
        exit_code: i32,
        stdout: String,
        stderr: String,
        files_processed: Vec<PathBuf>,
    },

    #[error("Environment setup failed for {language}: {message}")]
    EnvironmentSetupFailed {
        language: String,
        hook_id: String,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Dependency installation failed: {dependency}")]
    DependencyInstallFailed {
        dependency: String,
        hook_id: String,
        language: String,
        error: String,
    },
}

/// CLI argument and command-line interface errors
#[derive(Debug, Error)]
pub enum CliError {
    #[error("Invalid argument: {argument}")]
    InvalidArgument {
        argument: String,
        message: String,
        suggestion: Option<String>,
    },

    #[error("Conflicting arguments: {first} and {second}")]
    ConflictingArguments {
        first: String,
        second: String,
        suggestion: String,
    },

    #[error("Missing required argument: {argument}")]
    MissingArgument { argument: String, context: String },

    #[error("Invalid subcommand: {subcommand}")]
    InvalidSubcommand {
        subcommand: String,
        available: Vec<String>,
    },
}

/// Storage and database operation errors
#[derive(Debug, Error)]
pub enum StorageError {
    #[error("Database connection failed: {message}")]
    ConnectionFailed {
        message: String,
        database_path: Option<PathBuf>,
    },

    #[error("Database initialization failed: {message}")]
    InitializationFailed {
        message: String,
        database_path: PathBuf,
    },

    #[error("Database query failed: {query}")]
    QueryFailed {
        query: String,
        error: String,
        database_path: Option<PathBuf>,
    },

    #[error("Database transaction failed: {operation}")]
    TransactionFailed { operation: String, error: String },

    #[error("Database schema migration failed: {from_version} -> {to_version}")]
    MigrationFailed {
        from_version: u32,
        to_version: u32,
        error: String,
    },

    #[error("Cache directory creation failed: {path}")]
    CacheDirectoryFailed { path: PathBuf, error: String },

    #[error("Repository not found in cache: {url}@{revision}")]
    RepositoryNotFound {
        url: String,
        revision: String,
        suggestion: Option<String>,
    },

    #[error("Environment not found: {language} with dependencies {dependencies:?}")]
    EnvironmentNotFound {
        language: String,
        dependencies: Vec<String>,
        suggestion: Option<String>,
    },

    #[error("File locking failed: {path}")]
    FileLockFailed {
        path: PathBuf,
        error: String,
        timeout_secs: Option<u64>,
    },

    #[error("Repository cleanup failed: {path}")]
    CleanupFailed { path: PathBuf, error: String },

    #[error("Database corruption detected: {message}")]
    DatabaseCorrupted {
        message: String,
        database_path: PathBuf,
        suggestion: Option<String>,
    },

    #[error("Concurrent access conflict: {operation}")]
    ConcurrencyConflict {
        operation: String,
        error: String,
        retry_suggested: bool,
    },
}

/// Process execution errors with detailed context
#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("Process execution failed: {command}")]
    ExecutionFailed {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("Process timeout after {duration:?}: {command}")]
    Timeout {
        command: String,
        duration: std::time::Duration,
    },

    #[error("Resource limit exceeded: {limit_type}")]
    ResourceLimitExceeded {
        limit_type: String,
        current_value: u64,
        limit_value: u64,
    },

    #[error("Signal handling failed: {signal}")]
    SignalError { signal: String, error: String },

    #[error("Command not found: {command}")]
    CommandNotFound {
        command: String,
        suggestion: Option<String>,
    },

    #[error("Environment setup failed: {message}")]
    EnvironmentSetupFailed {
        message: String,
        variable: Option<String>,
    },

    #[error("Process spawn failed: {command}")]
    SpawnFailed { command: String, error: String },

    #[error("Output capture failed: {message}")]
    OutputCaptureFailed { message: String, command: String },

    #[error("Parallel execution failed: {failed_count} of {total_count} processes failed")]
    ParallelExecutionFailed {
        failed_count: usize,
        total_count: usize,
        errors: Vec<String>,
    },

    #[error("Process termination failed: {command}")]
    TerminationFailed { command: String, error: String },
}

/// File locking errors with detailed context
#[derive(Debug, Error)]
pub enum LockError {
    #[error("Lock acquisition timeout: {path}")]
    Timeout {
        path: PathBuf,
        timeout: std::time::Duration,
        lock_type: crate::file_lock::LockType,
    },

    #[error("Lock acquisition failed: {path}")]
    AcquisitionFailed {
        path: PathBuf,
        error: String,
        lock_type: crate::file_lock::LockType,
    },

    #[error("Deadlock detected in lock hierarchy: {paths:?}")]
    DeadlockDetected {
        paths: Vec<PathBuf>,
        current_order: Vec<u32>,
    },

    #[error("Stale lock cleanup failed: {path}")]
    StaleCleanupFailed {
        path: PathBuf,
        process_id: u32,
        error: String,
    },

    #[error(
        "Lock hierarchy violation: {path} level {level} < {previous_path} level {previous_level}"
    )]
    HierarchyViolation {
        path: PathBuf,
        level: u32,
        previous_path: PathBuf,
        previous_level: u32,
    },

    #[error("Unsupported lock operation: {operation}")]
    UnsupportedOperation {
        operation: String,
        platform: String,
        suggestion: Option<String>,
    },

    #[error("Platform-specific lock error on {platform}: {operation}")]
    PlatformSpecific {
        platform: String,
        operation: String,
        error: String,
    },

    #[error("Lock contention detected: {path}")]
    LockContention {
        path: PathBuf,
        waiting_processes: u32,
        estimated_wait: std::time::Duration,
    },
}

/// Format errors with colors and context
pub struct ErrorFormatter {
    use_colors: bool,
}

impl ErrorFormatter {
    pub fn new(use_colors: bool) -> Self {
        Self { use_colors }
    }

    /// Format an error with context and colors
    pub fn format_error(&self, error: &SnpError) -> String {
        // Log the error with structured context
        use tracing::error;

        match error {
            SnpError::Config(_) => {
                error!(error_type = "config", error = %error, "Configuration error occurred");
            }
            SnpError::Git(_) => {
                error!(error_type = "git", error = %error, "Git operation failed");
            }
            SnpError::HookExecution(_) => {
                error!(error_type = "hook_execution", error = %error, "Hook execution failed");
            }
            SnpError::Cli(_) => {
                error!(error_type = "cli", error = %error, "CLI error occurred");
            }
            SnpError::Storage(_) => {
                error!(error_type = "storage", error = %error, "Storage operation failed");
            }
            SnpError::Process(_) => {
                error!(error_type = "process", error = %error, "Process execution failed");
            }
            SnpError::Lock(_) => {
                error!(error_type = "lock", error = %error, "File locking failed");
            }
            SnpError::Io(_) => {
                error!(error_type = "io", error = %error, "IO operation failed");
            }
            SnpError::Classification(_) => {
                error!(error_type = "classification", error = %error, "File classification failed");
            }
            SnpError::Regex(_) => {
                error!(error_type = "regex", error = %error, "Regex processing failed");
            }
        }

        let mut output = String::new();

        if self.use_colors {
            output.push_str("\x1b[31m"); // Red color
        }
        output.push_str("Error: ");

        if self.use_colors {
            output.push_str("\x1b[0m"); // Reset color
        }

        output.push_str(&error.to_string());

        // Add context information
        match error {
            SnpError::Config(config_err) => {
                self.add_config_context(&mut output, config_err.as_ref());
            }
            SnpError::Git(git_err) => {
                self.add_git_context(&mut output, git_err.as_ref());
            }
            SnpError::HookExecution(hook_err) => {
                self.add_hook_context(&mut output, hook_err.as_ref());
            }
            SnpError::Cli(cli_err) => {
                self.add_cli_context(&mut output, cli_err.as_ref());
            }
            SnpError::Storage(storage_err) => {
                self.add_storage_context(&mut output, storage_err.as_ref());
            }
            SnpError::Process(process_err) => {
                self.add_process_context(&mut output, process_err.as_ref());
            }
            SnpError::Lock(lock_err) => {
                self.add_lock_context(&mut output, lock_err.as_ref());
            }
            SnpError::Classification(classification_err) => {
                self.add_classification_context(&mut output, classification_err);
            }
            SnpError::Regex(regex_err) => {
                self.add_regex_context(&mut output, regex_err.as_ref());
            }
            _ => {}
        }

        output
    }

    fn add_config_context(&self, output: &mut String, error: &ConfigError) {
        match error {
            ConfigError::InvalidYaml {
                file_path: Some(path),
                line: Some(line),
                ..
            } => {
                output.push_str(&format!("\n  --> {}:{}", path.display(), line));
            }
            ConfigError::NotFound {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            _ => {}
        }
    }

    fn add_git_context(&self, output: &mut String, error: &GitError) {
        match error {
            GitError::CommandFailed { stderr, .. } if !stderr.is_empty() => {
                output.push_str(&format!("\n  Git error: {stderr}"));
            }
            GitError::PermissionDenied {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            GitError::RepositoryNotFound {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            GitError::InvalidReference {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            _ => {}
        }
    }

    fn add_hook_context(&self, output: &mut String, error: &HookExecutionError) {
        match error {
            HookExecutionError::CommandNotFound {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            HookExecutionError::NonZeroExit { stderr, .. } if !stderr.is_empty() => {
                output.push_str(&format!("\n  Hook output: {stderr}"));
            }
            _ => {}
        }
    }

    fn add_cli_context(&self, output: &mut String, error: &CliError) {
        match error {
            CliError::InvalidArgument {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            CliError::ConflictingArguments { suggestion, .. } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            _ => {}
        }
    }

    fn add_storage_context(&self, output: &mut String, error: &StorageError) {
        match error {
            StorageError::DatabaseCorrupted {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            StorageError::RepositoryNotFound {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            StorageError::EnvironmentNotFound {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            StorageError::FileLockFailed {
                timeout_secs: Some(timeout),
                ..
            } => {
                output.push_str(&format!("\n  Timeout: {timeout} seconds"));
            }
            StorageError::ConcurrencyConflict {
                retry_suggested: true,
                ..
            } => {
                output.push_str("\n  Help: Try running the command again");
            }
            _ => {}
        }
    }

    fn add_process_context(&self, output: &mut String, error: &ProcessError) {
        match error {
            ProcessError::CommandNotFound {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            ProcessError::Timeout { duration, .. } => {
                output.push_str(&format!("\n  Timeout: {duration:?}"));
            }
            ProcessError::ExecutionFailed { stderr, .. } if !stderr.is_empty() => {
                output.push_str(&format!("\n  Process error: {stderr}"));
            }
            ProcessError::ParallelExecutionFailed { errors, .. } => {
                for (i, error) in errors.iter().enumerate() {
                    output.push_str(&format!("\n    {}: {}", i + 1, error));
                }
            }
            _ => {}
        }
    }

    fn add_lock_context(&self, output: &mut String, error: &LockError) {
        match error {
            LockError::Timeout { timeout, .. } => {
                output.push_str(&format!("\n  Timeout: {timeout:?}"));
                output.push_str(
                    "\n  Help: Consider increasing the lock timeout or checking for deadlocks",
                );
            }
            LockError::HierarchyViolation {
                level,
                previous_level,
                ..
            } => {
                output.push_str(&format!(
                    "\n  Lock levels: attempting {level} after {previous_level}"
                ));
                output.push_str("\n  Help: Acquire locks in consistent hierarchical order");
            }
            LockError::StaleCleanupFailed { process_id, .. } => {
                output.push_str(&format!("\n  Stale process ID: {process_id}"));
                output.push_str(
                    "\n  Help: Try running with elevated privileges or restart the application",
                );
            }
            LockError::UnsupportedOperation {
                suggestion: Some(suggestion),
                ..
            } => {
                output.push_str(&format!("\n  Help: {suggestion}"));
            }
            LockError::PlatformSpecific {
                platform,
                operation,
                ..
            } => {
                output.push_str(&format!("\n  Platform: {platform} operation: {operation}"));
            }
            LockError::LockContention {
                waiting_processes,
                estimated_wait,
                ..
            } => {
                output.push_str(&format!("\n  Waiting processes: {waiting_processes}"));
                output.push_str(&format!("\n  Estimated wait: {estimated_wait:?}"));
            }
            _ => {}
        }
    }

    fn add_classification_context(
        &self,
        output: &mut String,
        error: &crate::classification::ClassificationError,
    ) {
        output.push('\n');
        match error {
            crate::classification::ClassificationError::InvalidPattern { pattern, .. } => {
                output.push_str(&format!("  Pattern: {pattern}"));
            }
            crate::classification::ClassificationError::FileAccessError { path, .. } => {
                output.push_str(&format!("  File: {}", path.display()));
            }
            crate::classification::ClassificationError::ContentAnalysisError {
                path,
                analyzer,
                ..
            } => {
                output.push_str(&format!("  File: {}", path.display()));
                output.push_str(&format!("\n  Analyzer: {analyzer}"));
            }
            crate::classification::ClassificationError::EncodingError { path, .. } => {
                output.push_str(&format!("  File: {}", path.display()));
            }
        }
    }

    fn add_regex_context(&self, output: &mut String, error: &crate::regex_processor::RegexError) {
        output.push('\n');
        match error {
            crate::regex_processor::RegexError::InvalidPattern {
                pattern,
                suggestion,
                ..
            } => {
                output.push_str(&format!("  Pattern: {pattern}"));
                if let Some(suggestion) = suggestion {
                    output.push_str(&format!("\n  Help: {suggestion}"));
                }
            }
            crate::regex_processor::RegexError::ComplexityExceeded {
                pattern,
                complexity_score,
                ..
            } => {
                output.push_str(&format!("  Pattern: {pattern}"));
                output.push_str(&format!("\n  Complexity score: {complexity_score:.2}"));
            }
            crate::regex_processor::RegexError::SecurityViolation {
                pattern,
                vulnerability_type,
                ..
            } => {
                output.push_str(&format!("  Pattern: {pattern}"));
                output.push_str(&format!("\n  Security issue: {vulnerability_type}"));
            }
            crate::regex_processor::RegexError::CompilationTimeout {
                pattern, duration, ..
            } => {
                output.push_str(&format!("  Pattern: {pattern}"));
                output.push_str(&format!("\n  Compilation time: {duration:?}"));
            }
            crate::regex_processor::RegexError::CacheOverflow {
                current_size,
                max_size,
                ..
            } => {
                output.push_str(&format!("  Cache size: {current_size}/{max_size}"));
            }
        }
    }
}

/// Exit codes matching pre-commit behavior
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const GENERAL_ERROR: i32 = 1;
    pub const CONFIG_ERROR: i32 = 2;
    pub const GIT_ERROR: i32 = 3;
    pub const HOOK_FAILURE: i32 = 4;
    pub const PERMISSION_ERROR: i32 = 5;
    pub const TIMEOUT_ERROR: i32 = 6;
    pub const CLI_ERROR: i32 = 7;
    pub const STORAGE_ERROR: i32 = 8;
    pub const PROCESS_ERROR: i32 = 9;
    pub const LOCK_ERROR: i32 = 10;
}

impl SnpError {
    /// Get the appropriate exit code for this error
    pub fn exit_code(&self) -> i32 {
        match self {
            SnpError::Config(_) => exit_codes::CONFIG_ERROR,
            SnpError::Git(git_err) => match git_err.as_ref() {
                GitError::PermissionDenied { .. } => exit_codes::PERMISSION_ERROR,
                GitError::Timeout { .. } => exit_codes::TIMEOUT_ERROR,
                _ => exit_codes::GIT_ERROR,
            },
            SnpError::HookExecution(hook_err) => match hook_err.as_ref() {
                HookExecutionError::PermissionDenied { .. } => exit_codes::PERMISSION_ERROR,
                HookExecutionError::ExecutionTimeout { .. } => exit_codes::TIMEOUT_ERROR,
                _ => exit_codes::HOOK_FAILURE,
            },
            SnpError::Cli(_) => exit_codes::CLI_ERROR,
            SnpError::Storage(storage_err) => match storage_err.as_ref() {
                StorageError::FileLockFailed { .. } => exit_codes::TIMEOUT_ERROR,
                StorageError::CacheDirectoryFailed { .. } => exit_codes::PERMISSION_ERROR,
                _ => exit_codes::STORAGE_ERROR,
            },
            SnpError::Process(process_err) => match process_err.as_ref() {
                ProcessError::Timeout { .. } => exit_codes::TIMEOUT_ERROR,
                _ => exit_codes::PROCESS_ERROR,
            },
            SnpError::Lock(lock_err) => match lock_err.as_ref() {
                LockError::Timeout { .. } => exit_codes::TIMEOUT_ERROR,
                LockError::HierarchyViolation { .. } => exit_codes::LOCK_ERROR,
                LockError::StaleCleanupFailed { .. } => exit_codes::PERMISSION_ERROR,
                _ => exit_codes::LOCK_ERROR,
            },
            SnpError::Classification(_) => exit_codes::GENERAL_ERROR,
            SnpError::Regex(_) => exit_codes::GENERAL_ERROR,
            SnpError::Io(_) => exit_codes::GENERAL_ERROR,
        }
    }

    /// Create a user-friendly error message with context
    pub fn user_message(&self, use_colors: bool) -> String {
        let formatter = ErrorFormatter::new(use_colors);
        formatter.format_error(self)
    }
}

// Conversion from serde_yaml::Error to ConfigError
impl From<serde_yaml::Error> for Box<ConfigError> {
    fn from(error: serde_yaml::Error) -> Self {
        let location = error.location();
        Box::new(ConfigError::InvalidYaml {
            message: error.to_string(),
            line: location.as_ref().map(|l| l.line() as u32),
            column: location.as_ref().map(|l| l.column() as u32),
            file_path: None,
        })
    }
}

// Conversion from git2::Error to GitError
impl From<git2::Error> for Box<GitError> {
    fn from(error: git2::Error) -> Self {
        Box::new(GitError::CommandFailed {
            command: "git2 operation".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: error.message().to_string(),
            working_dir: None,
        })
    }
}

// Direct conversion from git2::Error to SnpError
impl From<git2::Error> for SnpError {
    fn from(error: git2::Error) -> Self {
        SnpError::Git(Box::<GitError>::from(error))
    }
}

// Conversion from rusqlite::Error to StorageError
impl From<rusqlite::Error> for Box<StorageError> {
    fn from(error: rusqlite::Error) -> Self {
        match error {
            rusqlite::Error::SqliteFailure(sqlite_error, message) => {
                Box::new(StorageError::QueryFailed {
                    query: "SQLite operation".to_string(),
                    error: message.unwrap_or_else(|| format!("SQLite error: {sqlite_error:?}")),
                    database_path: None,
                })
            }
            rusqlite::Error::InvalidPath(path) => Box::new(StorageError::ConnectionFailed {
                message: format!("Invalid database path: {}", path.display()),
                database_path: Some(path),
            }),
            _ => Box::new(StorageError::QueryFailed {
                query: "Database operation".to_string(),
                error: error.to_string(),
                database_path: None,
            }),
        }
    }
}

// Direct conversion from rusqlite::Error to SnpError
impl From<rusqlite::Error> for SnpError {
    fn from(error: rusqlite::Error) -> Self {
        SnpError::Storage(Box::<StorageError>::from(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display() {
        let error = SnpError::Config(Box::new(ConfigError::ValidationFailed {
            message: "test error".to_string(),
            file_path: None,
            errors: vec![],
        }));
        assert_eq!(
            error.to_string(),
            "Configuration error: Configuration validation failed: test error"
        );
    }

    #[test]
    fn test_io_error_conversion() {
        let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let snp_error = SnpError::from(io_error);
        assert!(snp_error.to_string().contains("IO operation failed"));
    }
}
