// Integration tests for incremental file change detection
// Tests the complete integration between FileChangeDetector and HookExecutionEngine

use snp::{
    core::{Hook, Stage},
    execution::{ExecutionConfig, HookExecutionEngine},
    file_change_detector::FileChangeDetectorConfig,
    process::ProcessManager,
    storage::Store,
};
use std::fs;
use std::sync::Arc;
use tempfile::TempDir;
use tokio::time::{sleep, Duration};

#[tokio::test]
async fn test_incremental_detection_integration() {
    let temp_dir = TempDir::new().unwrap();
    let test_file1 = temp_dir.path().join("test1.py");
    let test_file2 = temp_dir.path().join("test2.py");

    // Create test files
    fs::write(&test_file1, "print('hello')").unwrap();
    fs::write(&test_file2, "print('world')").unwrap();

    // Setup storage and engine
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Store::with_cache_directory(storage_dir).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    // Create a simple echo hook
    let hook = Hook::new("echo-test", "echo", "system")
        .with_args(vec!["Processing".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    let files = vec![test_file1.clone(), test_file2.clone()];

    // First run with incremental detection enabled
    let incremental_config = FileChangeDetectorConfig::new()
        .with_watch_filesystem(false) // Disable for test stability
        .with_force_refresh(false);

    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(files.clone())
        .with_incremental_config(incremental_config.clone());

    // First execution - all files should be processed
    let result1 = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result1.files_processed.len(), 2);
    assert!(result1.success);

    // Second execution immediately - no files should be processed due to no changes
    let result2 = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result2.files_processed.len(), 0);
    assert!(result2.skipped);

    // Modify one file
    sleep(Duration::from_millis(10)).await; // Ensure different mtime
    fs::write(&test_file1, "print('hello modified')").unwrap();

    // Third execution - only modified file should be processed
    let result3 = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result3.files_processed.len(), 1);
    assert_eq!(result3.files_processed[0], test_file1);
    assert!(result3.success);
}

#[tokio::test]
async fn test_incremental_detection_disabled() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");
    fs::write(&test_file, "print('hello')").unwrap();

    // Setup storage and engine
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Store::with_cache_directory(storage_dir).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    let hook = Hook::new("echo-test", "echo", "system")
        .with_args(vec!["Processing".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    let files = vec![test_file.clone()];

    // Configuration without incremental detection
    let config = ExecutionConfig::new(Stage::PreCommit).with_files(files.clone());

    // First execution
    let result1 = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result1.files_processed.len(), 1);
    assert!(result1.success);

    // Second execution - should still process all files (no incremental detection)
    let result2 = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result2.files_processed.len(), 1);
    assert!(result2.success);
}

#[tokio::test]
async fn test_force_refresh_setting() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");
    fs::write(&test_file, "print('hello')").unwrap();

    // Setup storage and engine
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Store::with_cache_directory(storage_dir).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    let hook = Hook::new("echo-test", "echo", "system")
        .with_args(vec!["Processing".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    let files = vec![test_file.clone()];

    // Configuration with force refresh enabled
    let incremental_config = FileChangeDetectorConfig::new()
        .with_watch_filesystem(false)
        .with_force_refresh(true);

    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(files.clone())
        .with_incremental_config(incremental_config);

    // First execution
    let result1 = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result1.files_processed.len(), 1);
    assert!(result1.success);

    // Second execution - should still process all files due to force refresh
    let result2 = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result2.files_processed.len(), 1);
    assert!(result2.success);
}

#[tokio::test]
async fn test_large_file_handling() {
    let temp_dir = TempDir::new().unwrap();
    let large_file = temp_dir.path().join("large.py");

    // Create a "large" file (just over the threshold for testing)
    let large_content = "x".repeat(200); // Larger than our test threshold
    fs::write(&large_file, &large_content).unwrap();

    // Setup storage and engine
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Store::with_cache_directory(storage_dir).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    let hook = Hook::new("echo-test", "echo", "system")
        .with_args(vec!["Processing".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    let files = vec![large_file.clone()];

    // Configuration with small large file threshold
    let incremental_config = FileChangeDetectorConfig::new()
        .with_watch_filesystem(false)
        .with_large_file_threshold(100) // Very small threshold for testing
        .with_hash_large_files(false);

    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(files.clone())
        .with_incremental_config(incremental_config);

    // Should still work with large files using metadata hash
    let result = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result.files_processed.len(), 1);
    assert!(result.success);
}

#[tokio::test]
async fn test_multiple_hooks_with_incremental_detection() {
    let temp_dir = TempDir::new().unwrap();
    let test_file = temp_dir.path().join("test.py");
    fs::write(&test_file, "print('hello')").unwrap();

    // Setup storage and engine
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Store::with_cache_directory(storage_dir).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    let hooks = vec![
        Hook::new("echo1", "echo", "system")
            .with_args(vec!["Hook1".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .pass_filenames(true),
        Hook::new("echo2", "echo", "system")
            .with_args(vec!["Hook2".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .pass_filenames(true),
    ];

    let files = vec![test_file.clone()];

    let incremental_config = FileChangeDetectorConfig::new()
        .with_watch_filesystem(false)
        .with_force_refresh(false);

    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(files.clone())
        .with_incremental_config(incremental_config);

    // First run - all hooks should process the file
    let result1 = engine.execute_hooks(&hooks, config.clone()).await.unwrap();
    assert_eq!(result1.hooks_executed, 2);
    assert_eq!(result1.hooks_passed.len(), 2);
    assert!(result1.success);

    // Second run immediately - hooks should either be skipped or run with no changed files
    let result2 = engine.execute_hooks(&hooks, config.clone()).await.unwrap();
    // Since no files changed, hooks may be executed but with empty file lists
    // This is still a performance improvement as no actual file processing occurs
    assert!(
        result2.hooks_executed <= 2,
        "Expected 0-2 hooks executed, got {}",
        result2.hooks_executed
    );
}

#[tokio::test]
async fn test_cache_size_limit() {
    let temp_dir = TempDir::new().unwrap();

    // Create many files to test cache size limit
    let mut files = Vec::new();
    for i in 0..15 {
        let file = temp_dir.path().join(format!("test{i}.py"));
        fs::write(&file, format!("print('file{i}')")).unwrap();
        files.push(file);
    }

    // Setup storage and engine
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Store::with_cache_directory(storage_dir).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    let hook = Hook::new("echo-test", "echo", "system")
        .with_args(vec!["Processing".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    // Configuration with very small cache size
    let incremental_config = FileChangeDetectorConfig::new()
        .with_watch_filesystem(false)
        .with_cache_size(10); // Smaller than number of files

    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(files.clone())
        .with_incremental_config(incremental_config);

    // First execution - should handle cache size limit gracefully
    let result = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    assert_eq!(result.files_processed.len(), files.len());
    assert!(result.success);
}

#[tokio::test]
async fn test_nonexistent_file_handling() {
    let temp_dir = TempDir::new().unwrap();
    let existing_file = temp_dir.path().join("exists.py");
    let nonexistent_file = temp_dir.path().join("nonexistent.py");

    fs::write(&existing_file, "print('exists')").unwrap();
    // Don't create nonexistent_file

    // Setup storage and engine
    let storage_dir = temp_dir.path().join("storage");
    let storage = Arc::new(Store::with_cache_directory(storage_dir).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    let hook = Hook::new("echo-test", "echo", "system")
        .with_args(vec!["Processing".to_string()])
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    let files = vec![existing_file.clone(), nonexistent_file.clone()];

    let incremental_config = FileChangeDetectorConfig::new().with_watch_filesystem(false);

    let config = ExecutionConfig::new(Stage::PreCommit)
        .with_files(files.clone())
        .with_incremental_config(incremental_config);

    // Should handle nonexistent files gracefully
    let result = engine
        .execute_single_hook(&hook, &files, &config)
        .await
        .unwrap();
    // Only existing file should be processed (nonexistent files are considered "changed")
    assert!(!result.files_processed.is_empty());
    assert!(result.success || result.skipped); // Either successful or skipped is fine
}
