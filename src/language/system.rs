// System language plugin for shell commands and system utilities
// This is the most fundamental language plugin providing direct access to system executables

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command as TokioCommand;

use crate::core::Hook;
use crate::error::{Result, SnpError};
use crate::execution::HookExecutionResult;

use super::dependency::{Dependency, DependencyManager, DependencyManagerConfig};
use super::environment::{
    EnvironmentConfig, EnvironmentInfo, IsolationLevel, LanguageEnvironment, ValidationReport,
};
use super::traits::{Command, Language, LanguageConfig, LanguageError};

/// System language plugin for shell commands and system utilities
pub struct SystemLanguagePlugin {
    config: SystemLanguageConfig,
    executable_cache: HashMap<String, PathBuf>,
    shell_detection_cache: Option<ShellInfo>,
    dependency_manager: SystemDependencyManager,
}

#[derive(Debug, Clone)]
pub struct SystemLanguageConfig {
    pub default_shell: Option<String>,
    pub shell_flags: Vec<String>,
    pub environment_isolation: IsolationLevel,
    pub allowed_executables: Option<Vec<String>>, // Security whitelist
    pub blocked_executables: Vec<String>,         // Security blacklist
    pub max_execution_time: Duration,
    pub enable_shell_expansion: bool,
}

#[derive(Debug, Clone)]
pub struct ShellInfo {
    pub shell_type: ShellType,
    pub shell_path: PathBuf,
    pub shell_version: Option<String>,
    pub supported_features: ShellFeatures,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ShellType {
    Bash,
    Zsh,
    Fish,
    Dash,
    PowerShell,
    Cmd,
    Unknown(String),
}

#[derive(Debug, Clone)]
pub struct ShellFeatures {
    pub supports_set_e: bool,        // Exit on error
    pub supports_set_u: bool,        // Exit on undefined variable
    pub supports_set_pipefail: bool, // Exit on pipe failure
    pub supports_arrays: bool,
    pub supports_associative_arrays: bool,
}

#[derive(Debug, Clone)]
pub struct PlatformInfo {
    pub os_type: OsType,
    pub architecture: String,
    pub executable_extension: Option<String>,
    pub path_separator: String,
    pub environment_separator: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OsType {
    Unix,
    Windows,
    MacOS,
}

/// System dependency manager (mostly a no-op since system commands are already installed)
pub struct SystemDependencyManager {
    #[allow(dead_code)]
    config: DependencyManagerConfig,
}

impl Default for SystemLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemLanguagePlugin {
    pub fn new() -> Self {
        Self {
            config: SystemLanguageConfig::default(),
            executable_cache: HashMap::new(),
            shell_detection_cache: None,
            dependency_manager: SystemDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    pub fn with_config(config: SystemLanguageConfig) -> Self {
        Self {
            config,
            executable_cache: HashMap::new(),
            shell_detection_cache: None,
            dependency_manager: SystemDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    /// Detect the system's shell and platform information
    pub async fn detect_system_environment(&mut self) -> Result<SystemEnvironment> {
        let platform_info = Self::detect_platform_info()?;
        let shell_info = self.detect_shell(&platform_info).await?;

        Ok(SystemEnvironment {
            shell_info,
            platform_info,
            environment_variables: Self::get_base_environment()?,
            working_directory: std::env::current_dir()?,
            executable_paths: Self::get_path_directories()?,
        })
    }

    fn detect_platform_info() -> Result<PlatformInfo> {
        let os_type = if cfg!(target_os = "windows") {
            OsType::Windows
        } else if cfg!(target_os = "macos") {
            OsType::MacOS
        } else {
            OsType::Unix
        };

        let executable_extension = match os_type {
            OsType::Windows => Some(".exe".to_string()),
            _ => None,
        };

        let path_separator = match os_type {
            OsType::Windows => ";".to_string(),
            _ => ":".to_string(),
        };

        let environment_separator = match os_type {
            OsType::Windows => "\r\n".to_string(),
            _ => "\n".to_string(),
        };

        Ok(PlatformInfo {
            os_type,
            architecture: std::env::consts::ARCH.to_string(),
            executable_extension,
            path_separator,
            environment_separator,
        })
    }

    async fn detect_shell(&mut self, platform_info: &PlatformInfo) -> Result<ShellInfo> {
        // Check cache first
        if let Some(ref shell_info) = self.shell_detection_cache {
            return Ok(shell_info.clone());
        }

        let shell_info = if let Some(ref shell_path) = self.config.default_shell {
            Self::analyze_shell(Path::new(shell_path)).await?
        } else {
            match platform_info.os_type {
                OsType::Unix | OsType::MacOS => Self::detect_unix_shell().await?,
                OsType::Windows => Self::detect_windows_shell().await?,
            }
        };

        // Cache the result
        self.shell_detection_cache = Some(shell_info.clone());
        Ok(shell_info)
    }

    async fn detect_unix_shell() -> Result<ShellInfo> {
        // Check SHELL environment variable
        if let Ok(shell_path) = std::env::var("SHELL") {
            if let Ok(info) = Self::analyze_shell(Path::new(&shell_path)).await {
                return Ok(info);
            }
        }

        // Try common shells in order of preference
        let common_shells = [
            "/bin/bash",
            "/usr/bin/bash",
            "/bin/zsh",
            "/usr/bin/zsh",
            "/bin/sh",
            "/usr/bin/sh",
        ];

        for shell_path in &common_shells {
            if Path::new(shell_path).exists() {
                if let Ok(info) = Self::analyze_shell(Path::new(shell_path)).await {
                    return Ok(info);
                }
            }
        }

        Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
            language: "system".to_string(),
            error: "No suitable shell found on system".to_string(),
            recovery_suggestion: Some("Install bash, zsh, or another POSIX shell".to_string()),
        }))
    }

    async fn detect_windows_shell() -> Result<ShellInfo> {
        // Try PowerShell first
        if let Ok(ps_path) = which::which("powershell") {
            if let Ok(info) = Self::analyze_shell(&ps_path).await {
                return Ok(info);
            }
        }

        // Fallback to cmd.exe
        if let Ok(cmd_path) = which::which("cmd") {
            return Ok(ShellInfo {
                shell_type: ShellType::Cmd,
                shell_path: cmd_path,
                shell_version: None,
                supported_features: ShellFeatures::cmd_features(),
            });
        }

        Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
            language: "system".to_string(),
            error: "No suitable shell found on Windows".to_string(),
            recovery_suggestion: Some(
                "Install PowerShell or ensure cmd.exe is available".to_string(),
            ),
        }))
    }

    async fn analyze_shell(shell_path: &Path) -> Result<ShellInfo> {
        let output = TokioCommand::new(shell_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let version_output = String::from_utf8_lossy(&output.stdout);
        let (shell_type, version) = Self::parse_shell_version(&version_output);
        let features = Self::detect_shell_features(&shell_type);

        Ok(ShellInfo {
            shell_type,
            shell_path: shell_path.to_path_buf(),
            shell_version: version,
            supported_features: features,
        })
    }

    fn parse_shell_version(version_output: &str) -> (ShellType, Option<String>) {
        let version_lower = version_output.to_lowercase();

        if version_lower.contains("bash") {
            (
                ShellType::Bash,
                Self::extract_version(&version_lower, "bash"),
            )
        } else if version_lower.contains("zsh") {
            (ShellType::Zsh, Self::extract_version(&version_lower, "zsh"))
        } else if version_lower.contains("fish") {
            (
                ShellType::Fish,
                Self::extract_version(&version_lower, "fish"),
            )
        } else if version_lower.contains("powershell") {
            (
                ShellType::PowerShell,
                Self::extract_version(&version_lower, "powershell"),
            )
        } else {
            (ShellType::Unknown(version_output.to_string()), None)
        }
    }

    fn extract_version(version_text: &str, shell_name: &str) -> Option<String> {
        // Simple version extraction - look for pattern like "bash 5.1.8"
        if let Some(pos) = version_text.find(shell_name) {
            let after_name = &version_text[pos + shell_name.len()..];
            // Look for version pattern like "5.1.8" or "5.1"
            if let Some(captures) = regex::Regex::new(r"(\d+\.[\d\.]+)")
                .ok()
                .and_then(|re| re.find(after_name))
            {
                return Some(captures.as_str().to_string());
            }
        }
        None
    }

    fn detect_shell_features(shell_type: &ShellType) -> ShellFeatures {
        match shell_type {
            ShellType::Bash => ShellFeatures {
                supports_set_e: true,
                supports_set_u: true,
                supports_set_pipefail: true,
                supports_arrays: true,
                supports_associative_arrays: true,
            },
            ShellType::Zsh => ShellFeatures {
                supports_set_e: true,
                supports_set_u: true,
                supports_set_pipefail: true,
                supports_arrays: true,
                supports_associative_arrays: true,
            },
            ShellType::Fish => ShellFeatures {
                supports_set_e: true,
                supports_set_u: false,
                supports_set_pipefail: true,
                supports_arrays: true,
                supports_associative_arrays: false,
            },
            ShellType::Dash => ShellFeatures {
                supports_set_e: true,
                supports_set_u: true,
                supports_set_pipefail: false,
                supports_arrays: false,
                supports_associative_arrays: false,
            },
            ShellType::PowerShell => ShellFeatures {
                supports_set_e: true,
                supports_set_u: false,
                supports_set_pipefail: false,
                supports_arrays: true,
                supports_associative_arrays: true,
            },
            ShellType::Cmd => ShellFeatures::cmd_features(),
            ShellType::Unknown(_) => ShellFeatures::minimal_features(),
        }
    }

    fn get_base_environment() -> Result<HashMap<String, String>> {
        let mut env = HashMap::new();

        // Copy essential environment variables
        for (key, value) in std::env::vars() {
            match key.as_str() {
                "PATH" | "HOME" | "USER" | "SHELL" | "TERM" | "LANG" | "LC_ALL" => {
                    env.insert(key, value);
                }
                _ if key.starts_with("SNP_") => {
                    env.insert(key, value);
                }
                _ => {}
            }
        }

        Ok(env)
    }

    fn get_path_directories() -> Result<Vec<PathBuf>> {
        if let Ok(path_var) = std::env::var("PATH") {
            let separator = if cfg!(windows) { ";" } else { ":" };
            Ok(path_var.split(separator).map(PathBuf::from).collect())
        } else {
            Ok(vec![])
        }
    }

    /// Find executable in PATH
    pub fn find_executable(&mut self, name: &str) -> Option<PathBuf> {
        // Check cache first
        if let Some(path) = self.executable_cache.get(name) {
            return Some(path.clone());
        }

        // Use which crate to find executable
        if let Ok(path) = which::which(name) {
            self.executable_cache.insert(name.to_string(), path.clone());
            Some(path)
        } else {
            None
        }
    }

    /// Build command for execution
    pub fn build_system_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        self.build_system_command_with_timeout(hook, env, files, None)
    }

    /// Build command for execution with optional timeout override
    pub fn build_system_command_with_timeout(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
        timeout_override: Option<Duration>,
    ) -> Result<Command> {
        // Parse the entry command - it might contain spaces like "cargo fmt"
        let entry_parts: Vec<&str> = hook.entry.split_whitespace().collect();
        if entry_parts.is_empty() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "system".to_string(),
                error: format!("Empty hook entry: {}", hook.entry),
                recovery_suggestion: None,
            }));
        }

        tracing::debug!(
            "System plugin: parsed entry '{}' into executable '{}' with {} initial args",
            hook.entry,
            entry_parts[0],
            entry_parts.len() - 1
        );
        tracing::debug!("Files being passed to hook {}: {:?}", hook.id, files);

        let mut command = Command::new(entry_parts[0]);
        tracing::debug!("Created command with executable: '{}'", command.executable);

        // Add the remaining parts of the entry as arguments
        for part in &entry_parts[1..] {
            command.arg(*part);
            tracing::debug!("Added argument: '{}'", part);
        }
        tracing::debug!(
            "Command after parsing - executable: '{}', args: {:?}",
            command.executable,
            command.arguments
        );

        // Add hook arguments
        for arg in &hook.args {
            command.arg(arg);
        }

        // Add file arguments if not disabled
        if hook.pass_filenames {
            for file in files {
                command.arg(file.to_string_lossy().as_ref());
            }
        }

        // Set environment variables
        for (key, value) in &env.environment_variables {
            command.env(key, value);
        }

        // Set working directory - use the project working directory from environment
        if let Some(working_dir_str) = env.environment_variables.get("SNP_WORKING_DIRECTORY") {
            let working_dir = PathBuf::from(working_dir_str);
            tracing::debug!("Setting working directory to: {}", working_dir.display());
            command.current_dir(working_dir);
        } else {
            // Fallback to current directory if not set
            tracing::debug!("No working directory set, using current directory");
        }

        // Set timeout - use override if provided, otherwise use config default
        let timeout = timeout_override.unwrap_or(self.config.max_execution_time);
        tracing::debug!("Setting command timeout to: {:?}", timeout);
        command.timeout(timeout);

        Ok(command)
    }

    /// Execute a system command
    pub async fn execute_system_command(&self, command: Command) -> Result<HookExecutionResult> {
        let start_time = Instant::now();

        tracing::debug!(
            "System executing command: {} with args: {:?} in dir: {:?}",
            command.executable,
            command.arguments,
            command.working_directory
        );

        // Build tokio command
        let mut tokio_cmd = TokioCommand::new(&command.executable);
        tokio_cmd.args(&command.arguments);
        tokio_cmd.envs(&command.environment);

        if let Some(ref cwd) = command.working_directory {
            tokio_cmd.current_dir(cwd);
        }

        tokio_cmd.stdout(Stdio::piped());
        tokio_cmd.stderr(Stdio::piped());

        // Execute with timeout
        let output = if let Some(timeout) = command.timeout {
            match tokio::time::timeout(timeout, tokio_cmd.output()).await {
                Ok(output) => output.map_err(|e| {
                    SnpError::from(LanguageError::EnvironmentSetupFailed {
                        language: "system".to_string(),
                        error: format!(
                            "Failed to execute '{}' with args {:?} in dir {:?}: {}",
                            command.executable, command.arguments, command.working_directory, e
                        ),
                        recovery_suggestion: Some(
                            "Check if the command exists and is executable".to_string(),
                        ),
                    })
                })?,
                Err(_) => {
                    // Handle timeout - create a failed result instead of returning error
                    let duration = start_time.elapsed();
                    return Ok(HookExecutionResult {
                        hook_id: "system".to_string(),
                        success: false,
                        skipped: false,
                        skip_reason: None,
                        exit_code: None,
                        duration,
                        files_processed: vec![],
                        files_modified: vec![],
                        stdout: String::new(),
                        stderr: format!("Process timeout after {}ms", timeout.as_millis()),
                        error: Some(crate::error::HookExecutionError::ExecutionTimeout {
                            hook_id: "system".to_string(),
                            timeout,
                            partial_output: None,
                        }),
                    });
                }
            }
        } else {
            tokio_cmd.output().await.map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "system".to_string(),
                    error: format!(
                        "Failed to execute '{}' with args {:?} in dir {:?}: {}",
                        command.executable, command.arguments, command.working_directory, e
                    ),
                    recovery_suggestion: Some(
                        "Check if the command exists and is executable".to_string(),
                    ),
                })
            })?
        };

        let duration = start_time.elapsed();

        Ok(HookExecutionResult {
            hook_id: "system".to_string(), // Will be overridden by caller
            success: output.status.success(),
            skipped: false,
            skip_reason: None,
            exit_code: output.status.code(),
            duration,
            files_processed: vec![], // Will be set by caller
            files_modified: vec![],
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            error: if output.status.success() {
                None
            } else {
                Some(crate::error::HookExecutionError::ExecutionFailed {
                    hook_id: "system".to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            },
        })
    }
}

/// System environment information
pub struct SystemEnvironment {
    pub shell_info: ShellInfo,
    pub platform_info: PlatformInfo,
    pub environment_variables: HashMap<String, String>,
    pub working_directory: PathBuf,
    pub executable_paths: Vec<PathBuf>,
}

impl Default for SystemLanguageConfig {
    fn default() -> Self {
        Self {
            default_shell: None,
            shell_flags: vec!["-e".to_string()], // Exit on error by default
            environment_isolation: IsolationLevel::Partial,
            allowed_executables: None, // No whitelist by default
            blocked_executables: vec![
                "rm".to_string(),
                "del".to_string(),
                "format".to_string(),
                "mkfs".to_string(),
            ],
            max_execution_time: Duration::from_secs(300), // 5 minutes
            enable_shell_expansion: false,                // Safer default
        }
    }
}

impl ShellFeatures {
    pub fn cmd_features() -> Self {
        Self {
            supports_set_e: false,
            supports_set_u: false,
            supports_set_pipefail: false,
            supports_arrays: false,
            supports_associative_arrays: false,
        }
    }

    pub fn minimal_features() -> Self {
        Self {
            supports_set_e: false,
            supports_set_u: false,
            supports_set_pipefail: false,
            supports_arrays: false,
            supports_associative_arrays: false,
        }
    }
}

#[async_trait]
impl Language for SystemLanguagePlugin {
    fn language_name(&self) -> &str {
        "system"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["sh", "bash", "zsh", "fish", "ps1", "bat", "cmd"]
    }

    fn detection_patterns(&self) -> &[&str] {
        &[
            r"^#\!/bin/(sh|bash|zsh|fish)",
            r"^#\!/usr/bin/(sh|bash|zsh|fish)",
            r"^#\!/usr/bin/env (sh|bash|zsh|fish)",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        let environment_id = format!("system-{}", uuid::Uuid::new_v4());
        let root_path = config
            .working_directory
            .clone()
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

        let mut environment_variables = Self::get_base_environment()?;

        // Add config environment variables
        for (key, value) in &config.environment_variables {
            environment_variables.insert(key.clone(), value.clone());
        }

        // Set timeout in environment variables if provided
        if let Some(timeout) = config.hook_timeout {
            environment_variables.insert(
                "SNP_HOOK_TIMEOUT".to_string(),
                timeout.as_millis().to_string(),
            );
        }

        // Set working directory in environment variables if provided
        if let Some(ref working_dir) = config.working_directory {
            environment_variables.insert(
                "SNP_WORKING_DIRECTORY".to_string(),
                working_dir.to_string_lossy().to_string(),
            );
        }

        // If we have a repository path, add it to the PATH so hooks can be found
        let executable_path = if let Some(repo_path) = &config.repository_path {
            // Add the repository path to PATH so hook executables can be found
            let current_path = environment_variables
                .get("PATH")
                .unwrap_or(&String::new())
                .clone();
            let new_path = if current_path.is_empty() {
                repo_path.to_string_lossy().to_string()
            } else {
                format!("{}:{}", repo_path.to_string_lossy(), current_path)
            };
            environment_variables.insert("PATH".to_string(), new_path);
            repo_path.clone()
        } else {
            root_path.clone()
        };

        Ok(LanguageEnvironment {
            language: "system".to_string(),
            environment_id,
            root_path: root_path.clone(),
            executable_path,
            environment_variables,
            installed_dependencies: vec![], // System doesn't install dependencies
            metadata: super::environment::EnvironmentMetadata {
                created_at: std::time::SystemTime::now(),
                last_used: std::time::SystemTime::now(),
                usage_count: 0,
                size_bytes: 0,
                dependency_count: 0,
                language_version: "system".to_string(),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        })
    }

    async fn install_dependencies(
        &self,
        _env: &LanguageEnvironment,
        _dependencies: &[Dependency],
    ) -> Result<()> {
        // System language doesn't install dependencies - they should already be available
        Ok(())
    }

    async fn cleanup_environment(&self, _env: &LanguageEnvironment) -> Result<()> {
        // System environment doesn't need cleanup
        Ok(())
    }

    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        tracing::debug!(
            "System plugin executing hook: {} with entry: {}",
            hook.id,
            hook.entry
        );
        let command = self.build_command(hook, env, files)?;
        tracing::debug!(
            "System plugin built command: {:?} with args: {:?}",
            command.executable,
            command.arguments
        );
        let mut result = self.execute_system_command(command).await?;

        // Set the correct hook ID and files
        result.hook_id = hook.id.clone();
        result.files_processed = files.to_vec();

        Ok(result)
    }

    fn build_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        // Check if there's a timeout in the environment metadata
        let timeout_override = env
            .environment_variables
            .get("SNP_HOOK_TIMEOUT")
            .and_then(|t| t.parse::<u64>().ok())
            .map(Duration::from_millis);

        self.build_system_command_with_timeout(hook, env, files, timeout_override)
    }

    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>> {
        // For system language, we parse the dependency specs but don't actually resolve them
        // since system dependencies are managed externally
        let mut resolved = Vec::new();
        for dep_str in dependencies {
            match Dependency::parse_spec(dep_str) {
                Ok(dep) => resolved.push(dep),
                Err(_) => {
                    // If parsing fails, create a basic dependency
                    resolved.push(Dependency::new(dep_str));
                }
            }
        }
        Ok(resolved)
    }

    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency> {
        // For system language, we can parse dependencies but they're managed externally
        Dependency::parse_spec(dep_spec)
    }

    fn get_dependency_manager(&self) -> &dyn DependencyManager {
        &self.dependency_manager
    }

    fn get_environment_info(&self, env: &LanguageEnvironment) -> EnvironmentInfo {
        EnvironmentInfo {
            environment_id: env.environment_id.clone(),
            language: env.language.clone(),
            language_version: "system".to_string(),
            created_at: env.metadata.created_at,
            last_used: env.metadata.last_used,
            usage_count: env.metadata.usage_count,
            size_bytes: env.metadata.size_bytes,
            dependency_count: env.installed_dependencies.len(),
            is_healthy: true, // System is always "healthy"
        }
    }

    fn validate_environment(&self, _env: &LanguageEnvironment) -> Result<ValidationReport> {
        Ok(ValidationReport {
            is_healthy: true,
            issues: vec![],
            recommendations: vec![],
            performance_score: 1.0, // Perfect score for system
        })
    }

    fn default_config(&self) -> LanguageConfig {
        LanguageConfig::default()
    }

    fn validate_config(&self, _config: &LanguageConfig) -> Result<()> {
        Ok(())
    }
}

#[async_trait]
impl DependencyManager for SystemDependencyManager {
    async fn install(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<super::dependency::InstallationResult> {
        // System dependencies are not installed by this manager
        Ok(super::dependency::InstallationResult {
            installed: vec![],
            failed: vec![],
            skipped: vec![],
            duration: Duration::from_millis(0),
        })
    }

    async fn resolve(
        &self,
        _dependencies: &[Dependency],
    ) -> Result<Vec<super::dependency::ResolvedDependency>> {
        Ok(vec![]) // System dependencies are already resolved
    }

    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<Vec<bool>> {
        // Assume all system dependencies are installed (they should be managed externally)
        Ok(vec![true; dependencies.len()])
    }

    async fn update(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<super::dependency::UpdateResult> {
        Ok(super::dependency::UpdateResult {
            updated: vec![],
            failed: vec![],
            no_update_available: vec![],
            duration: Duration::from_millis(0),
        })
    }

    async fn uninstall(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<()> {
        Ok(()) // System dependencies are not uninstalled by this manager
    }

    async fn list_installed(
        &self,
        _env: &LanguageEnvironment,
    ) -> Result<Vec<super::dependency::InstalledPackage>> {
        Ok(vec![]) // System doesn't track installed packages this way
    }
}
