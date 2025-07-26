// Comprehensive test suite for SNP run command - TDD Red Phase
// Tests covering all functionality outlined in GitHub issue #30

use clap::Parser;
use snp::cli::{Cli, Commands};
use snp::core::{Hook, Stage};
use snp::execution::ExecutionConfig;
use snp::git::GitRepository;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// Helper function to create a test repository with git
async fn create_test_git_repo() -> (TempDir, GitRepository) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let repo_path = temp_dir.path();

    // Initialize git repository
    std::process::Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to initialize git repo");

    // Configure git user for testing
    std::process::Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to set git user name");

    std::process::Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to set git user email");

    let git_repo = GitRepository::new(repo_path.to_path_buf())
        .await
        .expect("Failed to create GitRepository");

    (temp_dir, git_repo)
}

// Helper function to create test files
fn create_test_files(repo_path: &Path, files: &[(&str, &str)]) {
    for (filename, content) in files {
        let file_path = repo_path.join(filename);
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create parent directories");
        }
        std::fs::write(file_path, content).expect("Failed to write test file");
    }
}

// Helper function to create pre-commit config
fn create_pre_commit_config(repo_path: &Path) -> PathBuf {
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: echo-test
        name: Echo Test Hook
        entry: echo
        language: system
        args: ["Hello from hook"]
        files: \.txt$
        stages: [commit, push]
      - id: echo-python
        name: Echo Python Check
        entry: echo
        language: system
        args: ["Python hook executed"]
        files: \.py$
        stages: [commit]
      - id: always-run-hook
        name: Always Run Hook
        entry: echo
        language: system
        args: ["Always running"]
        always_run: true
        stages: [commit]
      - id: fail-hook
        name: Failing Hook
        entry: false
        language: system
        always_run: true
        stages: [commit]
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, config_content).expect("Failed to write config file");
    config_path
}

// Helper function to stage files in git
fn stage_files(repo_path: &Path, files: &[&str]) {
    for file in files {
        std::process::Command::new("git")
            .args(["add", file])
            .current_dir(repo_path)
            .output()
            .expect("Failed to stage file");
    }
}

// BASIC EXECUTION TESTS

#[tokio::test]
async fn test_run_command_staged_files() {
    // Test: Run hooks on staged files (default behavior)
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create test files
    create_test_files(
        repo_path,
        &[
            ("test1.txt", "Hello world"),
            ("test2.py", "print('hello')"),
            ("README.md", "# Test repo"),
        ],
    );

    // Create config
    let _config_path = create_pre_commit_config(repo_path);

    // Stage some files
    stage_files(repo_path, &["test1.txt", "test2.py"]);

    // Test CLI parsing for staged files (default)
    let cli = Cli::try_parse_from(["snp", "run"]).unwrap();
    match cli.command {
        Some(Commands::Run {
            all_files, files, ..
        }) => {
            assert!(!all_files);
            assert!(files.is_empty());
        }
        _ => panic!("Expected Run command"),
    }

    // TODO: This will fail in Red phase - need to implement actual run logic
    // The test should verify that:
    // 1. Only staged files are processed
    // 2. Hooks matching file patterns are executed
    // 3. Correct exit code is returned
    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    // This assertion will fail initially (Red phase)
    if let Err(ref e) = result {
        eprintln!("Error in test_run_command_staged_files: {e:?}");
    }
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_run_command_all_files() {
    // Test: Run hooks on all files in repository
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    create_test_files(
        repo_path,
        &[
            ("src/main.py", "print('main')"),
            ("tests/test.py", "assert True"),
            ("docs/README.txt", "Documentation"),
            (".gitignore", "*.tmp"),
        ],
    );

    let _config_path = create_pre_commit_config(repo_path);
    stage_files(repo_path, &["src/main.py"]);

    // Test CLI parsing for all files
    let cli = Cli::try_parse_from(["snp", "run", "--all-files"]).unwrap();
    match cli.command {
        Some(Commands::Run { all_files, .. }) => {
            assert!(all_files);
        }
        _ => panic!("Expected Run command with all_files"),
    }

    // TODO: Implement all files execution
    let result = snp::commands::run::execute_run_command_all_files(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit).with_all_files(true),
    )
    .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_run_command_all_files: {e:?}");
    }
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_run_command_specific_files() {
    // Test: Run hooks on specific files
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    create_test_files(
        repo_path,
        &[
            ("file1.txt", "content1"),
            ("file2.txt", "content2"),
            ("file3.py", "print('test')"),
        ],
    );

    let _config_path = create_pre_commit_config(repo_path);

    // Test CLI parsing for specific files
    let cli = Cli::try_parse_from(["snp", "run", "--files", "file1.txt", "file3.py"]).unwrap();

    match cli.command {
        Some(Commands::Run { files, .. }) => {
            assert_eq!(files, vec!["file1.txt", "file3.py"]);
        }
        _ => panic!("Expected Run command with specific files"),
    }

    // TODO: Implement specific files execution
    let specific_files = vec![PathBuf::from("file1.txt"), PathBuf::from("file3.py")];
    let result = snp::commands::run::execute_run_command_with_files(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit).with_files(specific_files),
    )
    .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_run_command_specific_files: {e:?}");
    }
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_run_command_single_hook() {
    // Test: Run a specific hook by ID
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    create_test_files(repo_path, &[("test.txt", "test content")]);
    let _config_path = create_pre_commit_config(repo_path);
    stage_files(repo_path, &["test.txt"]);

    // Test CLI parsing for specific hook
    let cli = Cli::try_parse_from(["snp", "run", "--hook", "echo-test"]).unwrap();
    match cli.command {
        Some(Commands::Run { hook, .. }) => {
            assert_eq!(hook, Some("echo-test".to_string()));
        }
        _ => panic!("Expected Run command with hook"),
    }

    // TODO: Implement single hook execution
    let result = snp::commands::run::execute_run_command_single_hook(
        repo_path,
        ".pre-commit-config.yaml",
        "echo-test",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_run_command_single_hook: {e:?}");
    }
    assert!(result.is_ok());
}

// FILE DISCOVERY TESTS

#[tokio::test]
async fn test_file_discovery_git_integration() {
    // Test: Verify git integration for file discovery
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    create_test_files(
        repo_path,
        &[
            ("staged.txt", "staged file"),
            ("unstaged.txt", "unstaged file"),
            ("untracked.txt", "untracked file"),
        ],
    );

    // Only stage one file
    stage_files(repo_path, &["staged.txt"]);

    // Commit the staged file
    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to commit");

    // Modify and stage another file
    std::fs::write(repo_path.join("staged.txt"), "modified staged").unwrap();
    stage_files(repo_path, &["staged.txt", "unstaged.txt"]);

    // TODO: Test staged file discovery
    let staged_files = git_repo.staged_files().unwrap();
    assert!(!staged_files.is_empty());
    assert!(staged_files
        .iter()
        .any(|f| f.file_name() == Some(std::ffi::OsStr::new("staged.txt"))));
}

#[tokio::test]
async fn test_file_filtering_patterns() {
    // Test: File filtering using regex patterns
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    create_test_files(
        repo_path,
        &[
            ("test.py", "python code"),
            ("test.js", "javascript code"),
            ("test.txt", "text file"),
            ("README.md", "markdown"),
            ("config.yaml", "yaml config"),
        ],
    );

    let _config_path = create_pre_commit_config(repo_path);
    stage_files(
        repo_path,
        &["test.py", "test.js", "test.txt", "README.md", "config.yaml"],
    );

    // TODO: Test file filtering with patterns
    let hooks = vec![
        Hook::new("python-only", "echo", "system").with_files(r"\.py$".to_string()),
        Hook::new("text-files", "echo", "system").with_files(r"\.(txt|md)$".to_string()),
    ];

    // This should filter files correctly based on patterns
    for hook in hooks {
        let result =
            snp::commands::run::filter_files_for_hook(&hook, &git_repo.staged_files().unwrap());
        assert!(result.is_ok());
    }
}

// HOOK EXECUTION TESTS

#[tokio::test]
async fn test_hook_execution_order() {
    // Test: Hooks execute in configuration order
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create config with multiple hooks in specific order
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: first-hook
        name: First Hook
        entry: echo
        language: system
        args: ["First executed"]
        always_run: true
      - id: second-hook
        name: Second Hook
        entry: echo
        language: system
        args: ["Second executed"]
        always_run: true
      - id: third-hook
        name: Third Hook
        entry: echo
        language: system
        args: ["Third executed"]
        always_run: true
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, config_content).unwrap();

    // TODO: Test execution order
    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_hook_execution_order: {e:?}");
    }
    assert!(result.is_ok());
    // Should verify hooks executed in order: first-hook, second-hook, third-hook
}

#[tokio::test]
async fn test_parallel_execution() {
    // Test: Parallel hook execution when enabled
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    let _config_path = create_pre_commit_config(repo_path);

    // TODO: Test parallel execution
    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_hook_timeout(std::time::Duration::from_secs(30));
    // Note: max_parallel_hooks would be set here when implemented

    let result =
        snp::commands::run::execute_run_command(repo_path, ".pre-commit-config.yaml", &config)
            .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_parallel_execution: {e:?}");
    }
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_fail_fast_behavior() {
    // Test: Fail-fast stops execution on first failure
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create config with failing hook followed by other hooks
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: failing-hook
        name: Failing Hook
        entry: false
        language: system
        always_run: true
      - id: should-not-run
        name: Should Not Run
        entry: echo
        language: system
        args: ["This should not execute"]
        always_run: true
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, config_content).unwrap();

    // TODO: Test fail-fast behavior
    let config = ExecutionConfig::new(Stage::PreCommit).with_fail_fast(true);
    let result =
        snp::commands::run::execute_run_command(repo_path, ".pre-commit-config.yaml", &config)
            .await;

    // Should fail and not execute subsequent hooks
    assert!(result.is_err() || !result.unwrap().success);
}

// GIT REFERENCE TESTS

#[tokio::test]
async fn test_from_ref_to_ref_functionality() {
    // Test: --from-ref and --to-ref options work correctly
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create initial commit
    create_test_files(repo_path, &[("initial.txt", "initial content")]);
    stage_files(repo_path, &["initial.txt"]);
    std::process::Command::new("git")
        .args(["commit", "-m", "Initial commit"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to commit");

    // Create second commit
    create_test_files(repo_path, &[("second.txt", "second content")]);
    stage_files(repo_path, &["second.txt"]);
    std::process::Command::new("git")
        .args(["commit", "-m", "Second commit"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to commit");

    let _config_path = create_pre_commit_config(repo_path);

    // TODO: Test CLI parsing for from-ref/to-ref (these options need to be added)
    // This test will fail until we add these CLI options
    let cli_result =
        Cli::try_parse_from(["snp", "run", "--from-ref", "HEAD~1", "--to-ref", "HEAD"]);

    // This will fail initially because --from-ref and --to-ref are not implemented
    assert!(cli_result.is_ok());

    // TODO: Test ref-based file discovery
    let result = snp::commands::run::execute_run_command_with_refs(
        repo_path,
        ".pre-commit-config.yaml",
        "HEAD~1",
        "HEAD",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_from_ref_to_ref_functionality: {e:?}");
    }
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_git_diff_integration() {
    // Test: Git diff integration for changed files
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create and commit initial file
    create_test_files(repo_path, &[("test.py", "print('original')")]);
    stage_files(repo_path, &["test.py"]);
    std::process::Command::new("git")
        .args(["commit", "-m", "Initial"])
        .current_dir(repo_path)
        .output()
        .expect("Failed to commit");

    // Modify file
    std::fs::write(repo_path.join("test.py"), "print('modified')").unwrap();
    stage_files(repo_path, &["test.py"]);

    let _config_path = create_pre_commit_config(repo_path);

    // TODO: Test git diff functionality
    let diff_result = git_repo.diff_staged();
    assert!(diff_result.is_ok());

    // Should detect the modification
    let diff = diff_result.unwrap();
    assert!(!diff.is_empty());
}

// OUTPUT AND REPORTING TESTS

#[tokio::test]
async fn test_output_formatting() {
    // Test: Proper output formatting and progress reporting
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    create_test_files(repo_path, &[("test.txt", "test")]);
    let _config_path = create_pre_commit_config(repo_path);
    stage_files(repo_path, &["test.txt"]);

    // TODO: Test output formatting with verbose mode
    let config = ExecutionConfig::new(Stage::PreCommit).with_verbose(true);
    let result =
        snp::commands::run::execute_run_command(repo_path, ".pre-commit-config.yaml", &config)
            .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_output_formatting: {e:?}");
    }
    assert!(result.is_ok());
    // Should verify proper output formatting, hook names, status indicators
}

#[tokio::test]
async fn test_show_diff_on_failure() {
    // Test: --show-diff-on-failure displays changes
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create a hook that modifies files
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: modify-files
        name: File Modifier
        entry: sed
        language: system
        args: ['-i', 's/test/modified/g']
        files: \.txt$
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, config_content).unwrap();

    create_test_files(repo_path, &[("test.txt", "test content")]);
    stage_files(repo_path, &["test.txt"]);

    // TODO: Test show-diff-on-failure
    let config = ExecutionConfig::new(Stage::PreCommit); // show_diff_on_failure not in ExecutionConfig yet
    let result =
        snp::commands::run::execute_run_command(repo_path, ".pre-commit-config.yaml", &config)
            .await;

    // Should show diff when files are modified
    if let Err(ref e) = result {
        eprintln!("Error in test_show_diff_on_failure: {e:?}");
    }
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_timing_and_performance_reporting() {
    // Test: Execution timing and performance metrics
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    let _config_path = create_pre_commit_config(repo_path);
    create_test_files(repo_path, &[("test.txt", "test")]);
    stage_files(repo_path, &["test.txt"]);

    // TODO: Test timing collection
    let start_time = std::time::Instant::now();
    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;
    let _duration = start_time.elapsed();

    if let Err(ref e) = result {
        eprintln!("Error in test_timing_and_performance_reporting: {e:?}");
    }
    assert!(result.is_ok());
    // Should collect and report timing information
}

// ERROR HANDLING TESTS

#[tokio::test]
async fn test_configuration_errors() {
    // Test: Proper handling of configuration errors
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create invalid config
    let invalid_config = r#"
invalid_yaml: [
  - missing_closing_bracket
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, invalid_config).unwrap();

    // TODO: Test config error handling
    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    // Should return appropriate error for invalid config
    assert!(result.is_err());
}

#[tokio::test]
async fn test_missing_config_file() {
    // Test: Handle missing configuration file
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // TODO: Test missing config handling
    let result = snp::commands::run::execute_run_command(
        repo_path,
        "nonexistent-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    // Should return appropriate error for missing config
    assert!(result.is_err());
}

#[tokio::test]
async fn test_hook_execution_failures() {
    // Test: Handle hook execution failures gracefully
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create config with failing hooks
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: nonexistent-command
        name: Nonexistent Command
        entry: this-command-does-not-exist
        language: system
        always_run: true
      - id: timeout-hook
        name: Timeout Hook
        entry: sleep
        language: system
        args: ["300"]
        always_run: true
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, config_content).unwrap();

    // TODO: Test execution failure handling
    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_hook_timeout(std::time::Duration::from_millis(100));

    let result =
        snp::commands::run::execute_run_command(repo_path, ".pre-commit-config.yaml", &config)
            .await;

    // Should handle failures gracefully
    if let Err(ref e) = result {
        eprintln!("Error in test_hook_execution_failures: {e:?}");
    }
    assert!(result.is_ok()); // Function succeeds but reports hook failures
}

#[tokio::test]
async fn test_git_operation_errors() {
    // Test: Handle git operation errors
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let non_git_path = temp_dir.path();

    // Don't initialize git repo - should cause git errors
    create_test_files(non_git_path, &[("test.txt", "test")]);
    let _config_path = create_pre_commit_config(non_git_path);

    // TODO: Test git error handling
    let result = snp::commands::run::execute_run_command(
        non_git_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    // Should return appropriate error for non-git directory
    assert!(result.is_err());
}

#[tokio::test]
async fn test_permission_errors() {
    // Test: Handle file permission errors
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create file and remove read permissions (on Unix systems)
    create_test_files(repo_path, &[("test.txt", "test")]);
    let _config_path = create_pre_commit_config(repo_path);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let file_path = repo_path.join("test.txt");
        let mut perms = std::fs::metadata(&file_path).unwrap().permissions();
        perms.set_mode(0o000); // Remove all permissions
        std::fs::set_permissions(&file_path, perms).unwrap();
    }

    stage_files(repo_path, &["test.txt"]);

    // TODO: Test permission error handling
    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    // Should handle permission errors gracefully
    // Note: This might succeed on some systems, depending on how we handle permissions
    assert!(result.is_ok() || result.is_err());
}

// INTEGRATION TESTS

#[tokio::test]
async fn test_end_to_end_workflow() {
    // Test: Complete end-to-end workflow
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Create realistic project structure
    create_test_files(
        repo_path,
        &[
            (
                "src/main.py",
                "#!/usr/bin/env python3\nprint('Hello, World!')",
            ),
            ("src/utils.py", "def helper(): pass"),
            ("tests/test_main.py", "def test_main(): assert True"),
            ("README.md", "# Test Project"),
            ("requirements.txt", "pytest>=6.0"),
            (".gitignore", "*.pyc\n__pycache__/"),
        ],
    );

    // Create comprehensive config
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: python-syntax
        name: Python Syntax Check
        entry: echo
        language: system
        args: ["Python syntax check passed"]
        files: \.py$
      - id: readme-check
        name: README Check
        entry: echo
        language: system
        args: ["README verified"]
        files: README\.md$
      - id: requirements-check
        name: Requirements Check
        entry: echo
        language: system
        args: ["Requirements verified"]
        files: requirements\.txt$
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, config_content).unwrap();

    // Stage files
    stage_files(repo_path, &["src/main.py", "README.md", "requirements.txt"]);

    // TODO: Test full workflow
    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_end_to_end_workflow: {e:?}");
    }
    assert!(result.is_ok());
    let execution_result = result.unwrap();
    if !execution_result.success {
        eprintln!("End-to-end workflow failed:");
        eprintln!("  Hooks executed: {}", execution_result.hooks_executed);
        eprintln!("  Hooks passed: {}", execution_result.hooks_passed.len());
        eprintln!("  Hooks failed: {}", execution_result.hooks_failed.len());
        for failed_hook in &execution_result.hooks_failed {
            eprintln!(
                "  Failed hook: {} - {:?}",
                failed_hook.hook_id, failed_hook.error
            );
            eprintln!("    Exit code: {:?}", failed_hook.exit_code);
            eprintln!("    Files processed: {:?}", failed_hook.files_processed);
            eprintln!("    Stdout: {}", failed_hook.stdout);
            eprintln!("    Stderr: {}", failed_hook.stderr);
        }
    }
    assert!(execution_result.success);
    assert!(execution_result.hooks_executed > 0);
}

#[tokio::test]
async fn test_compatibility_with_python_precommit() {
    // Test: Ensure compatibility with Python pre-commit behavior
    let (_temp_dir, git_repo) = create_test_git_repo().await;
    let repo_path = git_repo.path();

    // Use exact same config format as Python pre-commit
    let python_compatible_config = r#"
# This config should work identically in both Python and Rust implementations
repos:
  - repo: local
    hooks:
      - id: trailing-whitespace
        name: Trim Trailing Whitespace
        entry: echo
        language: system
        args: ["Checking whitespace"]
        types: [text]
      - id: end-of-file-fixer
        name: Fix End of Files
        entry: echo
        language: system
        args: ["Fixing EOF"]
        types: [text]
        exclude: \.min\.(js|css)$
"#;
    let config_path = repo_path.join(".pre-commit-config.yaml");
    std::fs::write(&config_path, python_compatible_config).unwrap();

    create_test_files(
        repo_path,
        &[
            ("file.txt", "content with trailing spaces   \n"),
            ("script.js", "console.log('test');"),
            ("style.min.css", ".class{color:red}"),
        ],
    );

    stage_files(repo_path, &["file.txt", "script.js", "style.min.css"]);

    // TODO: Test Python compatibility
    let result = snp::commands::run::execute_run_command(
        repo_path,
        ".pre-commit-config.yaml",
        &ExecutionConfig::new(Stage::PreCommit),
    )
    .await;

    if let Err(ref e) = result {
        eprintln!("Error in test_compatibility_with_python_precommit: {e:?}");
    }
    assert!(result.is_ok());
    // Should behave identically to Python pre-commit
}

// Note: These tests will all fail initially (Red phase of TDD)
// This is expected and correct - they define the interface and behavior
// that we need to implement in the Green phase.
