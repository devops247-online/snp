// Try repository command tests
// Tests hooks from a repository without adding to config

use serial_test::serial;
use std::path::Path;

use snp::commands::try_repo::{execute_try_repo_command, TryRepoConfig};
use snp::error::SnpError;

// =============================================================================
// TRY-REPO COMMAND TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_try_repo_basic_functionality() {
    // Test basic try-repo command with public repository

    let config = TryRepoConfig {
        repository_url: "https://github.com/pre-commit/pre-commit-hooks".to_string(),
        revision: None,
        hook_id: None,
        files: vec![],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    // Should succeed and execute hooks
    assert!(result.is_ok());
    let try_result = result.unwrap();
    assert!(try_result.repository_cloned);
    assert!(!try_result.hooks_executed.is_empty());
    assert!(try_result.temp_cleanup_success);
}

#[tokio::test]
#[serial]
async fn test_try_repo_specific_revision() {
    let config = TryRepoConfig {
        repository_url: "https://github.com/pre-commit/pre-commit-hooks".to_string(),
        revision: Some("v3.4.0".to_string()),
        hook_id: None,
        files: vec![],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    assert!(result.is_ok());
    let try_result = result.unwrap();
    assert_eq!(try_result.revision_used, "v3.4.0");
}

#[tokio::test]
#[serial]
async fn test_try_repo_specific_hook() {
    let config = TryRepoConfig {
        repository_url: "https://github.com/pre-commit/pre-commit-hooks".to_string(),
        revision: None,
        hook_id: Some("trailing-whitespace".to_string()),
        files: vec![],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    assert!(result.is_ok());
    let try_result = result.unwrap();
    assert_eq!(try_result.hooks_executed.len(), 1);
    assert_eq!(try_result.hooks_executed[0].hook_id, "trailing-whitespace");
}

#[tokio::test]
#[serial]
async fn test_try_repo_custom_files() {
    let config = TryRepoConfig {
        repository_url: "https://github.com/pre-commit/pre-commit-hooks".to_string(),
        revision: None,
        hook_id: None,
        files: vec!["test.py".to_string(), "README.md".to_string()],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    assert!(result.is_ok());
    let try_result = result.unwrap();
    assert_eq!(try_result.files_processed, vec!["test.py", "README.md"]);
}

#[tokio::test]
#[serial]
async fn test_try_repo_cleanup() {
    let config = TryRepoConfig {
        repository_url: "https://github.com/pre-commit/pre-commit-hooks".to_string(),
        revision: None,
        hook_id: None,
        files: vec![],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    assert!(result.is_ok());
    let try_result = result.unwrap();

    // Verify temporary directory was cleaned up
    assert!(try_result.temp_cleanup_success);
    assert!(!Path::new(&try_result.temp_directory_path).exists());
}

#[tokio::test]
#[serial]
async fn test_try_repo_invalid_repository() {
    let config = TryRepoConfig {
        repository_url: "https://github.com/nonexistent/invalid-repo".to_string(),
        revision: None,
        hook_id: None,
        files: vec![],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    // Should fail with git error
    assert!(result.is_err());
    match result.unwrap_err() {
        SnpError::Git(git_err) => {
            let error_message = git_err.to_string();
            // Should be either "Repository not found" or "Git command failed"
            assert!(
                error_message.contains("Repository not found")
                    || error_message.contains("Git command failed")
            );
        }
        _ => panic!("Expected Git error"),
    }
}

#[tokio::test]
#[serial]
async fn test_try_repo_network_error_handling() {
    // Test try-repo with network issues
    let config = TryRepoConfig {
        repository_url: "https://invalid-host-that-does-not-exist.com/repo".to_string(),
        revision: None,
        hook_id: None,
        files: vec![],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    // Should handle network errors gracefully
    assert!(result.is_err());
    match result.unwrap_err() {
        SnpError::Git(_) | SnpError::Network(_) => {
            // Expected error types for network issues
        }
        _ => panic!("Expected network or git error"),
    }
}

#[tokio::test]
#[serial]
async fn test_try_repo_invalid_url_handling() {
    // Test try-repo with malformed repository URL
    let config = TryRepoConfig {
        repository_url: "not-a-valid-url".to_string(),
        revision: None,
        hook_id: None,
        files: vec![],
        all_files: false,
        verbose: false,
    };

    let result = execute_try_repo_command(&config).await;

    // Should handle invalid URLs gracefully
    assert!(result.is_err());
    match result.unwrap_err() {
        SnpError::Config(_) | SnpError::Git(_) => {
            // Expected error types for invalid URLs
        }
        _ => panic!("Expected config or git error"),
    }
}
