// Comprehensive tests for the user_output module
// Tests user-facing output formatting, colors, verbosity levels, and terminal handling

use snp::execution::{ExecutionResult, HookExecutionResult};
use snp::user_output::{Colors, UserOutput, UserOutputConfig};
use std::time::Duration;

#[cfg(test)]
#[allow(clippy::uninlined_format_args)]
mod user_output_tests {
    use super::*;

    #[test]
    fn test_user_output_config_creation() {
        // Test with explicit "always" color
        let config = UserOutputConfig::new(true, false, Some("always".to_string()));
        assert!(config.verbose);
        assert!(!config.quiet);
        assert!(config.use_colors);

        // Test with explicit "never" color
        let config = UserOutputConfig::new(false, true, Some("never".to_string()));
        assert!(!config.verbose);
        assert!(config.quiet);
        assert!(!config.use_colors);

        // Test with "auto" color (should depend on terminal detection)
        let config = UserOutputConfig::new(false, false, Some("auto".to_string()));
        assert!(!config.verbose);
        assert!(!config.quiet);
        // use_colors will depend on terminal detection and environment variables

        // Test with None (should use auto detection)
        let config = UserOutputConfig::new(true, false, None);
        assert!(config.verbose);
        assert!(!config.quiet);
        // use_colors will depend on terminal detection

        // Test with invalid color option (should default to false)
        let config = UserOutputConfig::new(false, false, Some("invalid".to_string()));
        assert!(!config.verbose);
        assert!(!config.quiet);
        assert!(!config.use_colors);
    }

    #[test]
    fn test_color_detection_with_environment_variables() {
        // Save original environment
        let original_term = std::env::var("TERM").ok();
        let original_no_color = std::env::var("NO_COLOR").ok();

        // Test with dumb terminal
        std::env::set_var("TERM", "dumb");
        std::env::remove_var("NO_COLOR");
        let config = UserOutputConfig::new(false, false, Some("auto".to_string()));
        // Should be false due to dumb terminal
        assert!(!config.use_colors);

        // Test with NO_COLOR set
        std::env::set_var("TERM", "xterm-256color");
        std::env::set_var("NO_COLOR", "1");
        let config = UserOutputConfig::new(false, false, Some("auto".to_string()));
        // Should be false due to NO_COLOR
        assert!(!config.use_colors);

        // Test with smart terminal and no NO_COLOR
        std::env::set_var("TERM", "xterm-256color");
        std::env::remove_var("NO_COLOR");
        let _config = UserOutputConfig::new(false, false, Some("auto".to_string()));
        // Result depends on whether we're actually in a terminal

        // Restore original environment
        match original_term {
            Some(term) => std::env::set_var("TERM", term),
            None => std::env::remove_var("TERM"),
        }
        match original_no_color {
            Some(no_color) => std::env::set_var("NO_COLOR", no_color),
            None => std::env::remove_var("NO_COLOR"),
        }
    }

    #[test]
    fn test_colors_structure() {
        // Test with colors enabled
        let colors = Colors::new(true);
        assert_eq!(colors.green, "\x1b[32m");
        assert_eq!(colors.red, "\x1b[31m");
        assert_eq!(colors.yellow, "\x1b[33m");
        assert_eq!(colors.blue, "\x1b[34m");
        assert_eq!(colors.gray, "\x1b[90m");
        assert_eq!(colors.reset, "\x1b[0m");

        // Test with colors disabled
        let colors = Colors::new(false);
        assert_eq!(colors.green, "");
        assert_eq!(colors.red, "");
        assert_eq!(colors.yellow, "");
        assert_eq!(colors.blue, "");
        assert_eq!(colors.gray, "");
        assert_eq!(colors.reset, "");
    }

    // Note: format_hook_name and format_hook_name_with_status are private methods
    // and are tested indirectly through the public show_hook_start methods

    #[test]
    fn test_hook_execution_result_display() {
        let config = UserOutputConfig::new(false, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test successful hook result
        let success_result = HookExecutionResult {
            hook_id: "test-hook".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_secs(1),
            files_processed: vec!["file1.py".into(), "file2.py".into()],
            files_modified: vec![],
            error: None,
        };

        // This mainly tests that the method doesn't panic
        output.show_hook_result(&success_result);

        // Test failed hook result
        let failed_result = HookExecutionResult {
            hook_id: "failing-hook".to_string(),
            success: false,
            skipped: false,
            skip_reason: None,
            exit_code: Some(1),
            stdout: "Some output".to_string(),
            stderr: "Error message".to_string(),
            duration: Duration::from_millis(500),
            files_processed: vec!["file1.py".into()],
            files_modified: vec![],
            error: None,
        };

        output.show_hook_result(&failed_result);

        // Test skipped hook result
        let skipped_result = HookExecutionResult {
            hook_id: "skipped-hook".to_string(),
            success: true,
            skipped: true,
            skip_reason: Some("No files to check".to_string()),
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_millis(0),
            files_processed: vec![],
            files_modified: vec![],
            error: None,
        };

        output.show_hook_result(&skipped_result);
    }

    #[test]
    fn test_verbose_output() {
        let config = UserOutputConfig::new(true, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test verbose hook result display
        let result = HookExecutionResult {
            hook_id: "verbose-test".to_string(),
            success: false,
            skipped: false,
            skip_reason: None,
            exit_code: Some(2),
            stdout: "Standard output\nMultiple lines".to_string(),
            stderr: "Error output\nWith details".to_string(),
            duration: Duration::from_secs_f64(2.5),
            files_processed: vec!["file1.py".into(), "file2.py".into(), "file3.py".into()],
            files_modified: vec![],
            error: None,
        };

        // Should show additional details in verbose mode
        output.show_hook_result(&result);
    }

    #[test]
    fn test_quiet_mode() {
        let config = UserOutputConfig::new(false, true, Some("never".to_string()));
        let output = UserOutput::new(config);

        // In quiet mode, these should not output anything (but shouldn't panic)
        output.show_hook_start("test-hook");
        output.show_hook_start_with_files("test-hook", 5);
        output.show_status("Some status message");

        let result = HookExecutionResult {
            hook_id: "quiet-test".to_string(),
            success: false,
            skipped: false,
            skip_reason: None,
            exit_code: Some(1),
            stdout: "Output".to_string(),
            stderr: "Error".to_string(),
            duration: Duration::from_secs(1),
            files_processed: vec!["file.py".into()],
            files_modified: vec![],
            error: None,
        };

        output.show_hook_result(&result);

        let exec_result = ExecutionResult {
            success: false,
            hooks_executed: 3,
            hooks_passed: vec![],
            hooks_failed: vec![],
            hooks_skipped: vec!["hook4".to_string()],
            total_duration: Duration::from_secs(5),
            files_modified: vec![],
        };

        output.show_execution_summary(&exec_result);
    }

    #[test]
    fn test_execution_summary() {
        let config = UserOutputConfig::new(true, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test successful execution summary
        let success_result = ExecutionResult {
            success: true,
            hooks_executed: 5,
            hooks_passed: vec![],
            hooks_failed: vec![],
            hooks_skipped: vec!["hook6".to_string()],
            total_duration: Duration::from_secs_f64(3.5),
            files_modified: vec![],
        };

        output.show_execution_summary(&success_result);

        // Test failed execution summary
        let failed_result = ExecutionResult {
            success: false,
            hooks_executed: 4,
            hooks_passed: vec![],
            hooks_failed: vec![],
            hooks_skipped: vec!["hook5".to_string(), "hook6".to_string()],
            total_duration: Duration::from_secs(2),
            files_modified: vec![],
        };

        output.show_execution_summary(&failed_result);

        // Test execution with no skipped hooks
        let no_skipped_result = ExecutionResult {
            success: true,
            hooks_executed: 3,
            hooks_passed: vec![],
            hooks_failed: vec![],
            hooks_skipped: vec![],
            total_duration: Duration::from_secs(1),
            files_modified: vec![],
        };

        output.show_execution_summary(&no_skipped_result);
    }

    // Note: show_colorized_stderr is a private method and is tested indirectly
    // through the public show_hook_result method when stderr is present

    #[test]
    fn test_status_and_message_display() {
        let config = UserOutputConfig::new(false, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test status messages
        output.show_status("Installing dependencies...");
        output.show_status("Running hooks...");
        output.show_status("Cleaning up...");

        // Test error messages
        output.show_error("Failed to parse configuration file");
        output.show_error("Hook execution failed");

        // Test warning messages
        output.show_warning("Deprecated configuration option");
        output.show_warning("Performance optimization available");
    }

    #[test]
    fn test_hook_start_display() {
        let config = UserOutputConfig::new(false, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test basic hook start
        output.show_hook_start("trailing-whitespace");
        output.show_hook_start("check-yaml");

        // Test hook start with file counts
        output.show_hook_start_with_files("pylint", 5);
        output.show_hook_start_with_files("black", 0);
        output.show_hook_start_with_files("mypy", 25);
    }

    #[test]
    fn test_edge_cases() {
        let config = UserOutputConfig::new(false, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test with empty strings
        output.show_hook_start("");
        output.show_status("");
        output.show_error("");
        output.show_warning("");

        // Test with very long messages
        let long_message = "a".repeat(1000);
        output.show_status(&long_message);
        output.show_error(&long_message);

        // Test with special characters
        output.show_status("Message with 🌟 emoji and special chars: åäö");
        output.show_hook_start("hook-with-émojis-and-spëcial-chars");

        // Test with empty strings (tested through hook results with empty stderr)
        let empty_result = HookExecutionResult {
            hook_id: "empty-test".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::from_millis(10),
            files_processed: vec![],
            files_modified: vec![],
            error: None,
        };
        output.show_hook_result(&empty_result);
    }

    #[test]
    fn test_duration_formatting() {
        let config = UserOutputConfig::new(true, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test various duration formats
        let durations = [
            Duration::from_millis(500),
            Duration::from_secs(1),
            Duration::from_secs_f64(2.5),
            Duration::from_secs(60),
            Duration::from_secs(125),
        ];

        for duration in &durations {
            let result = HookExecutionResult {
                hook_id: "duration-test".to_string(),
                success: true,
                skipped: false,
                skip_reason: None,
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                duration: *duration,
                files_processed: vec![],
                files_modified: vec![],
                error: None,
            };

            output.show_hook_result(&result);
        }
    }

    #[test]
    fn test_concurrent_output() {
        use std::sync::Arc;
        use std::thread;

        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = Arc::new(UserOutput::new(config));

        // Test that multiple threads can use the output safely
        let handles: Vec<_> = (0..5)
            .map(|i| {
                let output = Arc::clone(&output);
                thread::spawn(move || {
                    output.show_status(&format!("Thread {} message", i));
                    output.show_hook_start(&format!("thread-{}-hook", i));

                    let result = HookExecutionResult {
                        hook_id: format!("thread-{}-hook", i),
                        success: i % 2 == 0,
                        skipped: false,
                        skip_reason: None,
                        exit_code: Some(if i % 2 == 0 { 0 } else { 1 }),
                        stdout: format!("Output from thread {}", i),
                        stderr: if i % 2 == 0 {
                            String::new()
                        } else {
                            format!("Error from thread {}", i)
                        },
                        duration: Duration::from_millis(100 * i as u64),
                        files_processed: vec![format!("file{}.py", i).into()],
                        files_modified: vec![],
                        error: None,
                    };

                    output.show_hook_result(&result);
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }

    #[test]
    fn test_memory_efficiency() {
        let config = UserOutputConfig::new(true, false, Some("always".to_string()));
        let output = UserOutput::new(config);

        // Test with large stderr content (tested indirectly through hook results)
        // Note: show_colorized_stderr is private, so we test it through public methods

        // Test with large stdout content
        let large_result = HookExecutionResult {
            hook_id: "large-output-test".to_string(),
            success: false,
            skipped: false,
            skip_reason: None,
            exit_code: Some(1),
            stdout: "output line\n".repeat(5000),
            stderr: "error line\n".repeat(5000),
            duration: Duration::from_secs(1),
            files_processed: (0..1000).map(|i| format!("file{}.py", i).into()).collect(),
            files_modified: vec![],
            error: None,
        };

        output.show_hook_result(&large_result);
    }

    #[test]
    fn test_config_combinations() {
        // Test all combinations of verbose and quiet
        let configs = [
            (false, false), // normal
            (true, false),  // verbose
            (false, true),  // quiet
            (true, true),   // verbose + quiet (quiet should take precedence)
        ];

        for (verbose, quiet) in &configs {
            let config = UserOutputConfig::new(*verbose, *quiet, Some("never".to_string()));
            let output = UserOutput::new(config);

            // All these should work without panicking
            output.show_status("Test message");
            output.show_hook_start("test-hook");

            let result = HookExecutionResult {
                hook_id: "test".to_string(),
                success: true,
                skipped: false,
                skip_reason: None,
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::from_secs(1),
                files_processed: vec![],
                files_modified: vec![],
                error: None,
            };

            output.show_hook_result(&result);
        }
    }
}
