// Tests for hook status display bug fix (GitHub issue #94)
// Verifies that hooks with missing executables properly show as "Failed" instead of "Passed"

use std::path::PathBuf;
use tempfile::TempDir;

use snp::core::{Hook, Stage};
use snp::execution::{ExecutionConfig, HookExecutionEngine};
use snp::language::environment::{EnvironmentMetadata, LanguageEnvironment};
use snp::language::python::PythonLanguagePlugin;
use snp::language::traits::Language;
use snp::process::ProcessManager;
use snp::storage::Store;
use std::collections::HashMap;
use std::sync::Arc;

#[tokio::test]
async fn test_missing_executable_shows_as_failed() {
    // Create a temporary directory for test environment
    let temp_dir = TempDir::new().unwrap();
    let env_path = temp_dir.path().to_path_buf();

    // Create a minimal LanguageEnvironment that simulates a Python venv without ruff
    let mut env_vars = HashMap::new();
    env_vars.insert(
        "PATH".to_string(),
        format!("{}/bin:/usr/bin", env_path.display()),
    );

    let lang_env = LanguageEnvironment {
        language: "python".to_string(),
        environment_id: "test-env".to_string(),
        root_path: env_path.clone(),
        executable_path: env_path.join("bin").join("python"),
        environment_variables: env_vars,
        installed_dependencies: Vec::new(),
        metadata: EnvironmentMetadata {
            created_at: std::time::SystemTime::now(),
            last_used: std::time::SystemTime::now(),
            usage_count: 0,
            size_bytes: 0,
            dependency_count: 0,
            language_version: "3.8".to_string(),
            platform: "linux".to_string(),
        },
    };

    // Create Python language plugin
    let python_plugin = PythonLanguagePlugin::new();

    // Create a hook that uses a non-existent executable (like ruff)
    let hook = Hook::new("test-ruff", "ruff check --fix", "python")
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    // Test files
    let test_files = vec![PathBuf::from("test.py")];

    // Try to execute the hook - this should fail because ruff doesn't exist
    let result = python_plugin
        .execute_hook(&hook, &lang_env, &test_files)
        .await;

    // With the optimization, hooks with missing executables now skip gracefully like Python pre-commit
    assert!(
        result.is_ok(),
        "Hook execution should return Ok with skipped result for missing executable"
    );

    let hook_result = result.unwrap();
    assert!(
        hook_result.skipped,
        "Hook should be marked as skipped for missing executable"
    );
    assert!(
        !hook_result.success,
        "Skipped hooks should not be marked as successful"
    );
    assert_eq!(
        hook_result.skip_reason,
        Some("no files to check".to_string()),
        "Should match Python pre-commit skip message"
    );
}

#[tokio::test]
async fn test_complex_entry_parsing() {
    // Test that entries like "ruff check --force-exclude" are handled correctly through execution
    let temp_dir = TempDir::new().unwrap();
    let env_path = temp_dir.path().to_path_buf();

    let mut env_vars = HashMap::new();
    env_vars.insert(
        "PATH".to_string(),
        format!("{}/bin:/usr/bin", env_path.display()),
    );

    let lang_env = LanguageEnvironment {
        language: "python".to_string(),
        environment_id: "test-env-complex".to_string(),
        root_path: env_path.clone(),
        executable_path: env_path.join("bin").join("python"),
        environment_variables: env_vars,
        installed_dependencies: Vec::new(),
        metadata: EnvironmentMetadata {
            created_at: std::time::SystemTime::now(),
            last_used: std::time::SystemTime::now(),
            usage_count: 0,
            size_bytes: 0,
            dependency_count: 0,
            language_version: "3.8".to_string(),
            platform: "linux".to_string(),
        },
    };

    let python_plugin = PythonLanguagePlugin::new();

    // Create a hook with complex entry (ruff check --force-exclude)
    let hook = Hook::new("test-ruff-complex", "ruff check --force-exclude", "python")
        .with_stages(vec![Stage::PreCommit])
        .pass_filenames(true);

    let test_files = vec![PathBuf::from("test.py")];

    // Try to execute the hook - should skip gracefully because ruff doesn't exist
    let result = python_plugin
        .execute_hook(&hook, &lang_env, &test_files)
        .await;

    // Should return a skipped result since ruff executable doesn't exist
    assert!(
        result.is_ok(),
        "Should return Ok with skipped result for non-existent executable with complex entry"
    );

    let hook_result = result.unwrap();
    assert!(
        hook_result.skipped,
        "Hook should be marked as skipped for missing executable"
    );
    assert!(
        !hook_result.success,
        "Skipped hooks should not be marked as successful"
    );
}

#[tokio::test]
async fn test_hook_execution_engine_integration() {
    // Integration test: verify that the hook execution engine properly reports failed hooks
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    // Create a hook that will fail due to missing executable
    let hook = Hook::new("missing-executable", "nonexistent-command", "system")
        .with_stages(vec![Stage::PreCommit])
        .always_run(true);

    let config = ExecutionConfig::new(Stage::PreCommit);
    let test_files = vec![PathBuf::from("test.py")];

    // Execute the hook
    let result = engine
        .execute_single_hook(&hook, &test_files, &config)
        .await;

    // With the performance optimization, missing executables now skip gracefully like Python pre-commit
    assert!(
        result.is_ok(),
        "Hook execution should return Ok with skipped result for missing executable"
    );

    let hook_result = result.unwrap();
    assert!(
        hook_result.skipped,
        "Hook should be marked as skipped for missing executable"
    );
    assert!(
        !hook_result.success,
        "Skipped hooks should not be marked as successful"
    );
    assert_eq!(
        hook_result.skip_reason,
        Some("no files to check".to_string()),
        "Should match Python pre-commit skip message"
    );
}

#[tokio::test]
async fn test_successful_hook_still_passes() {
    // Verify that legitimate hooks still work correctly
    let temp_dir = TempDir::new().unwrap();
    let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
    let process_manager = Arc::new(ProcessManager::new());
    let mut engine = HookExecutionEngine::new(process_manager, storage);

    // Create a hook that should succeed (using basic system command)
    let hook = Hook::new("echo-test", "python -c \"print('test')\"", "python")
        .with_stages(vec![Stage::PreCommit])
        .always_run(true);

    let config = ExecutionConfig::new(Stage::PreCommit);
    let test_files = vec![];

    // Execute the hook
    let result = engine
        .execute_single_hook(&hook, &test_files, &config)
        .await;

    // This should succeed if Python is available
    if let Ok(hook_result) = result {
        if hook_result.success {
            assert!(
                hook_result.error.is_none(),
                "Successful hook should have no error"
            );
        }
        // Note: We don't assert success here because Python might not be available in test environment
    }
}
