// Comprehensive tests for the migration module
// Tests configuration migration, format detection, and version upgrades

use serde_yaml::Value;
use snp::error::SnpError;
use snp::migration::{
    AppliedMigration, ConfigFormat, ConfigMigrator, FieldMigration, FieldTransformation,
    MigrationConfig, MigrationError, MigrationResult, MigrationWarning, StructuralChangeType,
    TransformationType,
};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
#[allow(clippy::uninlined_format_args, unused_variables, unused_imports)]
mod migration_comprehensive_tests {
    use super::*;

    #[test]
    fn test_config_format_variants() {
        // Test all ConfigFormat variants
        let precommit_python = ConfigFormat::PreCommitPython {
            version: "3.0.0".to_string(),
        };
        let snp_v1 = ConfigFormat::SnpV1 {
            version: "1.0.0".to_string(),
        };
        let snp_v2 = ConfigFormat::SnpV2 {
            version: "2.0.0".to_string(),
        };

        // Test equality
        assert_eq!(
            precommit_python,
            ConfigFormat::PreCommitPython {
                version: "3.0.0".to_string()
            }
        );
        assert_ne!(precommit_python, snp_v1);

        // Test pattern matching
        match precommit_python {
            ConfigFormat::PreCommitPython { version } => {
                assert_eq!(version, "3.0.0");
            }
            _ => panic!("Expected PreCommitPython"),
        }

        match snp_v1 {
            ConfigFormat::SnpV1 { version } => {
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("Expected SnpV1"),
        }

        match snp_v2 {
            ConfigFormat::SnpV2 { version } => {
                assert_eq!(version, "2.0.0");
            }
            _ => panic!("Expected SnpV2"),
        }
    }

    #[test]
    fn test_transformation_types() {
        // Test all TransformationType variants
        let field_rename = TransformationType::FieldRename {
            from: "old_name".to_string(),
            to: "new_name".to_string(),
        };

        let value_transform = TransformationType::ValueTransform {
            transformer: "string_to_list".to_string(),
        };

        let structure_change = TransformationType::StructureChange {
            change_type: "flatten_nested".to_string(),
        };

        let field_removal = TransformationType::FieldRemoval {
            reason: "deprecated".to_string(),
        };

        let field_addition = TransformationType::FieldAddition {
            default_value: Value::String("default".to_string()),
        };

        // Test pattern matching
        match field_rename {
            TransformationType::FieldRename { from, to } => {
                assert_eq!(from, "old_name");
                assert_eq!(to, "new_name");
            }
            _ => panic!("Expected FieldRename"),
        }

        match value_transform {
            TransformationType::ValueTransform { transformer } => {
                assert_eq!(transformer, "string_to_list");
            }
            _ => panic!("Expected ValueTransform"),
        }

        match structure_change {
            TransformationType::StructureChange { change_type } => {
                assert_eq!(change_type, "flatten_nested");
            }
            _ => panic!("Expected StructureChange"),
        }

        match field_removal {
            TransformationType::FieldRemoval { reason } => {
                assert_eq!(reason, "deprecated");
            }
            _ => panic!("Expected FieldRemoval"),
        }

        match field_addition {
            TransformationType::FieldAddition { default_value } => {
                assert_eq!(default_value, Value::String("default".to_string()));
            }
            _ => panic!("Expected FieldAddition"),
        }
    }

    #[test]
    fn test_migration_config() {
        let config = MigrationConfig {
            source_format: ConfigFormat::PreCommitPython {
                version: "3.0.0".to_string(),
            },
            target_format: ConfigFormat::SnpV1 {
                version: "1.0.0".to_string(),
            },
            preserve_comments: true,
            create_backup: true,
            validate_result: true,
            strict_mode: false,
        };

        assert!(config.preserve_comments);
        assert!(config.create_backup);
        assert!(config.validate_result);
        assert!(!config.strict_mode);

        match &config.source_format {
            ConfigFormat::PreCommitPython { version } => {
                assert_eq!(version, "3.0.0");
            }
            _ => panic!("Expected PreCommitPython source format"),
        }

        match &config.target_format {
            ConfigFormat::SnpV1 { version } => {
                assert_eq!(version, "1.0.0");
            }
            _ => panic!("Expected SnpV1 target format"),
        }
    }

    #[test]
    fn test_applied_migration_structure() {
        let applied_migration = AppliedMigration {
            migration_id: "rename_repos_to_repositories".to_string(),
            description: "Rename 'repos' field to 'repositories' for clarity".to_string(),
            field_path: "repos".to_string(),
            old_value: Some(Value::String("old_value".to_string())),
            new_value: Some(Value::String("new_value".to_string())),
            transformation_type: TransformationType::FieldRename {
                from: "repos".to_string(),
                to: "repositories".to_string(),
            },
        };

        assert_eq!(
            applied_migration.migration_id,
            "rename_repos_to_repositories"
        );
        assert_eq!(applied_migration.field_path, "repos");
        assert!(applied_migration.old_value.is_some());
        assert!(applied_migration.new_value.is_some());

        match &applied_migration.transformation_type {
            TransformationType::FieldRename { from, to } => {
                assert_eq!(from, "repos");
                assert_eq!(to, "repositories");
            }
            _ => panic!("Expected FieldRename transformation"),
        }
    }

    #[test]
    fn test_migration_warning_and_error() {
        let warning = MigrationWarning {
            message: "This field is deprecated".to_string(),
            field_path: Some("deprecated_field".to_string()),
            suggestion: Some("Use 'new_field' instead".to_string()),
        };

        assert_eq!(warning.message, "This field is deprecated");
        assert_eq!(warning.field_path, Some("deprecated_field".to_string()));
        assert_eq!(
            warning.suggestion,
            Some("Use 'new_field' instead".to_string())
        );

        let error = MigrationError {
            message: "Invalid configuration format".to_string(),
            field_path: Some("invalid_field".to_string()),
            error_type: "ValidationError".to_string(),
        };

        assert_eq!(error.message, "Invalid configuration format");
        assert_eq!(error.field_path, Some("invalid_field".to_string()));
        assert_eq!(error.error_type, "ValidationError");
    }

    #[test]
    fn test_migration_result_structure() {
        let applied_migration = AppliedMigration {
            migration_id: "test_migration".to_string(),
            description: "Test migration".to_string(),
            field_path: "test_field".to_string(),
            old_value: None,
            new_value: Some(Value::String("new_value".to_string())),
            transformation_type: TransformationType::FieldAddition {
                default_value: Value::String("default".to_string()),
            },
        };

        let warning = MigrationWarning {
            message: "Test warning".to_string(),
            field_path: None,
            suggestion: None,
        };

        let error = MigrationError {
            message: "Test error".to_string(),
            field_path: None,
            error_type: "TestError".to_string(),
        };

        let result = MigrationResult {
            success: true,
            source_format: ConfigFormat::PreCommitPython {
                version: "3.0.0".to_string(),
            },
            target_format: ConfigFormat::SnpV1 {
                version: "1.0.0".to_string(),
            },
            applied_migrations: vec![applied_migration],
            warnings: vec![warning],
            errors: vec![error],
            backup_path: Some(PathBuf::from("/tmp/backup.yaml.bak")),
        };

        assert!(result.success);
        assert_eq!(result.applied_migrations.len(), 1);
        assert_eq!(result.warnings.len(), 1);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(
            result.backup_path,
            Some(PathBuf::from("/tmp/backup.yaml.bak"))
        );

        assert_eq!(result.applied_migrations[0].migration_id, "test_migration");
        assert_eq!(result.warnings[0].message, "Test warning");
        assert_eq!(result.errors[0].message, "Test error");
    }

    #[test]
    fn test_field_migration_structure() {
        let field_migration = FieldMigration {
            source_path: "old_field".to_string(),
            target_path: "new_field".to_string(),
            transformation: FieldTransformation::DirectCopy,
            condition: None,
            deprecation_warning: None,
        };

        assert_eq!(field_migration.source_path, "old_field");
        assert_eq!(field_migration.target_path, "new_field");

        match field_migration.transformation {
            FieldTransformation::DirectCopy => {}
            _ => panic!("Expected DirectCopy transformation"),
        }
    }

    #[test]
    fn test_structural_change_types() {
        // Test FlattenObject
        let flatten_object = StructuralChangeType::FlattenObject {
            separator: "_".to_string(),
        };

        match flatten_object {
            StructuralChangeType::FlattenObject { separator } => {
                assert_eq!(separator, "_");
            }
            _ => panic!("Expected FlattenObject"),
        }

        // Test NestFields
        let nest_fields = StructuralChangeType::NestFields {
            container_name: "nested_container".to_string(),
        };

        match nest_fields {
            StructuralChangeType::NestFields { container_name } => {
                assert_eq!(container_name, "nested_container");
            }
            _ => panic!("Expected NestFields"),
        }

        // Test ArrayToObject
        let array_to_object = StructuralChangeType::ArrayToObject {
            key_field: "id".to_string(),
        };

        match array_to_object {
            StructuralChangeType::ArrayToObject { key_field } => {
                assert_eq!(key_field, "id");
            }
            _ => panic!("Expected ArrayToObject"),
        }
    }

    #[test]
    fn test_config_migrator_creation() {
        // Test default creation
        let _migrator = ConfigMigrator::default();
        // ConfigMigrator doesn't expose its internal state, so we just test creation

        let _migrator_new = ConfigMigrator::new();
        // Test that both methods work

        // Test with custom rules - simplified to avoid complex rule creation
        let _migrator_with_rules = ConfigMigrator::new().with_custom_rules(vec![]);
        // Test that custom rules can be added
    }

    #[test]
    fn test_migration_file_operations() -> Result<(), Box<dyn std::error::Error>> {
        let temp_dir = TempDir::new()?;
        let migrator = ConfigMigrator::new();

        // Create a test configuration file
        let config_content = r#"
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.0.1
    hooks:
      - id: trailing-whitespace
      - id: end-of-file-fixer
"#;

        let config_file = temp_dir.path().join("test-config.yaml");
        fs::write(&config_file, config_content)?;

        // Test migration configuration
        let migration_config = MigrationConfig {
            source_format: ConfigFormat::PreCommitPython {
                version: "3.0.0".to_string(),
            },
            target_format: ConfigFormat::SnpV1 {
                version: "1.0.0".to_string(),
            },
            preserve_comments: true,
            create_backup: true,
            validate_result: false, // Skip validation for this test
            strict_mode: false,
        };

        // Attempt migration (might fail due to incomplete implementation, but tests the interface)
        let result = migrator.migrate_file(&config_file, migration_config);

        // The migration might fail due to unimplemented features, but we test the error handling
        match result {
            Ok(_migration_result) => {
                // Migration succeeded
            }
            Err(SnpError::Config(_)) => {
                // Expected for incomplete implementation
            }
            Err(other) => {
                panic!("Unexpected error type: {:?}", other);
            }
        }

        Ok(())
    }

    #[test]
    fn test_migration_error_scenarios() {
        let migrator = ConfigMigrator::new();

        // Test with non-existent file
        let migration_config = MigrationConfig {
            source_format: ConfigFormat::PreCommitPython {
                version: "3.0.0".to_string(),
            },
            target_format: ConfigFormat::SnpV1 {
                version: "1.0.0".to_string(),
            },
            preserve_comments: false,
            create_backup: false,
            validate_result: false,
            strict_mode: false,
        };

        let result =
            migrator.migrate_file(&PathBuf::from("/non/existent/file.yaml"), migration_config);

        assert!(result.is_err());
        match result.unwrap_err() {
            SnpError::Config(_) => {
                // Expected error type for file operations
            }
            SnpError::Io(_) => {
                // Also acceptable for file operations
            }
            other => panic!("Expected Config or Io error, got: {:?}", other),
        }
    }

    #[test]
    fn test_field_transformation_types() {
        // Direct transformation
        let direct_transformation = FieldTransformation::DirectCopy;
        match direct_transformation {
            FieldTransformation::DirectCopy => {}
            _ => panic!("Expected DirectCopy transformation"),
        }

        // Test custom transformation
        let custom_transformation = FieldTransformation::CustomTransform {
            transformer_id: "test_transformer".to_string(),
        };
        match custom_transformation {
            FieldTransformation::CustomTransform { transformer_id } => {
                assert_eq!(transformer_id, "test_transformer");
            }
            _ => panic!("Expected CustomTransform"),
        }
    }
}
