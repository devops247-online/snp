// Install/Uninstall functionality for SNP git hooks
// Provides comprehensive hook management with backup and restore capabilities

use crate::error::{GitError, Result, SnpError};
use crate::git::GitRepository;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;
use tracing::{debug, info};

/// Git hook types supported by SNP
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HookType {
    PreCommit,
    PrePush,
    CommitMsg,
    PostCommit,
    PreRebase,
    PostRewrite,
    PostCheckout,
    PostMerge,
}

impl HookType {
    /// Parse hook type from string representation
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "pre-commit" => Ok(HookType::PreCommit),
            "pre-push" => Ok(HookType::PrePush),
            "commit-msg" => Ok(HookType::CommitMsg),
            "post-commit" => Ok(HookType::PostCommit),
            "pre-rebase" => Ok(HookType::PreRebase),
            "post-rewrite" => Ok(HookType::PostRewrite),
            "post-checkout" => Ok(HookType::PostCheckout),
            "post-merge" => Ok(HookType::PostMerge),
            _ => Err(SnpError::Git(Box::new(GitError::InvalidReference {
                reference: s.to_string(),
                repository: "hook type".to_string(),
                suggestion: Some("Valid hook types: pre-commit, pre-push, commit-msg, post-commit, pre-rebase, post-rewrite, post-checkout, post-merge".to_string()),
            }))),
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            HookType::PreCommit => "pre-commit",
            HookType::PrePush => "pre-push",
            HookType::CommitMsg => "commit-msg",
            HookType::PostCommit => "post-commit",
            HookType::PreRebase => "pre-rebase",
            HookType::PostRewrite => "post-rewrite",
            HookType::PostCheckout => "post-checkout",
            HookType::PostMerge => "post-merge",
        }
    }

    /// Get all supported hook types
    pub fn all() -> Vec<HookType> {
        vec![
            HookType::PreCommit,
            HookType::PrePush,
            HookType::CommitMsg,
            HookType::PostCommit,
            HookType::PreRebase,
            HookType::PostRewrite,
            HookType::PostCheckout,
            HookType::PostMerge,
        ]
    }
}

/// Configuration for hook installation
#[derive(Debug, Clone)]
pub struct InstallConfig {
    pub hook_types: Vec<String>,
    pub overwrite_existing: bool,
    pub allow_missing_config: bool,
    pub backup_existing: bool,
    pub hook_args: Vec<String>,
}

impl Default for InstallConfig {
    fn default() -> Self {
        Self {
            hook_types: vec!["pre-commit".to_string()],
            overwrite_existing: false,
            allow_missing_config: false,
            backup_existing: true,
            hook_args: Vec::new(),
        }
    }
}

/// Configuration for hook uninstallation
#[derive(Debug, Clone)]
pub struct UninstallConfig {
    pub hook_types: Vec<String>,
    pub restore_backups: bool,
    pub clean_backups: bool,
}

impl Default for UninstallConfig {
    fn default() -> Self {
        Self {
            hook_types: Vec::new(), // Empty means all installed SNP hooks
            restore_backups: true,
            clean_backups: false,
        }
    }
}

/// Configuration for individual hook generation
#[derive(Debug, Clone)]
pub struct HookConfig {
    pub hook_type: HookType,
    pub config_file: String,
    pub allow_missing_config: bool,
    pub additional_args: Vec<String>,
}

/// Result of hook installation
#[derive(Debug, Clone)]
pub struct InstallResult {
    pub hooks_installed: Vec<String>,
    pub hooks_backed_up: Vec<String>,
    pub hooks_overwritten: Vec<String>,
    pub warnings: Vec<String>,
}

/// Result of hook uninstallation
#[derive(Debug, Clone)]
pub struct UninstallResult {
    pub hooks_removed: Vec<String>,
    pub hooks_restored: Vec<String>,
    pub backups_cleaned: Vec<String>,
    pub warnings: Vec<String>,
}

/// Information about a backup operation
#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub hook_type: HookType,
    pub original_path: PathBuf,
    pub backup_path: PathBuf,
    pub backup_time: SystemTime,
    pub original_size: u64,
}

/// Information about a restore operation
#[derive(Debug, Clone)]
pub struct RestoreInfo {
    pub hook_type: HookType,
    pub restored_path: PathBuf,
    pub original_backup_path: PathBuf,
    pub restore_time: SystemTime,
}

/// Information about cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupResult {
    pub cleaned_files: usize,
    pub bytes_freed: u64,
    pub errors: Vec<String>,
}

/// Main hook management interface
pub struct GitHookManager {
    git_repo: GitRepository,
    backup_manager: HookBackupManager,
    template_generator: HookTemplateGenerator,
}

impl GitHookManager {
    /// Create a new GitHookManager
    pub fn new(git_repo: GitRepository) -> Self {
        let hooks_dir = git_repo.git_dir().join("hooks");
        let backup_manager = HookBackupManager::new(hooks_dir.clone());
        let template_generator = HookTemplateGenerator::new();

        Self {
            git_repo,
            backup_manager,
            template_generator,
        }
    }

    /// Install SNP hooks into git repository
    pub async fn install_hooks(&self, config: &InstallConfig) -> Result<InstallResult> {
        debug!("Installing hooks with config: {:?}", config);

        // Validate git repository
        self.validate_git_repository().await?;

        // Check for core.hooksPath configuration
        if self.git_repo.has_core_hooks_path()? {
            return Err(SnpError::Git(Box::new(GitError::CommandFailed {
                command: "git config core.hooksPath".to_string(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: "Cowardly refusing to install hooks with `core.hooksPath` set. Run `git config --unset-all core.hooksPath` to fix.".to_string(),
                working_dir: Some(self.git_repo.root_path().to_path_buf()),
            })));
        }

        // Validate config file if required
        if !config.allow_missing_config {
            let config_path = self.git_repo.root_path().join(".pre-commit-config.yaml");
            if !config_path.exists() {
                return Err(SnpError::Git(Box::new(GitError::CommandFailed {
                    command: "validate config".to_string(),
                    exit_code: Some(1),
                    stdout: String::new(),
                    stderr:
                        "No .pre-commit-config.yaml found. Use --allow-missing-config to bypass."
                            .to_string(),
                    working_dir: Some(self.git_repo.root_path().to_path_buf()),
                })));
            }
        }

        let mut result = InstallResult {
            hooks_installed: Vec::new(),
            hooks_backed_up: Vec::new(),
            hooks_overwritten: Vec::new(),
            warnings: Vec::new(),
        };

        // Determine hook types to install
        let hook_types = if config.hook_types.is_empty() {
            vec!["pre-commit".to_string()]
        } else {
            config.hook_types.clone()
        };

        // Install each hook type
        for hook_type_str in hook_types {
            let hook_type = HookType::parse(&hook_type_str)?;

            match self.install_single_hook(&hook_type, config).await {
                Ok((installed, backed_up, overwritten)) => {
                    if installed {
                        result.hooks_installed.push(hook_type_str.clone());
                    }
                    if backed_up {
                        result.hooks_backed_up.push(hook_type_str.clone());
                    }
                    if overwritten {
                        result.hooks_overwritten.push(hook_type_str.clone());
                    }
                }
                Err(e) => {
                    result
                        .warnings
                        .push(format!("Failed to install {hook_type_str}: {e}"));
                }
            }
        }

        if result.hooks_installed.is_empty() && result.warnings.is_empty() {
            result.warnings.push("No hooks were installed".to_string());
        }

        info!("Hook installation completed: {:?}", result);
        Ok(result)
    }

    /// Uninstall SNP hooks and restore backups
    pub async fn uninstall_hooks(&self, config: &UninstallConfig) -> Result<UninstallResult> {
        debug!("Uninstalling hooks with config: {:?}", config);

        let mut result = UninstallResult {
            hooks_removed: Vec::new(),
            hooks_restored: Vec::new(),
            backups_cleaned: Vec::new(),
            warnings: Vec::new(),
        };

        // Determine hook types to uninstall
        let hook_types = if config.hook_types.is_empty() {
            // Find all installed SNP hooks
            self.find_installed_snp_hooks().await?
        } else {
            config.hook_types.clone()
        };

        // Uninstall each hook type
        for hook_type_str in hook_types {
            let hook_type = HookType::parse(&hook_type_str)?;

            match self.uninstall_single_hook(&hook_type, config).await {
                Ok((removed, restored)) => {
                    if removed {
                        result.hooks_removed.push(hook_type_str.clone());
                    }
                    if restored {
                        result.hooks_restored.push(hook_type_str.clone());
                    }
                }
                Err(e) => {
                    result
                        .warnings
                        .push(format!("Failed to uninstall {hook_type_str}: {e}"));
                }
            }
        }

        // Clean up old backups if requested
        if config.clean_backups {
            match self.backup_manager.cleanup_backups(30).await {
                Ok(cleanup_result) => {
                    result
                        .backups_cleaned
                        .extend((0..cleanup_result.cleaned_files).map(|i| format!("backup_{i}")));
                }
                Err(e) => {
                    result.warnings.push(format!("Backup cleanup failed: {e}"));
                }
            }
        }

        info!("Hook uninstallation completed: {:?}", result);
        Ok(result)
    }

    /// Check which hooks are currently installed
    pub async fn check_installation_status(&self) -> Result<HashMap<String, bool>> {
        debug!("Checking installation status");

        let mut status = HashMap::new();

        for hook_type in HookType::all() {
            let hook_path = self.git_repo.hook_path(hook_type.as_str());
            let is_installed = self.is_snp_hook(&hook_path).await?;
            status.insert(hook_type.as_str().to_string(), is_installed);
        }

        Ok(status)
    }

    /// Validate git repository and hook directory
    pub async fn validate_git_repository(&self) -> Result<()> {
        // Check if we're in a git repository
        if self.git_repo.is_bare() {
            return Err(SnpError::Git(Box::new(GitError::RepositoryNotFound {
                path: self.git_repo.root_path().to_path_buf(),
                suggestion: Some(
                    "Bare repositories are not supported for pre-commit hooks".to_string(),
                ),
            })));
        }

        // Ensure hooks directory exists
        let hooks_dir = self.git_repo.git_dir().join("hooks");
        if !hooks_dir.exists() {
            fs::create_dir_all(&hooks_dir).map_err(|_e| {
                SnpError::Git(Box::new(GitError::PermissionDenied {
                    operation: format!("create hooks directory: {}", hooks_dir.display()),
                    path: hooks_dir.clone(),
                    suggestion: Some("Check directory permissions".to_string()),
                }))
            })?;
        }

        Ok(())
    }

    /// Install a single hook type
    async fn install_single_hook(
        &self,
        hook_type: &HookType,
        config: &InstallConfig,
    ) -> Result<(bool, bool, bool)> {
        let hook_path = self.git_repo.hook_path(hook_type.as_str());
        let mut backed_up = false;
        let mut overwritten = false;

        debug!(
            "Installing {} hook at {}",
            hook_type.as_str(),
            hook_path.display()
        );

        // Handle existing hook
        if hook_path.exists() {
            if self.is_snp_hook(&hook_path).await? {
                info!(
                    "SNP hook already installed for {}, skipping",
                    hook_type.as_str()
                );
                return Ok((false, false, false));
            }

            if config.overwrite_existing {
                // Remove existing hook without backup
                fs::remove_file(&hook_path).map_err(|_e| {
                    SnpError::Git(Box::new(GitError::PermissionDenied {
                        operation: format!("remove existing hook: {}", hook_path.display()),
                        path: hook_path.clone(),
                        suggestion: Some("Check file permissions".to_string()),
                    }))
                })?;
                overwritten = true;
            } else if config.backup_existing {
                // Backup existing hook
                self.backup_manager
                    .backup_hook(hook_type.clone(), &hook_path)
                    .await?;
                backed_up = true;
            } else {
                return Err(SnpError::Git(Box::new(GitError::CommandFailed {
                    command: "install hook".to_string(),
                    exit_code: Some(1),
                    stdout: String::new(),
                    stderr: format!(
                        "Hook {} already exists. Use --overwrite to replace it.",
                        hook_type.as_str()
                    ),
                    working_dir: Some(self.git_repo.root_path().to_path_buf()),
                })));
            }
        }

        // Generate hook script
        let hook_config = HookConfig {
            hook_type: hook_type.clone(),
            config_file: ".pre-commit-config.yaml".to_string(),
            allow_missing_config: config.allow_missing_config,
            additional_args: config.hook_args.clone(),
        };

        let script = self
            .template_generator
            .generate_hook_script(hook_type.clone(), &hook_config)?;

        // Write hook script
        debug!("Writing hook script to: {}", hook_path.display());
        debug!("Script content length: {} bytes", script.len());
        fs::write(&hook_path, script).map_err(|e| {
            debug!("Failed to write hook file: {}", e);
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("write hook file: {}", hook_path.display()),
                path: hook_path.clone(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;
        debug!("Hook file written successfully");

        // Make hook executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms).map_err(|_e| {
                SnpError::Git(Box::new(GitError::PermissionDenied {
                    operation: format!("set executable permissions: {}", hook_path.display()),
                    path: hook_path.clone(),
                    suggestion: Some("Check file permissions".to_string()),
                }))
            })?;
        }

        info!("Successfully installed {} hook", hook_type.as_str());
        Ok((true, backed_up, overwritten))
    }

    /// Uninstall a single hook type
    async fn uninstall_single_hook(
        &self,
        hook_type: &HookType,
        config: &UninstallConfig,
    ) -> Result<(bool, bool)> {
        let hook_path = self.git_repo.hook_path(hook_type.as_str());

        debug!(
            "Uninstalling {} hook at {}",
            hook_type.as_str(),
            hook_path.display()
        );

        // Check if hook exists and is ours
        if !hook_path.exists() {
            debug!("Hook {} does not exist, skipping", hook_type.as_str());
            return Ok((false, false));
        }

        if !self.is_snp_hook(&hook_path).await? {
            debug!("Hook {} is not an SNP hook, skipping", hook_type.as_str());
            return Ok((false, false));
        }

        // Remove SNP hook
        fs::remove_file(&hook_path).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("remove hook file: {}", hook_path.display()),
                path: hook_path.clone(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;

        let mut restored = false;

        // Restore backup if requested and available
        if config.restore_backups {
            match self.backup_manager.restore_hook(hook_type.clone()).await {
                Ok(_restore_info) => {
                    restored = true;
                    info!("Restored backup for {} hook", hook_type.as_str());
                }
                Err(e) => {
                    debug!(
                        "No backup to restore for {} hook: {}",
                        hook_type.as_str(),
                        e
                    );
                }
            }
        }

        info!("Successfully uninstalled {} hook", hook_type.as_str());
        Ok((true, restored))
    }

    /// Check if a hook file was installed by SNP
    async fn is_snp_hook(&self, hook_path: &Path) -> Result<bool> {
        if !hook_path.exists() {
            return Ok(false);
        }

        let content = fs::read_to_string(hook_path).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("read hook file: {}", hook_path.display()),
                path: hook_path.to_path_buf(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;

        Ok(content.contains("Generated by SNP"))
    }

    /// Find all installed SNP hooks
    async fn find_installed_snp_hooks(&self) -> Result<Vec<String>> {
        let mut installed_hooks = Vec::new();

        for hook_type in HookType::all() {
            let hook_path = self.git_repo.hook_path(hook_type.as_str());
            if self.is_snp_hook(&hook_path).await? {
                installed_hooks.push(hook_type.as_str().to_string());
            }
        }

        Ok(installed_hooks)
    }
}

/// Backup and restore management for existing git hooks
pub struct HookBackupManager {
    hooks_dir: PathBuf,
}

impl HookBackupManager {
    /// Create a new HookBackupManager
    pub fn new(hooks_dir: PathBuf) -> Self {
        Self { hooks_dir }
    }

    /// Create backup of existing hook
    pub async fn backup_hook(&self, hook_type: HookType, hook_path: &Path) -> Result<BackupInfo> {
        debug!("Creating backup for {} hook", hook_type.as_str());

        let backup_path = self.get_backup_path(&hook_type);
        let backup_time = SystemTime::now();

        // Get original file size
        let metadata = fs::metadata(hook_path).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("read hook metadata: {}", hook_path.display()),
                path: hook_path.to_path_buf(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;
        let original_size = metadata.len();

        // Copy to backup location
        fs::copy(hook_path, &backup_path).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("create backup: {}", backup_path.display()),
                path: backup_path.clone(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;

        info!(
            "Created backup for {} hook at {}",
            hook_type.as_str(),
            backup_path.display()
        );

        Ok(BackupInfo {
            hook_type,
            original_path: hook_path.to_path_buf(),
            backup_path,
            backup_time,
            original_size,
        })
    }

    /// Restore hook from backup
    pub async fn restore_hook(&self, hook_type: HookType) -> Result<RestoreInfo> {
        debug!("Restoring backup for {} hook", hook_type.as_str());

        let backup_path = self.get_backup_path(&hook_type);
        let hook_path = self.hooks_dir.join(hook_type.as_str());
        let restore_time = SystemTime::now();

        if !backup_path.exists() {
            return Err(SnpError::Git(Box::new(GitError::CommandFailed {
                command: "restore backup".to_string(),
                exit_code: Some(1),
                stdout: String::new(),
                stderr: format!("No backup found for {} hook", hook_type.as_str()),
                working_dir: Some(self.hooks_dir.clone()),
            })));
        }

        // Copy backup to hook location
        fs::copy(&backup_path, &hook_path).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("restore backup: {}", hook_path.display()),
                path: hook_path.clone(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;

        // Remove backup file
        fs::remove_file(&backup_path).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("remove backup file: {}", backup_path.display()),
                path: backup_path.clone(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;

        info!("Restored backup for {} hook", hook_type.as_str());

        Ok(RestoreInfo {
            hook_type,
            restored_path: hook_path,
            original_backup_path: backup_path,
            restore_time,
        })
    }

    /// List available backups
    pub async fn list_backups(&self) -> Result<Vec<BackupInfo>> {
        debug!("Listing available backups");

        let mut backups = Vec::new();

        for hook_type in HookType::all() {
            let backup_path = self.get_backup_path(&hook_type);
            if backup_path.exists() {
                let metadata = fs::metadata(&backup_path)?;
                let backup_time = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                let original_size = metadata.len();

                backups.push(BackupInfo {
                    hook_type: hook_type.clone(),
                    original_path: self.hooks_dir.join(hook_type.as_str()),
                    backup_path,
                    backup_time,
                    original_size,
                });
            }
        }

        Ok(backups)
    }

    /// Clean old backups
    pub async fn cleanup_backups(&self, retention_days: u32) -> Result<CleanupResult> {
        debug!("Cleaning up backups older than {} days", retention_days);

        let mut result = CleanupResult {
            cleaned_files: 0,
            bytes_freed: 0,
            errors: Vec::new(),
        };

        let cutoff_time = SystemTime::now()
            - std::time::Duration::from_secs(retention_days as u64 * 24 * 60 * 60);

        // Read hooks directory
        let entries = fs::read_dir(&self.hooks_dir).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("read hooks directory: {}", self.hooks_dir.display()),
                path: self.hooks_dir.clone(),
                suggestion: Some("Check directory permissions".to_string()),
            }))
        })?;

        for entry in entries {
            let entry = entry.map_err(|_e| {
                SnpError::Git(Box::new(GitError::PermissionDenied {
                    operation: "read directory entry".to_string(),
                    path: self.hooks_dir.clone(),
                    suggestion: Some("Check directory permissions".to_string()),
                }))
            })?;

            let path = entry.path();
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

            // Check if this is a backup file
            if file_name.ends_with(".legacy") {
                if let Ok(metadata) = fs::metadata(&path) {
                    let modified_time = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);

                    if modified_time < cutoff_time {
                        match fs::remove_file(&path) {
                            Ok(()) => {
                                result.cleaned_files += 1;
                                result.bytes_freed += metadata.len();
                            }
                            Err(e) => {
                                result.errors.push(format!(
                                    "Failed to remove {}: {}",
                                    path.display(),
                                    e
                                ));
                            }
                        }
                    }
                }
            }
        }

        info!(
            "Cleanup completed: {} files, {} bytes freed",
            result.cleaned_files, result.bytes_freed
        );
        Ok(result)
    }

    /// Get backup path for a hook type
    fn get_backup_path(&self, hook_type: &HookType) -> PathBuf {
        self.hooks_dir
            .join(format!("{}.legacy", hook_type.as_str()))
    }
}

/// Generate SNP hook scripts
pub struct HookTemplateGenerator {
    templates: HashMap<HookType, String>,
}

impl HookTemplateGenerator {
    /// Create a new HookTemplateGenerator
    pub fn new() -> Self {
        let mut templates = HashMap::new();

        // Base template for all hooks
        let base_template = r#"#!/usr/bin/env bash
# Generated by SNP (Shell Not Pass)
#
# This hook was installed by SNP to run pre-commit checks.
# To skip this hook, use --no-verify or set SKIP=hook_name
# To temporarily disable SNP, set PRE_COMMIT=0

# Exit on any error
set -e

# Skip if PRE_COMMIT is set to 0
if [ "$PRE_COMMIT" = "0" ]; then
    exit 0
fi

# Find SNP binary
SNP_BIN=""
if command -v snp >/dev/null 2>&1; then
    SNP_BIN="snp"
elif [ -f "./target/release/snp" ]; then
    SNP_BIN="./target/release/snp"
elif [ -f "./target/debug/snp" ]; then
    SNP_BIN="./target/debug/snp"
else
    echo "Error: SNP binary not found. Please install SNP or build it locally."
    exit 1
fi

# Run SNP with the appropriate hook stage
exec "$SNP_BIN" run --hook-stage="{hook_stage}" {args}
"#;

        // Create templates for each hook type
        for hook_type in HookType::all() {
            templates.insert(hook_type, base_template.to_string());
        }

        Self { templates }
    }

    /// Generate hook script content
    pub fn generate_hook_script(&self, hook_type: HookType, config: &HookConfig) -> Result<String> {
        debug!("Generating hook script for {}", hook_type.as_str());

        let template = self.get_template(&hook_type)?;

        // Replace placeholders
        let mut script = template.replace("{hook_stage}", hook_type.as_str());

        // Add additional arguments
        let args = if config.additional_args.is_empty() {
            String::new()
        } else {
            format!(" {}", config.additional_args.join(" "))
        };
        script = script.replace("{args}", &args);

        // Add config file argument if not default
        if config.config_file != ".pre-commit-config.yaml" {
            script = script.replace(
                "exec \"$SNP_BIN\" run",
                &format!("exec \"$SNP_BIN\" --config=\"{}\" run", config.config_file),
            );
        }

        // Add allow missing config flag if needed
        if config.allow_missing_config {
            script = script.replace(
                "exec \"$SNP_BIN\" run",
                "exec \"$SNP_BIN\" run --allow-missing-config",
            );
        }

        Ok(script)
    }

    /// Get template for specific hook type
    pub fn get_template(&self, hook_type: &HookType) -> Result<&String> {
        self.templates.get(hook_type).ok_or_else(|| {
            SnpError::Git(Box::new(GitError::InvalidReference {
                reference: hook_type.as_str().to_string(),
                repository: "hook template".to_string(),
                suggestion: Some(
                    "Supported hook types: pre-commit, pre-push, commit-msg, etc.".to_string(),
                ),
            }))
        })
    }

    /// Validate generated script
    pub fn validate_script(&self, script: &str) -> Result<bool> {
        // Basic validation checks
        if !script.contains("#!/usr/bin/env bash") {
            return Ok(false);
        }

        if !script.contains("Generated by SNP") {
            return Ok(false);
        }

        if !script.contains("exec \"$SNP_BIN\"") {
            return Ok(false);
        }

        Ok(true)
    }
}

impl Default for HookTemplateGenerator {
    fn default() -> Self {
        Self::new()
    }
}
