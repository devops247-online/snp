// SNP (Shell Not Pass) - Library module
// This file contains the core library functionality

pub mod cli;
pub mod config;
pub mod core;
pub mod error;
pub mod filesystem;
pub mod git;
pub mod logging;
pub mod process;
// pub mod storage;

// Re-export main types for easier access
pub use config::Config;
pub use core::{ExecutionContext, Hook, Repository, Stage};
pub use error::{
    exit_codes, CliError, ConfigError, GitError, HookExecutionError, ProcessError, Result,
    SnpError, StorageError,
};
pub use filesystem::{FileFilter, FileSystem};
pub use git::GitRepository;
pub use logging::{ColorConfig, LogConfig, LogFormat};
pub use process::{
    OutputHandler, ProcessConfig, ProcessEnvironment, ProcessManager, ProcessResult,
};
// pub use storage::{ConfigInfo, EnvironmentInfo, RepositoryInfo, Store};

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
