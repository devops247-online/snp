// Sample configuration command tests
// Tests generation of sample .pre-commit-config.yaml files

use serial_test::serial;
use std::fs;
use tempfile::tempdir;

use snp::commands::sample_config::{execute_sample_config_command, SampleConfigConfig};
use snp::config::Config;

// =============================================================================
// SAMPLE-CONFIG COMMAND TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_sample_config_generation() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let config = SampleConfigConfig {
        language: None,
        hook_type: None,
        output_file: Some(config_path),
    };

    let result = execute_sample_config_command(&config).await;

    assert!(result.is_ok());
    let sample_result = result.unwrap();

    // Should generate basic sample configuration
    assert!(sample_result.config_generated.contains("repos:"));
    assert!(sample_result.config_generated.contains("pre-commit-hooks"));
    assert!(sample_result
        .config_generated
        .contains("trailing-whitespace"));
    assert!(sample_result.language_detected.is_none());
}

#[tokio::test]
#[serial]
async fn test_language_specific_config() {
    let config = SampleConfigConfig {
        language: Some("python".to_string()),
        hook_type: None,
        output_file: None,
    };

    let result = execute_sample_config_command(&config).await;

    assert!(result.is_ok());
    let sample_result = result.unwrap();

    // Should include Python-specific hooks
    assert!(sample_result.config_generated.contains("black"));
    assert!(sample_result.config_generated.contains("flake8"));
    assert!(sample_result.language_detected == Some("python".to_string()));
}

#[tokio::test]
#[serial]
async fn test_hook_type_specific_config() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let config = SampleConfigConfig {
        language: None,
        hook_type: Some("pre-push".to_string()),
        output_file: Some(config_path),
    };

    let result = execute_sample_config_command(&config).await;

    assert!(result.is_ok());
    let sample_result = result.unwrap();

    // Should generate pre-push specific configuration
    assert!(sample_result.config_generated.contains("stages:"));
    assert!(sample_result.config_generated.contains("pre-push"));
}

#[tokio::test]
#[serial]
async fn test_project_language_detection() {
    let temp_dir = tempdir().unwrap();

    // Create Python project files
    fs::write(temp_dir.path().join("setup.py"), "# Python setup").unwrap();
    fs::write(temp_dir.path().join("main.py"), "print('hello')").unwrap();

    let config = SampleConfigConfig {
        language: None,
        hook_type: None,
        output_file: Some(temp_dir.path().join(".pre-commit-config.yaml")),
    };

    let result = execute_sample_config_command(&config).await;

    assert!(result.is_ok());
    let sample_result = result.unwrap();

    // Should detect Python and suggest appropriate hooks
    assert_eq!(sample_result.language_detected, Some("python".to_string()));
    assert!(sample_result.config_generated.contains("black"));
}

#[tokio::test]
#[serial]
async fn test_config_template_validation() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let config = SampleConfigConfig {
        language: None,
        hook_type: None,
        output_file: Some(config_path),
    };

    let result = execute_sample_config_command(&config).await;

    assert!(result.is_ok());
    let sample_result = result.unwrap();

    // Generated config should be valid YAML
    let parsed_config: serde_yaml::Value =
        serde_yaml::from_str(&sample_result.config_generated).unwrap();
    assert!(parsed_config.get("repos").is_some());

    // Should be parseable by our config system
    let _config: Config = serde_yaml::from_str(&sample_result.config_generated).unwrap();
}

#[tokio::test]
#[serial]
async fn test_rust_language_specific_config() {
    let config = SampleConfigConfig {
        language: Some("rust".to_string()),
        hook_type: None,
        output_file: None,
    };

    let result = execute_sample_config_command(&config).await;

    assert!(result.is_ok());
    let sample_result = result.unwrap();

    // Should include Rust-specific hooks (using fmt hook id, not rustfmt)
    assert!(sample_result.config_generated.contains("fmt"));
    assert!(sample_result.config_generated.contains("clippy"));
    assert!(sample_result.language_detected == Some("rust".to_string()));
}

#[tokio::test]
#[serial]
async fn test_output_file_creation() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let config = SampleConfigConfig {
        language: None,
        hook_type: None,
        output_file: Some(config_path.clone()),
    };

    let result = execute_sample_config_command(&config).await;

    assert!(result.is_ok());
    let sample_result = result.unwrap();

    // Should create the output file
    assert!(config_path.exists());
    assert_eq!(sample_result.output_location, config_path.to_string_lossy());

    // File should contain the generated config
    let file_content = fs::read_to_string(&config_path).unwrap();
    assert_eq!(file_content, sample_result.config_generated);
}
