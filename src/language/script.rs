// Script language plugin for executing shell scripts from hook repositories
// This plugin handles the "script" language type which executes scripts relative to the repository root

use async_trait::async_trait;
use std::path::{Path, PathBuf};

use crate::core::Hook;
use crate::error::{Result, SnpError};
use crate::execution::HookExecutionResult;

use super::dependency::{Dependency, DependencyManager};
use super::environment::{
    EnvironmentConfig, EnvironmentInfo, LanguageEnvironment, ValidationReport,
};
use super::system::SystemLanguagePlugin;
use super::traits::{Command, Language, LanguageConfig, LanguageError};

/// Script language plugin for executing shell scripts from repositories
///
/// This plugin handles the "script" language type which is used by pre-commit
/// to execute shell scripts that are part of the hook repository itself,
/// as opposed to system commands that are globally available.
pub struct ScriptLanguagePlugin {
    system_plugin: SystemLanguagePlugin,
}

impl Default for ScriptLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptLanguagePlugin {
    pub fn new() -> Self {
        Self {
            system_plugin: SystemLanguagePlugin::new(),
        }
    }

    /// Resolve script path relative to repository root
    fn resolve_script_path(&self, hook: &Hook, env: &LanguageEnvironment) -> Result<PathBuf> {
        // The entry should be a relative path within the repository
        let script_path = Path::new(&hook.entry);

        // If it's already absolute, use it as-is (shouldn't happen but handle it)
        if script_path.is_absolute() {
            return Ok(script_path.to_path_buf());
        }

        // Resolve relative to the repository executable path
        let full_path = env.executable_path.join(script_path);

        tracing::debug!(
            "Script plugin: resolving '{}' relative to '{}' -> '{}'",
            hook.entry,
            env.executable_path.display(),
            full_path.display()
        );

        // Verify the script exists
        if !full_path.exists() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "script".to_string(),
                error: format!(
                    "Script not found: {} (resolved to {})",
                    hook.entry,
                    full_path.display()
                ),
                recovery_suggestion: Some(
                    "Check that the script path is correct and the file exists in the repository"
                        .to_string(),
                ),
            }));
        }

        Ok(full_path)
    }

    /// Build a modified hook with the resolved script path
    fn build_resolved_hook(&self, hook: &Hook, script_path: &Path) -> Hook {
        let mut resolved_hook = hook.clone();

        // Replace the entry with the resolved absolute path
        resolved_hook.entry = script_path.to_string_lossy().to_string();

        tracing::debug!(
            "Script plugin: converted hook entry from '{}' to '{}'",
            hook.entry,
            resolved_hook.entry
        );

        resolved_hook
    }
}

#[async_trait]
impl Language for ScriptLanguagePlugin {
    fn language_name(&self) -> &str {
        "script"
    }

    fn supported_extensions(&self) -> &[&str] {
        // Script language supports shell script extensions
        &["sh", "bash", "zsh", "fish", "ps1", "bat", "cmd"]
    }

    fn detection_patterns(&self) -> &[&str] {
        // Same as system plugin for shell scripts
        &[
            r"^#\!/bin/(sh|bash|zsh|fish)",
            r"^#\!/usr/bin/(sh|bash|zsh|fish)",
            r"^#\!/usr/bin/env (sh|bash|zsh|fish)",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        // Delegate to system plugin but ensure we have repository path set correctly
        let mut env = self.system_plugin.setup_environment(config).await?;

        // Update the language name to "script"
        env.language = "script".to_string();

        // For script language, the executable_path should point to the repository root
        // where the script files are located, not the working directory
        if let Some(repo_path) = &config.repository_path {
            env.executable_path = repo_path.clone();
            tracing::debug!(
                "Script plugin: set executable_path to repository root: {}",
                env.executable_path.display()
            );
        } else {
            // If no repository path is set, this is an error for script language
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "script".to_string(),
                error: "No repository path provided for script language".to_string(),
                recovery_suggestion: Some(
                    "Script language requires a repository path to locate script files".to_string(),
                ),
            }));
        }

        Ok(env)
    }

    async fn install_dependencies(
        &self,
        env: &LanguageEnvironment,
        dependencies: &[Dependency],
    ) -> Result<()> {
        // Script language doesn't install dependencies - they should be system dependencies
        self.system_plugin
            .install_dependencies(env, dependencies)
            .await
    }

    async fn cleanup_environment(&self, env: &LanguageEnvironment) -> Result<()> {
        // Delegate to system plugin
        self.system_plugin.cleanup_environment(env).await
    }

    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        tracing::debug!(
            "Script plugin executing hook: {} with entry: {}",
            hook.id,
            hook.entry
        );

        // Resolve the script path relative to repository root
        let script_path = self.resolve_script_path(hook, env)?;

        // Create a modified hook with the resolved path
        let resolved_hook = self.build_resolved_hook(hook, &script_path);

        // Delegate execution to system plugin with the resolved hook
        let mut result = self
            .system_plugin
            .execute_hook(&resolved_hook, env, files)
            .await?;

        // Restore the original hook ID
        result.hook_id = hook.id.clone();

        Ok(result)
    }

    fn build_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        tracing::debug!(
            "Script plugin building command for hook: {} with entry: {}",
            hook.id,
            hook.entry
        );

        // Resolve the script path relative to repository root
        let script_path = self.resolve_script_path(hook, env)?;

        // Create a modified hook with the resolved path
        let resolved_hook = self.build_resolved_hook(hook, &script_path);

        // Delegate to system plugin with the resolved hook
        self.system_plugin.build_command(&resolved_hook, env, files)
    }

    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>> {
        // Delegate to system plugin
        self.system_plugin.resolve_dependencies(dependencies).await
    }

    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency> {
        // Delegate to system plugin
        self.system_plugin.parse_dependency(dep_spec)
    }

    fn get_dependency_manager(&self) -> &dyn DependencyManager {
        // Delegate to system plugin
        self.system_plugin.get_dependency_manager()
    }

    fn get_environment_info(&self, env: &LanguageEnvironment) -> EnvironmentInfo {
        // Delegate to system plugin but update language name
        let mut info = self.system_plugin.get_environment_info(env);
        info.language = "script".to_string();
        info
    }

    fn validate_environment(&self, env: &LanguageEnvironment) -> Result<ValidationReport> {
        // Delegate to system plugin
        self.system_plugin.validate_environment(env)
    }

    fn default_config(&self) -> LanguageConfig {
        // Use system plugin default config
        self.system_plugin.default_config()
    }

    fn validate_config(&self, config: &LanguageConfig) -> Result<()> {
        // Delegate to system plugin
        self.system_plugin.validate_config(config)
    }
}
