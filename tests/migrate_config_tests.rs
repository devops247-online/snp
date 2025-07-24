// Configuration migration command tests
// Tests migration between different configuration formats/versions

use serial_test::serial;
use std::fs;
use tempfile::tempdir;

use snp::commands::migrate_config::{execute_migrate_config_command, MigrateConfigConfig};
use snp::config::Config;

// =============================================================================
// MIGRATE-CONFIG COMMAND TESTS
// =============================================================================

#[tokio::test]
#[serial]
async fn test_config_version_detection() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    // Create old format config
    let old_config = r#"# Old format configuration
- repo: https://github.com/pre-commit/pre-commit-hooks
  sha: v3.4.0
  hooks:
  - id: trailing-whitespace
"#;
    fs::write(&config_path, old_config).unwrap();

    let config = MigrateConfigConfig {
        config_file: config_path.clone(),
        from_version: None,
        backup: true,
        dry_run: false,
    };

    let result = execute_migrate_config_command(&config).await;

    assert!(result.is_ok());
    let migrate_result = result.unwrap();

    // Should detect old format and need migration
    assert!(migrate_result.migration_needed);
    assert_eq!(migrate_result.from_version, "legacy-list");
    assert_eq!(migrate_result.to_version, "current");
}

#[tokio::test]
#[serial]
async fn test_config_migration() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    // Create config with old field names
    let old_config = r#"repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  sha: v3.4.0
  hooks:
  - id: trailing-whitespace
    language: python_venv
    stages: [commit]
"#;
    fs::write(&config_path, old_config).unwrap();

    let config = MigrateConfigConfig {
        config_file: config_path.clone(),
        from_version: None,
        backup: true,
        dry_run: false,
    };

    let result = execute_migrate_config_command(&config).await;

    assert!(result.is_ok());
    let migrate_result = result.unwrap();
    // Should migrate sha -> rev, python_venv -> python, commit -> pre-commit
    assert!(migrate_result.migration_applied);

    let new_content = fs::read_to_string(&config_path).unwrap();
    assert!(new_content.contains("rev: v3.4.0"));
    assert!(!new_content.contains("sha:"));
    assert!(new_content.contains("language: python"));
    assert!(!new_content.contains("python_venv"));
    assert!(new_content.contains("pre-commit"));
    assert!(!new_content.contains("stages: [commit]"));
}

#[tokio::test]
#[serial]
async fn test_migration_backup() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");
    let backup_path = temp_dir.path().join(".pre-commit-config.yaml.bak");

    let old_config = r#"repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.0.0
  hooks:
  - id: trailing-whitespace
  - id: end-of-file-fixer
"#
    .to_string();
    fs::write(&config_path, &old_config).unwrap();

    let config = MigrateConfigConfig {
        config_file: config_path.clone(),
        from_version: None,
        backup: true,
        dry_run: false,
    };

    let result = execute_migrate_config_command(&config).await;

    assert!(result.is_ok());
    let migrate_result = result.unwrap();

    // Should create backup if migration applied
    if migrate_result.migration_applied {
        assert!(backup_path.exists());
        assert_eq!(
            migrate_result.backup_location,
            Some(backup_path.to_string_lossy().to_string())
        );

        let backup_content = fs::read_to_string(&backup_path).unwrap();
        assert_eq!(backup_content, old_config);
    }
}

#[tokio::test]
#[serial]
async fn test_migration_dry_run() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let old_config = r#"repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  sha: v3.4.0
  hooks:
  - id: trailing-whitespace
"#;
    fs::write(&config_path, old_config).unwrap();

    let config = MigrateConfigConfig {
        config_file: config_path.clone(),
        from_version: None,
        backup: false,
        dry_run: true,
    };

    let result = execute_migrate_config_command(&config).await;

    assert!(result.is_ok());
    let migrate_result = result.unwrap();

    // Should preview changes without applying them
    assert!(migrate_result.migration_needed);
    assert!(!migrate_result.migration_applied);
    assert!(migrate_result.preview_changes.is_some());

    // Original file should be unchanged
    let unchanged_content = fs::read_to_string(&config_path).unwrap();
    assert!(unchanged_content.contains("sha: v3.4.0"));
}

#[tokio::test]
#[serial]
async fn test_migration_validation() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    let old_config = r#"repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.0.0
  hooks:
  - id: trailing-whitespace
  - id: end-of-file-fixer
"#
    .to_string();
    fs::write(&config_path, old_config).unwrap();

    let config = MigrateConfigConfig {
        config_file: config_path.clone(),
        from_version: None,
        backup: false,
        dry_run: false,
    };

    let result = execute_migrate_config_command(&config).await;

    assert!(result.is_ok());
    let migrate_result = result.unwrap();

    // Migrated config should be valid
    assert!(migrate_result.validation_success);

    // Should be parseable by our config system
    let new_content = fs::read_to_string(&config_path).unwrap();
    let _config: Config = serde_yaml::from_str(&new_content).unwrap();
}

#[tokio::test]
#[serial]
async fn test_corrupted_config_handling() {
    // Test migrate-config with corrupted YAML
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    // Create invalid YAML
    fs::write(&config_path, "invalid: yaml: content: [").unwrap();

    let config = MigrateConfigConfig {
        config_file: config_path,
        from_version: None,
        backup: false,
        dry_run: false,
    };

    let result = execute_migrate_config_command(&config).await;

    // Should handle corrupted YAML gracefully
    assert!(result.is_err());
    match result.unwrap_err() {
        snp::error::SnpError::Config(_) => {
            // Expected error type for corrupted configs
        }
        _ => panic!("Expected config error"),
    }
}

#[tokio::test]
#[serial]
async fn test_no_migration_needed() {
    let temp_dir = tempdir().unwrap();
    let config_path = temp_dir.path().join(".pre-commit-config.yaml");

    // Create already-current format config
    let current_config = r#"repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.0.0
  hooks:
  - id: trailing-whitespace
    language: python
    stages: [pre-commit]
"#;
    fs::write(&config_path, current_config).unwrap();

    let config = MigrateConfigConfig {
        config_file: config_path.clone(),
        from_version: None,
        backup: false,
        dry_run: false,
    };

    let result = execute_migrate_config_command(&config).await;

    assert!(result.is_ok());
    let migrate_result = result.unwrap();

    // Should detect no migration needed
    assert!(!migrate_result.migration_needed);
    assert!(!migrate_result.migration_applied);
    assert!(migrate_result.backup_location.is_none());
}
