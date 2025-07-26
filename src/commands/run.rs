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
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
use std::time::{SystemTime, UNIX_EPOCH};

static STORAGE_COUNTER: AtomicU64 = AtomicU64::new(0);

// Use OnceLock to create a global mutex for test storage creation
use std::sync::{Mutex, OnceLock};

static TEST_STORAGE_CREATION_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

/// Create an isolated storage instance for tests to prevent database locking issues
fn create_test_storage() -> Result<Arc<Store>> {
    // Use a global lock to prevent concurrent storage creation during tests
    // This is only used during tests to prevent SQLite database locking issues
    let _lock = TEST_STORAGE_CREATION_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap();

    let counter = STORAGE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let thread_id = std::thread::current().id();
    let process_id = std::process::id();

    // Add timestamp and random component for maximum uniqueness
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    // Use thread ID hash and timestamp for pseudo-randomness to avoid adding rand dependency
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    thread_id.hash(&mut hasher);
    let thread_hash = hasher.finish();
    let random_component = thread_hash.wrapping_mul(timestamp as u64);

    let unique_cache_dir = std::env::temp_dir().join(format!(
        "snp_test_cache_{counter}_{process_id}_{thread_id:?}_{timestamp}_{random_component}"
    ));

    // Ensure the directory doesn't exist from previous runs
    if unique_cache_dir.exists() {
        let _ = std::fs::remove_dir_all(&unique_cache_dir);
    }

    std::fs::create_dir_all(&unique_cache_dir)?;

    // Add a small random delay to reduce timing conflicts during concurrent test execution
    // This helps prevent simultaneous SQLite database initialization
    let delay_ms = random_component % 50; // 0-49ms random delay
    if delay_ms > 0 {
        std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    }

    tracing::debug!("Created test storage directory: {:?}", unique_cache_dir);

    // Create storage instance with enhanced isolation
    let store = Store::with_cache_directory(unique_cache_dir.clone())?;

    Ok(Arc::new(store))
}

/// Check if we're running in a test environment
fn is_test_environment() -> bool {
    cfg!(test)
        || std::env::var("CARGO_TEST").is_ok()
        || std::env::var("CARGO").is_ok()
        || std::thread::current().name().unwrap_or("").contains("test")
}

/// Execute the run command with the given configuration
pub async fn execute_run_command(
    repo_path: &Path,
    config_file: &str,
    execution_config: &ExecutionConfig,
) -> Result<ExecutionResult> {
    // Initialize components
    let git_repo = GitRepository::discover_from_path(repo_path)?;

    // Initialize storage with graceful fallback on failure
    // For tests, always use unique temporary directories to avoid database locking
    let storage = if is_test_environment() {
        create_test_storage()?
    } else {
        match Store::new() {
            Ok(store) => Arc::new(store),
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize storage, continuing without caching: {}",
                    e
                );
                // Create a fallback store for production use
                create_test_storage()?
            }
        }
    };

    let process_manager = Arc::new(ProcessManager::new());
    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Load configuration with resolved external repository hooks
    let config_path = repo_path.join(config_file);
    let config = Config::from_file_with_resolved_hooks(&config_path).await?;

    // Get hooks for the specified stage
    let hooks = get_hooks_for_stage(&config, &execution_config.stage)?;
    tracing::debug!(
        "Found {} hooks for stage {:?}",
        hooks.len(),
        execution_config.stage
    );

    // Get files to process - if no files specified, use staged files
    let mut config = execution_config.clone();
    if config.files.is_empty() && !config.all_files {
        let staged_files = git_repo.staged_files()?;
        tracing::debug!("Using {} staged files", staged_files.len());
        config = config.with_files(staged_files);
    }

    // Set working directory from repo_path
    config = config.with_working_directory(repo_path.to_path_buf());

    // Execute hooks
    tracing::debug!("Executing hooks");
    execution_engine
        .execute_hooks(&hooks, config)
        .await
        .map_err(|e| {
            tracing::error!("Hook execution failed: {}", e);
            e
        })
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
    // Load configuration with resolved external repository hooks and filter to single hook
    let config_path = repo_path.join(config_file);
    let config = Config::from_file_with_resolved_hooks(&config_path).await?;

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

    // Use the same storage initialization logic as the main execute_run_command
    let git_repo = GitRepository::discover_from_path(repo_path)?;
    let storage = if is_test_environment() {
        create_test_storage()?
    } else {
        match Store::new() {
            Ok(store) => Arc::new(store),
            Err(e) => {
                tracing::warn!(
                    "Failed to initialize storage, continuing without caching: {}",
                    e
                );
                create_test_storage()?
            }
        }
    };

    let process_manager = Arc::new(ProcessManager::new());
    let mut execution_engine = HookExecutionEngine::new(process_manager, storage);

    // Get files to process - if no files specified, use staged files
    let mut config = execution_config.clone();
    if config.files.is_empty() && !config.all_files {
        let staged_files = git_repo.staged_files()?;
        tracing::debug!("Using {} staged files", staged_files.len());
        config = config.with_files(staged_files);
    }

    // Set working directory from repo_path
    config = config.with_working_directory(repo_path.to_path_buf());

    // Execute hooks
    tracing::debug!("Executing single hook: {}", hook_id);
    execution_engine
        .execute_hooks(&filtered_hooks, config)
        .await
        .map_err(|e| {
            tracing::error!("Single hook execution failed: {}", e);
            e
        })
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
pub fn get_hooks_for_stage(config: &Config, stage: &Stage) -> Result<Vec<Hook>> {
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
