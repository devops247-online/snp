// Run command implementation - Core functionality for executing hooks
// Based on Python pre-commit's run.py logic with Rust-specific optimizations

use crate::config::Config;
use crate::core::{Hook, Stage};
use crate::error::{Result, SnpError};
use crate::execution::{ExecutionConfig, ExecutionResult, HookExecutionEngine};
use crate::git::GitRepository;
use crate::process::ProcessManager;
use crate::storage::Store;
use std::path::{Path, PathBuf};
use std::sync::Arc;
// use std::time::Duration; // Commented out to avoid unused import warning

/// Execute the run command with the given configuration
pub async fn execute_run_command(
    repo_path: &Path,
    config_file: &str,
    execution_config: &ExecutionConfig,
) -> Result<ExecutionResult> {
    // Initialize components
    let git_repo = GitRepository::discover_from_path(repo_path)?;
    
    // Initialize storage with graceful fallback on failure
    let storage = match Store::new() {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::warn!("Failed to initialize storage, continuing without caching: {}", e);
            // Create a minimal dummy store that will fail operations gracefully
            Arc::new(Store::with_cache_directory(tempfile::tempdir()?.path().to_path_buf())?)
        }
    };
    
    let process_manager = Arc::new(ProcessManager::new());
    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Load configuration
    let config_path = repo_path.join(config_file);
    let config = Config::from_file(&config_path)?;

    // Get hooks for the specified stage
    let hooks = get_hooks_for_stage(&config, &execution_config.stage)?;

    // Get files to process - if no files specified, use staged files
    let mut config = execution_config.clone();
    if config.files.is_empty() && !config.all_files {
        let staged_files = git_repo.staged_files()?;
        config = config.with_files(staged_files);
    }

    // Execute hooks
    execution_engine.execute_hooks(&hooks, config).await
}

/// Execute run command on all files in the repository
pub async fn execute_run_command_all_files(
    repo_path: &Path,
    config_file: &str,
    execution_config: &ExecutionConfig,
) -> Result<ExecutionResult> {
    let git_repo = GitRepository::discover_from_path(repo_path)?;
    let all_files = git_repo.all_files()?;

    let config = execution_config.clone().with_files(all_files);
    execute_run_command(repo_path, config_file, &config).await
}

/// Execute run command with specific files
pub async fn execute_run_command_with_files(
    repo_path: &Path,
    config_file: &str,
    execution_config: &ExecutionConfig,
) -> Result<ExecutionResult> {
    // Files are already set in execution_config
    execute_run_command(repo_path, config_file, execution_config).await
}

/// Execute run command for a single hook
pub async fn execute_run_command_single_hook(
    repo_path: &Path,
    config_file: &str,
    hook_id: &str,
    execution_config: &ExecutionConfig,
) -> Result<ExecutionResult> {
    // Load configuration and filter to single hook
    let config_path = repo_path.join(config_file);
    let config = Config::from_file(&config_path)?;

    let hooks = get_hooks_for_stage(&config, &execution_config.stage)?;
    let filtered_hooks: Vec<Hook> = hooks
        .into_iter()
        .filter(|hook| hook.id == hook_id)
        .collect();

    if filtered_hooks.is_empty() {
        return Err(SnpError::Config(Box::new(
            crate::error::ConfigError::InvalidHookId {
                hook_id: hook_id.to_string(),
                available_hooks: vec![], // TODO: Get available hook IDs
            },
        )));
    }

    // Initialize execution engine with graceful storage fallback
    let storage = match Store::new() {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::warn!("Failed to initialize storage, continuing without caching: {}", e);
            Arc::new(Store::with_cache_directory(tempfile::tempdir()?.path().to_path_buf())?)
        }
    };
    let process_manager = Arc::new(ProcessManager::new());
    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    execution_engine
        .execute_hooks(&filtered_hooks, execution_config.clone())
        .await
}

/// Execute run command with git refs (from-ref to to-ref)
pub async fn execute_run_command_with_refs(
    repo_path: &Path,
    config_file: &str,
    from_ref: &str,
    to_ref: &str,
    execution_config: &ExecutionConfig,
) -> Result<ExecutionResult> {
    let git_repo = GitRepository::discover_from_path(repo_path)?;
    let changed_files = git_repo.changed_files(from_ref, to_ref)?;

    let config = execution_config.clone().with_files(changed_files);
    execute_run_command(repo_path, config_file, &config).await
}

/// Filter files for a specific hook based on its patterns
pub fn filter_files_for_hook(hook: &Hook, files: &[PathBuf]) -> Result<Vec<PathBuf>> {
    // Use ExecutionContext to filter files for the hook
    let execution_context =
        crate::core::ExecutionContext::new(Stage::PreCommit).with_files(files.to_vec());

    execution_context.filtered_files(hook)
}

/// Get hooks for a specific stage from configuration
fn get_hooks_for_stage(config: &Config, stage: &Stage) -> Result<Vec<Hook>> {
    let mut hooks = Vec::new();

    for repo in &config.repos {
        for hook_config in &repo.hooks {
            let hook = Hook::from_config(hook_config, &repo.repo)?;

            // Check if hook runs for this stage
            if hook.runs_for_stage(stage) {
                hooks.push(hook);
            }
        }
    }

    Ok(hooks)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    async fn create_test_repo() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to init git");

        // Configure git
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to config git");

        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to config git");

        (temp_dir, repo_path)
    }

    #[tokio::test]
    async fn test_run_command_integration() {
        let (_temp_dir, repo_path) = create_test_repo().await;

        // Create a basic config
        let config_content = r#"
repos:
  - repo: local
    hooks:
      - id: echo-test
        name: Echo Test
        entry: echo
        language: system
        args: ["test"]
        always_run: true
"#;
        fs::write(repo_path.join(".pre-commit-config.yaml"), config_content).unwrap();

        let config = ExecutionConfig::new(Stage::PreCommit);
        let result = execute_run_command(&repo_path, ".pre-commit-config.yaml", &config).await;

        // This test will initially fail (Red phase) until we properly implement
        // all the required components and integrations
        assert!(result.is_ok());
    }
}
