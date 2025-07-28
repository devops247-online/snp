// Integration tests for the complete resource pool system
// Validates end-to-end functionality of resource pools with real hook execution

use clap::Parser;
use snp::cli::{Cli, Commands};
use snp::execution::{ExecutionConfig, HookExecutionEngine};
use snp::process::ProcessManager;
use snp::storage::Store;
use std::path::PathBuf;
use std::sync::Arc;
use tempfile::TempDir;

/// Test that resource pool integration doesn't break basic functionality
#[tokio::test]
async fn test_resource_pool_basic_integration() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());

    let execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Get pool stats to ensure pool manager is initialized
    let stats = execution_engine.get_resource_pool_stats().await;

    // Should start with no pools
    assert_eq!(stats.git_pools.len(), 0);
    assert_eq!(stats.language_pools.len(), 0);
    assert_eq!(stats.total_git_resources, 0);
    assert_eq!(stats.total_language_resources, 0);
}

/// Test that pooled execution can handle empty hook lists gracefully
#[tokio::test]
async fn test_pooled_execution_empty_hooks() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());

    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    let hooks = vec![];
    let config = ExecutionConfig::new(snp::core::Stage::PreCommit);

    // Execute with pools should handle empty hook list
    let result = execution_engine
        .execute_hooks_with_pools(&hooks, config)
        .await;
    assert!(result.is_ok());

    let execution_result = result.unwrap();
    assert!(execution_result.success);
    assert_eq!(execution_result.hooks_executed, 0);
    assert_eq!(execution_result.hooks_passed.len(), 0);
    assert_eq!(execution_result.hooks_failed.len(), 0);
}

/// Test that resource pools can be acquired and returned properly
#[tokio::test]
async fn test_resource_pool_acquisition() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());

    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Create a mock hook that would use language pools
    let mut hook = snp::core::Hook::new("test-hook", "echo 'test'", "python");
    hook.additional_dependencies = vec!["requests".to_string()];

    let hooks = vec![hook];
    let config = ExecutionConfig::new(snp::core::Stage::PreCommit)
        .with_files(vec![PathBuf::from("test.py")]);

    // Execute with pools - this should create and use pools
    let result = execution_engine
        .execute_hooks_with_pools(&hooks, config)
        .await;
    assert!(result.is_ok());

    // Check that pools were used
    let stats = execution_engine.get_resource_pool_stats().await;
    assert!(!stats.language_pools.is_empty());
}

/// Test CLI parsing with --use-pools flag
#[test]
fn test_cli_use_pools_flag() {
    let cli = Cli::try_parse_from(["snp", "run", "--use-pools", "--all-files"]).unwrap();

    match cli.command {
        Some(Commands::Run {
            use_pools,
            all_files,
            ..
        }) => {
            assert!(use_pools);
            assert!(all_files);
        }
        _ => panic!("Expected Run command with use_pools=true"),
    }
}

/// Test that pool statistics are properly collected
#[tokio::test]
async fn test_pool_statistics_collection() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());

    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Initial stats should be empty
    let initial_stats = execution_engine.get_resource_pool_stats().await;
    assert_eq!(initial_stats.total_git_resources, 0);
    assert_eq!(initial_stats.total_language_resources, 0);

    // Create hooks that will use different types of pools
    let python_hook = snp::core::Hook::new("python-hook", "echo 'python'", "python");
    let nodejs_hook = snp::core::Hook::new("nodejs-hook", "echo 'nodejs'", "nodejs");

    let hooks = vec![python_hook, nodejs_hook];
    let config = ExecutionConfig::new(snp::core::Stage::PreCommit)
        .with_files(vec![PathBuf::from("test.py"), PathBuf::from("test.js")]);

    // Execute with pools
    let _result = execution_engine
        .execute_hooks_with_pools(&hooks, config)
        .await;

    // Stats should show pool usage
    let final_stats = execution_engine.get_resource_pool_stats().await;
    // At minimum, we should have language pools created
    assert!(!final_stats.language_pools.is_empty());
}

/// Test pool resource grouping logic
#[tokio::test]
async fn test_pool_resource_grouping() {
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());

    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Create multiple hooks with the same language but different dependencies
    let mut hook1 = snp::core::Hook::new("hook1", "echo 'hook1'", "python");
    hook1.additional_dependencies = vec!["requests".to_string()];

    let mut hook2 = snp::core::Hook::new("hook2", "echo 'hook2'", "python");
    hook2.additional_dependencies = vec!["requests".to_string()]; // Same dependencies

    let mut hook3 = snp::core::Hook::new("hook3", "echo 'hook3'", "python");
    hook3.additional_dependencies = vec!["flask".to_string()]; // Different dependencies

    let hooks = vec![hook1, hook2, hook3];
    let config = ExecutionConfig::new(snp::core::Stage::PreCommit)
        .with_files(vec![PathBuf::from("test.py")]);

    // Execute with pools - should efficiently group hooks with similar dependencies
    let result = execution_engine
        .execute_hooks_with_pools(&hooks, config)
        .await;
    assert!(result.is_ok());

    let execution_result = result.unwrap();
    assert_eq!(execution_result.hooks_executed, 3);

    // Check that language pools were created for Python
    let stats = execution_engine.get_resource_pool_stats().await;
    assert!(stats.language_pools.keys().any(|k| k.contains("python")));
}
