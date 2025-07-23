// Comprehensive logging system for SNP
use std::io::{self, IsTerminal};
use tracing::Level;
use tracing_subscriber::{fmt, EnvFilter};

use crate::error::Result;

/// Logging configuration
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// Log level (trace, debug, info, warn, error)
    pub level: Level,
    /// Output format (pretty for terminals, json for programmatic use)
    pub format: LogFormat,
    /// Color output configuration
    pub color: ColorConfig,
    /// Whether to show targets (module names)
    pub show_targets: bool,
    /// Whether to show timestamps
    pub show_timestamps: bool,
}

/// Log output format options
#[derive(Debug, Clone, PartialEq)]
pub enum LogFormat {
    /// Pretty output for terminals (similar to pre-commit format)
    Pretty,
    /// JSON output for programmatic use
    Json,
    /// Compact format for structured logging
    Compact,
}

/// Color output configuration
#[derive(Debug, Clone, PartialEq)]
pub enum ColorConfig {
    /// Automatically detect if colors should be used
    Auto,
    /// Always use colors
    Always,
    /// Never use colors
    Never,
}

impl Default for LogConfig {
    fn default() -> Self {
        Self {
            level: Level::INFO,
            format: LogFormat::Pretty,
            color: ColorConfig::Auto,
            show_targets: false,
            show_timestamps: false,
        }
    }
}

impl LogConfig {
    /// Create logging configuration from CLI arguments
    pub fn from_cli(verbose: bool, quiet: bool, color: Option<String>) -> Self {
        let level = if quiet {
            Level::ERROR
        } else if verbose {
            Level::DEBUG
        } else {
            Level::INFO
        };

        let color_config = match color.as_deref() {
            Some("always") => ColorConfig::Always,
            Some("never") => ColorConfig::Never,
            Some("auto") | None => ColorConfig::Auto,
            _ => ColorConfig::Auto,
        };

        Self {
            level,
            format: LogFormat::Pretty,
            color: color_config,
            show_targets: false,
            show_timestamps: false,
        }
    }

    /// Check if colors should be used based on configuration and terminal
    pub fn should_use_colors(&self) -> bool {
        match self.color {
            ColorConfig::Always => true,
            ColorConfig::Never => false,
            ColorConfig::Auto => {
                io::stderr().is_terminal()
                    && std::env::var("TERM").map_or(true, |term| term != "dumb")
                    && std::env::var("NO_COLOR").is_err()
            }
        }
    }
}

/// Initialize the logging system with the given configuration
pub fn init_logging(config: LogConfig) -> Result<()> {
    let env_filter =
        EnvFilter::new(format!("snp={}", config.level)).add_directive("snp=trace".parse().unwrap());

    match config.format {
        LogFormat::Pretty => init_pretty_logging(config, env_filter),
        LogFormat::Json => init_json_logging(config, env_filter),
        LogFormat::Compact => init_compact_logging(config, env_filter),
    }?;

    Ok(())
}

/// Initialize pretty logging (similar to pre-commit format)
fn init_pretty_logging(config: LogConfig, env_filter: EnvFilter) -> Result<()> {
    fmt()
        .with_env_filter(env_filter)
        .with_target(config.show_targets)
        .init();

    Ok(())
}

/// Initialize JSON logging for programmatic use
fn init_json_logging(_config: LogConfig, env_filter: EnvFilter) -> Result<()> {
    fmt().with_env_filter(env_filter).json().init();

    Ok(())
}

/// Initialize compact logging
fn init_compact_logging(config: LogConfig, env_filter: EnvFilter) -> Result<()> {
    fmt()
        .with_env_filter(env_filter)
        .compact()
        .with_target(config.show_targets)
        .init();

    Ok(())
}

/// Logging utilities for common operations
pub mod utils {
    use std::path::Path;
    use tracing::{debug, error, info, span, warn, Level, Span};

    /// Create a span for hook execution
    pub fn hook_execution_span(hook_id: &str, file_count: usize) -> Span {
        span!(Level::INFO, "hook_execution", hook_id = %hook_id, file_count = file_count)
    }

    /// Create a span for configuration loading
    pub fn config_loading_span(config_path: &Path) -> Span {
        span!(Level::DEBUG, "config_loading", path = %config_path.display())
    }

    /// Create a span for git operations
    pub fn git_operation_span(operation: &str, repository: Option<&str>) -> Span {
        span!(
            Level::DEBUG,
            "git_operation",
            operation = %operation,
            repository = repository
        )
    }

    /// Log hook execution start
    pub fn log_hook_start(hook_id: &str, files: &[std::path::PathBuf]) {
        info!(
            hook_id = %hook_id,
            file_count = files.len(),
            "Starting hook execution"
        );
    }

    /// Log hook execution completion
    pub fn log_hook_completion(hook_id: &str, success: bool, duration_ms: u128) {
        if success {
            info!(
                hook_id = %hook_id,
                duration_ms = duration_ms,
                "Hook execution completed successfully"
            );
        } else {
            error!(
                hook_id = %hook_id,
                duration_ms = duration_ms,
                "Hook execution failed"
            );
        }
    }

    /// Log configuration validation
    pub fn log_config_validation(config_path: &Path, valid: bool) {
        if valid {
            debug!(path = %config_path.display(), "Configuration validation passed");
        } else {
            error!(path = %config_path.display(), "Configuration validation failed");
        }
    }

    /// Log warning with context
    pub fn log_warning_with_context(message: &str, context: &str) {
        warn!(context = %context, "{}", message);
    }

    /// Log repository operation
    pub fn log_repository_operation(operation: &str, repo_url: &str, success: bool) {
        if success {
            info!(
                operation = %operation,
                repository = %repo_url,
                "Repository operation completed"
            );
        } else {
            error!(
                operation = %operation,
                repository = %repo_url,
                "Repository operation failed"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_config_default() {
        let config = LogConfig::default();
        assert_eq!(config.level, Level::INFO);
        assert_eq!(config.format, LogFormat::Pretty);
        assert_eq!(config.color, ColorConfig::Auto);
        assert!(!config.show_targets);
        assert!(!config.show_timestamps);
    }

    #[test]
    fn test_log_config_from_cli_verbose() {
        let config = LogConfig::from_cli(true, false, None);
        assert_eq!(config.level, Level::DEBUG);
        assert_eq!(config.color, ColorConfig::Auto);
    }

    #[test]
    fn test_log_config_from_cli_quiet() {
        let config = LogConfig::from_cli(false, true, None);
        assert_eq!(config.level, Level::ERROR);
    }

    #[test]
    fn test_log_config_color_always() {
        let config = LogConfig::from_cli(false, false, Some("always".to_string()));
        assert_eq!(config.color, ColorConfig::Always);
        assert!(config.should_use_colors());
    }

    #[test]
    fn test_log_config_color_never() {
        let config = LogConfig::from_cli(false, false, Some("never".to_string()));
        assert_eq!(config.color, ColorConfig::Never);
        assert!(!config.should_use_colors());
    }
}
