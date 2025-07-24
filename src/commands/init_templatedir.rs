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
