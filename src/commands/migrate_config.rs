// Configuration migration command implementation
// Migrates configurations between different formats/versions

use crate::config::Config;
use crate::error::{Result, SnpError};
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct MigrateConfigConfig {
    pub config_file: PathBuf,
    pub from_version: Option<String>,
    pub backup: bool,
    pub dry_run: bool,
}

#[derive(Debug)]
pub struct MigrateConfigResult {
    pub migration_needed: bool,
    pub migration_applied: bool,
    pub from_version: String,
    pub to_version: String,
    pub backup_location: Option<String>,
    pub preview_changes: Option<String>,
    pub validation_success: bool,
}

pub async fn execute_migrate_config_command(
    config: &MigrateConfigConfig,
) -> Result<MigrateConfigResult> {
    info!(
        "Migrating configuration file: {}",
        config.config_file.display()
    );

    // Read the configuration file
    let content = fs::read_to_string(&config.config_file)?;

    // Detect current format version
    let detected_version = detect_config_version(&content)?;
    let from_version = config.from_version.clone().unwrap_or(detected_version);

    // Check if migration is needed
    let migration_needed = needs_migration(&from_version, &content)?;
    if !migration_needed {
        return Ok(MigrateConfigResult {
            migration_needed: false,
            migration_applied: false,
            from_version,
            to_version: "current".to_string(),
            backup_location: None,
            preview_changes: None,
            validation_success: true,
        });
    }

    // Perform migration
    let migrated_content = migrate_content(&content, &from_version)?;

    // Validate migrated configuration
    let validation_success = validate_migrated_config(&migrated_content);

    // Handle dry run
    if config.dry_run {
        return Ok(MigrateConfigResult {
            migration_needed: true,
            migration_applied: false,
            from_version,
            to_version: "current".to_string(),
            backup_location: None,
            preview_changes: Some(migrated_content),
            validation_success,
        });
    }

    // Create backup if requested
    let backup_location = if config.backup {
        let backup_path = create_backup(&config.config_file, &content)?;
        Some(backup_path)
    } else {
        None
    };

    // Write migrated configuration
    fs::write(&config.config_file, &migrated_content)?;

    Ok(MigrateConfigResult {
        migration_needed: true,
        migration_applied: true,
        from_version,
        to_version: "current".to_string(),
        backup_location,
        preview_changes: None,
        validation_success,
    })
}

fn detect_config_version(content: &str) -> Result<String> {
    // Parse as YAML to check structure
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(content)
        .map_err(|e| SnpError::Config(Box::<crate::error::ConfigError>::from(e)))?;

    // Check for legacy list format (old pre-commit format)
    if yaml_value.is_sequence() {
        return Ok("legacy-list".to_string());
    }

    // Check for specific old field names
    if let Some(mapping) = yaml_value.as_mapping() {
        for (key, value) in mapping {
            if let Some(key_str) = key.as_str() {
                if key_str == "repos" {
                    if let Some(repos) = value.as_sequence() {
                        for repo in repos {
                            if let Some(repo_map) = repo.as_mapping() {
                                // Check for 'sha' field (old format)
                                if repo_map
                                    .get(serde_yaml::Value::String("sha".to_string()))
                                    .is_some()
                                {
                                    return Ok("sha-format".to_string());
                                }
                                // Check for 'python_venv' language (old format)
                                if let Some(hooks) =
                                    repo_map.get(serde_yaml::Value::String("hooks".to_string()))
                                {
                                    if let Some(hooks_seq) = hooks.as_sequence() {
                                        for hook in hooks_seq {
                                            if let Some(hook_map) = hook.as_mapping() {
                                                if let Some(lang) =
                                                    hook_map.get(serde_yaml::Value::String(
                                                        "language".to_string(),
                                                    ))
                                                {
                                                    if let Some(lang_str) = lang.as_str() {
                                                        if lang_str == "python_venv" {
                                                            return Ok(
                                                                "python_venv-format".to_string()
                                                            );
                                                        }
                                                    }
                                                }
                                                // Check for old stage names
                                                if let Some(stages) = hook_map.get(
                                                    serde_yaml::Value::String("stages".to_string()),
                                                ) {
                                                    if let Some(stages_seq) = stages.as_sequence() {
                                                        for stage in stages_seq {
                                                            if let Some(stage_str) = stage.as_str()
                                                            {
                                                                if stage_str == "commit"
                                                                    || stage_str == "push"
                                                                {
                                                                    return Ok("old-stages-format"
                                                                        .to_string());
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok("current".to_string())
}

fn needs_migration(version: &str, _content: &str) -> Result<bool> {
    match version {
        "current" => Ok(false),
        "legacy-list" | "sha-format" | "python_venv-format" | "old-stages-format" => Ok(true),
        _ => Ok(true), // Unknown versions need migration
    }
}

fn migrate_content(content: &str, _from_version: &str) -> Result<String> {
    debug!("Migrating content (applying all migrations)");

    let mut migrated_content = content.to_string();

    // Apply all migrations in sequence since a file can have multiple old formats
    // Only apply legacy-list migration if it's actually a list format
    if migrated_content.trim_start().starts_with('-') {
        migrated_content = migrate_from_legacy_list(&migrated_content)?;
    }

    // Always apply these migrations as they are safe to run multiple times
    migrated_content = migrate_sha_to_rev(&migrated_content)?;
    migrated_content = migrate_python_language(&migrated_content)?;
    migrated_content = migrate_stage_names(&migrated_content)?;

    Ok(migrated_content)
}

fn migrate_from_legacy_list(content: &str) -> Result<String> {
    // Convert from old list format to repos format
    let yaml_value: serde_yaml::Value = serde_yaml::from_str(content)
        .map_err(|e| SnpError::Config(Box::<crate::error::ConfigError>::from(e)))?;

    if yaml_value.is_sequence() {
        // Convert list format to repos format
        let mut new_config = std::collections::BTreeMap::new();
        new_config.insert("repos".to_string(), yaml_value);

        let new_yaml = serde_yaml::Value::Mapping(
            new_config
                .into_iter()
                .map(|(k, v)| (serde_yaml::Value::String(k), v))
                .collect(),
        );

        return serde_yaml::to_string(&new_yaml)
            .map_err(|e| SnpError::Config(Box::<crate::error::ConfigError>::from(e)));
    }

    Ok(content.to_string())
}

fn migrate_sha_to_rev(content: &str) -> Result<String> {
    // Replace 'sha:' with 'rev:'
    Ok(content.replace("sha:", "rev:"))
}

fn migrate_python_language(content: &str) -> Result<String> {
    // Replace 'python_venv' language with 'python' (handle various spacing)
    let mut migrated = content.to_string();
    migrated = migrated.replace("language: python_venv", "language: python");
    migrated = migrated.replace("language:python_venv", "language:python");
    migrated = migrated.replace("python_venv", "python");
    Ok(migrated)
}

fn migrate_stage_names(content: &str) -> Result<String> {
    let mut migrated = content.to_string();

    // Replace old stage names with new ones
    migrated = migrated.replace("stages: [commit]", "stages: [pre-commit]");
    migrated = migrated.replace("stages: [push]", "stages: [pre-push]");
    migrated = migrated.replace("- commit", "- pre-commit");
    migrated = migrated.replace("- push", "- pre-push");

    Ok(migrated)
}

fn validate_migrated_config(content: &str) -> bool {
    // Try to parse the migrated configuration
    match serde_yaml::from_str::<Config>(content) {
        Ok(_) => true,
        Err(_) => {
            // If strict parsing fails, try as generic YAML
            serde_yaml::from_str::<serde_yaml::Value>(content).is_ok()
        }
    }
}

fn create_backup(config_file: &Path, content: &str) -> Result<String> {
    let backup_path = config_file.with_extension("yaml.bak");
    fs::write(&backup_path, content)?;
    Ok(backup_path.to_string_lossy().to_string())
}
