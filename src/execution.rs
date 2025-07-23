// Hook Execution Engine - Core component that orchestrates hook execution pipeline
// Provides environment setup, file filtering, process execution, output collection, and result aggregation

use crate::core::{ExecutionContext, Hook, Stage};
use crate::error::{HookExecutionError, Result, SnpError};
use crate::process::{ProcessConfig, ProcessManager};
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
}

/// Individual hook execution result
#[derive(Debug, Clone)]
pub struct HookExecutionResult {
    pub hook_id: String,
    pub success: bool,
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
    process_manager: Arc<ProcessManager>,
    #[allow(dead_code)] // Storage will be used for caching in future refactor phase
    storage: Arc<Store>,
}

impl HookExecutionEngine {
    pub fn new(process_manager: Arc<ProcessManager>, storage: Arc<Store>) -> Self {
        Self {
            process_manager,
            storage,
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
            return Ok(result);
        }

        result.files_processed = filtered_files.clone();

        // Prepare command
        let command_parts = hook.command();
        if command_parts.is_empty() {
            return Err(SnpError::HookExecution(Box::new(
                HookExecutionError::InvalidCommand {
                    hook_id: hook.id.clone(),
                    command: hook.entry.clone(),
                    error: "Empty command".to_string(),
                },
            )));
        }

        let mut process_config = ProcessConfig::new(&command_parts[0]);
        if command_parts.len() > 1 {
            let args: Vec<std::ffi::OsString> = command_parts[1..]
                .iter()
                .map(|s| s.as_str().into())
                .collect();
            process_config = process_config.with_args(args);
        }

        // Add filenames if pass_filenames is true
        if hook.pass_filenames && !filtered_files.is_empty() {
            let file_args: Vec<_> = filtered_files
                .iter()
                .map(|p| p.as_os_str().into())
                .collect();
            process_config = process_config.with_additional_args(file_args);
        }

        // Set timeout
        process_config = process_config.with_timeout(config.hook_timeout);

        // Execute the hook
        let process_result = match self.process_manager.execute_async(process_config).await {
            Ok(result) => result,
            Err(SnpError::Process(process_error)) => {
                let duration = start_time.elapsed().unwrap_or(Duration::new(0, 0));
                match process_error.as_ref() {
                    crate::error::ProcessError::Timeout {
                        command: _,
                        duration: timeout_duration,
                    } => {
                        // Handle timeout as a special case
                        let error = HookExecutionError::ExecutionTimeout {
                            hook_id: hook.id.clone(),
                            timeout: *timeout_duration,
                            partial_output: None,
                        };
                        return Ok(result
                            .failure(-1, duration, error)
                            .with_output(String::new(), String::new()));
                    }
                    crate::error::ProcessError::SpawnFailed { command, error } => {
                        // Handle command not found / spawn failures as hook execution failures
                        let hook_error = HookExecutionError::ExecutionFailed {
                            hook_id: hook.id.clone(),
                            exit_code: -1,
                            stdout: String::new(),
                            stderr: format!("Failed to execute command '{command}': {error}"),
                        };
                        return Ok(result.failure(-1, duration, hook_error).with_output(
                            String::new(),
                            format!("Failed to execute command '{command}': {error}"),
                        ));
                    }
                    _ => {
                        return Err(SnpError::Process(process_error));
                    }
                }
            }
            Err(e) => return Err(e),
        };

        let duration = start_time.elapsed().unwrap_or(Duration::new(0, 0));

        match process_result.exit_code() {
            Some(0) => Ok(result
                .success(0, duration)
                .with_output(process_result.stdout(), process_result.stderr())),
            Some(code) => {
                let error = HookExecutionError::ExecutionFailed {
                    hook_id: hook.id.clone(),
                    exit_code: code,
                    stdout: process_result.stdout(),
                    stderr: process_result.stderr(),
                };
                Ok(result
                    .failure(code, duration, error)
                    .with_output(process_result.stdout(), process_result.stderr()))
            }
            None => {
                let error = HookExecutionError::ExecutionTimeout {
                    hook_id: hook.id.clone(),
                    timeout: config.hook_timeout,
                    partial_output: Some(process_result.stdout()),
                };
                Ok(result
                    .failure(-1, duration, error)
                    .with_output(process_result.stdout(), process_result.stderr()))
            }
        }
    }

    /// Execute multiple hooks for a given stage
    pub async fn execute_hooks(
        &mut self,
        hooks: &[Hook],
        config: ExecutionConfig,
    ) -> Result<ExecutionResult> {
        let mut result = ExecutionResult::new();

        for hook in hooks {
            if config.fail_fast && !result.success {
                result.add_skipped(hook.id.clone());
                continue;
            }

            let hook_result = self
                .execute_single_hook(hook, &config.files, &config)
                .await?;
            result.add_result(hook_result);
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
