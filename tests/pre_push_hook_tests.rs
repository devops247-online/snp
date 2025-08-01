//! Comprehensive tests for pre-push hook functionality
//!
//! These tests verify that the fix for pre-push hooks running on all files
//! (not just staged files) works correctly and doesn't break pre-commit behavior.

use std::path::PathBuf;
use std::process::Command;
use tempfile::TempDir;
use tokio::fs;

use snp::commands::run;
use snp::core::Stage;
use snp::execution::ExecutionConfig;
use snp::git::GitRepository;

/// Helper function to create a test Git repository with files
async fn create_test_git_repo_with_files() -> anyhow::Result<(TempDir, GitRepository, Vec<PathBuf>)>
{
    let temp_dir = tempfile::TempDir::new()?;
    let repo_path = temp_dir.path();

    // Initialize git repo
    let output = Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "Failed to init git repo: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Configure git for testing
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()?;

    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()?;

    // Create multiple files in the repository
    let test_files = vec![
        repo_path.join("src/main.py"),
        repo_path.join("src/utils.py"),
        repo_path.join("tests/test_main.py"),
        repo_path.join("README.md"),
        repo_path.join("requirements.txt"),
    ];

    // Create directories
    fs::create_dir_all(repo_path.join("src")).await?;
    fs::create_dir_all(repo_path.join("tests")).await?;

    // Create files with content
    fs::write(
        &test_files[0],
        "#!/usr/bin/env python3\nprint('Hello, world!')\n",
    )
    .await?;
    fs::write(
        &test_files[1],
        "def utility_function():\n    return 'utility'\n",
    )
    .await?;
    fs::write(
        &test_files[2],
        "import unittest\n\nclass TestMain(unittest.TestCase):\n    pass\n",
    )
    .await?;
    fs::write(
        &test_files[3],
        "# Test Project\n\nThis is a test project.\n",
    )
    .await?;
    fs::write(&test_files[4], "requests>=2.25.0\npytest>=6.0.0\n").await?;

    // Add all files and create initial commit
    let output = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "Failed to add files: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let output = Command::new("git")
        .args(["commit", "-m", "Initial commit with test files"])
        .current_dir(repo_path)
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "Failed to commit: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    // Discover git repository
    let git_repo = GitRepository::discover_from_path(repo_path)?;

    Ok((temp_dir, git_repo, test_files))
}

/// Helper function to create a test config file
async fn create_test_config_file(repo_path: &std::path::Path) -> anyhow::Result<()> {
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: python-check
        name: Python syntax check
        entry: python3 -m py_compile
        language: system
        files: \.py$
        stages: [pre-push]
      - id: readme-check
        name: README check
        entry: echo "Checking README"
        language: system
        files: README\.md$
        stages: [pre-push]
      - id: pre-commit-only
        name: Pre-commit only hook
        entry: echo "Pre-commit only"
        language: system
        files: \.py$
        stages: [pre-commit]
"#;

    fs::write(repo_path.join(".pre-commit-config.yaml"), config_content).await?;
    Ok(())
}

#[tokio::test]
async fn test_pre_push_hooks_run_on_all_files_by_default() {
    // Create test repository with multiple files
    let (_temp_dir, git_repo, _test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config file
    create_test_config_file(repo_path).await.unwrap();

    // Create execution config for pre-push stage (no specific files provided)
    let execution_config = ExecutionConfig::new(Stage::PrePush).with_verbose(true);

    // Execute run command
    let result = run::execute_run_command(repo_path, ".pre-commit-config.yaml", &execution_config)
        .await
        .unwrap();

    // Verify that hooks executed successfully
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // Check that files were processed - pre-push should see all tracked files
    let mut total_files_processed = 0;
    for hook_result in &result.hooks_passed {
        if hook_result.hook_id == "python-check" {
            // Should process all Python files (3 files: main.py, utils.py, test_main.py)
            assert_eq!(
                hook_result.files_processed.len(),
                3,
                "Python check hook should process all 3 Python files, got {}",
                hook_result.files_processed.len()
            );
            total_files_processed += hook_result.files_processed.len();
        }
        if hook_result.hook_id == "readme-check" {
            // Should process README.md (1 file)
            assert_eq!(
                hook_result.files_processed.len(),
                1,
                "README check hook should process 1 README file, got {}",
                hook_result.files_processed.len()
            );
            total_files_processed += hook_result.files_processed.len();
        }
    }

    // Total processed files should be 4 (3 Python + 1 README)
    assert_eq!(
        total_files_processed, 4,
        "Total files processed should be 4 (3 Python + 1 README), got {total_files_processed}"
    );

    println!(
        "✅ Pre-push hooks correctly ran on all tracked files ({total_files_processed} files)"
    );
}

#[tokio::test]
async fn test_pre_commit_hooks_still_run_on_staged_files_only() {
    // Create test repository with multiple files
    let (_temp_dir, git_repo, test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config file
    create_test_config_file(repo_path).await.unwrap();

    // Modify one file and stage it
    let modified_file = &test_files[0]; // main.py
    fs::write(
        modified_file,
        "#!/usr/bin/env python3\nprint('Hello, modified world!')\n",
    )
    .await
    .unwrap();

    let output = Command::new("git")
        .args(["add", "src/main.py"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "Failed to stage file");

    // Create execution config for pre-commit stage (no specific files provided)
    let execution_config = ExecutionConfig::new(Stage::PreCommit).with_verbose(true);

    // Execute run command
    let result = run::execute_run_command(repo_path, ".pre-commit-config.yaml", &execution_config)
        .await
        .unwrap();

    // Verify that hooks executed successfully
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // Check that only staged files were processed
    let mut total_files_processed = 0;
    for hook_result in &result.hooks_passed {
        if hook_result.hook_id == "pre-commit-only" {
            // Should process only the staged Python file (1 file: main.py)
            assert_eq!(
                hook_result.files_processed.len(),
                1,
                "Pre-commit hook should process only 1 staged Python file, got {}",
                hook_result.files_processed.len()
            );

            // Verify it's the correct file
            assert!(
                hook_result.files_processed[0].ends_with("main.py"),
                "Should process main.py, got {:?}",
                hook_result.files_processed[0]
            );

            total_files_processed += hook_result.files_processed.len();
        }
    }

    // Total processed files should be 1 (only staged main.py)
    assert_eq!(
        total_files_processed, 1,
        "Pre-commit should process only 1 staged file, got {total_files_processed}"
    );

    println!(
        "✅ Pre-commit hooks correctly ran on staged files only ({total_files_processed} file)"
    );
}

#[tokio::test]
async fn test_pre_push_with_explicit_files() {
    // Create test repository with multiple files
    let (_temp_dir, git_repo, test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config file
    create_test_config_file(repo_path).await.unwrap();

    // Create execution config with explicit files (should override default behavior)
    let explicit_files = vec![test_files[0].clone(), test_files[1].clone()]; // Only main.py and utils.py
    let execution_config = ExecutionConfig::new(Stage::PrePush)
        .with_files(explicit_files.clone())
        .with_verbose(true);

    // Execute run command
    let result = run::execute_run_command(repo_path, ".pre-commit-config.yaml", &execution_config)
        .await
        .unwrap();

    // Verify that hooks executed successfully
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // Check that only explicit files were processed
    let mut total_files_processed = 0;
    for hook_result in &result.hooks_passed {
        if hook_result.hook_id == "python-check" {
            // Should process only the 2 explicitly specified Python files
            assert_eq!(
                hook_result.files_processed.len(),
                2,
                "Python check hook should process 2 explicit files, got {}",
                hook_result.files_processed.len()
            );
            total_files_processed += hook_result.files_processed.len();
        }
        if hook_result.hook_id == "readme-check" {
            // Should process 0 files (README not in explicit list)
            assert_eq!(
                hook_result.files_processed.len(),
                0,
                "README check hook should process 0 files when not in explicit list, got {}",
                hook_result.files_processed.len()
            );
        }
    }

    // Total processed files should be 2 (only explicit Python files)
    assert_eq!(
        total_files_processed, 2,
        "Should process only 2 explicit files, got {total_files_processed}"
    );

    println!("✅ Pre-push hooks correctly used explicit files ({total_files_processed} files)");
}

#[tokio::test]
async fn test_pre_push_with_all_files_flag() {
    // Create test repository with multiple files
    let (_temp_dir, git_repo, _test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config file
    create_test_config_file(repo_path).await.unwrap();

    // Create execution config with all_files flag
    let execution_config = ExecutionConfig::new(Stage::PrePush)
        .with_all_files(true)
        .with_verbose(true);

    // Execute run command with all files
    let result =
        run::execute_run_command_all_files(repo_path, ".pre-commit-config.yaml", &execution_config)
            .await
            .unwrap();

    // Verify that hooks executed successfully
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // Check that all tracked files were processed (same as default pre-push behavior)
    let mut total_files_processed = 0;
    for hook_result in &result.hooks_passed {
        if hook_result.hook_id == "python-check" {
            // Should process all Python files (3 files)
            assert_eq!(
                hook_result.files_processed.len(),
                3,
                "Python check hook should process all 3 Python files, got {}",
                hook_result.files_processed.len()
            );
            total_files_processed += hook_result.files_processed.len();
        }
        if hook_result.hook_id == "readme-check" {
            // Should process README.md (1 file)
            assert_eq!(
                hook_result.files_processed.len(),
                1,
                "README check hook should process 1 README file, got {}",
                hook_result.files_processed.len()
            );
            total_files_processed += hook_result.files_processed.len();
        }
    }

    // Total processed files should be 4 (3 Python + 1 README)
    assert_eq!(
        total_files_processed, 4,
        "Should process all 4 tracked files, got {total_files_processed}"
    );

    println!(
        "✅ Pre-push hooks with --all-files correctly processed all files ({total_files_processed} files)"
    );
}

#[tokio::test]
async fn test_pre_push_with_from_ref_to_ref() {
    // Create test repository with multiple files
    let (_temp_dir, git_repo, test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config file
    create_test_config_file(repo_path).await.unwrap();

    // Create a second commit with changes
    fs::write(
        &test_files[0],
        "#!/usr/bin/env python3\nprint('Hello, second commit!')\n",
    )
    .await
    .unwrap();
    fs::write(
        repo_path.join("new_file.py"),
        "# New file in second commit\npass\n",
    )
    .await
    .unwrap();

    let output = Command::new("git")
        .args(["add", "."])
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Failed to add files for second commit"
    );

    let output = Command::new("git")
        .args(["commit", "-m", "Second commit with changes"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "Failed to create second commit");

    // Create execution config for testing from HEAD~1 to HEAD
    let execution_config = ExecutionConfig::new(Stage::PrePush).with_verbose(true);

    // Execute run command with refs (changed files between commits)
    let result = run::execute_run_command_with_refs(
        repo_path,
        ".pre-commit-config.yaml",
        "HEAD~1",
        "HEAD",
        &execution_config,
    )
    .await
    .unwrap();

    // Verify that hooks executed successfully
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // Check that only changed files were processed
    let mut total_files_processed = 0;
    for hook_result in &result.hooks_passed {
        if hook_result.hook_id == "python-check" {
            // Should process changed Python files (main.py + new_file.py = 2 files)
            assert!(
                !hook_result.files_processed.is_empty(),
                "Python check hook should process at least 1 changed Python file, got {}",
                hook_result.files_processed.len()
            );
            total_files_processed += hook_result.files_processed.len();
        }
    }

    // Should have processed some changed files
    assert!(
        total_files_processed > 0,
        "Should process changed files between commits, got {total_files_processed}"
    );

    println!(
        "✅ Pre-push hooks with from-ref/to-ref correctly processed changed files ({total_files_processed} files)"
    );
}

#[tokio::test]
async fn test_pre_push_single_hook_execution() {
    // Create test repository with multiple files
    let (_temp_dir, git_repo, _test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config file
    create_test_config_file(repo_path).await.unwrap();

    // Create execution config for pre-push stage (no specific files provided)
    let execution_config = ExecutionConfig::new(Stage::PrePush).with_verbose(true);

    // Execute single hook command
    let result = run::execute_run_command_single_hook(
        repo_path,
        ".pre-commit-config.yaml",
        "python-check",
        &execution_config,
    )
    .await
    .unwrap();

    // Verify that the hook executed successfully
    assert_eq!(
        result.hooks_executed, 1,
        "Should have executed exactly 1 hook"
    );
    assert_eq!(result.hooks_passed.len(), 1, "Should have 1 passed hook");

    // Check that the specific hook processed all matching files
    let hook_result = &result.hooks_passed[0];
    assert_eq!(hook_result.hook_id, "python-check");

    // Should process all Python files (3 files: main.py, utils.py, test_main.py)
    assert_eq!(
        hook_result.files_processed.len(),
        3,
        "Single pre-push hook should process all 3 Python files, got {}",
        hook_result.files_processed.len()
    );

    println!(
        "✅ Single pre-push hook correctly processed all matching files ({} files)",
        hook_result.files_processed.len()
    );
}

#[tokio::test]
async fn test_stage_specific_hook_filtering() {
    // Create test repository with multiple files
    let (_temp_dir, git_repo, test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config file
    create_test_config_file(repo_path).await.unwrap();

    // Test 1: Pre-push stage should only run pre-push hooks
    let pre_push_config = ExecutionConfig::new(Stage::PrePush).with_verbose(true);

    let pre_push_result =
        run::execute_run_command(repo_path, ".pre-commit-config.yaml", &pre_push_config)
            .await
            .unwrap();

    // Should have executed 2 pre-push hooks (python-check and readme-check)
    assert_eq!(
        pre_push_result.hooks_executed, 2,
        "Pre-push stage should execute 2 hooks, got {}",
        pre_push_result.hooks_executed
    );

    // Verify hook IDs
    let executed_hook_ids: Vec<&str> = pre_push_result
        .hooks_passed
        .iter()
        .map(|r| r.hook_id.as_str())
        .collect();
    assert!(
        executed_hook_ids.contains(&"python-check"),
        "Should execute python-check hook"
    );
    assert!(
        executed_hook_ids.contains(&"readme-check"),
        "Should execute readme-check hook"
    );
    assert!(
        !executed_hook_ids.contains(&"pre-commit-only"),
        "Should NOT execute pre-commit-only hook"
    );

    // Test 2: Pre-commit stage should only run pre-commit hooks
    // First modify and stage a file to ensure there's something staged
    fs::write(
        &test_files[0],
        "#!/usr/bin/env python3\nprint('Modified for staging test')\n",
    )
    .await
    .unwrap();
    let output = Command::new("git")
        .args(["add", "src/main.py"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "Failed to stage file");

    let pre_commit_config = ExecutionConfig::new(Stage::PreCommit).with_verbose(true);

    let pre_commit_result =
        run::execute_run_command(repo_path, ".pre-commit-config.yaml", &pre_commit_config)
            .await
            .unwrap();

    // Should have executed 1 pre-commit hook (pre-commit-only)
    assert_eq!(
        pre_commit_result.hooks_executed, 1,
        "Pre-commit stage should execute 1 hook, got {}",
        pre_commit_result.hooks_executed
    );

    // Verify hook ID
    let executed_hook_id = &pre_commit_result.hooks_passed[0].hook_id;
    assert_eq!(
        executed_hook_id, "pre-commit-only",
        "Should execute pre-commit-only hook"
    );

    println!("✅ Stage-specific hook filtering works correctly");
    println!(
        "   Pre-push executed: {:?}",
        pre_push_result
            .hooks_passed
            .iter()
            .map(|r| &r.hook_id)
            .collect::<Vec<_>>()
    );
    println!(
        "   Pre-commit executed: {:?}",
        pre_commit_result
            .hooks_passed
            .iter()
            .map(|r| &r.hook_id)
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn test_pre_push_no_files_behavior() {
    // Create an empty git repository (no tracked files)
    let temp_dir = tempfile::TempDir::new().unwrap();
    let repo_path = temp_dir.path();

    // Initialize git repo
    let output = Command::new("git")
        .args(["init"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "Failed to init git repo");

    // Configure git
    Command::new("git")
        .args(["config", "user.name", "Test User"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@example.com"])
        .current_dir(repo_path)
        .output()
        .unwrap();

    // Create config file but no tracked files
    create_test_config_file(repo_path).await.unwrap();

    // Create execution config for pre-push stage
    let execution_config = ExecutionConfig::new(Stage::PrePush).with_verbose(true);

    // Execute run command
    let result = run::execute_run_command(repo_path, ".pre-commit-config.yaml", &execution_config)
        .await
        .unwrap();

    // Hooks should still execute but process 0 files
    // Hooks may or may not execute on empty repository, but should handle gracefully
    // No need to assert hooks_executed value as any non-negative value is valid

    // If hooks executed, they should have processed 0 files
    for hook_result in &result.hooks_passed {
        assert_eq!(
            hook_result.files_processed.len(),
            0,
            "Hook {} should process 0 files in empty repo, got {}",
            hook_result.hook_id,
            hook_result.files_processed.len()
        );
    }

    println!("✅ Pre-push hooks handle empty repository correctly");
}

#[tokio::test]
async fn test_comparison_pre_commit_vs_pre_push_file_detection() {
    // This test directly compares pre-commit vs pre-push file detection behavior

    // Create test repository with multiple files
    let (_temp_dir, git_repo, test_files) = create_test_git_repo_with_files().await.unwrap();
    let repo_path = git_repo.root_path();

    // Create config with hooks for both stages
    let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: all-files-checker
        name: Check all files
        entry: echo "Processing file:"
        language: system
        files: \.(py|md|txt)$
        stages: [pre-commit, pre-push]
"#;

    fs::write(repo_path.join(".pre-commit-config.yaml"), config_content)
        .await
        .unwrap();

    // Modify and stage only one file
    fs::write(
        &test_files[0],
        "#!/usr/bin/env python3\nprint('Modified file')\n",
    )
    .await
    .unwrap();
    let output = Command::new("git")
        .args(["add", "src/main.py"])
        .current_dir(repo_path)
        .output()
        .unwrap();
    assert!(output.status.success(), "Failed to stage file");

    // Test pre-commit (should process only staged files)
    let pre_commit_config = ExecutionConfig::new(Stage::PreCommit).with_verbose(true);
    let pre_commit_result =
        run::execute_run_command(repo_path, ".pre-commit-config.yaml", &pre_commit_config)
            .await
            .unwrap();

    // Test pre-push (should process all tracked files)
    let pre_push_config = ExecutionConfig::new(Stage::PrePush).with_verbose(true);
    let pre_push_result =
        run::execute_run_command(repo_path, ".pre-commit-config.yaml", &pre_push_config)
            .await
            .unwrap();

    // Extract file counts
    let pre_commit_files = pre_commit_result.hooks_passed[0].files_processed.len();
    let pre_push_files = pre_push_result.hooks_passed[0].files_processed.len();

    // Pre-commit should process fewer files than pre-push
    assert!(
        pre_commit_files < pre_push_files,
        "Pre-commit should process fewer files ({pre_commit_files}) than pre-push ({pre_push_files})"
    );

    // Pre-commit should process 1 file (only staged)
    assert_eq!(
        pre_commit_files, 1,
        "Pre-commit should process exactly 1 staged file, got {pre_commit_files}"
    );

    // Pre-push should process 5 files (3 Python + 1 README + 1 requirements.txt)
    assert_eq!(
        pre_push_files, 5,
        "Pre-push should process all 5 tracked files, got {pre_push_files}"
    );

    println!("✅ File detection comparison successful:");
    println!("   Pre-commit processed: {pre_commit_files} files (staged only)");
    println!("   Pre-push processed: {pre_push_files} files (all tracked)");
}
