// User-friendly output system for SNP
// Provides clean, readable output similar to Python pre-commit

use crate::execution::{ExecutionResult, HookExecutionResult};
use std::io::{self, IsTerminal, Write};

/// Simple output configuration for user-facing display
#[derive(Debug, Clone)]
pub struct UserOutputConfig {
    pub verbose: bool,
    pub quiet: bool,
    pub use_colors: bool,
}

impl UserOutputConfig {
    pub fn new(verbose: bool, quiet: bool, color: Option<String>) -> Self {
        let use_colors = match color.as_deref() {
            Some("always") => true,
            Some("never") => false,
            Some("auto") | None => {
                io::stderr().is_terminal()
                    && std::env::var("TERM").map_or(true, |term| term != "dumb")
                    && std::env::var("NO_COLOR").is_err()
            }
            _ => false,
        };

        Self {
            verbose,
            quiet,
            use_colors,
        }
    }
}

/// Simple color constants
#[derive(Clone, Debug)]
pub struct Colors {
    pub green: &'static str,
    pub red: &'static str,
    pub yellow: &'static str,
    pub blue: &'static str,
    pub reset: &'static str,
}

impl Colors {
    pub fn new(use_colors: bool) -> Self {
        if use_colors {
            Self {
                green: "\x1b[32m",
                red: "\x1b[31m",
                yellow: "\x1b[33m",
                blue: "\x1b[34m",
                reset: "\x1b[0m",
            }
        } else {
            Self {
                green: "",
                red: "",
                yellow: "",
                blue: "",
                reset: "",
            }
        }
    }
}

/// Simple user-friendly output formatter
#[derive(Clone, Debug)]
pub struct UserOutput {
    config: UserOutputConfig,
    colors: Colors,
}

impl UserOutput {
    pub fn new(config: UserOutputConfig) -> Self {
        let colors = Colors::new(config.use_colors);
        Self { config, colors }
    }

    /// Show hook execution start with dotted progress line
    pub fn show_hook_start(&self, hook_id: &str) {
        if self.config.quiet {
            return;
        }

        let formatted_name = self.format_hook_name(hook_id);
        print!("{formatted_name}");
        io::stdout().flush().unwrap_or(());
    }

    /// Show hook execution result
    pub fn show_hook_result(&self, result: &HookExecutionResult) {
        if self.config.quiet {
            return;
        }

        if result.success {
            println!("{}Passed{}", self.colors.green, self.colors.reset);
        } else {
            println!("{}Failed{}", self.colors.red, self.colors.reset);

            // Show failure details
            if let Some(exit_code) = result.exit_code {
                println!("- hook id: {}", result.hook_id);
                println!("- exit code: {exit_code}");
            }

            // Show stderr if present
            if !result.stderr.is_empty() {
                println!();
                print!("{}", result.stderr);
                if !result.stderr.ends_with('\n') {
                    println!();
                }
            }
        }

        // Show verbose details if requested
        if self.config.verbose {
            self.show_verbose_details(result);
        }
    }

    /// Show verbose execution details
    fn show_verbose_details(&self, result: &HookExecutionResult) {
        println!(
            "  {}Duration: {:.2}s{}",
            self.colors.blue,
            result.duration.as_secs_f64(),
            self.colors.reset
        );

        if !result.files_processed.is_empty() {
            println!(
                "  {}Files processed: {}{}",
                self.colors.blue,
                result.files_processed.len(),
                self.colors.reset
            );
        }

        if !result.stdout.is_empty() {
            println!("  {}--- stdout ---{}", self.colors.blue, self.colors.reset);
            for line in result.stdout.lines() {
                println!("  {line}");
            }
        }

        if !result.stderr.is_empty() && self.config.verbose {
            println!(
                "  {}--- stderr ---{}",
                self.colors.yellow, self.colors.reset
            );
            for line in result.stderr.lines() {
                println!("  {line}");
            }
        }
    }

    /// Show final execution summary
    pub fn show_execution_summary(&self, result: &ExecutionResult) {
        if self.config.quiet {
            return;
        }

        println!();

        if result.success {
            if self.config.verbose {
                println!(
                    "{}✓ All hooks passed!{} Executed {} hooks in {:.2}s",
                    self.colors.green,
                    self.colors.reset,
                    result.hooks_executed,
                    result.total_duration.as_secs_f64()
                );
            }
        } else {
            println!(
                "{}✗ Some hooks failed!{}",
                self.colors.red, self.colors.reset,
            );

            if self.config.verbose {
                println!(
                    "Executed: {}, Passed: {}, Failed: {}",
                    result.hooks_executed,
                    result.hooks_passed.len(),
                    result.hooks_failed.len()
                );
            }
        }

        if !result.hooks_skipped.is_empty() && self.config.verbose {
            println!(
                "{}Skipped: {}{} ({})",
                self.colors.yellow,
                result.hooks_skipped.len(),
                self.colors.reset,
                result.hooks_skipped.join(", ")
            );
        }
    }

    /// Format hook name with dots like Python pre-commit
    fn format_hook_name(&self, hook_id: &str) -> String {
        const MAX_WIDTH: usize = 68;

        // Convert hook ID to a friendly name
        let name = hook_id.replace(['-', '_'], " ");

        // Capitalize first letter of each word
        let formatted_name: String = name
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    None => String::new(),
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                }
            })
            .collect::<Vec<_>>()
            .join(" ");

        if formatted_name.len() >= MAX_WIDTH {
            formatted_name
        } else {
            let dots_count = MAX_WIDTH - formatted_name.len();
            format!("{}{}", formatted_name, ".".repeat(dots_count))
        }
    }

    /// Show simple status message
    pub fn show_status(&self, message: &str) {
        if !self.config.quiet {
            println!("{message}");
        }
    }

    /// Show error message
    pub fn show_error(&self, message: &str) {
        if !self.config.quiet {
            eprintln!("{}Error: {}{}", self.colors.red, message, self.colors.reset);
        }
    }

    /// Show warning message
    pub fn show_warning(&self, message: &str) {
        if !self.config.quiet {
            eprintln!(
                "{}Warning: {}{}",
                self.colors.yellow, message, self.colors.reset
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_hook_name() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let formatted = output.format_hook_name("trailing-whitespace");
        assert!(formatted.starts_with("Trailing Whitespace"));
        assert!(formatted.contains("."));
        assert_eq!(formatted.len(), 68);
    }

    #[test]
    fn test_format_hook_name_long() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let long_name =
            "this-is-a-very-long-hook-name-that-exceeds-maximum-width-and-should-not-have-dots";
        let formatted = output.format_hook_name(long_name);
        assert!(!formatted.contains("."));
    }

    #[test]
    fn test_colors_disabled() {
        let colors = Colors::new(false);
        assert_eq!(colors.green, "");
        assert_eq!(colors.red, "");
        assert_eq!(colors.reset, "");
    }

    #[test]
    fn test_colors_enabled() {
        let colors = Colors::new(true);
        assert_eq!(colors.green, "\x1b[32m");
        assert_eq!(colors.red, "\x1b[31m");
        assert_eq!(colors.reset, "\x1b[0m");
    }

    #[test]
    fn test_user_output_config() {
        let config = UserOutputConfig::new(true, false, Some("always".to_string()));
        assert!(config.verbose);
        assert!(!config.quiet);
        assert!(config.use_colors);
    }
}
