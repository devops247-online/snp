// Integration tests for validation module
use snp::{SchemaValidator, ValidationConfig};
use std::path::Path;

#[test]
fn test_validate_real_config_file() {
    let mut validator = SchemaValidator::new(ValidationConfig::default());

    // Test with the project's .pre-commit-config.yaml
    let config_path = Path::new(".pre-commit-config.yaml");
    if config_path.exists() {
        let result = validator.validate_file(config_path);

        // The file should be parseable (may have warnings but should not fail parsing)
        println!("Validation result for .pre-commit-config.yaml:");
        println!("Valid: {}", result.is_valid);
        println!("Errors: {}", result.errors.len());
        println!("Warnings: {}", result.warnings.len());

        if !result.errors.is_empty() {
            println!("\nErrors:");
            for error in &result.errors {
                println!("  - [{}] {}", error.field_path, error.message);
            }
        }
    }
}

#[test]
fn test_validate_yaml_string() {
    let mut validator = SchemaValidator::new(ValidationConfig::default());

    let yaml = r#"
repos:
- repo: https://github.com/psf/black
  rev: "23.1.0"
  hooks:
  - id: black
    entry: black --check --diff
    language: python
    types: [python]
    stages: [pre-commit]
"#;

    let result = validator.validate_yaml(yaml);

    assert!(result.is_valid, "Valid YAML should pass validation");
    assert!(
        result.errors.is_empty(),
        "No errors expected for valid config"
    );
}

#[test]
fn test_validate_invalid_yaml() {
    let mut validator = SchemaValidator::new(ValidationConfig::default());

    let yaml = r#"
repos:
- repo: ""
  rev: "v1.0.0"
  hooks:
  - id: ""
    entry: ""
    language: "unknown-language"
    stages: [invalid-stage]
"#;

    let result = validator.validate_yaml(yaml);

    assert!(!result.is_valid, "Invalid YAML should fail validation");
    assert!(
        !result.errors.is_empty(),
        "Errors expected for invalid config"
    );

    // Check that we have the expected error types
    let has_empty_id_error = result
        .errors
        .iter()
        .any(|e| e.message.contains("Hook ID cannot be empty"));
    let has_empty_entry_error = result
        .errors
        .iter()
        .any(|e| e.message.contains("Hook entry cannot be empty"));
    let has_invalid_stage_error = result
        .errors
        .iter()
        .any(|e| e.message.contains("Invalid stage"));

    assert!(has_empty_id_error, "Should detect empty hook ID");
    assert!(has_empty_entry_error, "Should detect empty hook entry");
    assert!(has_invalid_stage_error, "Should detect invalid stage");
}
