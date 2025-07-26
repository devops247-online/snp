// Hook Execution Engine - Core component that orchestrates hook execution pipeline
// Provides environment setup, file filtering, process execution, output collection, and result aggregation

use crate::core::{ExecutionContext, Hook, Stage};
use crate::error::{HookExecutionError, Result};
use crate::language::environment::{EnvironmentConfig, EnvironmentManager};
use crate::language::registry::LanguageRegistry;
use crate::process::ProcessManager;
use crate::storage::Store;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

/// Hook execution configuration
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub stage: Stage,
    pub files: Vec<PathBuf>,
    pub all_files: bool,
    pub fail_fast: bool,
    pub show_diff_on_failure: bool,
    pub hook_timeout: Duration,
    pub max_parallel_hooks: usize,
    pub verbose: bool,
    pub color: bool,
    pub user_output: Option<crate::user_output::UserOutput>,
    pub working_directory: Option<PathBuf>,
}

impl ExecutionConfig {
    pub fn new(stage: Stage) -> Self {
        Self {
            stage,
            files: Vec::new(),
            all_files: false,
            fail_fast: false,
            show_diff_on_failure: false,
            hook_timeout: Duration::from_secs(60),
            max_parallel_hooks: 1,
            verbose: false,
            color: true,
            user_output: None,
            working_directory: None,
        }
    }

    pub fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.files = files;
        self
    }

    pub fn with_all_files(mut self, all_files: bool) -> Self {
        self.all_files = all_files;
        self
    }

    pub fn with_fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn with_hook_timeout(mut self, timeout: Duration) -> Self {
        self.hook_timeout = timeout;
        self
    }

    pub fn with_user_output(mut self, user_output: crate::user_output::UserOutput) -> Self {
        self.user_output = Some(user_output);
        self
    }

    pub fn with_working_directory(mut self, working_directory: PathBuf) -> Self {
        self.working_directory = Some(working_directory);
        self
    }
}

/// Individual hook execution result
#[derive(Debug, Clone)]
pub struct HookExecutionResult {
    pub hook_id: String,
    pub success: bool,
    pub skipped: bool,
    pub exit_code: Option<i32>,
    pub duration: Duration,
    pub files_processed: Vec<PathBuf>,
    pub files_modified: Vec<PathBuf>,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<HookExecutionError>,
}

impl HookExecutionResult {
    pub fn new(hook_id: String) -> Self {
        Self {
            hook_id,
            success: false,
            skipped: false,
            exit_code: None,
            duration: Duration::new(0, 0),
            files_processed: Vec::new(),
            files_modified: Vec::new(),
            stdout: String::new(),
            stderr: String::new(),
            error: None,
        }
    }

    pub fn success(mut self, exit_code: i32, duration: Duration) -> Self {
        self.success = true;
        self.exit_code = Some(exit_code);
        self.duration = duration;
        self
    }

    pub fn failure(
        mut self,
        exit_code: i32,
        duration: Duration,
        error: HookExecutionError,
    ) -> Self {
        self.success = false;
        self.exit_code = Some(exit_code);
        self.duration = duration;
        self.error = Some(error);
        self
    }

    pub fn with_output(mut self, stdout: String, stderr: String) -> Self {
        self.stdout = stdout;
        self.stderr = stderr;
        self
    }

    pub fn with_files(mut self, processed: Vec<PathBuf>, modified: Vec<PathBuf>) -> Self {
        self.files_processed = processed;
        self.files_modified = modified;
        self
    }

    pub fn skipped(mut self) -> Self {
        self.skipped = true;
        self.success = false; // Skipped hooks are not considered successful
        self
    }
}

/// Aggregated execution result for all hooks
#[derive(Debug)]
pub struct ExecutionResult {
    pub success: bool,
    pub hooks_executed: usize,
    pub hooks_passed: Vec<HookExecutionResult>,
    pub hooks_failed: Vec<HookExecutionResult>,
    pub hooks_skipped: Vec<String>,
    pub total_duration: Duration,
    pub files_modified: Vec<PathBuf>,
}

impl ExecutionResult {
    pub fn new() -> Self {
        Self {
            success: true,
            hooks_executed: 0,
            hooks_passed: Vec::new(),
            hooks_failed: Vec::new(),
            hooks_skipped: Vec::new(),
            total_duration: Duration::new(0, 0),
            files_modified: Vec::new(),
        }
    }

    pub fn add_result(&mut self, result: HookExecutionResult) {
        self.hooks_executed += 1;
        self.total_duration += result.duration;

        // Collect modified files
        for file in &result.files_modified {
            if !self.files_modified.contains(file) {
                self.files_modified.push(file.clone());
            }
        }

        if result.success {
            self.hooks_passed.push(result);
        } else {
            self.success = false;
            self.hooks_failed.push(result);
        }
    }

    pub fn add_skipped(&mut self, hook_id: String) {
        self.hooks_skipped.push(hook_id);
    }
}

impl Default for ExecutionResult {
    fn default() -> Self {
        Self::new()
    }
}

/// Main hook execution engine
pub struct HookExecutionEngine {
    #[allow(dead_code)]
    process_manager: Arc<ProcessManager>,
    storage: Arc<Store>,
    language_registry: Arc<crate::language::registry::LanguageRegistry>,
    environment_manager: Arc<std::sync::Mutex<crate::language::environment::EnvironmentManager>>,
}

impl HookExecutionEngine {
    pub fn new(process_manager: Arc<ProcessManager>, storage: Arc<Store>) -> Self {
        // Initialize language registry with built-in plugins
        let language_registry = Arc::new(LanguageRegistry::new());
        if let Err(e) = language_registry.load_builtin_plugins() {
            tracing::warn!("Failed to load builtin language plugins: {}", e);
        }

        // Initialize environment manager
        let cache_root = storage.cache_directory().join("environments");
        let environment_manager = Arc::new(std::sync::Mutex::new(EnvironmentManager::new(
            storage.clone(),
            cache_root,
        )));

        Self {
            process_manager,
            storage,
            language_registry,
            environment_manager,
        }
    }

    /// Execute a single hook with the given files
    pub async fn execute_single_hook(
        &mut self,
        hook: &Hook,
        files: &[PathBuf],
        config: &ExecutionConfig,
    ) -> Result<HookExecutionResult> {
        let start_time = SystemTime::now();
        let mut result = HookExecutionResult::new(hook.id.clone());

        // Check if hook should run for this stage
        if !hook.runs_for_stage(&config.stage) {
            return Ok(result);
        }

        // Filter files for this hook
        let execution_context = ExecutionContext::new(config.stage.clone())
            .with_files(files.to_vec())
            .with_verbose(config.verbose);

        let filtered_files = execution_context.filtered_files(hook)?;

        // Skip if no files match and not always_run
        if filtered_files.is_empty() && !hook.always_run {
            let _duration = start_time.elapsed().unwrap_or_default();
            return Ok(result.skipped().with_files(vec![], vec![]));
        }

        result.files_processed = filtered_files.clone();

        // Determine the language for this hook (default to "system" for most pre-commit hooks)
        let language = if hook.language.is_empty() {
            "system"
        } else {
            &hook.language
        };

        tracing::debug!("Hook {} uses language: {}", hook.id, language);

        // Get the language plugin
        let language_plugin = match self.language_registry.get_plugin(language) {
            Some(plugin) => plugin,
            None => {
                // Default to system plugin if language not found
                self.language_registry.get_plugin("system").unwrap()
            }
        };

        // Create environment configuration
        let mut env_config = EnvironmentConfig::new()
            .with_dependencies(hook.additional_dependencies.clone())
            .with_timeout(config.hook_timeout);

        // Set working directory if provided
        if let Some(ref working_dir) = config.working_directory {
            env_config = env_config.with_working_directory(working_dir.clone());
        }

        // Check if this hook needs repository installation
        // If the hook entry is not a system command (doesn't start with / and not in PATH),
        // we need to find and install it from a repository
        let executable_name = hook.entry.split_whitespace().next().unwrap_or(&hook.entry);
        let needs_repo_installation = !hook.entry.starts_with('/')
            && !hook.entry.contains('/')
            && which::which(executable_name).is_err();

        if needs_repo_installation {
            // Try to find the repository that contains this hook
            // We'll search through repositories in storage to find one that contains this hook
            let repo_path = self.find_repository_for_hook(&hook.id).await?;
            if let Some(path) = repo_path {
                env_config = env_config.with_repository_path(path);
            }
        }

        // Get or create environment for this hook
        tracing::debug!("Setting up environment for hook {}", hook.id);
        let environment = {
            // First, let the language plugin set up the environment
            let language_env = language_plugin.setup_environment(&env_config).await?;
            tracing::debug!("Language plugin setup completed for hook {}", hook.id);

            // Register it with the environment manager
            let _env_manager = self.environment_manager.lock().unwrap();
            std::sync::Arc::new(language_env)
        };
        tracing::debug!("Environment ready for hook {}", hook.id);

        // Install additional dependencies if needed
        tracing::debug!("Installing additional dependencies for hook {}", hook.id);
        if !hook.additional_dependencies.is_empty() {
            let dependencies = language_plugin
                .resolve_dependencies(&hook.additional_dependencies)
                .await?;
            language_plugin
                .install_dependencies(&environment, &dependencies)
                .await?;
        }
        tracing::debug!("Dependencies installed for hook {}", hook.id);

        // Execute hook through language plugin
        tracing::debug!("Executing hook {} through language plugin", hook.id);
        let mut hook_result = language_plugin
            .execute_hook(hook, &environment, &filtered_files)
            .await?;
        tracing::debug!("Hook {} execution completed", hook.id);

        // Ensure duration is set properly
        let duration = start_time.elapsed().unwrap_or(Duration::new(0, 0));
        hook_result.duration = duration;

        Ok(hook_result)
    }

    /// Find the repository path that contains the given hook
    async fn find_repository_for_hook(&self, hook_id: &str) -> Result<Option<PathBuf>> {
        // Get list of all repositories from storage
        let repos = self.storage.list_repositories()?;

        for repo_info in repos {
            let hooks_file = repo_info.path.join(".pre-commit-hooks.yaml");
            if hooks_file.exists() {
                // Read and parse the hooks file
                if let Ok(content) = std::fs::read_to_string(&hooks_file) {
                    if let Ok(hook_defs) = serde_yaml::from_str::<Vec<serde_yaml::Value>>(&content)
                    {
                        for hook_def in hook_defs {
                            if let Some(id) = hook_def.get("id").and_then(|v| v.as_str()) {
                                if id == hook_id {
                                    return Ok(Some(repo_info.path));
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Execute multiple hooks for a given stage
    pub async fn execute_hooks(
        &mut self,
        hooks: &[Hook],
        config: ExecutionConfig,
    ) -> Result<ExecutionResult> {
        tracing::debug!("Starting execution of {} hooks", hooks.len());
        let mut result = ExecutionResult::new();

        for (i, hook) in hooks.iter().enumerate() {
            tracing::debug!("Executing hook {}/{}: {}", i + 1, hooks.len(), hook.id);

            if config.fail_fast && !result.success {
                tracing::debug!("Skipping hook {} due to fail_fast", hook.id);
                result.add_skipped(hook.id.clone());
                continue;
            }

            let hook_result = match self.execute_single_hook(hook, &config.files, &config).await {
                Ok(result) => result,
                Err(e) => {
                    tracing::error!("Failed to execute hook {}: {}", hook.id, e);
                    // Create a failed result for this hook and continue with others
                    HookExecutionResult::new(hook.id.clone()).failure(
                        -1,
                        Duration::from_secs(0),
                        crate::error::HookExecutionError::EnvironmentSetupFailed {
                            language: hook.language.clone(),
                            hook_id: hook.id.clone(),
                            message: e.to_string(),
                            suggestion: None,
                        },
                    )
                }
            };

            // Handle skipped hooks differently
            if hook_result.skipped {
                result.add_skipped(hook.id.clone());
            } else {
                // Show hook start with user-friendly output
                if let Some(ref user_output) = config.user_output {
                    user_output.show_hook_start(&hook.id);
                }

                // Show hook result with user-friendly output
                if let Some(ref user_output) = config.user_output {
                    user_output.show_hook_result(&hook_result);
                }

                result.add_result(hook_result);
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Hook, Stage};
    use crate::process::ProcessManager;
    use crate::storage::Store;
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn create_test_engine() -> HookExecutionEngine {
        let process_manager = Arc::new(ProcessManager::new());
        // Create isolated storage for each test
        let temp_dir = TempDir::new().unwrap();
        let storage = Arc::new(Store::with_cache_directory(temp_dir.path().to_path_buf()).unwrap());
        HookExecutionEngine::new(process_manager, storage)
    }

    #[tokio::test]
    async fn test_single_hook_execution() {
        let mut engine = create_test_engine();

        // Test basic hook execution with success scenario
        let hook = Hook::new("echo-test", "echo", "system")
            .with_args(vec!["hello".to_string()])
            .with_stages(vec![Stage::PreCommit]);

        let config =
            ExecutionConfig::new(Stage::PreCommit).with_files(vec![PathBuf::from("test.txt")]);

        let result = engine
            .execute_single_hook(&hook, &config.files, &config)
            .await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        assert_eq!(hook_result.hook_id, "echo-test");
        // Note: This test will likely fail initially (Red phase)
        // because we haven't implemented ProcessManager::execute yet
    }

    #[tokio::test]
    async fn test_hook_execution_with_failure() {
        let mut engine = create_test_engine();

        // Test hook execution failure scenario
        let hook = Hook::new("false-test", "false", "system")
            .with_stages(vec![Stage::PreCommit])
            .always_run(true);

        let config = ExecutionConfig::new(Stage::PreCommit);

        let result = engine.execute_single_hook(&hook, &[], &config).await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        assert_eq!(hook_result.hook_id, "false-test");
        assert!(!hook_result.success);
        assert!(hook_result.error.is_some());
    }

    #[tokio::test]
    async fn test_hook_argument_passing() {
        let mut engine = create_test_engine();

        // Test hook argument passing and environment setup
        let hook = Hook::new("echo-args", "echo", "system")
            .with_args(vec!["--test".to_string(), "value".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .always_run(true);

        let config = ExecutionConfig::new(Stage::PreCommit);

        let result = engine.execute_single_hook(&hook, &[], &config).await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        assert!(hook_result.stdout.contains("--test"));
        assert!(hook_result.stdout.contains("value"));
    }

    #[tokio::test]
    async fn test_hook_output_capture() {
        let mut engine = create_test_engine();

        // Test output capture and result reporting
        let hook = Hook::new("output-test", "echo", "system")
            .with_args(vec!["test output".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .always_run(true); // Set always_run to true so it runs without files

        let config = ExecutionConfig::new(Stage::PreCommit);

        let result = engine.execute_single_hook(&hook, &[], &config).await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        assert!(hook_result.stdout.contains("test output"));
    }

    #[tokio::test]
    async fn test_hook_timeout_handling() {
        let mut engine = create_test_engine();

        // Test timeout handling and resource limits
        let hook = Hook::new("sleep-test", "sleep", "system")
            .with_args(vec!["2".to_string()])
            .with_stages(vec![Stage::PreCommit])
            .always_run(true);

        let config =
            ExecutionConfig::new(Stage::PreCommit).with_hook_timeout(Duration::from_millis(100));

        let result = engine.execute_single_hook(&hook, &[], &config).await;

        if let Err(ref e) = result {
            eprintln!("Hook execution failed: {e}");
        }
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        assert!(!hook_result.success);
        assert!(matches!(
            hook_result.error,
            Some(HookExecutionError::ExecutionTimeout { .. })
        ));
    }

    #[tokio::test]
    async fn test_multiple_hook_execution() {
        let mut engine = create_test_engine();

        // Test sequential execution of multiple hooks
        let hooks = vec![
            Hook::new("echo1", "echo", "system")
                .with_args(vec!["first".to_string()])
                .with_stages(vec![Stage::PreCommit])
                .always_run(true),
            Hook::new("echo2", "echo", "system")
                .with_args(vec!["second".to_string()])
                .with_stages(vec![Stage::PreCommit])
                .always_run(true),
        ];

        let config = ExecutionConfig::new(Stage::PreCommit);

        let result = engine.execute_hooks(&hooks, config).await;
        assert!(result.is_ok());

        let execution_result = result.unwrap();
        assert_eq!(execution_result.hooks_executed, 2);
        assert_eq!(execution_result.hooks_passed.len(), 2);
        assert!(execution_result.success);
    }

    #[tokio::test]
    async fn test_fail_fast_behavior() {
        let mut engine = create_test_engine();

        // Test fail-fast behavior and error propagation
        let hooks = vec![
            Hook::new("fail-hook", "false", "system")
                .with_stages(vec![Stage::PreCommit])
                .always_run(true),
            Hook::new("never-run", "echo", "system")
                .with_args(vec!["should not run".to_string()])
                .with_stages(vec![Stage::PreCommit])
                .always_run(true),
        ];

        let config = ExecutionConfig::new(Stage::PreCommit).with_fail_fast(true);

        let result = engine.execute_hooks(&hooks, config).await;
        assert!(result.is_ok());

        let execution_result = result.unwrap();
        assert!(!execution_result.success);
        assert_eq!(execution_result.hooks_failed.len(), 1);
        assert_eq!(execution_result.hooks_skipped.len(), 1);
        assert_eq!(execution_result.hooks_skipped[0], "never-run");
    }

    #[tokio::test]
    async fn test_hook_result_aggregation() {
        let mut engine = create_test_engine();

        // Test hook result aggregation and reporting
        let hooks = vec![
            Hook::new("success", "echo", "system")
                .with_args(vec!["success".to_string()])
                .with_stages(vec![Stage::PreCommit])
                .always_run(true),
            Hook::new("failure", "false", "system")
                .with_stages(vec![Stage::PreCommit])
                .always_run(true),
        ];

        let config = ExecutionConfig::new(Stage::PreCommit);

        let result = engine.execute_hooks(&hooks, config).await;
        assert!(result.is_ok());

        let execution_result = result.unwrap();
        assert!(!execution_result.success);
        assert_eq!(execution_result.hooks_executed, 2);
        assert_eq!(execution_result.hooks_passed.len(), 1);
        assert_eq!(execution_result.hooks_failed.len(), 1);
    }

    #[tokio::test]
    async fn test_file_filtering_integration() {
        let mut engine = create_test_engine();

        // Test staged file discovery and filtering
        let hook = Hook::new("python-only", "echo", "system")
            .with_files(r"\.py$".to_string())
            .with_stages(vec![Stage::PreCommit])
            .pass_filenames(true);

        let files = vec![
            PathBuf::from("test.py"),
            PathBuf::from("test.js"),
            PathBuf::from("script.py"),
        ];

        let config = ExecutionConfig::new(Stage::PreCommit).with_files(files);

        let result = engine
            .execute_single_hook(&hook, &config.files, &config)
            .await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        assert_eq!(hook_result.files_processed.len(), 2);
        assert!(hook_result
            .files_processed
            .contains(&PathBuf::from("test.py")));
        assert!(hook_result
            .files_processed
            .contains(&PathBuf::from("script.py")));
    }

    #[tokio::test]
    async fn test_hook_skipping_wrong_stage() {
        let mut engine = create_test_engine();

        // Test that hooks are skipped when stage doesn't match
        let hook = Hook::new("pre-push-only", "echo", "system").with_stages(vec![Stage::PrePush]);

        let config = ExecutionConfig::new(Stage::PreCommit);

        let result = engine.execute_single_hook(&hook, &[], &config).await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        // Hook should be skipped (no execution occurred)
        assert_eq!(hook_result.duration, Duration::new(0, 0));
    }

    #[tokio::test]
    async fn test_always_run_hook() {
        let mut engine = create_test_engine();

        // Test always_run behavior
        let hook = Hook::new("always-run", "echo", "system")
            .with_args(vec!["always".to_string()])
            .with_files(r"\.nonexistent$".to_string()) // Pattern that won't match
            .always_run(true)
            .with_stages(vec![Stage::PreCommit]);

        let config =
            ExecutionConfig::new(Stage::PreCommit).with_files(vec![PathBuf::from("test.py")]); // Doesn't match pattern

        let result = engine
            .execute_single_hook(&hook, &config.files, &config)
            .await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        // Should run despite no matching files
        assert!(hook_result.success);
        assert!(hook_result.stdout.contains("always"));
    }
}
