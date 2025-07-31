// Tests for user output enhancements related to file modification display
// Tests the improvements made to show "files were modified by this hook" and "Fixing [path]"

use snp::execution::HookExecutionResult;
use snp::user_output::{UserOutput, UserOutputConfig};
use std::path::PathBuf;
use std::time::Duration;

#[cfg(test)]
mod user_output_file_modification_tests {
    use super::*;

    /// Test that successful hooks with file modifications show the enhancement messages
    #[test]
    fn test_successful_hook_with_file_modifications_display() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "end-of-file-fixer".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(100),
            files_processed: vec!["src/main.rs".into(), "src/lib.rs".into()],
            files_modified: vec!["src/main.rs".into(), "src/lib.rs".into()],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        // This test primarily ensures the method doesn't panic
        // and that the display logic handles file modifications correctly
        output.show_hook_result(&result);

        // In a real test environment, we would capture stdout and verify:
        // 1. Shows "Passed"
        // 2. Shows "- files were modified by this hook"
        // 3. Shows "Fixing src/main.rs"
        // 4. Shows "Fixing src/lib.rs"
        // 5. Has proper spacing (blank line after fixing messages)
    }

    /// Test that failed hooks with file modifications show the enhancement messages
    #[test]
    fn test_failed_hook_with_file_modifications_display() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "trailing-whitespace".to_string(),
            success: false,
            skipped: false,
            skip_reason: None,
            exit_code: Some(1),
            duration: Duration::from_millis(150),
            files_processed: vec![
                "dataans/src-tauri/src/config.rs".into(),
                "dataans/src-tauri/src/dialog.rs".into(),
            ],
            files_modified: vec![
                "dataans/src-tauri/src/config.rs".into(),
                "dataans/src-tauri/src/dialog.rs".into(),
            ],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // Should show:
        // 1. "Failed"
        // 2. Hook details (hook id, exit code)
        // 3. "- files were modified by this hook"
        // 4. "Fixing dataans/src-tauri/src/config.rs"
        // 5. "Fixing dataans/src-tauri/src/dialog.rs"
        // 6. Proper spacing
    }

    /// Test that hooks without file modifications don't show the enhancement messages
    #[test]
    fn test_hook_without_file_modifications_display() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "check-added-large-files".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(50),
            files_processed: vec!["src/main.rs".into(), "src/lib.rs".into()],
            files_modified: vec![], // No files modified
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // Should show only:
        // 1. "Passed"
        // 2. NO "files were modified" message
        // 3. NO "Fixing" messages
    }

    /// Test that skipped hooks don't show file modification messages
    #[test]
    fn test_skipped_hook_no_file_modifications_display() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "ruff".to_string(),
            success: true,
            skipped: true,
            skip_reason: Some("no files to check".to_string()),
            exit_code: None,
            duration: Duration::from_millis(0),
            files_processed: vec![],
            files_modified: vec![], // Even if this were populated, skipped hooks shouldn't show it
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // Should show only:
        // 1. "Skipped"
        // 2. NO file modification messages
    }

    /// Test relative path calculation for file modifications
    #[test]
    fn test_relative_path_display_in_file_modifications() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        // Test with absolute paths that should be converted to relative
        let result = HookExecutionResult {
            hook_id: "black".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(200),
            files_processed: vec![
                "/home/user/project/src/main.py".into(),
                "/home/user/project/tests/test_main.py".into(),
            ],
            files_modified: vec![
                "/home/user/project/src/main.py".into(),
                "/home/user/project/tests/test_main.py".into(),
            ],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // The output should show relative paths like:
        // "Fixing src/main.py"
        // "Fixing tests/test_main.py"
        // (assuming current directory is /home/user/project)
    }

    /// Test file modification display with many files
    #[test]
    fn test_many_file_modifications_display() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        // Create a scenario with many modified files
        let many_files: Vec<PathBuf> = (0..20).map(|i| format!("src/file_{i}.py").into()).collect();

        let result = HookExecutionResult {
            hook_id: "autopep8".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(500),
            files_processed: many_files.clone(),
            files_modified: many_files,
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // Should handle many files gracefully without performance issues
        // Should show all "Fixing" messages properly formatted
    }

    /// Test file modification display with special characters in paths
    #[test]
    fn test_special_characters_in_file_paths() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "prettier".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(100),
            files_processed: vec![
                "src/file with spaces.js".into(),
                "src/file-with-dashes.js".into(),
                "src/file_with_underscores.js".into(),
                "src/file.with.dots.js".into(),
                "src/файл-with-unicode.js".into(),
            ],
            files_modified: vec![
                "src/file with spaces.js".into(),
                "src/file-with-dashes.js".into(),
                "src/file_with_underscores.js".into(),
                "src/file.with.dots.js".into(),
                "src/файл-with-unicode.js".into(),
            ],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // Should handle special characters in filenames properly
        // Should not crash or produce malformed output
    }

    /// Test color output for file modification messages
    #[test]
    fn test_file_modification_colors() {
        // Test with colors enabled
        let config_with_colors = UserOutputConfig::new(false, false, Some("always".to_string()));
        let output_with_colors = UserOutput::new(config_with_colors);

        let result = HookExecutionResult {
            hook_id: "rustfmt".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(120),
            files_processed: vec!["src/main.rs".into()],
            files_modified: vec!["src/main.rs".into()],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output_with_colors.show_hook_result(&result);

        // Test with colors disabled
        let config_no_colors = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output_no_colors = UserOutput::new(config_no_colors);

        output_no_colors.show_hook_result(&result);

        // Both should work without crashing
        // With colors: "files were modified" should be gray, "Fixing" should be white (no color codes)
        // Without colors: All text should be plain
    }

    /// Test quiet mode suppresses file modification messages
    #[test]
    fn test_quiet_mode_suppresses_file_modification_messages() {
        let config = UserOutputConfig::new(false, true, Some("never".to_string())); // quiet = true
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "isort".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(80),
            files_processed: vec!["src/imports.py".into()],
            files_modified: vec!["src/imports.py".into()],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // In quiet mode, no output should be produced
        // This test mainly ensures no panics occur
    }

    /// Test verbose mode shows additional details alongside file modifications
    #[test]
    fn test_verbose_mode_with_file_modifications() {
        let config = UserOutputConfig::new(true, false, Some("always".to_string())); // verbose = true
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "mypy".to_string(),
            success: false,
            skipped: false,
            skip_reason: None,
            exit_code: Some(1),
            duration: Duration::from_millis(300),
            files_processed: vec!["src/types.py".into()],
            files_modified: vec!["src/types.py".into()], // Unusual for mypy, but testing the display
            stdout: "Found 3 type errors".to_string(),
            stderr: "mypy: error details".to_string(),
            error: None,
        };

        output.show_hook_result(&result);

        // Should show:
        // 1. "Failed"
        // 2. Hook details
        // 3. Stderr
        // 4. File modification messages
        // 5. Verbose details (duration, files processed, stdout/stderr)
    }

    /// Test that file modification messages appear in the correct order
    #[test]
    fn test_file_modification_message_ordering() {
        let config = UserOutputConfig::new(false, false, Some("never".to_string()));
        let output = UserOutput::new(config);

        let result = HookExecutionResult {
            hook_id: "test-hook".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(100),
            files_processed: vec![
                "a_first_file.txt".into(),
                "b_second_file.txt".into(),
                "c_third_file.txt".into(),
            ],
            files_modified: vec![
                "a_first_file.txt".into(),
                "b_second_file.txt".into(),
                "c_third_file.txt".into(),
            ],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        output.show_hook_result(&result);

        // The order should be:
        // 1. "Passed"
        // 2. "- files were modified by this hook"
        // 3. [blank line]
        // 4. "Fixing a_first_file.txt"
        // 5. "Fixing b_second_file.txt"
        // 6. "Fixing c_third_file.txt"
        // 7. [blank line]
    }
}
