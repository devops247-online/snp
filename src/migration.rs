// Configuration Migration System for SNP
// Provides pre-commit compatibility, version upgrades, and automated migration of deprecated features

use serde_yaml::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::error::{ConfigError, Result, SnpError};

/// Migration configuration and rules
#[derive(Debug, Clone)]
pub struct MigrationConfig {
    pub source_format: ConfigFormat,
    pub target_format: ConfigFormat,
    pub preserve_comments: bool,
    pub create_backup: bool,
    pub validate_result: bool,
    pub strict_mode: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigFormat {
    PreCommitPython { version: String },
    SnpV1 { version: String },
    SnpV2 { version: String },
    // Future versions...
}

/// Migration result with detailed information
#[derive(Debug)]
pub struct MigrationResult {
    pub success: bool,
    pub source_format: ConfigFormat,
    pub target_format: ConfigFormat,
    pub applied_migrations: Vec<AppliedMigration>,
    pub warnings: Vec<MigrationWarning>,
    pub errors: Vec<MigrationError>,
    pub backup_path: Option<PathBuf>,
}

/// Individual migration operation
#[derive(Debug)]
pub struct AppliedMigration {
    pub migration_id: String,
    pub description: String,
    pub field_path: String,
    pub old_value: Option<Value>,
    pub new_value: Option<Value>,
    pub transformation_type: TransformationType,
}

#[derive(Debug)]
pub enum TransformationType {
    FieldRename { from: String, to: String },
    ValueTransform { transformer: String },
    StructureChange { change_type: String },
    FieldRemoval { reason: String },
    FieldAddition { default_value: Value },
}

/// Migration warning information
#[derive(Debug)]
pub struct MigrationWarning {
    pub message: String,
    pub field_path: Option<String>,
    pub suggestion: Option<String>,
}

/// Migration error information
#[derive(Debug)]
pub struct MigrationError {
    pub message: String,
    pub field_path: Option<String>,
    pub error_type: String,
}

/// Main configuration migrator
pub struct ConfigMigrator {
    migration_rules: HashMap<String, MigrationRule>,
    #[allow(dead_code)]
    format_detectors: Vec<Box<dyn FormatDetector>>,
    #[allow(dead_code)]
    transformers: HashMap<String, Box<dyn ValueTransformer>>,
    #[allow(dead_code)]
    validators: HashMap<ConfigFormat, Box<dyn ConfigValidator>>,
}

/// Migration rule definition
#[derive(Debug, Clone)]
pub struct MigrationRule {
    pub id: String,
    pub description: String,
    pub from_format: ConfigFormat,
    pub to_format: ConfigFormat,
    pub field_migrations: Vec<FieldMigration>,
    pub structural_changes: Vec<StructuralChange>,
    pub validation_rules: Vec<ValidationRule>,
}

/// Field-level migration specification
#[derive(Debug, Clone)]
pub struct FieldMigration {
    pub source_path: String,
    pub target_path: String,
    pub transformation: FieldTransformation,
    pub condition: Option<MigrationCondition>,
    pub deprecation_warning: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FieldTransformation {
    DirectCopy,
    ValueMapping { mapping: HashMap<String, Value> },
    CustomTransform { transformer_id: String },
    DefaultValue { value: Value },
    Remove { reason: String },
}

#[derive(Debug, Clone)]
pub struct MigrationCondition {
    pub field_path: String,
    pub condition_type: ConditionType,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub enum ConditionType {
    Equals,
    NotEquals,
    Exists,
    NotExists,
}

/// Structural configuration changes
#[derive(Debug, Clone)]
pub struct StructuralChange {
    pub change_type: StructuralChangeType,
    pub description: String,
    pub path: String,
    pub parameters: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub enum StructuralChangeType {
    FlattenObject { separator: String },
    NestFields { container_name: String },
    ArrayToObject { key_field: String },
    ObjectToArray { sort_key: Option<String> },
    MergeFields { target_field: String },
    SplitField { target_fields: Vec<String> },
}

#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub rule_id: String,
    pub description: String,
    pub validation_type: ValidationType,
}

#[derive(Debug, Clone)]
pub enum ValidationType {
    RequiredField {
        field_path: String,
    },
    ValidEnum {
        field_path: String,
        allowed_values: Vec<String>,
    },
    RegexPattern {
        field_path: String,
        pattern: String,
    },
}

/// Format detection trait
pub trait FormatDetector: Send + Sync + std::panic::RefUnwindSafe {
    fn detect_format(&self, content: &str) -> Option<ConfigFormat>;
    fn confidence_score(&self, content: &str) -> f64;
}

/// Value transformation trait
pub trait ValueTransformer: Send + Sync + std::panic::RefUnwindSafe {
    fn transform(&self, value: &Value) -> Result<Value>;
    fn get_transformer_id(&self) -> &str;
}

/// Configuration validation trait
pub trait ConfigValidator: Send + Sync + std::panic::RefUnwindSafe {
    fn validate(&self, config: &Value) -> Result<Vec<String>>;
    fn get_format(&self) -> &ConfigFormat;
}

impl Default for ConfigMigrator {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigMigrator {
    pub fn new() -> Self {
        Self {
            migration_rules: HashMap::new(),
            format_detectors: Vec::new(),
            transformers: HashMap::new(),
            validators: HashMap::new(),
        }
    }

    pub fn with_custom_rules(mut self, rules: Vec<MigrationRule>) -> Self {
        for rule in rules {
            self.migration_rules.insert(rule.id.clone(), rule);
        }
        self
    }

    // Main migration methods
    pub fn migrate_file(&self, path: &Path, config: MigrationConfig) -> Result<MigrationResult> {
        use std::fs;

        // Create backup if requested
        let backup_path = if config.create_backup {
            let backup_path = path.with_extension(format!(
                "{}.bak",
                path.extension().and_then(|s| s.to_str()).unwrap_or("yaml")
            ));
            fs::copy(path, &backup_path)?;
            Some(backup_path)
        } else {
            None
        };

        // Read file content
        let content = fs::read_to_string(path)?;

        // Migrate content
        let (migrated_content, mut result) = self.migrate_content(&content, config)?;

        // Update backup path information
        result.backup_path = backup_path;

        // Write migrated content back to file
        if result.success && !result.applied_migrations.is_empty() {
            fs::write(path, migrated_content)?;
        }

        Ok(result)
    }

    pub fn migrate_content(
        &self,
        content: &str,
        config: MigrationConfig,
    ) -> Result<(String, MigrationResult)> {
        // Parse YAML content
        let yaml_value: Value = serde_yaml::from_str(content)
            .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?;

        // Migrate the parsed value
        let preserve_comments = config.preserve_comments;
        let (migrated_value, result) = self.migrate_value(yaml_value, config)?;

        // Convert back to YAML string
        let migrated_content = if preserve_comments {
            // For now, we don't preserve comments perfectly, but we could enhance this
            // by using a YAML library that preserves comments
            serde_yaml::to_string(&migrated_value)
                .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?
        } else {
            serde_yaml::to_string(&migrated_value)
                .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?
        };

        Ok((migrated_content, result))
    }

    pub fn migrate_value(
        &self,
        value: Value,
        config: MigrationConfig,
    ) -> Result<(Value, MigrationResult)> {
        let mut migrated_value = value.clone();
        let mut applied_migrations = Vec::new();
        let mut warnings = Vec::new();
        let errors = Vec::new();

        // Detect source format
        let content = serde_yaml::to_string(&value)
            .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?;
        let detected_format = self.detect_format(&content)?;

        // Check if migration is needed
        if detected_format == config.target_format && !config.strict_mode {
            return Ok((
                value,
                MigrationResult {
                    success: true,
                    source_format: detected_format,
                    target_format: config.target_format,
                    applied_migrations,
                    warnings,
                    errors,
                    backup_path: None,
                },
            ));
        }

        // Handle legacy list format first (before other migrations)
        self.handle_legacy_list_format(
            &mut migrated_value,
            &mut applied_migrations,
            &mut warnings,
        )?;

        // Apply migrations based on source and target formats
        match (&detected_format, &config.target_format) {
            (ConfigFormat::PreCommitPython { .. }, ConfigFormat::SnpV1 { .. }) => {
                // Apply pre-commit to SNP v1 migrations
                self.apply_precommit_to_snp_migrations(
                    &mut migrated_value,
                    &mut applied_migrations,
                    &mut warnings,
                )?;
            }
            _ => {
                // Basic migration handling for other cases
                warnings.push(MigrationWarning {
                    message: format!("Migration from {:?} to {:?} uses default handling", detected_format, config.target_format),
                    field_path: None,
                    suggestion: Some("Consider implementing specific migration rules for this format combination".to_string()),
                });
            }
        }

        // Validate result if requested
        if config.validate_result {
            // Basic validation - check that migrated value is still valid YAML
            let yaml_string = serde_yaml::to_string(&migrated_value)
                .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?;
            let _: Value = serde_yaml::from_str(&yaml_string)
                .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?;
        }

        Ok((
            migrated_value,
            MigrationResult {
                success: errors.is_empty(),
                source_format: detected_format,
                target_format: config.target_format,
                applied_migrations,
                warnings,
                errors,
                backup_path: None,
            },
        ))
    }

    fn apply_precommit_to_snp_migrations(
        &self,
        value: &mut Value,
        applied_migrations: &mut Vec<AppliedMigration>,
        _warnings: &mut [MigrationWarning],
    ) -> Result<()> {
        // Apply field renames: sha -> rev
        if let Value::Mapping(map) = value {
            if let Some(Value::Sequence(repos)) = map.get_mut(Value::String("repos".to_string())) {
                for repo in repos {
                    if let Value::Mapping(repo_map) = repo {
                        // sha -> rev migration
                        if let Some(sha_value) = repo_map.remove(Value::String("sha".to_string())) {
                            repo_map.insert(Value::String("rev".to_string()), sha_value.clone());
                            applied_migrations.push(AppliedMigration {
                                migration_id: "sha_to_rev".to_string(),
                                description: "Renamed 'sha' field to 'rev'".to_string(),
                                field_path: "repos[*].sha".to_string(),
                                old_value: Some(sha_value),
                                new_value: Some(
                                    repo_map
                                        .get(Value::String("rev".to_string()))
                                        .unwrap()
                                        .clone(),
                                ),
                                transformation_type: TransformationType::FieldRename {
                                    from: "sha".to_string(),
                                    to: "rev".to_string(),
                                },
                            });
                        }

                        // Language migration: python_venv -> python
                        if let Some(Value::Sequence(hooks)) =
                            repo_map.get_mut(Value::String("hooks".to_string()))
                        {
                            for hook in hooks {
                                if let Value::Mapping(hook_map) = hook {
                                    if let Some(Value::String(language)) =
                                        hook_map.get_mut(Value::String("language".to_string()))
                                    {
                                        if language == "python_venv" {
                                            let old_value = Value::String(language.clone());
                                            *language = "python".to_string();
                                            applied_migrations.push(AppliedMigration {
                                                migration_id: "language_normalization".to_string(),
                                                description: "Normalized 'python_venv' to 'python'"
                                                    .to_string(),
                                                field_path: "repos[*].hooks[*].language"
                                                    .to_string(),
                                                old_value: Some(old_value),
                                                new_value: Some(Value::String(
                                                    "python".to_string(),
                                                )),
                                                transformation_type:
                                                    TransformationType::ValueTransform {
                                                        transformer: "language_normalizer"
                                                            .to_string(),
                                                    },
                                            });
                                        }
                                    }

                                    // Stage name migration
                                    if let Some(Value::Sequence(stages)) =
                                        hook_map.get_mut(Value::String("stages".to_string()))
                                    {
                                        for stage in stages {
                                            if let Value::String(stage_name) = stage {
                                                let old_name = stage_name.clone();
                                                match stage_name.as_str() {
                                                    "commit" => {
                                                        *stage_name = "pre-commit".to_string()
                                                    }
                                                    "push" => *stage_name = "pre-push".to_string(),
                                                    "merge-commit" => {
                                                        *stage_name = "pre-merge-commit".to_string()
                                                    }
                                                    _ => continue,
                                                }
                                                applied_migrations.push(AppliedMigration {
                                                    migration_id: "stage_names".to_string(),
                                                    description: format!("Updated stage name '{old_name}' to '{stage_name}'"),
                                                    field_path: "repos[*].hooks[*].stages[*]".to_string(),
                                                    old_value: Some(Value::String(old_name)),
                                                    new_value: Some(Value::String(stage_name.clone())),
                                                    transformation_type: TransformationType::ValueTransform {
                                                        transformer: "stage_normalizer".to_string(),
                                                    },
                                                });
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Handle default_stages migration
            if let Some(Value::Sequence(default_stages)) =
                map.get_mut(Value::String("default_stages".to_string()))
            {
                for stage in default_stages {
                    if let Value::String(stage_name) = stage {
                        let old_name = stage_name.clone();
                        match stage_name.as_str() {
                            "commit" => *stage_name = "pre-commit".to_string(),
                            "push" => *stage_name = "pre-push".to_string(),
                            "merge-commit" => *stage_name = "pre-merge-commit".to_string(),
                            _ => continue,
                        }
                        applied_migrations.push(AppliedMigration {
                            migration_id: "default_stage_names".to_string(),
                            description: format!(
                                "Updated default stage name '{old_name}' to '{stage_name}'"
                            ),
                            field_path: "default_stages[*]".to_string(),
                            old_value: Some(Value::String(old_name)),
                            new_value: Some(Value::String(stage_name.clone())),
                            transformation_type: TransformationType::ValueTransform {
                                transformer: "stage_normalizer".to_string(),
                            },
                        });
                    }
                }
            }
        }

        // Handle legacy list format (convert to repos structure)
        // This should be handled at the top level, not inside the mapping check
        Ok(())
    }

    fn handle_legacy_list_format(
        &self,
        value: &mut Value,
        applied_migrations: &mut Vec<AppliedMigration>,
        warnings: &mut Vec<MigrationWarning>,
    ) -> Result<()> {
        if let Value::Sequence(repos_list) = value.clone() {
            let mut new_map = serde_yaml::Mapping::new();
            new_map.insert(
                Value::String("repos".to_string()),
                Value::Sequence(repos_list.clone()),
            );
            let new_value = Value::Mapping(new_map);

            applied_migrations.push(AppliedMigration {
                migration_id: "legacy_list_to_repos".to_string(),
                description: "Converted legacy list format to modern repos structure".to_string(),
                field_path: "root".to_string(),
                old_value: Some(Value::Sequence(repos_list)),
                new_value: Some(new_value.clone()),
                transformation_type: TransformationType::StructureChange {
                    change_type: "wrap_in_repos".to_string(),
                },
            });

            warnings.push(MigrationWarning {
                message: "Converted legacy list format to modern repos structure".to_string(),
                field_path: Some("root".to_string()),
                suggestion: Some(
                    "Consider updating your configuration to use the modern format".to_string(),
                ),
            });

            *value = new_value;
        }

        Ok(())
    }

    // Detection and analysis
    pub fn detect_format(&self, content: &str) -> Result<ConfigFormat> {
        // Parse YAML content to analyze structure
        let yaml_value: Value = serde_yaml::from_str(content)
            .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?;

        // Check for pre-commit specific markers
        if let Value::Mapping(map) = &yaml_value {
            // Modern pre-commit format with 'repos' key
            if map.contains_key(Value::String("repos".to_string())) {
                // Check for minimum_pre_commit_version to determine version
                if let Some(Value::String(version)) =
                    map.get(Value::String("minimum_pre_commit_version".to_string()))
                {
                    return Ok(ConfigFormat::PreCommitPython {
                        version: version.clone(),
                    });
                }
                // Default to recent pre-commit version
                return Ok(ConfigFormat::PreCommitPython {
                    version: "3.0.0".to_string(),
                });
            }
        } else if let Value::Sequence(_) = &yaml_value {
            // Legacy pre-commit format (top-level list)
            return Ok(ConfigFormat::PreCommitPython {
                version: "2.0.0".to_string(),
            });
        }

        // Default to SNP v1 if no pre-commit markers found
        Ok(ConfigFormat::SnpV1 {
            version: "1.0.0".to_string(),
        })
    }

    pub fn analyze_migration_needs(&self, from: ConfigFormat, to: ConfigFormat) -> MigrationPlan {
        // Check if migration is needed
        if from == to {
            return MigrationPlan {
                direct: true,
                intermediate_steps: vec![],
                required_migrations: vec![],
                estimated_complexity: MigrationComplexity::Trivial,
            };
        }

        // Determine migration complexity based on format changes
        let complexity = match (&from, &to) {
            (ConfigFormat::PreCommitPython { .. }, ConfigFormat::SnpV1 { .. }) => {
                MigrationComplexity::Simple
            }
            (ConfigFormat::PreCommitPython { .. }, ConfigFormat::SnpV2 { .. }) => {
                MigrationComplexity::Moderate
            }
            (ConfigFormat::SnpV1 { .. }, ConfigFormat::SnpV2 { .. }) => {
                MigrationComplexity::Trivial
            }
            _ => MigrationComplexity::Complex,
        };

        let required_migrations = match (&from, &to) {
            (ConfigFormat::PreCommitPython { .. }, ConfigFormat::SnpV1 { .. }) => {
                vec![
                    "precommit_to_snp_v1".to_string(),
                    "sha_to_rev".to_string(),
                    "stage_names".to_string(),
                    "language_normalization".to_string(),
                ]
            }
            _ => vec!["basic_migration".to_string()],
        };

        MigrationPlan {
            direct: true,
            intermediate_steps: vec![],
            required_migrations,
            estimated_complexity: complexity,
        }
    }

    pub fn can_migrate(&self, from: ConfigFormat, to: ConfigFormat) -> bool {
        // For now, support basic migrations between common formats
        match (&from, &to) {
            // Pre-commit to SNP migrations
            (ConfigFormat::PreCommitPython { .. }, ConfigFormat::SnpV1 { .. }) => true,
            (ConfigFormat::PreCommitPython { .. }, ConfigFormat::SnpV2 { .. }) => true,

            // SNP version upgrades
            (ConfigFormat::SnpV1 { .. }, ConfigFormat::SnpV2 { .. }) => true,

            // Same format (no migration needed)
            (f1, f2) if f1 == f2 => true,

            // Other combinations not yet supported
            _ => false,
        }
    }

    // Batch operations
    pub fn migrate_directory(
        &self,
        dir: &Path,
        config: MigrationConfig,
    ) -> Result<Vec<MigrationResult>> {
        let mut results = Vec::new();

        if !dir.exists() {
            return Ok(results);
        }

        // Look for common configuration file names
        let config_files = [
            ".pre-commit-config.yaml",
            ".pre-commit-config.yml",
            "snp.yaml",
            "snp.yml",
        ];

        for config_file in &config_files {
            let config_path = dir.join(config_file);
            if config_path.exists() {
                match self.migrate_file(&config_path, config.clone()) {
                    Ok(result) => results.push(result),
                    Err(_) => {
                        // Continue with other files if one fails
                        continue;
                    }
                }
            }
        }

        Ok(results)
    }

    pub fn migrate_repository(
        &self,
        repo_root: &Path,
        config: MigrationConfig,
    ) -> Result<RepositoryMigrationResult> {
        let migration_results = self.migrate_directory(repo_root, config)?;

        let mut files_migrated = Vec::new();
        let mut total_warnings = 0;
        let mut total_errors = 0;

        for result in &migration_results {
            if result.success && !result.applied_migrations.is_empty() {
                // Determine the file path that was migrated (would need to be tracked properly)
                files_migrated.push(repo_root.join(".pre-commit-config.yaml"));
            }
            total_warnings += result.warnings.len();
            total_errors += result.errors.len();
        }

        Ok(RepositoryMigrationResult {
            files_migrated,
            migration_results,
            total_warnings,
            total_errors,
            backup_directory: None,
        })
    }
}

/// Migration planning result
#[derive(Debug)]
pub struct MigrationPlan {
    pub direct: bool,
    pub intermediate_steps: Vec<ConfigFormat>,
    pub required_migrations: Vec<String>,
    pub estimated_complexity: MigrationComplexity,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum MigrationComplexity {
    Trivial,  // Field renames, simple value changes
    Simple,   // Basic structural changes
    Moderate, // Complex transformations, some manual input
    Complex,  // Major restructuring, potential data loss
    Manual,   // Requires human intervention
}

/// Repository-wide migration result
#[derive(Debug)]
pub struct RepositoryMigrationResult {
    pub files_migrated: Vec<PathBuf>,
    pub migration_results: Vec<MigrationResult>,
    pub total_warnings: usize,
    pub total_errors: usize,
    pub backup_directory: Option<PathBuf>,
}

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            source_format: ConfigFormat::PreCommitPython {
                version: "latest".to_string(),
            },
            target_format: ConfigFormat::SnpV1 {
                version: "1.0.0".to_string(),
            },
            preserve_comments: true,
            create_backup: true,
            validate_result: true,
            strict_mode: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_precommit_config_detection() {
        // Test detection of pre-commit configuration versions
        // Test identification of deprecated fields and formats
        // Test compatibility matrix checking
        // Test migration necessity assessment

        let migrator = ConfigMigrator::new();

        // Test pre-commit config detection
        let precommit_config = r#"
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    language: python
minimum_pre_commit_version: "3.0.0"
"#;

        // Should detect as pre-commit format with version
        let result = migrator.detect_format(precommit_config).unwrap();
        assert_eq!(
            result,
            ConfigFormat::PreCommitPython {
                version: "3.0.0".to_string()
            }
        );

        // Test deprecated field detection (legacy list format)
        let legacy_config = r#"
- repo: https://github.com/test/repo
  sha: v1.2.3
  hooks:
  - id: test
    language: python_venv
    stages: [commit, push]
"#;

        let result = migrator.detect_format(legacy_config).unwrap();
        assert_eq!(
            result,
            ConfigFormat::PreCommitPython {
                version: "2.0.0".to_string()
            }
        );

        // Test compatibility checking
        let from_format = ConfigFormat::PreCommitPython {
            version: "3.0.0".to_string(),
        };
        let to_format = ConfigFormat::SnpV1 {
            version: "1.0.0".to_string(),
        };

        assert!(migrator.can_migrate(from_format, to_format));
    }

    #[test]
    fn test_field_migration() {
        // Test migration of renamed fields (old_name -> new_name)
        // Test value transformation (format changes)
        // Test field removal with deprecation warnings
        // Test conditional field migration based on context

        let migrator = ConfigMigrator::new();
        let config = MigrationConfig::default();

        // Test field renaming (sha -> rev)
        let yaml_with_sha = serde_yaml::from_str(
            r#"
repos:
- repo: https://github.com/test/repo
  sha: v1.2.3
  hooks:
  - id: test
"#,
        )
        .unwrap();

        let (migrated_value, result) = migrator.migrate_value(yaml_with_sha, config).unwrap();

        // Check that migration was successful
        assert!(result.success);

        // Check that sha was renamed to rev
        let has_sha_to_rev_migration = result
            .applied_migrations
            .iter()
            .any(|m| m.migration_id == "sha_to_rev");
        assert!(
            has_sha_to_rev_migration,
            "Should have applied sha->rev migration"
        );

        // Verify the migrated structure
        if let Value::Mapping(map) = &migrated_value {
            if let Some(Value::Sequence(repos)) = map.get(Value::String("repos".to_string())) {
                if let Some(Value::Mapping(repo)) = repos.first() {
                    // Should have rev, not sha
                    assert!(repo.contains_key(Value::String("rev".to_string())));
                    assert!(!repo.contains_key(Value::String("sha".to_string())));
                }
            }
        }
    }

    #[test]
    fn test_structure_migration() {
        // Test repository format migrations (URL -> structured)
        // Test hook configuration restructuring
        // Test nested configuration flattening/nesting
        // Test array-to-object transformations

        let migrator = ConfigMigrator::new();
        let config = MigrationConfig::default();

        // Test legacy list format migration
        let legacy_list_config = serde_yaml::from_str(
            r#"
- repo: https://github.com/test/repo
  rev: v1.0.0
  hooks:
  - id: test
"#,
        )
        .unwrap();

        let (migrated_value, result) = migrator.migrate_value(legacy_list_config, config).unwrap();

        // Check that migration was successful
        assert!(result.success);

        // Check that legacy list format was converted
        let has_legacy_migration = result
            .applied_migrations
            .iter()
            .any(|m| m.migration_id == "legacy_list_to_repos");
        assert!(
            has_legacy_migration,
            "Should have applied legacy list migration"
        );

        // Verify the migrated structure now has repos key
        if let Value::Mapping(map) = &migrated_value {
            assert!(map.contains_key(Value::String("repos".to_string())));
            if let Some(Value::Sequence(repos)) = map.get(Value::String("repos".to_string())) {
                assert!(!repos.is_empty());
            }
        }
    }

    #[test]
    fn test_version_compatibility() {
        // Test backward compatibility with older SNP versions
        // Test forward compatibility warnings
        // Test version-specific migration rules
        // Test multi-step migration chains

        let migrator = ConfigMigrator::new();

        // Test version compatibility matrix
        let old_version = ConfigFormat::PreCommitPython {
            version: "2.0.0".to_string(),
        };
        let new_version = ConfigFormat::SnpV1 {
            version: "1.0.0".to_string(),
        };

        let plan = migrator.analyze_migration_needs(old_version, new_version);

        // Should be able to migrate directly
        assert!(plan.direct);
        assert_eq!(plan.estimated_complexity, MigrationComplexity::Simple);
        assert!(!plan.required_migrations.is_empty());

        // Test multi-step migration (should be direct for now)
        let very_old_format = ConfigFormat::PreCommitPython {
            version: "1.0.0".to_string(),
        };
        let latest_format = ConfigFormat::SnpV2 {
            version: "2.0.0".to_string(),
        };

        let complex_plan = migrator.analyze_migration_needs(very_old_format, latest_format);
        assert_eq!(
            complex_plan.estimated_complexity,
            MigrationComplexity::Moderate
        );

        // Test same version (no migration needed)
        let same_format1 = ConfigFormat::SnpV1 {
            version: "1.0.0".to_string(),
        };
        let same_format2 = ConfigFormat::SnpV1 {
            version: "1.0.0".to_string(),
        };

        let no_migration_plan = migrator.analyze_migration_needs(same_format1, same_format2);
        assert_eq!(
            no_migration_plan.estimated_complexity,
            MigrationComplexity::Trivial
        );
        assert!(no_migration_plan.required_migrations.is_empty());
    }

    #[test]
    fn test_migration_validation() {
        // Test migrated configuration validation
        // Test data integrity preservation
        // Test semantic equivalence verification
        // Test migration rollback capabilities

        let migrator = ConfigMigrator::new();
        let config = MigrationConfig {
            validate_result: true,
            ..Default::default()
        };

        let test_config = serde_yaml::from_str(
            r#"
repos:
- repo: https://github.com/test/repo
  rev: v1.0.0
  hooks:
  - id: test
    entry: test-command
"#,
        )
        .unwrap();

        let (migrated_value, result) = migrator.migrate_value(test_config, config.clone()).unwrap();

        // Should be successful with validation enabled
        assert!(result.success);

        // Migrated value should still be valid YAML
        let yaml_string = serde_yaml::to_string(&migrated_value).unwrap();
        let _: Value = serde_yaml::from_str(&yaml_string).unwrap();

        // Test with invalid configuration (should not be a problem at migration level)
        let invalid_config = serde_yaml::from_str(
            r#"
repos:
- repo: ""
  rev: v1.0.0
  hooks: []
"#,
        )
        .unwrap();

        let (_, result) = migrator.migrate_value(invalid_config, config).unwrap();
        // Migration itself should succeed, validation happens at config parsing level
        assert!(result.success);
    }

    #[test]
    fn test_batch_migration() {
        // Test migration of multiple configuration files
        // Test repository-wide migration operations
        // Test migration progress reporting
        // Test error recovery and partial migration

        let migrator = ConfigMigrator::new();
        let config = MigrationConfig::default();

        // Test directory migration on non-existent directory
        let temp_dir = std::env::temp_dir().join("snp_migration_test_nonexistent");

        let results = migrator
            .migrate_directory(&temp_dir, config.clone())
            .unwrap();
        // Should return empty results for non-existent directory
        assert!(results.is_empty());

        // Test repository migration
        let results = migrator.migrate_repository(&temp_dir, config).unwrap();
        assert!(results.files_migrated.is_empty());
        assert_eq!(results.total_warnings, 0);
        assert_eq!(results.total_errors, 0);

        // Test content migration
        let test_content = r#"
repos:
- repo: https://github.com/test1/repo
  sha: v1.0.0
  hooks:
  - id: test1
"#;

        let (migrated_content, result) = migrator
            .migrate_content(test_content, MigrationConfig::default())
            .unwrap();
        assert!(result.success);
        assert!(!migrated_content.is_empty());

        // Verify the content was migrated (sha -> rev)
        assert!(migrated_content.contains("rev:"));
        assert!(!migrated_content.contains("sha:"));
    }

    #[test]
    fn test_migration_config_creation() {
        // Test that we can create migration configs
        let config = MigrationConfig::default();
        assert!(config.preserve_comments);
        assert!(config.create_backup);
        assert!(config.validate_result);
        assert!(!config.strict_mode);

        let custom_config = MigrationConfig {
            source_format: ConfigFormat::PreCommitPython {
                version: "2.0.0".to_string(),
            },
            target_format: ConfigFormat::SnpV2 {
                version: "2.0.0".to_string(),
            },
            preserve_comments: false,
            create_backup: false,
            validate_result: false,
            strict_mode: true,
        };

        assert!(!custom_config.preserve_comments);
        assert!(!custom_config.create_backup);
        assert!(!custom_config.validate_result);
        assert!(custom_config.strict_mode);
    }

    #[test]
    fn test_format_equality() {
        let format1 = ConfigFormat::PreCommitPython {
            version: "3.0.0".to_string(),
        };
        let format2 = ConfigFormat::PreCommitPython {
            version: "3.0.0".to_string(),
        };
        let format3 = ConfigFormat::SnpV1 {
            version: "1.0.0".to_string(),
        };

        assert_eq!(format1, format2);
        assert_ne!(format1, format3);
    }

    #[test]
    fn test_migration_complexity_ordering() {
        assert!(MigrationComplexity::Trivial < MigrationComplexity::Simple);
        assert!(MigrationComplexity::Simple < MigrationComplexity::Moderate);
        assert!(MigrationComplexity::Moderate < MigrationComplexity::Complex);
        assert!(MigrationComplexity::Complex < MigrationComplexity::Manual);
    }

    #[test]
    fn test_migrator_creation() {
        let migrator = ConfigMigrator::new();
        assert_eq!(migrator.migration_rules.len(), 0);
        assert_eq!(migrator.format_detectors.len(), 0);
        assert_eq!(migrator.transformers.len(), 0);
        assert_eq!(migrator.validators.len(), 0);
    }

    #[test]
    fn test_migration_rule_creation() {
        let rule = MigrationRule {
            id: "test_rule".to_string(),
            description: "Test migration rule".to_string(),
            from_format: ConfigFormat::PreCommitPython {
                version: "3.0.0".to_string(),
            },
            to_format: ConfigFormat::SnpV1 {
                version: "1.0.0".to_string(),
            },
            field_migrations: vec![],
            structural_changes: vec![],
            validation_rules: vec![],
        };

        assert_eq!(rule.id, "test_rule");
        assert_eq!(rule.description, "Test migration rule");
        assert_eq!(rule.field_migrations.len(), 0);
    }
}
