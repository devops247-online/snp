// Hook implementation template generation command tests
// Tests generation of hook implementation templates for users

use serial_test::serial;
use std::fs;
use tempfile::tempdir;

use snp::commands::hook_impl::{execute_hook_impl_command, HookImplConfig};

// =============================================================================
// HOOK-IMPL COMMAND TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_hook_impl_template_generation() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "python".to_string(),
        hook_type: "linter".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "my-custom-hook".to_string(),
        description: Some("A custom linting hook".to_string()),
    };

    let result = execute_hook_impl_command(&config).await;

    assert!(result.is_ok());
    let impl_result = result.unwrap();

    // Should generate implementation template
    assert!(impl_result.template_generated);
    assert!(impl_result.files_created.contains(&"setup.py".to_string()));
    assert!(impl_result
        .files_created
        .contains(&"my_custom_hook.py".to_string()));
    assert!(impl_result
        .files_created
        .contains(&".pre-commit-hooks.yaml".to_string()));
}

#[tokio::test]
#[serial]
async fn test_language_specific_templates() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "rust".to_string(),
        hook_type: "formatter".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "rust-formatter".to_string(),
        description: None,
    };

    let result = execute_hook_impl_command(&config).await;

    assert!(result.is_ok());
    let impl_result = result.unwrap();

    // Should generate Rust-specific files
    assert!(impl_result
        .files_created
        .contains(&"Cargo.toml".to_string()));
    assert!(impl_result
        .files_created
        .contains(&"src/main.rs".to_string()));
    assert!(impl_result.template_language == "rust");
}

#[tokio::test]
#[serial]
async fn test_hook_scaffold_creation() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "python".to_string(),
        hook_type: "checker".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "security-checker".to_string(),
        description: Some("Security vulnerability checker".to_string()),
    };

    let result = execute_hook_impl_command(&config).await;

    assert!(result.is_ok());
    let _impl_result = result.unwrap();

    // Should create complete project scaffold
    assert!(temp_dir.path().join("setup.py").exists());
    assert!(temp_dir.path().join("README.md").exists());
    assert!(temp_dir.path().join("tests").exists());
    assert!(temp_dir.path().join(".pre-commit-hooks.yaml").exists());

    // README should contain description
    let readme_content = fs::read_to_string(temp_dir.path().join("README.md")).unwrap();
    assert!(readme_content.contains("Security vulnerability checker"));
}

#[tokio::test]
#[serial]
async fn test_test_template_generation() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "python".to_string(),
        hook_type: "linter".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "pylint-hook".to_string(),
        description: None,
    };

    let result = execute_hook_impl_command(&config).await;

    assert!(result.is_ok());
    let impl_result = result.unwrap();

    // Should include test templates
    assert!(impl_result
        .files_created
        .contains(&"tests/test_pylint_hook.py".to_string()));
    assert!(temp_dir.path().join("tests/test_pylint_hook.py").exists());

    let test_content =
        fs::read_to_string(temp_dir.path().join("tests/test_pylint_hook.py")).unwrap();
    assert!(test_content.contains("def test_"));
}

#[tokio::test]
#[serial]
async fn test_implementation_validation() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "python".to_string(),
        hook_type: "formatter".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "black-formatter".to_string(),
        description: None,
    };

    let result = execute_hook_impl_command(&config).await;

    assert!(result.is_ok());
    let impl_result = result.unwrap();

    // Generated implementation should be valid
    assert!(impl_result.validation_success);

    // .pre-commit-hooks.yaml should be valid
    let hooks_yaml_path = temp_dir.path().join(".pre-commit-hooks.yaml");
    let hooks_content = fs::read_to_string(&hooks_yaml_path).unwrap();
    let _hooks: Vec<serde_yaml::Value> = serde_yaml::from_str(&hooks_content).unwrap();
}

#[tokio::test]
#[serial]
async fn test_go_language_template() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "go".to_string(),
        hook_type: "linter".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "go-linter".to_string(),
        description: Some("Go linting hook".to_string()),
    };

    let result = execute_hook_impl_command(&config).await;

    assert!(result.is_ok());
    let impl_result = result.unwrap();

    // Should generate Go-specific files
    assert!(impl_result.files_created.contains(&"go.mod".to_string()));
    assert!(impl_result.files_created.contains(&"main.go".to_string()));
    assert!(impl_result.template_language == "go");
}

#[tokio::test]
#[serial]
async fn test_nodejs_language_template() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "node".to_string(),
        hook_type: "formatter".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "prettier-hook".to_string(),
        description: None,
    };

    let result = execute_hook_impl_command(&config).await;

    assert!(result.is_ok());
    let impl_result = result.unwrap();

    // Should generate Node.js-specific files
    assert!(impl_result
        .files_created
        .contains(&"package.json".to_string()));
    assert!(impl_result.files_created.contains(&"index.js".to_string()));
    assert!(impl_result.template_language == "node");
}

#[tokio::test]
#[serial]
async fn test_unsupported_language_handling() {
    let temp_dir = tempdir().unwrap();

    let config = HookImplConfig {
        language: "unsupported-language".to_string(),
        hook_type: "linter".to_string(),
        output_directory: temp_dir.path().to_path_buf(),
        hook_name: "test-hook".to_string(),
        description: None,
    };

    let result = execute_hook_impl_command(&config).await;

    // Should handle unsupported languages gracefully
    assert!(result.is_err());
    match result.unwrap_err() {
        snp::error::SnpError::Config(_) => {
            // Expected error type for unsupported languages
        }
        _ => panic!("Expected config error"),
    }
}
