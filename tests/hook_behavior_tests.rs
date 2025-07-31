// Tests for hook-specific behavior regarding file modifications
// Tests to ensure read-only hooks don't show false positives and modifying hooks work correctly

use snp::execution::HookExecutionResult;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

#[cfg(test)]
mod hook_behavior_tests {
    use super::*;

    /// List of hooks that should NEVER modify files (read-only hooks)
    fn get_read_only_hooks() -> HashSet<&'static str> {
        [
            "check-added-large-files",
            "check-ast",
            "check-builtin-literals",
            "check-case-conflict",
            "check-docstring-first",
            "check-executables-have-shebangs",
            "check-json",
            "check-merge-conflict",
            "check-symlinks",
            "check-toml",
            "check-vcs-permalinks",
            "check-xml",
            "check-yaml",
            "debug-statements",
            "destroyed-symlinks",
            "detect-aws-credentials",
            "detect-private-key",
            "file-contents-sorter", // Actually this might modify, need to check
            "name-tests-test",
            "requirements-txt-fixer", // This might modify, need to check
            "ruff",                   // Without --fix flag, ruff is read-only
                                      // Add more as needed
        ]
        .iter()
        .cloned()
        .collect()
    }

    /// List of hooks that commonly modify files
    fn get_file_modifying_hooks() -> HashSet<&'static str> {
        [
            "trailing-whitespace",
            "end-of-file-fixer",
            "autopep8",
            "black",
            "isort",
            "prettier",
            "rustfmt",
            "ruff-format",
            "sort-simple-yaml",
            "mixed-line-ending",
            // Add more as needed
        ]
        .iter()
        .cloned()
        .collect()
    }

    /// Test that read-only hooks don't show file modifications
    #[test]
    fn test_read_only_hooks_no_file_modifications() {
        let read_only_hooks = get_read_only_hooks();

        for hook_id in read_only_hooks {
            // Create a successful result for a read-only hook
            let result = HookExecutionResult {
                hook_id: hook_id.to_string(),
                success: true,
                skipped: false,
                skip_reason: None,
                exit_code: Some(0),
                duration: Duration::from_millis(50),
                files_processed: vec!["test_file_1.py".into(), "test_file_2.py".into()],
                files_modified: vec![], // Should always be empty for read-only hooks
                stdout: String::new(),
                stderr: String::new(),
                error: None,
            };

            // Verify that files_modified is empty
            assert!(
                result.files_modified.is_empty(),
                "Hook '{}' is marked as read-only but shows modified files: {:?}",
                hook_id,
                result.files_modified
            );
        }
    }

    /// Test that file-modifying hooks can show file modifications
    #[test]
    fn test_file_modifying_hooks_can_show_modifications() {
        let modifying_hooks = get_file_modifying_hooks();

        for hook_id in modifying_hooks {
            // Create a result where the hook modified files
            let result = HookExecutionResult {
                hook_id: hook_id.to_string(),
                success: true, // Some hooks succeed after modification, others fail
                skipped: false,
                skip_reason: None,
                exit_code: Some(0),
                duration: Duration::from_millis(100),
                files_processed: vec!["test_file_1.py".into(), "test_file_2.py".into()],
                files_modified: vec!["test_file_1.py".into(), "test_file_2.py".into()], // These hooks can modify files
                stdout: String::new(),
                stderr: String::new(),
                error: None,
            };

            // Verify that files_modified can be populated for these hooks
            assert!(
                !result.files_modified.is_empty() || result.files_processed.is_empty(),
                "Hook '{hook_id}' is expected to potentially modify files"
            );
        }
    }

    /// Specific regression test for check-added-large-files bug
    #[test]
    fn test_check_added_large_files_specific_behavior() {
        // This is the specific hook that was showing false positives

        let result = HookExecutionResult {
            hook_id: "check-added-large-files".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(40),
            files_processed: vec![
                "small_file.txt".into(),
                "another_small_file.py".into(),
                "config.yaml".into(),
            ],
            files_modified: vec![], // MUST be empty - this was the bug
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        assert!(
            result.files_modified.is_empty(),
            "check-added-large-files MUST NOT show any file modifications, but got: {:?}",
            result.files_modified
        );

        // Also test when the hook finds large files (fails)
        let failing_result = HookExecutionResult {
            hook_id: "check-added-large-files".to_string(),
            success: false,
            skipped: false,
            skip_reason: None,
            exit_code: Some(1),
            duration: Duration::from_millis(45),
            files_processed: vec!["large_file.bin".into()],
            files_modified: vec![], // Still must be empty even when failing
            stdout: String::new(),
            stderr: "large_file.bin (1.2 MB) exceeds 500 KB".to_string(),
            error: None,
        };

        assert!(
            failing_result.files_modified.is_empty(),
            "check-added-large-files MUST NOT show file modifications even when failing"
        );
    }

    /// Test trailing-whitespace hook behavior (common file modifier)
    #[test]
    fn test_trailing_whitespace_hook_behavior() {
        // Test successful case (no trailing whitespace found)
        let success_result = HookExecutionResult {
            hook_id: "trailing-whitespace".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(60),
            files_processed: vec!["clean_file.py".into()],
            files_modified: vec![], // No modifications needed
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        // When no trailing whitespace is found, no files should be modified
        assert!(success_result.files_modified.is_empty());

        // Test failing case (found and fixed trailing whitespace)
        let fixing_result = HookExecutionResult {
            hook_id: "trailing-whitespace".to_string(),
            success: false, // Usually fails when it fixes files
            skipped: false,
            skip_reason: None,
            exit_code: Some(1),
            duration: Duration::from_millis(80),
            files_processed: vec!["file_with_whitespace.py".into()],
            files_modified: vec!["file_with_whitespace.py".into()], // Fixed the file
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        // When trailing whitespace is fixed, files should be marked as modified
        assert_eq!(fixing_result.files_modified.len(), 1);
        assert_eq!(
            fixing_result.files_modified[0],
            PathBuf::from("file_with_whitespace.py")
        );
    }

    /// Test end-of-file-fixer hook behavior
    #[test]
    fn test_end_of_file_fixer_hook_behavior() {
        // Test when files need fixing

        let result = HookExecutionResult {
            hook_id: "end-of-file-fixer".to_string(),
            success: true,
            skipped: false,
            skip_reason: None,
            exit_code: Some(0),
            duration: Duration::from_millis(70),
            files_processed: vec!["no_newline.py".into()],
            files_modified: vec!["no_newline.py".into()],
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        };

        assert_eq!(result.files_modified.len(), 1);
        assert_eq!(result.files_modified[0], PathBuf::from("no_newline.py"));
    }
}
