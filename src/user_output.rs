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
    pub gray: &'static str,
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
                gray: "\x1b[90m", // Bright black/gray
                reset: "\x1b[0m",
            }
        } else {
            Self {
                green: "",
                red: "",
                yellow: "",
                blue: "",
                gray: "",
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

    /// Show hook execution start with file count information
    pub fn show_hook_start_with_files(&self, hook_id: &str, file_count: usize) {
        if self.config.quiet {
            return;
        }

        let formatted_name = if file_count == 0 {
            self.format_hook_name_with_status(hook_id, "(no files to check)")
        } else {
            self.format_hook_name(hook_id)
        };
        print!("{formatted_name}");
        io::stdout().flush().unwrap_or(());
    }

    /// Show hook execution result
    pub fn show_hook_result(&self, result: &HookExecutionResult) {
        if self.config.quiet {
            return;
        }

        if result.skipped {
            // Just show "Skipped" since the reason was already shown in the hook start line
            println!("{}Skipped{}", self.colors.yellow, self.colors.reset);
        } else if result.success {
            println!("{}Passed{}", self.colors.green, self.colors.reset);
        } else {
            println!("{}Failed{}", self.colors.red, self.colors.reset);

            // Show failure details in gray
            if let Some(exit_code) = result.exit_code {
                println!(
                    "{}- hook id: {}{}",
                    self.colors.gray, result.hook_id, self.colors.reset
                );
                println!(
                    "{}- exit code: {}{}",
                    self.colors.gray, exit_code, self.colors.reset
                );
            }

            // Show stderr if present with smart coloring
            if !result.stderr.is_empty() {
                println!();
                self.show_colorized_stderr(&result.stderr);
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
        const MAX_WIDTH: usize = 79;

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

    /// Format hook name with status like "(no files to check)"
    fn format_hook_name_with_status(&self, hook_id: &str, status: &str) -> String {
        const MAX_WIDTH: usize = 79;

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

        let total_content_len = formatted_name.len() + status.len();
        if total_content_len >= MAX_WIDTH {
            format!("{formatted_name}{status}")
        } else {
            let dots_count = MAX_WIDTH - total_content_len;
            format!("{formatted_name}{}{status}", ".".repeat(dots_count))
        }
    }

    /// Show stderr with smart coloring based on content patterns
    fn show_colorized_stderr(&self, stderr: &str) {
        for line in stderr.lines() {
            if line.trim().is_empty() {
                println!();
                continue;
            }

            // Filter out Cargo blocking messages
            if line.contains("Blocking waiting for file lock on package cache") {
                continue;
            }

            // Check for different error patterns and apply appropriate colors
            if line.contains("error[") || line.contains("error:") {
                // Rust compiler errors
                self.colorize_rust_error_line(line);
            } else if line.contains("warning:") {
                // Warnings
                self.colorize_warning_line(line);
            } else if line.trim().starts_with("Checking ") {
                // Cargo checking messages
                self.colorize_checking_line(line);
            } else if line.contains(" --> ") {
                // File location indicators with blue arrows
                self.colorize_file_location_line(line);
            } else if line.trim_start().starts_with('|')
                || (line
                    .chars()
                    .take_while(|c| c.is_whitespace() || c.is_ascii_digit())
                    .count()
                    > 0
                    && line.contains(" |"))
            {
                // Line number indicators with blue pipes (checked before caret patterns)
                self.colorize_line_number_line(line);
            } else if line.trim().starts_with('^')
                || line.contains("^^^^^")
                || (line
                    .trim_start()
                    .chars()
                    .all(|c| c == '^' || c == '~' || c.is_whitespace())
                    && line.contains('^'))
            {
                // Error indicators (^^^^^ or ~~~~) - lines that are mostly just these characters or start with ^
                self.colorize_error_indicator_line(line);
            } else {
                // Default text
                println!("{line}");
            }
        }
    }

    /// Colorize Rust compiler error lines
    fn colorize_rust_error_line(&self, line: &str) {
        if let Some(error_pos) = line.find("error[") {
            // Split around the error code
            let before = &line[..error_pos];
            let after = &line[error_pos..];

            if let Some(colon_pos) = after.find(':') {
                let error_part = &after[..colon_pos]; // "error[E0433]"
                let colon_and_message = &after[colon_pos..]; // ":message"
                print!("{before}");
                print!("{}{error_part}{}", self.colors.red, self.colors.reset);
                println!("{colon_and_message}"); // Colon and message in default color
            } else {
                println!("{}{line}{}", self.colors.red, self.colors.reset);
            }
        } else if line.contains("error:") {
            // Simple "error:" prefix
            if let Some(error_pos) = line.find("error:") {
                let before = &line[..error_pos];
                let after = &line[error_pos..];
                print!("{before}");
                print!("{}error{}", self.colors.red, self.colors.reset);
                println!("{}", &after[5..]); // Skip "error" but keep colon in default color
            } else {
                println!("{line}");
            }
        } else {
            println!("{line}");
        }
    }

    /// Colorize warning lines
    fn colorize_warning_line(&self, line: &str) {
        if let Some(warning_pos) = line.find("warning:") {
            let before = &line[..warning_pos];
            let after = &line[warning_pos..];
            print!("{before}");
            print!("{}warning{}", self.colors.yellow, self.colors.reset);
            println!("{}", &after[7..]); // Skip "warning" but keep colon in default color
        } else {
            println!("{line}");
        }
    }

    /// Colorize file location lines with blue arrows
    fn colorize_file_location_line(&self, line: &str) {
        if let Some(arrow_pos) = line.find(" --> ") {
            let before = &line[..arrow_pos];
            let after = &line[arrow_pos + 5..]; // Skip " --> "
            print!("{before}");
            print!("{} --> {}", self.colors.blue, self.colors.reset);
            println!("{after}");
        } else {
            println!("{line}");
        }
    }

    /// Colorize line number lines with blue pipes, handling mixed caret patterns
    fn colorize_line_number_line(&self, line: &str) {
        if let Some(pipe_pos) = line.find(" |") {
            let before = &line[..pipe_pos];
            let after = &line[pipe_pos + 2..]; // Skip " |"
            print!("{before}");
            print!("{} |{}", self.colors.blue, self.colors.reset);

            // Check if the remainder contains caret patterns that need red coloring
            if after.contains("^^^^^") || after.contains("^^^^") {
                self.colorize_caret_part(after);
            } else {
                println!("{after}");
            }
        } else if line.trim_start().starts_with('|') {
            // Handle lines that start with just a pipe
            let trimmed = line.trim_start();
            let leading_spaces = &line[..line.len() - trimmed.len()];
            let after = &trimmed[1..]; // Skip the "|"
            print!("{leading_spaces}");
            print!("{}|{}", self.colors.blue, self.colors.reset);

            // Check if the remainder contains caret patterns that need red coloring
            if after.contains("^^^^^") || after.contains("^^^^") {
                self.colorize_caret_part(after);
            } else {
                println!("{after}");
            }
        } else {
            println!("{line}");
        }
    }

    /// Helper method to colorize caret patterns within a line segment
    fn colorize_caret_part(&self, text: &str) {
        // Color the entire caret part (carets + error message) in red
        println!("{}{}{}", self.colors.red, text, self.colors.reset);
    }

    /// Colorize error indicator lines (lines with ^^^^^ patterns)
    fn colorize_error_indicator_line(&self, line: &str) {
        // For pure caret lines, color everything red (carets + message)
        println!("{}{}{}", self.colors.red, line, self.colors.reset);
    }

    /// Colorize Cargo checking messages
    fn colorize_checking_line(&self, line: &str) {
        if line.trim_start().starts_with("Checking ") {
            let trimmed = line.trim_start();
            let leading_spaces = &line[..line.len() - trimmed.len()];
            let after_checking = &trimmed[9..]; // Skip "Checking "
            print!("{leading_spaces}");
            print!("{}Checking{} ", self.colors.green, self.colors.reset);
            println!("{after_checking}");
        } else {
            println!("{line}");
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
        assert_eq!(formatted.len(), 79);
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
        assert_eq!(colors.gray, "");
        assert_eq!(colors.reset, "");
    }

    #[test]
    fn test_colors_enabled() {
        let colors = Colors::new(true);
        assert_eq!(colors.green, "\x1b[32m");
        assert_eq!(colors.red, "\x1b[31m");
        assert_eq!(colors.gray, "\x1b[90m");
        assert_eq!(colors.reset, "\x1b[0m");
    }

    #[test]
    fn test_user_output_config() {
        let config = UserOutputConfig::new(true, false, Some("always".to_string()));
        assert!(config.verbose);
        assert!(!config.quiet);
        assert!(config.use_colors);
    }

    #[test]
    fn test_colorized_stderr() {
        let config = UserOutputConfig::new(false, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test Rust error coloring
        let rust_error = r#"error[E0433]: failed to resolve: could not find `Win32` in `windows`
 --> src/window_api/win.rs:8:5
  |
8 |     Win32::{
  |     ^^^^^ could not find `Win32` in `windows`

error: could not compile `whatawhat` (lib) due to 3 previous errors"#;

        // This test mainly checks that the methods don't panic
        // In a real scenario, you'd capture the output to verify colors
        output.show_colorized_stderr(rust_error);
    }
}
