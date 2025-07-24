// Initialize template directory command tests
// Tests setup of git template directories with pre-commit hooks

use serial_test::serial;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use tempfile::tempdir;

use snp::commands::init_templatedir::{execute_init_templatedir_command, InitTemplateDirConfig};
use snp::error::SnpError;

// =============================================================================
// INIT-TEMPLATEDIR COMMAND TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_template_dir_initialization() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec!["pre-commit".to_string(), "pre-push".to_string()],
        allow_missing_config: true,
        config_file: None,
    };

    let result = execute_init_templatedir_command(&config).await;

    assert!(result.is_ok());
    let init_result = result.unwrap();

    // Should create template directory structure
    assert!(init_result.template_dir_created);
    assert!(template_dir.join("hooks").exists());
    assert!(init_result
        .hooks_installed
        .contains(&"pre-commit".to_string()));
    assert!(init_result
        .hooks_installed
        .contains(&"pre-push".to_string()));
}

#[tokio::test]
#[serial]
async fn test_hook_template_creation() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec!["pre-commit".to_string()],
        allow_missing_config: true,
        config_file: None,
    };

    let result = execute_init_templatedir_command(&config).await;

    assert!(result.is_ok());

    // Should create hook script files
    let pre_commit_hook = template_dir.join("hooks").join("pre-commit");
    assert!(pre_commit_hook.exists());

    // Hook script should be executable
    let metadata = fs::metadata(&pre_commit_hook).unwrap();
    assert!(metadata.permissions().mode() & 0o111 != 0);

    // Should contain SNP hook implementation
    let hook_content = fs::read_to_string(&pre_commit_hook).unwrap();
    assert!(hook_content.contains("snp"));
}

#[tokio::test]
#[serial]
async fn test_template_permissions() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec!["pre-commit".to_string()],
        allow_missing_config: true,
        config_file: None,
    };

    let result = execute_init_templatedir_command(&config).await;

    assert!(result.is_ok());
    let init_result = result.unwrap();

    // Should set proper permissions
    assert!(init_result.permissions_set);

    let hooks_dir = template_dir.join("hooks");
    let hooks_metadata = fs::metadata(&hooks_dir).unwrap();
    assert!(hooks_metadata.is_dir());
    assert!(hooks_metadata.permissions().mode() & 0o755 != 0);
}

#[tokio::test]
#[serial]
async fn test_template_dir_validation() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec!["pre-commit".to_string()],
        allow_missing_config: true,
        config_file: None,
    };

    let result = execute_init_templatedir_command(&config).await;

    assert!(result.is_ok());
    let init_result = result.unwrap();

    // Should validate template directory structure
    assert!(init_result.validation_success);
    assert!(init_result.git_config_warning.is_some()); // Should warn about init.templateDir
}

#[tokio::test]
#[serial]
async fn test_multiple_hook_types() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec![
            "pre-commit".to_string(),
            "pre-push".to_string(),
            "commit-msg".to_string(),
        ],
        allow_missing_config: true,
        config_file: None,
    };

    let result = execute_init_templatedir_command(&config).await;

    assert!(result.is_ok());
    let init_result = result.unwrap();

    // Should install all requested hook types
    assert_eq!(init_result.hooks_installed.len(), 3);
    assert!(init_result
        .hooks_installed
        .contains(&"pre-commit".to_string()));
    assert!(init_result
        .hooks_installed
        .contains(&"pre-push".to_string()));
    assert!(init_result
        .hooks_installed
        .contains(&"commit-msg".to_string()));

    // All hook files should exist
    let hooks_dir = template_dir.join("hooks");
    assert!(hooks_dir.join("pre-commit").exists());
    assert!(hooks_dir.join("pre-push").exists());
    assert!(hooks_dir.join("commit-msg").exists());
}

#[tokio::test]
#[serial]
async fn test_permission_error_handling() {
    // Test init-templatedir with permission issues
    let config = InitTemplateDirConfig {
        template_directory: PathBuf::from("/root/template"), // Should be inaccessible
        hook_types: vec!["pre-commit".to_string()],
        allow_missing_config: true,
        config_file: None,
    };

    let result = execute_init_templatedir_command(&config).await;

    // Should handle permission errors gracefully
    assert!(result.is_err());
    match result.unwrap_err() {
        SnpError::Io(_) | SnpError::Filesystem(_) => {
            // Expected error types for permission issues
        }
        _ => panic!("Expected filesystem error"),
    }
}

#[tokio::test]
#[serial]
async fn test_existing_template_dir_overwrite() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");

    // Create existing template directory
    fs::create_dir_all(template_dir.join("hooks")).unwrap();
    fs::write(template_dir.join("hooks").join("pre-commit"), "old hook").unwrap();

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec!["pre-commit".to_string()],
        allow_missing_config: true,
        config_file: None,
    };

    let result = execute_init_templatedir_command(&config).await;

    assert!(result.is_ok());
    let init_result = result.unwrap();

    // Should overwrite existing hooks
    assert!(init_result.template_dir_created);

    // New hook content should be different
    let hook_content = fs::read_to_string(template_dir.join("hooks").join("pre-commit")).unwrap();
    assert!(hook_content != "old hook");
    assert!(hook_content.contains("snp"));
}

#[tokio::test]
#[serial]
async fn test_config_file_integration() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");
    let config_file = temp_dir.path().join(".pre-commit-config.yaml");

    // Create a config file
    fs::write(
        &config_file,
        r#"repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.0.0
  hooks:
  - id: trailing-whitespace
  - id: end-of-file-fixer
"#,
    )
    .unwrap();

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec!["pre-commit".to_string()],
        allow_missing_config: false,
        config_file: Some(config_file),
    };

    let result = execute_init_templatedir_command(&config).await;

    assert!(result.is_ok());
    let init_result = result.unwrap();

    // Should work with config file present
    assert!(init_result.template_dir_created);
    assert!(init_result.validation_success);
}

#[tokio::test]
#[serial]
async fn test_missing_config_file_handling() {
    let temp_dir = tempdir().unwrap();
    let template_dir = temp_dir.path().join("git-template");
    let missing_config = temp_dir.path().join("nonexistent-config.yaml");

    let config = InitTemplateDirConfig {
        template_directory: template_dir.clone(),
        hook_types: vec!["pre-commit".to_string()],
        allow_missing_config: false,
        config_file: Some(missing_config),
    };

    let result = execute_init_templatedir_command(&config).await;

    // Should fail when config is required but missing
    assert!(result.is_err());
    match result.unwrap_err() {
        SnpError::Config(_) => {
            // Expected error type for missing config
        }
        _ => panic!("Expected config error"),
    }
}
