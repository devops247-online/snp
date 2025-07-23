// Process management utilities for executing hooks with proper timeout handling,
// output capture, environment management, and signal handling

use crate::error::{ProcessError, Result, SnpError};
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::ExitStatus;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

/// Helper function to create a failed ExitStatus
fn create_failed_exit_status() -> ExitStatus {
    #[cfg(unix)]
    {
        ExitStatus::from_raw(1 << 8) // Exit code 1 on Unix
    }
    #[cfg(windows)]
    {
        // On Windows, we can't easily create an ExitStatus with a specific code
        // This is a limitation, but for now we'll use a command that always fails
        std::process::Command::new("cmd")
            .args(&["/C", "exit 1"])
            .status()
            .unwrap_or_else(|_| {
                // If even this fails, we have bigger problems
                panic!("Failed to create failed exit status")
            })
    }
}

/// Process execution configuration
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    pub command: String,
    pub args: Vec<OsString>,
    pub working_dir: Option<PathBuf>,
    pub environment: HashMap<String, String>,
    pub timeout: Option<Duration>,
    pub capture_output: bool,
    pub inherit_env: bool,
}

impl ProcessConfig {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            working_dir: None,
            environment: HashMap::new(),
            timeout: None,
            capture_output: true,
            inherit_env: true,
        }
    }

    pub fn with_args(mut self, args: Vec<impl Into<OsString>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_working_dir(mut self, dir: PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
    }

    pub fn with_environment(mut self, env: HashMap<String, String>) -> Self {
        self.environment = env;
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_capture_output(mut self, capture: bool) -> Self {
        self.capture_output = capture;
        self
    }

    pub fn with_inherit_env(mut self, inherit: bool) -> Self {
        self.inherit_env = inherit;
        self
    }

    pub fn with_additional_args(mut self, additional_args: Vec<OsString>) -> Self {
        self.args.extend(additional_args);
        self
    }
}

/// Process execution result
#[derive(Debug)]
pub struct ProcessResult {
    pub exit_status: ExitStatus,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub duration: Duration,
    pub timed_out: bool,
}

impl ProcessResult {
    pub fn success(&self) -> bool {
        self.exit_status.success()
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_status.code()
    }

    pub fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    pub fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
}

/// Main process manager
pub struct ProcessManager {
    max_concurrent: usize,
    default_timeout: Duration,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self {
            max_concurrent: 4,
            default_timeout: Duration::from_secs(60),
        }
    }

    pub fn with_config(max_concurrent: usize, default_timeout: Duration) -> Self {
        Self {
            max_concurrent,
            default_timeout,
        }
    }

    // Synchronous execution
    pub fn execute(&self, config: ProcessConfig) -> Result<ProcessResult> {
        tokio::runtime::Runtime::new()
            .map_err(SnpError::Io)?
            .block_on(self.execute_async(config))
    }

    pub fn execute_with_callback<F>(
        &self,
        config: ProcessConfig,
        mut callback: F,
    ) -> Result<ProcessResult>
    where
        F: FnMut(&[u8]) -> Result<()>,
    {
        let result = self.execute(config)?;
        callback(&result.stdout)?;
        Ok(result)
    }

    // Asynchronous execution
    pub async fn execute_async(&self, config: ProcessConfig) -> Result<ProcessResult> {
        use std::process::Stdio;
        use tokio::io::{AsyncReadExt, BufReader};
        use tokio::process::Command;
        use tokio::time::timeout;

        let start_time = std::time::Instant::now();

        // Build the command
        let mut cmd = Command::new(&config.command);
        cmd.args(&config.args);

        // Set working directory
        if let Some(ref dir) = config.working_dir {
            cmd.current_dir(dir);
        }

        // Set environment
        if !config.inherit_env {
            cmd.env_clear();
        }
        for (key, value) in &config.environment {
            cmd.env(key, value);
        }

        // Configure I/O
        if config.capture_output {
            cmd.stdout(Stdio::piped());
            cmd.stderr(Stdio::piped());
        } else {
            cmd.stdout(Stdio::inherit());
            cmd.stderr(Stdio::inherit());
        }
        cmd.stdin(Stdio::null());

        // Spawn the process
        let mut child = cmd.spawn().map_err(|e| {
            SnpError::Process(Box::new(ProcessError::SpawnFailed {
                command: config.command.clone(),
                error: e.to_string(),
            }))
        })?;

        // Set up timeout
        let timeout_duration = config.timeout.unwrap_or(self.default_timeout);

        // Execute with timeout
        let execution_result = timeout(timeout_duration, async {
            let mut stdout_data = Vec::new();
            let mut stderr_data = Vec::new();

            if config.capture_output {
                // Capture output
                if let Some(stdout) = child.stdout.as_mut() {
                    let mut reader = BufReader::new(stdout);
                    reader.read_to_end(&mut stdout_data).await.map_err(|e| {
                        SnpError::Process(Box::new(ProcessError::OutputCaptureFailed {
                            message: format!("Failed to read stdout: {e}"),
                            command: config.command.clone(),
                        }))
                    })?;
                }

                if let Some(stderr) = child.stderr.as_mut() {
                    let mut reader = BufReader::new(stderr);
                    reader.read_to_end(&mut stderr_data).await.map_err(|e| {
                        SnpError::Process(Box::new(ProcessError::OutputCaptureFailed {
                            message: format!("Failed to read stderr: {e}"),
                            command: config.command.clone(),
                        }))
                    })?;
                }
            }

            // Wait for process to complete
            let exit_status = child.wait().await.map_err(|e| {
                SnpError::Process(Box::new(ProcessError::ExecutionFailed {
                    command: config.command.clone(),
                    exit_code: None,
                    stderr: format!("Failed to wait for process: {e}"),
                }))
            })?;

            Ok::<ProcessResult, SnpError>(ProcessResult {
                exit_status,
                stdout: stdout_data,
                stderr: stderr_data,
                duration: start_time.elapsed(),
                timed_out: false,
            })
        })
        .await;

        match execution_result {
            Ok(result) => result,
            Err(_) => {
                // Timeout occurred - kill the process
                let _ = child.kill().await;
                let _ = child.wait().await;

                Err(SnpError::Process(Box::new(ProcessError::Timeout {
                    command: config.command,
                    duration: timeout_duration,
                })))
            }
        }
    }

    pub async fn execute_parallel(
        &self,
        configs: Vec<ProcessConfig>,
    ) -> Result<Vec<ProcessResult>> {
        use tokio::task::JoinHandle;

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(self.max_concurrent));
        let mut tasks: Vec<JoinHandle<Result<ProcessResult>>> = Vec::new();

        // Create tasks for each config
        for config in configs {
            let semaphore = semaphore.clone();
            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.map_err(|e| {
                    SnpError::Process(Box::new(ProcessError::ExecutionFailed {
                        command: "semaphore".to_string(),
                        exit_code: None,
                        stderr: e.to_string(),
                    }))
                })?;

                // Create a new ProcessManager for this task
                let manager = ProcessManager::with_config(1, Duration::from_secs(30));
                manager.execute_async(config).await
            });
            tasks.push(task);
        }

        // Wait for all tasks to complete
        let mut process_results = Vec::new();
        for task in tasks {
            match task.await {
                Ok(Ok(result)) => {
                    process_results.push(result);
                }
                Ok(Err(e)) => {
                    // Process execution failed, but we want to continue with other processes
                    // Create a failed ProcessResult to maintain result order
                    process_results.push(ProcessResult {
                        exit_status: create_failed_exit_status(),
                        stdout: Vec::new(),
                        stderr: e.to_string().into_bytes(),
                        duration: Duration::from_secs(0),
                        timed_out: false,
                    });
                }
                Err(join_error) => {
                    // Task join failed
                    process_results.push(ProcessResult {
                        exit_status: create_failed_exit_status(),
                        stdout: Vec::new(),
                        stderr: format!("Task failed: {join_error}").into_bytes(),
                        duration: Duration::from_secs(0),
                        timed_out: false,
                    });
                }
            }
        }

        Ok(process_results)
    }

    // Process control
    pub fn terminate_all(&self) -> Result<()> {
        // For now, this is a no-op since we don't track running processes
        // In a full implementation, we would maintain a list of active processes
        // and send termination signals to them
        Ok(())
    }

    pub fn set_resource_limits(&mut self, _limits: ResourceLimits) {
        // Resource limits would be implemented using platform-specific APIs
        // For now, this is a no-op placeholder
    }
}

impl Default for ProcessManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource limits for process execution
#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_memory_mb: Option<u64>,
    pub max_cpu_percent: Option<f32>,
    pub max_open_files: Option<u64>,
}

/// Environment builder for process execution
pub struct ProcessEnvironment {
    base_env: HashMap<String, String>,
    language_paths: HashMap<String, PathBuf>,
}

impl ProcessEnvironment {
    pub fn new() -> Self {
        Self {
            base_env: HashMap::new(),
            language_paths: HashMap::new(),
        }
    }

    pub fn inherit_system() -> Self {
        let mut env = Self::new();
        env.base_env = std::env::vars().collect();
        env
    }

    pub fn clean() -> Self {
        Self::new()
    }

    pub fn set_var(&mut self, key: &str, value: &str) -> &mut Self {
        self.base_env.insert(key.to_string(), value.to_string());
        self
    }

    pub fn remove_var(&mut self, key: &str) -> &mut Self {
        self.base_env.remove(key);
        self
    }

    pub fn add_to_path(&mut self, path: PathBuf) -> &mut Self {
        let current_path = self.base_env.get("PATH").cloned().unwrap_or_default();
        let separator = if cfg!(windows) { ";" } else { ":" };
        let new_path = if current_path.is_empty() {
            path.to_string_lossy().to_string()
        } else {
            format!("{}{}{}", path.to_string_lossy(), separator, current_path)
        };
        self.base_env.insert("PATH".to_string(), new_path);
        self
    }

    pub fn set_language_env(&mut self, language: &str, env_path: PathBuf) -> &mut Self {
        self.language_paths.insert(language.to_string(), env_path);
        self
    }

    pub fn build(&self) -> HashMap<String, String> {
        self.base_env.clone()
    }
}

impl Default for ProcessEnvironment {
    fn default() -> Self {
        Self::new()
    }
}

/// Streaming output handler for long-running processes
pub trait OutputHandler: Send + Sync {
    fn handle_stdout(&mut self, data: &[u8]) -> Result<()>;
    fn handle_stderr(&mut self, data: &[u8]) -> Result<()>;
    fn handle_completion(&mut self, result: &ProcessResult) -> Result<()>;
}

/// Built-in output handlers
pub struct BufferedOutputHandler {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

impl BufferedOutputHandler {
    pub fn new() -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
        }
    }
}

impl Default for BufferedOutputHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputHandler for BufferedOutputHandler {
    fn handle_stdout(&mut self, data: &[u8]) -> Result<()> {
        self.stdout.extend_from_slice(data);
        Ok(())
    }

    fn handle_stderr(&mut self, data: &[u8]) -> Result<()> {
        self.stderr.extend_from_slice(data);
        Ok(())
    }

    fn handle_completion(&mut self, _result: &ProcessResult) -> Result<()> {
        Ok(())
    }
}

pub struct StreamingOutputHandler<W: std::io::Write> {
    writer: W,
    include_stderr: bool,
}

impl<W: std::io::Write> StreamingOutputHandler<W> {
    pub fn new(writer: W, include_stderr: bool) -> Self {
        Self {
            writer,
            include_stderr,
        }
    }
}

impl<W: std::io::Write + Send + Sync> OutputHandler for StreamingOutputHandler<W> {
    fn handle_stdout(&mut self, data: &[u8]) -> Result<()> {
        self.writer.write_all(data).map_err(SnpError::Io)?;
        self.writer.flush().map_err(SnpError::Io)?;
        Ok(())
    }

    fn handle_stderr(&mut self, data: &[u8]) -> Result<()> {
        if self.include_stderr {
            self.writer.write_all(data).map_err(SnpError::Io)?;
            self.writer.flush().map_err(SnpError::Io)?;
        }
        Ok(())
    }

    fn handle_completion(&mut self, _result: &ProcessResult) -> Result<()> {
        self.writer.flush().map_err(SnpError::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_process_config_builder() {
        let config = ProcessConfig::new("echo")
            .with_args(vec!["hello", "world"])
            .with_timeout(Duration::from_secs(30))
            .with_capture_output(true);

        assert_eq!(config.command, "echo");
        assert_eq!(config.args.len(), 2);
        assert_eq!(config.timeout, Some(Duration::from_secs(30)));
        assert!(config.capture_output);
    }

    #[test]
    fn test_process_environment_builder() {
        let mut env = ProcessEnvironment::new();
        env.set_var("TEST_VAR", "test_value")
            .add_to_path(PathBuf::from("/usr/local/bin"));

        let built = env.build();
        assert_eq!(built.get("TEST_VAR"), Some(&"test_value".to_string()));
        assert!(built.get("PATH").unwrap().contains("/usr/local/bin"));
    }

    #[test]
    fn test_buffered_output_handler() {
        let mut handler = BufferedOutputHandler::new();
        handler.handle_stdout(b"test stdout").unwrap();
        handler.handle_stderr(b"test stderr").unwrap();

        assert_eq!(handler.stdout, b"test stdout");
        assert_eq!(handler.stderr, b"test stderr");
    }
}
