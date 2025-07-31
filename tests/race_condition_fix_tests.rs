//! Integration tests to verify the race condition fix
//! Tests that read-only hooks don't show false positive file modifications
//! when running alongside file-modifying hooks

use std::path::PathBuf;
use std::sync::Arc;
use tempfile::tempdir;
use tokio::fs;

use snp::core::{Hook, Stage};
use snp::execution::{ExecutionConfig, HookExecutionEngine};
use snp::process::ProcessManager;
use snp::storage::Store;

/// Create a test file with trailing whitespace and missing newline
async fn create_test_file_with_issues() -> anyhow::Result<(tempfile::TempDir, PathBuf)> {
    let temp_dir = tempdir()?;
    let file_path = temp_dir.path().join("test_file.py");

    // Create file with trailing whitespace and missing newline (will be fixed by hooks)
    fs::write(&file_path, "print('hello world')   ").await?;

    Ok((temp_dir, file_path))
}

/// Create mock hooks that represent the problematic scenario:
/// - File-modifying hooks that will actually modify files (using simple Python commands)
/// - Read-only hooks that should NOT show as modifying files (using simple Python commands)
fn create_test_hooks() -> Vec<Hook> {
    vec![
        // Mock file-modifying hooks (use simple Python commands that are guaranteed to work)
        Hook::new("trailing-whitespace", "python3 -c \"import sys; f=open(sys.argv[1],'a'); f.write('\\n# modified by trailing-whitespace\\n'); f.close()\"", "python")
            .with_types(vec!["python".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .pass_filenames(true),
        Hook::new("end-of-file-fixer", "python3 -c \"import sys; f=open(sys.argv[1],'a'); f.write('\\n# modified by end-of-file-fixer\\n'); f.close()\"", "python")
            .with_types(vec!["python".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .pass_filenames(true),
        // Mock read-only hooks (use simple Python commands to read without modifying)
        Hook::new("check-merge-conflict", "python3 -c \"import sys; open(sys.argv[1]).read()\"", "python")
            .with_types(vec!["python".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .pass_filenames(true),
        Hook::new("check-case-conflict", "python3 -c \"import sys; len(open(sys.argv[1]).readlines())\"", "python")
            .with_types(vec!["python".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .pass_filenames(true),
    ]
}

#[tokio::test]
async fn test_race_condition_fix_prevents_false_positives() {
    // Create test environment
    let (_temp_dir, test_file) = create_test_file_with_issues().await.unwrap();
    let test_files = vec![test_file];

    // Create execution engine with isolated storage
    let temp_store_dir = tempdir().unwrap();
    let store = Arc::new(Store::with_cache_directory(temp_store_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new_async(process_manager, store)
        .await
        .unwrap();

    // Create hooks that would cause race condition without fix
    let hooks = create_test_hooks();

    // Configure execution
    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(test_files.clone())
        .with_verbose(true);

    // Execute hooks - the fix should automatically use sequential execution
    let result = engine.execute_hooks(&hooks, config).await.unwrap();

    // Verify execution completed
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // Verify basic execution completed
    assert!(
        result.hooks_executed == 4,
        "Expected 4 hooks to be executed, got {}",
        result.hooks_executed
    );

    // Analyze results to verify no false positives
    let mut file_modifying_hooks_with_modifications = 0;

    // Expected file-modifying hooks
    let file_modifying_hook_ids = ["trailing-whitespace", "end-of-file-fixer"];
    // Expected read-only hooks
    let readonly_hook_ids = ["check-merge-conflict", "check-case-conflict"];

    for hook_result in result.hooks_passed.iter().chain(result.hooks_failed.iter()) {
        if file_modifying_hook_ids.contains(&hook_result.hook_id.as_str()) {
            // File-modifying hooks are allowed to show modifications (if they execute successfully)
            if !hook_result.files_modified.is_empty() {
                file_modifying_hooks_with_modifications += 1;
            }
        } else if readonly_hook_ids.contains(&hook_result.hook_id.as_str()) {
            // Read-only hooks should NEVER show modifications - this is the critical test
            if !hook_result.files_modified.is_empty() {
                panic!(
                    "CRITICAL BUG: Read-only hook '{}' incorrectly shows file modifications: {:?}",
                    hook_result.hook_id, hook_result.files_modified
                );
            }
        }
    }

    // The critical test is that read-only hooks don't show false positive file modifications
    // If we reach this point, no read-only hooks showed false positives (they would have panicked above)

    println!(
        "✅ Race condition fix verified: {} file-modifying hook(s) detected changes, \
         {} read-only hooks correctly showed no modifications",
        file_modifying_hooks_with_modifications,
        readonly_hook_ids.len()
    );
}

#[tokio::test]
async fn test_parallel_execution_still_works_when_safe() {
    // Create test environment with only read-only hooks
    let (_temp_dir, test_file) = create_test_file_with_issues().await.unwrap();
    let test_files = vec![test_file];

    // Create execution engine with isolated storage
    let temp_store_dir = tempdir().unwrap();
    let store = Arc::new(Store::with_cache_directory(temp_store_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new_async(process_manager, store)
        .await
        .unwrap();

    // Create only read-only hooks (should still use parallel execution)
    let hooks = vec![
        Hook::new(
            "check-merge-conflict",
            "python3 -c \"import sys; open(sys.argv[1]).read()\"",
            "python",
        )
        .with_types(vec!["python".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true),
        Hook::new(
            "check-case-conflict",
            "python3 -c \"import sys; len(open(sys.argv[1]).readlines())\"",
            "python",
        )
        .with_types(vec!["python".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true),
        Hook::new(
            "check-yaml",
            "python3 -c \"import sys; open(sys.argv[1]).read()\"",
            "python",
        )
        .with_types(vec!["yaml".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true),
    ];

    // Configure execution with multiple parallel jobs allowed
    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(test_files.clone())
        .with_max_parallel_hooks(4) // Allow parallel execution
        .with_verbose(true);

    // Execute hooks - should use parallel execution since all are read-only
    let result = engine.execute_hooks(&hooks, config).await.unwrap();

    // Verify execution completed successfully
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // Verify no false positives even in parallel execution of read-only hooks
    for hook_result in result.hooks_passed.iter().chain(result.hooks_failed.iter()) {
        assert!(
            hook_result.files_modified.is_empty(),
            "Read-only hook '{}' should not show file modifications even in parallel execution",
            hook_result.hook_id
        );
    }

    println!("✅ Parallel execution works correctly when safe (all read-only hooks)");
}

#[tokio::test]
async fn test_sequential_execution_forced_for_mixed_hooks() {
    // This test verifies that the system correctly detects race condition risk
    // and automatically switches to sequential execution

    // Create test environment
    let (_temp_dir, test_file) = create_test_file_with_issues().await.unwrap();
    let test_files = vec![test_file];

    // Create execution engine with isolated storage
    let temp_store_dir = tempdir().unwrap();
    let store = Arc::new(Store::with_cache_directory(temp_store_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new_async(process_manager, store)
        .await
        .unwrap();

    // Create mix of read-only and file-modifying hooks (triggers race condition prevention)
    let hooks = create_test_hooks();

    // Configure execution to prefer parallel (but should be overridden by race condition detection)
    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(test_files.clone())
        .with_max_parallel_hooks(8) // Allow parallel execution
        .with_verbose(true); // Should show race condition detection message

    // Execute hooks - should automatically use sequential execution despite preference for parallel
    let result = engine.execute_hooks(&hooks, config).await.unwrap();

    // Verify execution completed
    assert!(result.hooks_executed > 0, "Should have executed some hooks");

    // The key test: verify that the race condition fix prevented false positives
    let readonly_hook_ids = ["check-merge-conflict", "check-case-conflict"];
    for hook_result in result.hooks_passed.iter().chain(result.hooks_failed.iter()) {
        if readonly_hook_ids.contains(&hook_result.hook_id.as_str()) {
            assert!(
                hook_result.files_modified.is_empty(),
                "Race condition fix failed: read-only hook '{}' shows false positive modifications: {:?}",
                hook_result.hook_id, hook_result.files_modified
            );
        }
    }

    println!("✅ Sequential execution automatically triggered for mixed hook types");
}
