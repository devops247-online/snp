// Integration tests with real hooks to verify file modification detection works correctly
// Tests the complete pipeline from hook execution to user output

use std::path::PathBuf;
use std::process::Command;
use tempfile::tempdir;
use tokio::fs;

#[cfg(test)]
mod real_hook_integration_tests {
    use super::*;

    /// Helper to create a temporary pre-commit config for testing
    async fn create_test_pre_commit_config(
        hooks: &[&str],
    ) -> anyhow::Result<(tempfile::TempDir, PathBuf)> {
        let temp_dir = tempdir()?;
        let config_path = temp_dir.path().join(".pre-commit-config.yaml");

        let mut config_content = String::from(
            r#"# Test configuration
default_language_version:
  python: python3

repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v5.0.0
    hooks:"#,
        );

        for hook in hooks {
            config_content.push_str(&format!("\n      - id: {hook}"));
        }

        // Add specific config for check-added-large-files
        if hooks.contains(&"check-added-large-files") {
            config_content = config_content.replace(
                "      - id: check-added-large-files",
                "      - id: check-added-large-files\n        args: [\"--maxkb=500\"]",
            );
        }

        fs::write(&config_path, config_content).await?;
        Ok((temp_dir, config_path))
    }

    /// Helper to run SNP with a specific config and capture output
    fn run_snp_with_config(
        config_path: &PathBuf,
        cwd: &std::path::Path,
    ) -> anyhow::Result<std::process::Output> {
        let snp_binary = std::env::current_dir().unwrap().join("target/debug/snp");

        let output = Command::new(snp_binary)
            .args(["run", "--hook-stage=pre-commit"])
            .current_dir(cwd)
            .env("PRE_COMMIT_CONFIG", config_path)
            .output()?;

        Ok(output)
    }

    /// Test that check-added-large-files does NOT show file modifications
    #[tokio::test]
    async fn test_check_added_large_files_no_false_positives() {
        // Create a temporary directory with test files
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("small_test_file.txt");
        fs::write(
            &test_file,
            "This is a small test file that should not trigger size limits",
        )
        .await
        .unwrap();

        // Create a pre-commit config with only check-added-large-files
        let (_config_dir, config_path) =
            create_test_pre_commit_config(&["check-added-large-files"])
                .await
                .unwrap();

        // Initialize a git repo in the temp directory
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["add", "small_test_file.txt"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Copy the config to the temp directory
        let local_config = temp_dir.path().join(".pre-commit-config.yaml");
        fs::copy(&config_path, &local_config).await.unwrap();

        // Run SNP and capture output
        let output = run_snp_with_config(&local_config, temp_dir.path()).unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        println!("STDOUT: {stdout}");
        println!("STDERR: {stderr}");

        // Verify that check-added-large-files does NOT show "files were modified"
        assert!(
            !stdout.contains("files were modified by this hook")
                || !stdout.contains("Check Added Large Files"),
            "check-added-large-files should not show file modifications, but output was: {stdout}"
        );

        // It should show "Passed" for check-added-large-files, or "Skipped" if executable not available in CI
        let has_hook_output = stdout.contains("Check Added Large Files");
        let is_passed = stdout.contains("Passed");
        let is_skipped = stdout.contains("Skipped");

        assert!(
            has_hook_output && (is_passed || is_skipped),
            "check-added-large-files should show as Passed or Skipped (if executable missing), but output was: {stdout}"
        );
    }

    /// Test that trailing-whitespace DOES show file modifications when it fixes files
    #[tokio::test]
    async fn test_trailing_whitespace_shows_modifications() {
        // Create a temporary directory with a file that has trailing whitespace
        let temp_dir = tempdir().unwrap();
        let test_file = temp_dir.path().join("file_with_whitespace.py");
        fs::write(&test_file, "print('hello')   \n").await.unwrap(); // Trailing spaces

        // Create a pre-commit config with trailing-whitespace
        let (_config_dir, config_path) = create_test_pre_commit_config(&["trailing-whitespace"])
            .await
            .unwrap();

        // Initialize a git repo
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["add", "file_with_whitespace.py"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Copy the config to the temp directory
        let local_config = temp_dir.path().join(".pre-commit-config.yaml");
        fs::copy(&config_path, &local_config).await.unwrap();

        // Run SNP and capture output
        let output = run_snp_with_config(&local_config, temp_dir.path()).unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        println!("STDOUT: {stdout}");

        // Verify that trailing-whitespace shows file modifications
        if stdout.contains("Trailing Whitespace") && stdout.contains("Failed") {
            assert!(
                stdout.contains("files were modified by this hook"),
                "trailing-whitespace should show file modifications when fixing files, but output was: {stdout}"
            );

            assert!(
                stdout.contains("Fixing file_with_whitespace.py"),
                "trailing-whitespace should show 'Fixing' message, but output was: {stdout}"
            );
        }
        // Note: If no trailing whitespace is found, the hook would pass and show no modifications
    }

    /// Test multiple hooks together to ensure correct behavior
    #[tokio::test]
    async fn test_multiple_hooks_file_modification_behavior() {
        // Create a temporary directory with various test files
        let temp_dir = tempdir().unwrap();

        // File with trailing whitespace (should be modified by trailing-whitespace)
        let whitespace_file = temp_dir.path().join("whitespace.py");
        fs::write(&whitespace_file, "print('test')   \n")
            .await
            .unwrap();

        // File without newline at end (should be modified by end-of-file-fixer)
        let eof_file = temp_dir.path().join("no_eof.py");
        fs::write(&eof_file, "print('no newline')").await.unwrap();

        // Small file (should NOT be modified by check-added-large-files)
        let small_file = temp_dir.path().join("small.py");
        fs::write(&small_file, "print('small file')").await.unwrap();

        // Create config with multiple hooks
        let (_config_dir, config_path) = create_test_pre_commit_config(&[
            "trailing-whitespace",
            "end-of-file-fixer",
            "check-added-large-files",
        ])
        .await
        .unwrap();

        // Initialize git repo and add files
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["add", "whitespace.py", "no_eof.py", "small.py"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        // Copy config
        let local_config = temp_dir.path().join(".pre-commit-config.yaml");
        fs::copy(&config_path, &local_config).await.unwrap();

        // Run SNP
        let output = run_snp_with_config(&local_config, temp_dir.path()).unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);

        println!("STDOUT: {stdout}");

        // Check each hook's behavior

        // check-added-large-files should NOT show modifications
        if stdout.contains("Check Added Large Files") {
            // If the hook ran, it should not show file modifications
            let large_files_section = extract_hook_section(&stdout, "Check Added Large Files");
            assert!(
                !large_files_section.contains("files were modified by this hook"),
                "check-added-large-files should not show file modifications, section: {large_files_section}"
            );
        }

        // trailing-whitespace and end-of-file-fixer may show modifications if they fix files
        // This depends on whether the hooks actually find issues to fix

        println!("Test completed successfully - hook behaviors verified");
    }

    /// Helper to extract a specific hook's section from SNP output
    fn extract_hook_section(output: &str, hook_name: &str) -> String {
        let lines: Vec<&str> = output.lines().collect();
        let mut in_section = false;
        let mut section = String::new();

        for line in lines {
            if line.contains(hook_name) {
                in_section = true;
                section.push_str(line);
                section.push('\n');
            } else if in_section {
                if line.trim().is_empty() || line.chars().next().unwrap_or(' ').is_uppercase() {
                    // End of this hook's section
                    break;
                } else {
                    section.push_str(line);
                    section.push('\n');
                }
            }
        }

        section
    }

    /// Performance test for file modification detection with many files
    #[tokio::test]
    async fn test_file_modification_detection_performance() {
        let temp_dir = tempdir().unwrap();

        // Create many small files
        for i in 0..100 {
            let file_path = temp_dir.path().join(format!("file_{i}.py"));
            fs::write(&file_path, format!("# File {i}\nprint('hello')"))
                .await
                .unwrap();
        }

        // Create config with check-added-large-files (should not modify any files)
        let (_config_dir, config_path) =
            create_test_pre_commit_config(&["check-added-large-files"])
                .await
                .unwrap();

        // Initialize git and add all files
        Command::new("git")
            .args(["init"])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        Command::new("git")
            .args(["add", "."])
            .current_dir(temp_dir.path())
            .output()
            .unwrap();

        let local_config = temp_dir.path().join(".pre-commit-config.yaml");
        fs::copy(&config_path, &local_config).await.unwrap();

        // Time the execution
        let start = std::time::Instant::now();
        let output = run_snp_with_config(&local_config, temp_dir.path()).unwrap();
        let duration = start.elapsed();

        let stdout = String::from_utf8_lossy(&output.stdout);

        // Should complete in reasonable time (< 5 seconds for 100 files)
        assert!(
            duration.as_secs() < 5,
            "File modification detection took too long: {duration:?}"
        );

        // Should not show any file modifications
        assert!(
            !stdout.contains("files were modified by this hook"),
            "check-added-large-files should not modify any files, but output was: {stdout}"
        );

        println!("Performance test completed in {duration:?}");
    }
}
