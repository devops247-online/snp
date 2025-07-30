// Initialize template directory command implementation
// Sets up git template directories with pre-commit hooks

use crate::config::Config;
use crate::error::{Result, SnpError};
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct InitTemplateDirConfig {
    pub template_directory: PathBuf,
    pub hook_types: Vec<String>,
    pub allow_missing_config: bool,
    pub config_file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct InitTemplateDirResult {
    pub template_dir_created: bool,
    pub hooks_installed: Vec<String>,
    pub permissions_set: bool,
    pub validation_success: bool,
    pub git_config_warning: Option<String>,
}

pub async fn execute_init_templatedir_command(
    config: &InitTemplateDirConfig,
) -> Result<InitTemplateDirResult> {
    info!(
        "Initializing git template directory: {}",
        config.template_directory.display()
    );

    // Validate config file if required
    if !config.allow_missing_config {
        validate_config_file(config.config_file.as_ref())?;
    }

    // Create template directory structure
    let template_dir_created = create_template_directory(&config.template_directory)?;

    // Install hooks
    let hooks_installed = install_git_hooks(&config.template_directory, &config.hook_types).await?;

    // Set proper permissions
    let permissions_set = set_hook_permissions(&config.template_directory, &hooks_installed)?;

    // Validate the template directory
    let validation_success =
        validate_template_directory(&config.template_directory, &hooks_installed);

    // Generate git config warning
    let git_config_warning = generate_git_config_warning(&config.template_directory);

    Ok(InitTemplateDirResult {
        template_dir_created,
        hooks_installed,
        permissions_set,
        validation_success,
        git_config_warning,
    })
}

fn validate_config_file(config_file: Option<&PathBuf>) -> Result<()> {
    if let Some(config_path) = config_file {
        if !config_path.exists() {
            return Err(SnpError::Config(Box::new(
                crate::error::ConfigError::NotFound {
                    path: config_path.clone(),
                    suggestion: Some(
                        "Create a .pre-commit-config.yaml file or use --allow-missing-config"
                            .to_string(),
                    ),
                },
            )));
        }

        // Try to parse the config file
        let content = fs::read_to_string(config_path)?;
        let _config: Config = serde_yaml::from_str(&content)
            .map_err(|e| SnpError::Config(Box::<crate::error::ConfigError>::from(e)))?;

        debug!("Configuration file validated successfully");
    }

    Ok(())
}

fn create_template_directory(template_dir: &PathBuf) -> Result<bool> {
    // Create the main template directory
    if !template_dir.exists() {
        fs::create_dir_all(template_dir)?;
        debug!("Created template directory: {}", template_dir.display());
    }

    // Create hooks subdirectory
    let hooks_dir = template_dir.join("hooks");
    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir)?;
        debug!("Created hooks directory: {}", hooks_dir.display());
    }

    // Set directory permissions
    #[cfg(unix)]
    {
        let metadata = fs::metadata(&hooks_dir)?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&hooks_dir, permissions)?;
    }

    Ok(true)
}

async fn install_git_hooks(template_dir: &Path, hook_types: &[String]) -> Result<Vec<String>> {
    let mut installed_hooks = Vec::new();
    let hooks_dir = template_dir.join("hooks");

    for hook_type in hook_types {
        debug!("Installing {} hook", hook_type);

        let hook_content = generate_hook_script(hook_type);
        let hook_path = hooks_dir.join(hook_type);

        fs::write(&hook_path, hook_content)?;

        // Make hook executable
        #[cfg(unix)]
        {
            let metadata = fs::metadata(&hook_path)?;
            let mut permissions = metadata.permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(&hook_path, permissions)?;
        }

        installed_hooks.push(hook_type.clone());
        debug!("Successfully installed {} hook", hook_type);
    }

    Ok(installed_hooks)
}

fn generate_hook_script(hook_type: &str) -> String {
    format!(
        r#"#!/bin/sh
# {hook_type} hook installed by SNP (Shell Not Pass)
# See https://github.com/devops247-online/snp for more information

# Check if snp is available
if ! command -v snp >/dev/null 2>&1; then
    echo "Error: snp command not found"
    echo "Please install SNP or ensure it's in your PATH"
    exit 1
fi

# Run the appropriate snp command
case "{hook_type}" in
    "pre-commit")
        exec snp run --hook-stage pre-commit "$@"
        ;;
    "pre-push")
        exec snp run --hook-stage pre-push "$@"
        ;;
    "commit-msg")
        exec snp run --hook-stage commit-msg "$@"
        ;;
    "post-commit")
        exec snp run --hook-stage post-commit "$@"
        ;;
    "post-checkout")
        exec snp run --hook-stage post-checkout "$@"
        ;;
    "post-merge")
        exec snp run --hook-stage post-merge "$@"
        ;;
    "pre-rebase")
        exec snp run --hook-stage pre-rebase "$@"
        ;;
    "post-rewrite")
        exec snp run --hook-stage post-rewrite "$@"
        ;;
    *)
        echo "Warning: Unknown hook type '{hook_type}'"
        exec snp run --hook-stage {hook_type} "$@"
        ;;
esac
"#
    )
}

fn set_hook_permissions(template_dir: &Path, hook_names: &[String]) -> Result<bool> {
    let hooks_dir = template_dir.join("hooks");

    #[cfg(unix)]
    {
        for hook_name in hook_names {
            let hook_path = hooks_dir.join(hook_name);
            if hook_path.exists() {
                let metadata = fs::metadata(&hook_path)?;
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&hook_path, permissions)?;
                debug!("Set executable permissions for {}", hook_name);
            }
        }
    }

    #[cfg(not(unix))]
    {
        debug!("Cannot set executable permissions on non-Unix systems");
    }

    Ok(true)
}

fn validate_template_directory(template_dir: &Path, hook_names: &[String]) -> bool {
    // Check that template directory exists
    if !template_dir.exists() {
        debug!(
            "Template directory does not exist: {}",
            template_dir.display()
        );
        return false;
    }

    // Check that hooks directory exists
    let hooks_dir = template_dir.join("hooks");
    if !hooks_dir.exists() {
        debug!("Hooks directory does not exist: {}", hooks_dir.display());
        return false;
    }

    // Check that all hooks were created and are executable
    for hook_name in hook_names {
        let hook_path = hooks_dir.join(hook_name);
        if !hook_path.exists() {
            debug!("Hook {} does not exist", hook_name);
            return false;
        }

        #[cfg(unix)]
        {
            if let Ok(metadata) = fs::metadata(&hook_path) {
                let permissions = metadata.permissions();
                if permissions.mode() & 0o111 == 0 {
                    debug!("Hook {} is not executable", hook_name);
                    return false;
                }
            }
        }
    }

    true
}

fn generate_git_config_warning(template_dir: &Path) -> Option<String> {
    Some(format!(
        r#"Template directory created successfully at: {}

To use this template for new repositories, run:
    git config --global init.templateDir '{}'

Or for a specific repository:
    git config init.templateDir '{}'

This will ensure that new git repositories automatically include the pre-commit hooks.

For existing repositories, you can copy the hooks manually:
    cp {}hooks/* /path/to/repo/.git/hooks/

Note: You may need to make the hooks executable:
    chmod +x /path/to/repo/.git/hooks/*"#,
        template_dir.display(),
        template_dir.display(),
        template_dir.display(),
        template_dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[allow(dead_code)]
    fn create_test_config() -> InitTemplateDirConfig {
        let temp_dir = TempDir::new().unwrap();
        InitTemplateDirConfig {
            template_directory: temp_dir.path().join("template"),
            hook_types: vec!["pre-commit".to_string(), "pre-push".to_string()],
            allow_missing_config: true,
            config_file: None,
        }
    }

    #[test]
    fn test_init_templatedir_config_creation() {
        let config = InitTemplateDirConfig {
            template_directory: PathBuf::from("/tmp/template"),
            hook_types: vec!["pre-commit".to_string(), "commit-msg".to_string()],
            allow_missing_config: false,
            config_file: Some(PathBuf::from(".pre-commit-config.yaml")),
        };

        assert_eq!(config.template_directory, PathBuf::from("/tmp/template"));
        assert_eq!(config.hook_types.len(), 2);
        assert!(!config.allow_missing_config);
        assert!(config.config_file.is_some());
    }

    #[test]
    fn test_init_templatedir_result_structure() {
        let result = InitTemplateDirResult {
            template_dir_created: true,
            hooks_installed: vec!["pre-commit".to_string(), "pre-push".to_string()],
            permissions_set: true,
            validation_success: true,
            git_config_warning: Some("Warning message".to_string()),
        };

        assert!(result.template_dir_created);
        assert_eq!(result.hooks_installed.len(), 2);
        assert!(result.permissions_set);
        assert!(result.validation_success);
        assert!(result.git_config_warning.is_some());
    }

    #[test]
    fn test_validate_config_file_missing() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent_config = temp_dir.path().join("non-existent.yaml");

        let result = validate_config_file(Some(&non_existent_config));
        assert!(result.is_err());

        match result.unwrap_err() {
            SnpError::Config(_) => {}
            _ => panic!("Expected ConfigError for missing file"),
        }
    }

    #[test]
    fn test_validate_config_file_valid() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("config.yaml");

        let config_content = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    name: Test Hook
    entry: echo test
    language: system
"#;
        fs::write(&config_file, config_content).unwrap();

        let result = validate_config_file(Some(&config_file));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_file_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("invalid.yaml");

        fs::write(&config_file, "invalid yaml content [").unwrap();

        let result = validate_config_file(Some(&config_file));
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_config_file_none() {
        let result = validate_config_file(None);
        assert!(result.is_ok());
    }

    #[test]
    fn test_create_template_directory() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");

        let result = create_template_directory(&template_dir);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Check that directories were created
        assert!(template_dir.exists());
        assert!(template_dir.join("hooks").exists());
    }

    #[test]
    fn test_create_template_directory_existing() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");

        // Create the directory first
        fs::create_dir_all(&template_dir).unwrap();

        let result = create_template_directory(&template_dir);
        assert!(result.is_ok());
        assert!(result.unwrap());
    }

    #[tokio::test]
    async fn test_install_git_hooks_empty() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");
        fs::create_dir_all(template_dir.join("hooks")).unwrap();

        let result = install_git_hooks(&template_dir, &[]).await;
        assert!(result.is_ok());

        let hooks = result.unwrap();
        assert!(hooks.is_empty());
    }

    #[tokio::test]
    async fn test_install_git_hooks_single() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");
        fs::create_dir_all(template_dir.join("hooks")).unwrap();

        let hook_types = vec!["pre-commit".to_string()];
        let result = install_git_hooks(&template_dir, &hook_types).await;
        assert!(result.is_ok());

        let hooks = result.unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0], "pre-commit");

        // Check that hook file was created
        let hook_path = template_dir.join("hooks").join("pre-commit");
        assert!(hook_path.exists());

        // Check hook content
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("# pre-commit hook installed by SNP"));
        assert!(content.contains("snp run --hook-stage pre-commit"));
    }

    #[tokio::test]
    async fn test_install_git_hooks_multiple() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");
        fs::create_dir_all(template_dir.join("hooks")).unwrap();

        let hook_types = vec![
            "pre-commit".to_string(),
            "pre-push".to_string(),
            "commit-msg".to_string(),
        ];
        let result = install_git_hooks(&template_dir, &hook_types).await;
        assert!(result.is_ok());

        let hooks = result.unwrap();
        assert_eq!(hooks.len(), 3);

        // Check that all hook files were created
        for hook_type in &hook_types {
            let hook_path = template_dir.join("hooks").join(hook_type);
            assert!(hook_path.exists());
        }
    }

    #[test]
    fn test_generate_hook_script_pre_commit() {
        let script = generate_hook_script("pre-commit");

        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("# pre-commit hook installed by SNP"));
        assert!(script.contains("snp run --hook-stage pre-commit"));
        assert!(script.contains("command -v snp"));
    }

    #[test]
    fn test_generate_hook_script_pre_push() {
        let script = generate_hook_script("pre-push");

        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("# pre-push hook installed by SNP"));
        assert!(script.contains("snp run --hook-stage pre-push"));
    }

    #[test]
    fn test_generate_hook_script_commit_msg() {
        let script = generate_hook_script("commit-msg");

        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("# commit-msg hook installed by SNP"));
        assert!(script.contains("snp run --hook-stage commit-msg"));
    }

    #[test]
    fn test_generate_hook_script_unknown() {
        let script = generate_hook_script("custom-hook");

        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("# custom-hook hook installed by SNP"));
        assert!(script.contains("Warning: Unknown hook type 'custom-hook'"));
        assert!(script.contains("snp run --hook-stage custom-hook"));
    }

    #[test]
    fn test_set_hook_permissions() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");
        let hooks_dir = template_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create test hooks
        let hook_names = vec!["pre-commit".to_string(), "pre-push".to_string()];
        for hook_name in &hook_names {
            let hook_path = hooks_dir.join(hook_name);
            fs::write(&hook_path, "#!/bin/sh\necho test").unwrap();
        }

        let result = set_hook_permissions(&template_dir, &hook_names);
        assert!(result.is_ok());
        assert!(result.unwrap());

        // On Unix systems, check that permissions were set
        #[cfg(unix)]
        {
            for hook_name in &hook_names {
                let hook_path = hooks_dir.join(hook_name);
                let metadata = fs::metadata(&hook_path).unwrap();
                let permissions = metadata.permissions();
                assert!(permissions.mode() & 0o111 != 0); // Should be executable
            }
        }
    }

    #[test]
    fn test_validate_template_directory_success() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");
        let hooks_dir = template_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create test hooks
        let hook_names = vec!["pre-commit".to_string()];
        for hook_name in &hook_names {
            let hook_path = hooks_dir.join(hook_name);
            fs::write(&hook_path, "#!/bin/sh\necho test").unwrap();

            // Make executable on Unix
            #[cfg(unix)]
            {
                let metadata = fs::metadata(&hook_path).unwrap();
                let mut permissions = metadata.permissions();
                permissions.set_mode(0o755);
                fs::set_permissions(&hook_path, permissions).unwrap();
            }
        }

        let result = validate_template_directory(&template_dir, &hook_names);
        assert!(result);
    }

    #[test]
    fn test_validate_template_directory_missing_dir() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("non-existent");

        let hook_names = vec!["pre-commit".to_string()];
        let result = validate_template_directory(&template_dir, &hook_names);
        assert!(!result);
    }

    #[test]
    fn test_validate_template_directory_missing_hooks_dir() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");
        fs::create_dir_all(&template_dir).unwrap();
        // Don't create hooks directory

        let hook_names = vec!["pre-commit".to_string()];
        let result = validate_template_directory(&template_dir, &hook_names);
        assert!(!result);
    }

    #[test]
    fn test_validate_template_directory_missing_hook() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");
        let hooks_dir = template_dir.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();

        // Don't create the hook file
        let hook_names = vec!["pre-commit".to_string()];
        let result = validate_template_directory(&template_dir, &hook_names);
        assert!(!result);
    }

    #[test]
    fn test_generate_git_config_warning() {
        let temp_dir = TempDir::new().unwrap();
        let template_dir = temp_dir.path().join("template");

        let warning = generate_git_config_warning(&template_dir);
        assert!(warning.is_some());

        let warning_text = warning.unwrap();
        assert!(warning_text.contains("Template directory created successfully"));
        assert!(warning_text.contains("git config --global init.templateDir"));
        assert!(warning_text.contains(&template_dir.display().to_string()));
        assert!(warning_text.contains("chmod +x"));
    }

    #[tokio::test]
    async fn test_execute_init_templatedir_command_basic() {
        let temp_dir = TempDir::new().unwrap();
        let config = InitTemplateDirConfig {
            template_directory: temp_dir.path().join("template"),
            hook_types: vec!["pre-commit".to_string()],
            allow_missing_config: true,
            config_file: None,
        };

        let result = execute_init_templatedir_command(&config).await;
        assert!(result.is_ok());

        let init_result = result.unwrap();
        assert!(init_result.template_dir_created);
        assert_eq!(init_result.hooks_installed.len(), 1);
        assert!(init_result.permissions_set);
        assert!(init_result.validation_success);
        assert!(init_result.git_config_warning.is_some());
    }

    #[tokio::test]
    async fn test_execute_init_templatedir_command_with_config() {
        let temp_dir = TempDir::new().unwrap();
        let config_file = temp_dir.path().join("config.yaml");

        // Create valid config file
        let config_content = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    name: Test Hook
    entry: echo test
    language: system
"#;
        fs::write(&config_file, config_content).unwrap();

        let config = InitTemplateDirConfig {
            template_directory: temp_dir.path().join("template"),
            hook_types: vec!["pre-commit".to_string(), "pre-push".to_string()],
            allow_missing_config: false,
            config_file: Some(config_file),
        };

        let result = execute_init_templatedir_command(&config).await;
        assert!(result.is_ok());

        let init_result = result.unwrap();
        assert!(init_result.template_dir_created);
        assert_eq!(init_result.hooks_installed.len(), 2);
        assert!(init_result.validation_success);
    }

    #[tokio::test]
    async fn test_execute_init_templatedir_command_missing_config() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent_config = temp_dir.path().join("non-existent.yaml");

        let config = InitTemplateDirConfig {
            template_directory: temp_dir.path().join("template"),
            hook_types: vec!["pre-commit".to_string()],
            allow_missing_config: false,
            config_file: Some(non_existent_config),
        };

        let result = execute_init_templatedir_command(&config).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            SnpError::Config(_) => {}
            _ => panic!("Expected ConfigError for missing config file"),
        }
    }

    #[tokio::test]
    async fn test_execute_init_templatedir_command_empty_hooks() {
        let temp_dir = TempDir::new().unwrap();
        let config = InitTemplateDirConfig {
            template_directory: temp_dir.path().join("template"),
            hook_types: vec![],
            allow_missing_config: true,
            config_file: None,
        };

        let result = execute_init_templatedir_command(&config).await;
        assert!(result.is_ok());

        let init_result = result.unwrap();
        assert!(init_result.template_dir_created);
        assert!(init_result.hooks_installed.is_empty());
        assert!(init_result.permissions_set);
        assert!(init_result.validation_success);
    }

    #[test]
    fn test_init_templatedir_config_defaults() {
        let config = InitTemplateDirConfig {
            template_directory: PathBuf::from("/tmp"),
            hook_types: vec![],
            allow_missing_config: false,
            config_file: None,
        };

        assert_eq!(config.template_directory, PathBuf::from("/tmp"));
        assert!(config.hook_types.is_empty());
        assert!(!config.allow_missing_config);
        assert!(config.config_file.is_none());
    }

    #[test]
    fn test_init_templatedir_result_defaults() {
        let result = InitTemplateDirResult {
            template_dir_created: false,
            hooks_installed: vec![],
            permissions_set: false,
            validation_success: false,
            git_config_warning: None,
        };

        assert!(!result.template_dir_created);
        assert!(result.hooks_installed.is_empty());
        assert!(!result.permissions_set);
        assert!(!result.validation_success);
        assert!(result.git_config_warning.is_none());
    }

    #[test]
    fn test_hook_script_completeness() {
        let all_hook_types = vec![
            "pre-commit",
            "pre-push",
            "commit-msg",
            "post-commit",
            "post-checkout",
            "post-merge",
            "pre-rebase",
            "post-rewrite",
        ];

        for hook_type in all_hook_types {
            let script = generate_hook_script(hook_type);
            assert!(script.contains("#!/bin/sh"));
            assert!(script.contains(&format!("# {hook_type} hook installed by SNP")));
            assert!(script.contains(&format!("snp run --hook-stage {hook_type}")));
            assert!(script.contains("command -v snp"));
        }
    }
}
