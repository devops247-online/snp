// Comprehensive tests for validation commands (validate-config and validate-manifest)
use assert_cmd::Command;
use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

/// Test helper to create a temporary config file
fn create_temp_config(content: &str) -> (TempDir, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");
    fs::write(&config_path, content).expect("Failed to write config file");
    (temp_dir, config_path.to_string_lossy().to_string())
}

/// Test helper to create a temporary manifest file
fn create_temp_manifest(content: &str) -> (TempDir, String) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let manifest_path = temp_dir.path().join(".pre-commit-hooks.yaml");
    fs::write(&manifest_path, content).expect("Failed to write manifest file");
    (temp_dir, manifest_path.to_string_lossy().to_string())
}

#[test]
fn test_validate_config_command_valid_yaml() {
    let valid_config = r#"
repos:
  - repo: https://github.com/psf/black
    rev: 23.1.0
    hooks:
      - id: black
        language: python
        entry: black
        types: [python]
"#;

    let (_temp_dir, config_path) = create_temp_config(valid_config);

    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg(&config_path)
        .assert()
        .success(); // Should exit with code 0
}

#[test]
fn test_validate_config_command_invalid_yaml_syntax() {
    let invalid_config = r#"
repos:
  - repo: https://github.com/psf/black
    rev: 23.1.0
    hooks:
      - id: black
        language: python
        entry: black
        types: [python
        # Missing closing bracket - invalid YAML
"#;

    let (_temp_dir, config_path) = create_temp_config(invalid_config);

    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg(&config_path)
        .assert()
        .failure() // Should exit with code 1
        .stderr(predicate::str::contains("YAML"));
}

#[test]
fn test_validate_config_command_invalid_schema() {
    let invalid_config = r#"
repos:
  - repo: https://github.com/psf/black
    # Missing required 'rev' field
    hooks:
      - id: black
        # Missing required 'entry' field
        language: python
"#;

    let (_temp_dir, config_path) = create_temp_config(invalid_config);

    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg(&config_path)
        .assert()
        .failure(); // Should exit with code 1
}

#[test]
fn test_validate_config_command_nonexistent_file() {
    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg("nonexistent-config.yaml")
        .assert()
        .failure(); // Should exit with code 1
}

#[test]
fn test_validate_manifest_command_valid_manifest() {
    let valid_manifest = r#"
- id: black
  name: Black
  description: 'The uncompromising Python code formatter'
  entry: black
  language: python
  language_version: python3
  types: [python]
- id: black-jupyter
  name: Black (Jupyter)
  description: 'The uncompromising Python code formatter (Jupyter)'
  entry: black
  language: python
  language_version: python3
  files: \.ipynb$
  types_or: [python, jupyter]
"#;

    let (_temp_dir, manifest_path) = create_temp_manifest(valid_manifest);

    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-manifest")
        .arg(&manifest_path)
        .assert()
        .success(); // Should exit with code 0
}

#[test]
fn test_validate_manifest_command_missing_required_fields() {
    let invalid_manifest = r#"
- id: black
  # Missing required 'name' field
  # Missing required 'entry' field
  language: python
  types: [python]
"#;

    let (_temp_dir, manifest_path) = create_temp_manifest(invalid_manifest);

    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-manifest")
        .arg(&manifest_path)
        .assert()
        .failure(); // Should exit with code 1
}

#[test]
fn test_validate_manifest_command_nonexistent_file() {
    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-manifest")
        .arg("nonexistent-manifest.yaml")
        .assert()
        .failure(); // Should exit with code 1
}

#[test]
fn test_validation_commands_exit_codes() {
    // Test that exit codes match Python pre-commit behavior
    
    // Valid config should return 0
    let valid_config = r#"
repos:
  - repo: https://github.com/psf/black
    rev: 23.1.0
    hooks:
      - id: black
        language: python
        entry: black
"#;
    let (_temp_dir, config_path) = create_temp_config(valid_config);
    
    let output = Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg(&config_path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0));
    
    // Invalid config should return 1
    let invalid_config = r#"
repos:
  - repo: invalid-repo
    # Missing rev
    hooks:
      - id: test
        # Missing entry
        language: system
"#;
    let (_temp_dir2, invalid_path) = create_temp_config(invalid_config);
    
    let output2 = Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg(&invalid_path)
        .output()
        .unwrap();
    assert_eq!(output2.status.code(), Some(1));
}

#[test]
fn test_validation_commands_meta_and_local_repos() {
    // Test with meta repository
    let meta_config = r#"
repos:
  - repo: meta
    hooks:
      - id: check-hooks-apply
      - id: check-useless-excludes
"#;
    
    let (_temp_dir, config_path) = create_temp_config(meta_config);
    
    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg(&config_path)
        .assert()
        .success(); // Meta hooks should be valid
    
    // Test with local repository
    let local_config = r#"
repos:
  - repo: local
    hooks:
      - id: local-test
        name: Local Test
        entry: ./test.sh
        language: system
        files: \.py$
"#;
    
    let (_temp_dir2, local_path) = create_temp_config(local_config);
    
    Command::cargo_bin("snp")
        .unwrap()
        .arg("validate-config")
        .arg(&local_path)
        .assert()
        .success(); // Local hooks should be valid
}