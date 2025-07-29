// Python language plugin for Python-based hooks with virtual environment management
// Provides comprehensive Python support including venv creation, pip dependency management, and hook execution

use async_trait::async_trait;
// use pep440_rs::{Version as Pep440Version}; // Commented out until needed
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use tokio::fs;
use tokio::process::Command as TokioCommand;

use crate::core::Hook;
use crate::error::{Result, SnpError};
use crate::execution::HookExecutionResult;

use super::dependency::{
    Dependency, DependencyManager, DependencySource, InstallationResult, InstalledPackage,
    ResolvedDependency, UpdateResult, VersionSpec,
};
use super::environment::{
    EnvironmentConfig, EnvironmentInfo, LanguageEnvironment, ValidationReport,
};
use super::traits::{Command, Language, LanguageConfig, LanguageError};
use thiserror::Error;

/// Python-specific error types
#[derive(Debug, Error)]
pub enum PythonError {
    #[error("Environment creation failed: {env_type} at {path:?}: {error}")]
    EnvironmentCreationFailed {
        env_type: String,
        path: PathBuf,
        error: String,
        recovery_suggestion: Option<String>,
    },
}

/// Global coordination for repository installations to prevent concurrent installs
/// Key: "env_path:repo_path", Value: Mutex for that specific installation
static REPOSITORY_INSTALL_LOCKS: Lazy<Mutex<HashMap<String, Arc<Mutex<()>>>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Python language plugin for Python-based hooks with virtual environment management
pub struct PythonLanguagePlugin {
    config: PythonLanguageConfig,
    python_cache: RwLock<HashMap<String, PythonInfo>>,
    environment_cache: RwLock<HashMap<String, PathBuf>>,
    dependency_manager: PythonDependencyManager,
}

#[derive(Debug, Clone)]
pub struct PythonLanguageConfig {
    pub default_python_version: Option<String>,
    pub prefer_system_python: bool,
    pub venv_backend: VenvBackend,
    pub pip_config: PipConfig,
    pub cache_environments: bool,
    pub max_environment_age: Duration,
}

#[derive(Debug, Clone)]
pub enum VenvBackend {
    Venv,       // python -m venv
    Virtualenv, // virtualenv command
    Auto,       // Auto-detect best available
}

#[derive(Debug, Clone)]
pub struct PipConfig {
    pub index_url: Option<String>,
    pub extra_index_urls: Vec<String>,
    pub trusted_hosts: Vec<String>,
    pub timeout: Duration,
    pub retries: u32,
    pub use_cache: bool,
}

#[derive(Debug, Clone)]
pub struct PythonInfo {
    pub executable_path: PathBuf,
    pub version: semver::Version,
    pub implementation: PythonImplementation,
    pub architecture: String,
    pub supports_venv: bool,
    pub pip_version: Option<semver::Version>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PythonImplementation {
    CPython,
    PyPy,
    Jython,
    IronPython,
    Unknown(String),
}

/// System environment information
pub struct PythonEnvironment {
    pub python_info: PythonInfo,
    pub environment_variables: HashMap<String, String>,
    pub working_directory: PathBuf,
}

/// Python dependency manager using pip
#[derive(Clone)]
pub struct PythonDependencyManager {
    config: PipConfig,
}

#[derive(Debug, Clone)]
pub struct PipInfo {
    pub version: semver::Version,
    pub capabilities: PipCapabilities,
}

#[derive(Debug, Clone)]
pub struct PipCapabilities {
    pub supports_hash_checking: bool,
    pub supports_build_isolation: bool,
    pub supports_wheel_cache: bool,
}

impl Default for PythonLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PythonLanguagePlugin {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            python_cache: RwLock::new(HashMap::new()),
            environment_cache: RwLock::new(HashMap::new()),
            dependency_manager: self.dependency_manager.clone(),
        }
    }
}

impl PythonLanguagePlugin {
    pub fn new() -> Self {
        Self {
            config: PythonLanguageConfig::default(),
            python_cache: RwLock::new(HashMap::new()),
            environment_cache: RwLock::new(HashMap::new()),
            dependency_manager: PythonDependencyManager::new(),
        }
    }

    pub fn with_config(config: PythonLanguageConfig) -> Self {
        Self {
            config,
            python_cache: RwLock::new(HashMap::new()),
            environment_cache: RwLock::new(HashMap::new()),
            dependency_manager: PythonDependencyManager::new(),
        }
    }

    /// Detect available Python interpreters
    pub async fn detect_python_interpreters(&self) -> Result<Vec<PythonInfo>> {
        let mut interpreters = Vec::new();

        // Common Python executable names to check
        let python_names = [
            "python3",
            "python3.12",
            "python3.11",
            "python3.10",
            "python3.9",
            "python3.8",
            "python",
            "py", // Windows py launcher
        ];

        for name in &python_names {
            if let Ok(path) = which::which(name) {
                if let Ok(info) = self.analyze_python_interpreter(&path).await {
                    interpreters.push(info);
                }
            }
        }

        // Sort by version (newest first)
        interpreters.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(interpreters)
    }

    /// Analyze a Python interpreter to get version and capability information
    async fn analyze_python_interpreter(&self, path: &Path) -> Result<PythonInfo> {
        let path_str = path.to_string_lossy().to_string();

        // Check cache first
        if let Some(cached_info) = self.python_cache.read().unwrap().get(&path_str) {
            return Ok(cached_info.clone());
        }

        // Get Python version
        let version_output = TokioCommand::new(path)
            .args([
                "-c",
                "import sys; print('.'.join(map(str, sys.version_info[:3])))",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !version_output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!("Failed to get Python version from {}", path.display()),
                recovery_suggestion: Some("Ensure Python interpreter is functional".to_string()),
            }));
        }

        let version_str = String::from_utf8_lossy(&version_output.stdout)
            .trim()
            .to_string();
        let version = semver::Version::parse(&version_str).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!("Invalid Python version format: {e}"),
                recovery_suggestion: None,
            })
        })?;

        // Get Python implementation info
        let impl_output = TokioCommand::new(path)
            .args(["-c", "import sys; print(sys.implementation.name)"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let implementation = if impl_output.status.success() {
            let impl_name = String::from_utf8_lossy(&impl_output.stdout)
                .trim()
                .to_lowercase();
            match impl_name.as_str() {
                "cpython" => PythonImplementation::CPython,
                "pypy" => PythonImplementation::PyPy,
                "jython" => PythonImplementation::Jython,
                "ironpython" => PythonImplementation::IronPython,
                other => PythonImplementation::Unknown(other.to_string()),
            }
        } else {
            PythonImplementation::Unknown("unknown".to_string())
        };

        // Get architecture
        let arch_output = TokioCommand::new(path)
            .args(["-c", "import platform; print(platform.architecture()[0])"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        let architecture = if arch_output.status.success() {
            String::from_utf8_lossy(&arch_output.stdout)
                .trim()
                .to_string()
        } else {
            "unknown".to_string()
        };

        // Check venv support (Python 3.3+)
        let supports_venv = version >= semver::Version::new(3, 3, 0);

        // Try to get pip version
        let pip_version = self.get_pip_version(path).await.ok();

        let python_info = PythonInfo {
            executable_path: path.to_path_buf(),
            version,
            implementation,
            architecture,
            supports_venv,
            pip_version,
        };

        // Cache the result
        self.python_cache
            .write()
            .unwrap()
            .insert(path_str, python_info.clone());

        Ok(python_info)
    }

    /// Get pip version for a Python interpreter
    async fn get_pip_version(&self, python_path: &Path) -> Result<semver::Version> {
        let output = TokioCommand::new(python_path)
            .args(["-m", "pip", "--version"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: "pip not available".to_string(),
                recovery_suggestion: Some("Install pip for this Python interpreter".to_string()),
            }));
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        // Parse version from output like "pip 21.0.1 from ..."
        if let Some(version_part) = version_output.split_whitespace().nth(1) {
            semver::Version::parse(version_part).map_err(|e| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "python".to_string(),
                    error: format!("Failed to parse pip version: {e}"),
                    recovery_suggestion: None,
                })
            })
        } else {
            Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: "Failed to extract pip version".to_string(),
                recovery_suggestion: None,
            }))
        }
    }

    /// Find suitable Python interpreter based on version requirement
    pub async fn find_suitable_python(&self, version_requirement: &str) -> Result<PythonInfo> {
        let interpreters = self.detect_python_interpreters().await?;

        if interpreters.is_empty() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: "No Python interpreters found on system".to_string(),
                recovery_suggestion: Some("Install Python 3.6 or later".to_string()),
            }));
        }

        // If no specific version requested, return the newest
        if version_requirement == "python" || version_requirement == "python3" {
            return Ok(interpreters[0].clone());
        }

        // Try to match specific version
        for interpreter in &interpreters {
            if self.version_matches(version_requirement, &interpreter.version) {
                return Ok(interpreter.clone());
            }
        }

        // No exact match found
        Err(SnpError::from(LanguageError::UnsupportedVersion {
            language: "python".to_string(),
            version: version_requirement.to_string(),
            supported_versions: interpreters
                .iter()
                .map(|i| format!("python{}.{}", i.version.major, i.version.minor))
                .collect(),
        }))
    }

    /// Check if a Python version matches a requirement string
    fn version_matches(&self, requirement: &str, version: &semver::Version) -> bool {
        // Simple version matching - can be enhanced
        if requirement.starts_with("python") {
            let version_part = requirement.strip_prefix("python").unwrap_or("");
            if version_part.is_empty() {
                return true; // Any Python version
            }

            // Handle major version only (e.g., "3" matches "3.12.0")
            if let Ok(major) = version_part.parse::<u64>() {
                return version.major == major;
            }

            // Handle major.minor version (e.g., "3.9" matches "3.9.x")
            if let Some(dot_pos) = version_part.find('.') {
                let major_part = &version_part[..dot_pos];
                let minor_part = &version_part[dot_pos + 1..];

                if let (Ok(major), Ok(minor)) =
                    (major_part.parse::<u64>(), minor_part.parse::<u64>())
                {
                    return version.major == major && version.minor == minor;
                }
            }

            // Try to parse as semver requirement (for complex cases)
            if let Ok(req) = semver::VersionReq::parse(&format!("={version_part}")) {
                return req.matches(version);
            }

            // Simple string matching as fallback
            let version_str = format!("{}.{}", version.major, version.minor);
            return version_part == version_str;
        }

        false
    }

    /// Create or reuse Python virtual environment
    pub async fn setup_python_environment(
        &self,
        python_info: &PythonInfo,
        env_config: &EnvironmentConfig,
    ) -> Result<PathBuf> {
        let env_id = self.generate_environment_id(python_info, env_config);

        // Check cache first
        let cached_env_path = self.environment_cache.read().unwrap().get(&env_id).cloned();
        if let Some(env_path) = cached_env_path {
            if env_path.exists() && self.validate_venv_directory(&env_path).await? {
                return Ok(env_path);
            }
        }

        // Create new virtual environment
        let env_path = self
            .create_virtual_environment(python_info, &env_id)
            .await?;

        // Cache the environment path
        self.environment_cache
            .write()
            .unwrap()
            .insert(env_id, env_path.clone());

        Ok(env_path)
    }

    /// Generate unique environment ID
    fn generate_environment_id(
        &self,
        python_info: &PythonInfo,
        config: &EnvironmentConfig,
    ) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        python_info.version.to_string().hash(&mut hasher);
        python_info.executable_path.hash(&mut hasher);
        config.additional_dependencies.hash(&mut hasher);

        format!("python-{}-{:x}", python_info.version, hasher.finish())
    }

    /// Create virtual environment
    async fn create_virtual_environment(
        &self,
        python_info: &PythonInfo,
        env_id: &str,
    ) -> Result<PathBuf> {
        let temp_dir = std::env::temp_dir();
        let env_path = temp_dir.join("snp-python-envs").join(env_id);

        // If environment already exists, validate it
        if env_path.exists() {
            if self
                .validate_venv_directory(&env_path)
                .await
                .unwrap_or(false)
            {
                return Ok(env_path);
            } else {
                // Environment exists but is invalid, clean it up
                tracing::debug!("Cleaning up invalid virtual environment: {:?}", env_path);
                if let Err(e) = fs::remove_dir_all(&env_path).await {
                    tracing::warn!("Failed to cleanup invalid venv directory: {}", e);
                    // Continue anyway, the creation might still succeed
                }
            }
        }

        // Create parent directory
        fs::create_dir_all(env_path.parent().unwrap()).await?;

        match self.config.venv_backend {
            VenvBackend::Venv => self.create_venv_environment(python_info, &env_path).await,
            VenvBackend::Virtualenv => {
                self.create_virtualenv_environment(python_info, &env_path)
                    .await
            }
            VenvBackend::Auto => {
                // Try venv first, fallback to virtualenv, then legacy venv
                if python_info.supports_venv {
                    match self.create_venv_environment(python_info, &env_path).await {
                        Ok(path) => Ok(path),
                        Err(_) => {
                            // Fallback to legacy venv method as a last resort
                            tracing::debug!("Main venv creation failed, trying legacy method");
                            self.create_venv_environment_legacy(python_info, &env_path)
                                .await
                        }
                    }
                } else if which::which("virtualenv").is_ok() {
                    self.create_virtualenv_environment(python_info, &env_path)
                        .await
                } else {
                    Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                        language: "python".to_string(),
                        error: "No virtual environment backend available".to_string(),
                        recovery_suggestion: Some(
                            "Install virtualenv or use Python 3.3+".to_string(),
                        ),
                    }))
                }
            }
        }
    }

    /// Create virtual environment using python -m venv
    async fn create_venv_environment(
        &self,
        python_info: &PythonInfo,
        env_path: &Path,
    ) -> Result<PathBuf> {
        // Use the centralized venv creation method
        self.dependency_manager
            .create_python_venv(python_info, env_path)
            .await?;

        // Verify the environment was created successfully
        if !self
            .dependency_manager
            .is_valid_environment(env_path)
            .await
            .unwrap_or(false)
        {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: "Created environment is not valid".to_string(),
                recovery_suggestion: Some("Check Python installation and permissions".to_string()),
            }));
        }

        Ok(env_path.to_path_buf())
    }

    async fn create_venv_environment_legacy(
        &self,
        python_info: &PythonInfo,
        env_path: &Path,
    ) -> Result<PathBuf> {
        // Clean up any partially created directory first
        if env_path.exists() {
            tracing::debug!(
                "Removing existing directory before venv creation: {:?}",
                env_path
            );
            if let Err(e) = tokio::fs::remove_dir_all(env_path).await {
                tracing::warn!("Failed to remove existing directory: {}", e);
                // Continue anyway - venv creation might still work
            }
        }

        // Retry mechanism for CI environments where transient failures can occur
        const MAX_RETRIES: u32 = 3;
        let mut last_error = None;

        for attempt in 1..=MAX_RETRIES {
            let output = TokioCommand::new(&python_info.executable_path)
                .args(["-m", "venv", env_path.to_str().unwrap()])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await?;

            if output.status.success() {
                // Ensure pip is available in the virtual environment
                match self.ensure_pip_in_environment(env_path).await {
                    Ok(_) => return Ok(env_path.to_path_buf()),
                    Err(e) if attempt < MAX_RETRIES => {
                        last_error = Some(e);
                        // Clean up failed environment before retry
                        let _ = tokio::fs::remove_dir_all(env_path).await;
                        continue;
                    }
                    Err(e) => return Err(e),
                }
            }

            let error_msg = String::from_utf8_lossy(&output.stderr);

            // Check for specific "File exists" error and clean up before retry
            if error_msg.contains("File exists") && attempt < MAX_RETRIES {
                tracing::debug!("File exists error detected, cleaning up and retrying");
                let _ = tokio::fs::remove_dir_all(env_path).await;
                tokio::time::sleep(tokio::time::Duration::from_millis(100 * attempt as u64)).await;
            }

            last_error = Some(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!(
                    "venv creation failed (attempt {attempt}/{MAX_RETRIES}): {error_msg}"
                ),
                recovery_suggestion: Some(
                    "Check Python installation and permissions. Try running with clean cache."
                        .to_string(),
                ),
            }));

            if attempt < MAX_RETRIES {
                // Clean up failed environment before retry
                let _ = tokio::fs::remove_dir_all(env_path).await;
                // Small delay before retry
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            }
        }

        Err(last_error.unwrap())
    }

    /// Ensure pip is available in the virtual environment
    async fn ensure_pip_in_environment(&self, env_path: &Path) -> Result<()> {
        let pip_exe = if cfg!(windows) {
            env_path.join("Scripts").join("pip.exe")
        } else {
            env_path.join("bin").join("pip")
        };

        // If pip already exists, we're good
        if pip_exe.exists() {
            return Ok(());
        }

        // Get the Python executable in the venv
        let python_exe = if cfg!(windows) {
            env_path.join("Scripts").join("python.exe")
        } else {
            env_path.join("bin").join("python")
        };

        if !python_exe.exists() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: "Python executable not found in virtual environment".to_string(),
                recovery_suggestion: Some("Recreate virtual environment".to_string()),
            }));
        }

        // Install pip using ensurepip module
        let output = TokioCommand::new(&python_exe)
            .args(["-m", "ensurepip", "--upgrade"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!(
                    "Failed to install pip: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                recovery_suggestion: Some(
                    "Install pip manually in virtual environment".to_string(),
                ),
            }));
        }

        Ok(())
    }

    /// Create virtual environment using virtualenv
    async fn create_virtualenv_environment(
        &self,
        python_info: &PythonInfo,
        env_path: &Path,
    ) -> Result<PathBuf> {
        let output = TokioCommand::new("virtualenv")
            .args([
                "-p",
                python_info.executable_path.to_str().unwrap(),
                env_path.to_str().unwrap(),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!(
                    "virtualenv creation failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                recovery_suggestion: Some("Install virtualenv package".to_string()),
            }));
        }

        // Ensure pip is available in the virtual environment
        self.ensure_pip_in_environment(env_path).await?;

        Ok(env_path.to_path_buf())
    }

    /// Find executable in virtual environment, including console scripts
    fn find_executable_in_venv(&self, env_path: &Path, entry: &str) -> Result<PathBuf> {
        // First try direct path in venv bin
        let venv_bin_path = env_path.join("bin").join(entry);
        if venv_bin_path.exists() {
            tracing::debug!("Found executable directly in venv: {:?}", venv_bin_path);
            return Ok(venv_bin_path);
        }

        // Try alternative executable names (some packages use different naming)
        let alternative_names = [
            format!("{entry}-script"),
            format!("{entry}.py"),
            entry.replace("-", "_"),
        ];

        for alt_name in &alternative_names {
            let alt_path = env_path.join("bin").join(alt_name);
            if alt_path.exists() {
                tracing::debug!("Found executable with alternative name: {:?}", alt_path);
                return Ok(alt_path);
            }
        }

        // Special handling for known pre-commit hooks that might need module execution
        let module_based_hooks = [
            ("mixed-line-ending", "mixed_line_ending"), // This is actually a module
            ("debug-statements", "debug_statement_hook"), // Example of module mapping
        ];

        for (hook_name, module_name) in &module_based_hooks {
            if entry == *hook_name {
                tracing::debug!(
                    "Entry '{}' is a module-based hook, using module: {}",
                    entry,
                    module_name
                );
                return Ok(PathBuf::from(format!("__module__{module_name}")));
            }
        }

        // For most pre-commit hooks, if we can't find the executable, it might be a race condition
        // or the installation hasn't completed yet. Return the expected path and let execution
        // handle the error with proper retry logic.
        let expected_path = env_path.join("bin").join(entry);
        tracing::debug!(
            "Could not find executable '{}' in venv at {:?}, will retry during execution",
            entry,
            expected_path
        );
        Ok(expected_path)
    }

    /// Get the PATH environment variable for the virtual environment
    fn get_venv_path(&self, env_path: &Path) -> Result<String> {
        let venv_bin = if cfg!(windows) {
            env_path.join("Scripts")
        } else {
            env_path.join("bin")
        };

        let current_path = std::env::var("PATH").unwrap_or_default();
        let path_sep = if cfg!(windows) { ";" } else { ":" };
        Ok(format!(
            "{}{}{}",
            venv_bin.display(),
            path_sep,
            current_path
        ))
    }

    /// Validate existing virtual environment (internal method)
    async fn validate_venv_directory(&self, env_path: &Path) -> Result<bool> {
        // Check if Python executable exists
        let python_exe = match self.get_python_executable(env_path) {
            Ok(exe) => exe,
            Err(_) => return Ok(false),
        };

        if !python_exe.exists() {
            return Ok(false);
        }

        // Check if pip executable exists (don't fail if it doesn't, we can install it)
        let pip_exe = if cfg!(windows) {
            env_path.join("Scripts").join("pip.exe")
        } else {
            env_path.join("bin").join("pip")
        };

        if !pip_exe.exists() {
            return Ok(false);
        }

        // Try to run Python to verify it works
        let output = TokioCommand::new(&python_exe)
            .args(["-c", "print('test')"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(output.status.success())
    }

    /// Get Python executable path from virtual environment
    fn get_python_executable(&self, env_path: &Path) -> Result<PathBuf> {
        let python_exe = if cfg!(windows) {
            env_path.join("Scripts").join("python.exe")
        } else {
            env_path.join("bin").join("python")
        };

        if python_exe.exists() {
            Ok(python_exe)
        } else {
            Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!("Python executable not found at {}", python_exe.display()),
                recovery_suggestion: Some("Recreate virtual environment".to_string()),
            }))
        }
    }

    /// Get environment variables for virtual environment
    fn get_python_environment_variables(&self, env_path: &Path) -> Result<HashMap<String, String>> {
        let mut env_vars = HashMap::new();

        // Set VIRTUAL_ENV
        env_vars.insert(
            "VIRTUAL_ENV".to_string(),
            env_path.to_string_lossy().to_string(),
        );

        // PYTHONHOME should not be set in virtual environments
        // We don't add it to the environment variables map at all

        // Set PIP_DISABLE_PIP_VERSION_CHECK
        env_vars.insert("PIP_DISABLE_PIP_VERSION_CHECK".to_string(), "1".to_string());

        // Update PATH
        let bin_dir = if cfg!(windows) {
            env_path.join("Scripts")
        } else {
            env_path.join("bin")
        };

        // Use the centralized PATH construction method
        match self.get_venv_path(env_path) {
            Ok(venv_path) => {
                env_vars.insert("PATH".to_string(), venv_path);
            }
            Err(_) => {
                // Fallback to manual construction
                if let Ok(current_path) = std::env::var("PATH") {
                    let path_sep = if cfg!(windows) { ";" } else { ":" };
                    let new_path =
                        format!("{}{}{}", bin_dir.to_string_lossy(), path_sep, current_path);
                    env_vars.insert("PATH".to_string(), new_path);
                } else {
                    env_vars.insert("PATH".to_string(), bin_dir.to_string_lossy().to_string());
                }
            }
        }

        Ok(env_vars)
    }

    /// Parse requirements.txt file
    pub async fn parse_requirements_file(
        &self,
        requirements_path: &Path,
    ) -> Result<Vec<Dependency>> {
        let content = fs::read_to_string(requirements_path).await?;
        self.parse_requirements_content(&content)
    }

    /// Parse requirements content
    fn parse_requirements_content(&self, content: &str) -> Result<Vec<Dependency>> {
        let mut dependencies = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Skip -e editable installs and other pip options for now
            if line.starts_with('-') {
                continue;
            }

            // Skip environment markers for now (e.g., ; python_version >= "3.7")
            let dep_spec = if let Some(pos) = line.find(';') {
                line[..pos].trim()
            } else {
                line
            };

            if let Ok(dep) = self.parse_dependency_spec(dep_spec) {
                dependencies.push(dep);
            }
        }

        Ok(dependencies)
    }

    /// Parse a single dependency specification
    fn parse_dependency_spec(&self, spec: &str) -> Result<Dependency> {
        // Simple parsing - can be enhanced with proper PEP 440 parsing
        if let Some(eq_pos) = spec.find("==") {
            let name = spec[..eq_pos].trim().to_string();
            let version = spec[eq_pos + 2..].trim().to_string();
            Ok(Dependency {
                name,
                version_spec: VersionSpec::Exact(version),
                source: DependencySource::Registry { registry_url: None },
                extras: Vec::new(),
                metadata: HashMap::new(),
            })
        } else if let Some(ge_pos) = spec.find(">=") {
            let name = spec[..ge_pos].trim().to_string();
            let version = spec[ge_pos + 2..].trim().to_string();
            Ok(Dependency {
                name,
                version_spec: VersionSpec::Range {
                    min: Some(version),
                    max: None,
                },
                source: DependencySource::Registry { registry_url: None },
                extras: Vec::new(),
                metadata: HashMap::new(),
            })
        } else {
            // No version constraint
            Ok(Dependency {
                name: spec.trim().to_string(),
                version_spec: VersionSpec::Any,
                source: DependencySource::Registry { registry_url: None },
                extras: Vec::new(),
                metadata: HashMap::new(),
            })
        }
    }

    /// Get or create a coordination lock for a specific repository installation
    fn get_repository_install_lock(env_path: &Path, repo_path: &Path) -> Arc<Mutex<()>> {
        let lock_key = format!(
            "{}:{}",
            env_path.to_string_lossy(),
            repo_path.to_string_lossy()
        );

        // Get or create the lock for this specific env/repo combination
        let mut locks = REPOSITORY_INSTALL_LOCKS.lock().unwrap();
        if let Some(existing_lock) = locks.get(&lock_key) {
            Arc::clone(existing_lock)
        } else {
            let new_lock = Arc::new(Mutex::new(()));
            locks.insert(lock_key, Arc::clone(&new_lock));
            new_lock
        }
    }

    /// Install a repository as a Python package in the virtual environment with coordination
    async fn install_repository(&self, env_path: &Path, repo_path: &Path) -> Result<()> {
        tracing::debug!(
            "Installing repository {:?} in environment {:?}",
            repo_path,
            env_path
        );

        // Get coordination lock for this specific env/repo combination
        let install_lock = Self::get_repository_install_lock(env_path, repo_path);

        // Use a block to ensure the lock is dropped before any async operations
        {
            let _lock_guard = install_lock.lock().unwrap();
            tracing::debug!("Acquired installation lock for repository {:?}", repo_path);

            // Get pip executable from virtual environment
            let pip_executable = env_path.join("bin").join("pip");
            if !pip_executable.exists() {
                return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "python".to_string(),
                    error: "pip not found in virtual environment".to_string(),
                    recovery_suggestion: Some(
                        "Ensure virtual environment was created correctly".to_string(),
                    ),
                }));
            }

            // Note: We'll do the async installation check outside the lock to avoid Send issues
        } // Lock is dropped here

        // Perform async operations without holding the lock
        let pip_executable = env_path.join("bin").join("pip");

        // Check if repository is already installed and up-to-date
        if self
            .is_repository_installed(&pip_executable, repo_path)
            .await?
        {
            tracing::debug!("Repository already installed and up-to-date, skipping installation");
            return Ok(());
        }

        // Install repository in editable mode
        tracing::debug!("Running pip install for repository: {:?}", repo_path);
        let output = TokioCommand::new(&pip_executable)
            .args([
                "install", "-e", // Editable install
                ".",  // Install current directory
            ])
            .current_dir(repo_path) // Run in repository directory
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            tracing::debug!("Repository installation successful: {}", stdout);

            // Mark installation timestamp for future checks
            self.mark_repository_installed(env_path, repo_path).await?;
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            tracing::error!("Repository installation failed: {}", stderr);
            tracing::debug!("Pip stdout: {}", stdout);

            Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!("Failed to install repository: {stderr}"),
                recovery_suggestion: Some(
                    "Check repository contains setup.py or pyproject.toml".to_string(),
                ),
            }))
        }
    }

    /// Check if a repository is already installed and up-to-date
    async fn is_repository_installed(
        &self,
        pip_executable: &Path,
        repo_path: &Path,
    ) -> Result<bool> {
        // Use repository path hash as identifier instead of trying to extract package names
        // This is more reliable since many pre-commit repos don't have proper package configs
        let repo_identifier = self.get_repository_identifier(repo_path);

        // Check install marker based on repository path hash, not package name
        let install_marker_path = pip_executable
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join(".snp_installs")
            .join(format!("{repo_identifier}.marker"));

        if !install_marker_path.exists() {
            tracing::debug!(
                "No install marker found for repository {}, needs installation",
                repo_identifier
            );
            return Ok(false);
        }

        // Check if repository files are newer than install marker
        let install_time = match std::fs::metadata(&install_marker_path) {
            Ok(metadata) => metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH),
            Err(_) => {
                tracing::debug!("Could not read install marker metadata, needs installation");
                return Ok(false);
            }
        };

        let repo_modified_time = self.get_repository_modified_time(repo_path).await?;

        if repo_modified_time > install_time {
            tracing::debug!("Repository files newer than install marker, needs reinstallation");
            return Ok(false);
        }

        tracing::debug!("Repository {} is up-to-date", repo_identifier);
        Ok(true)
    }

    /// Get the most recent modification time of important files in the repository
    async fn get_repository_modified_time(
        &self,
        repo_path: &Path,
    ) -> Result<std::time::SystemTime> {
        let mut latest_time = std::time::SystemTime::UNIX_EPOCH;

        // Check important files for modifications
        let important_files = [
            "setup.py",
            "pyproject.toml",
            "setup.cfg",
            "requirements.txt",
            "Pipfile",
            "poetry.lock",
        ];

        for file in &important_files {
            let file_path = repo_path.join(file);
            if file_path.exists() {
                if let Ok(metadata) = std::fs::metadata(&file_path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified > latest_time {
                            latest_time = modified;
                        }
                    }
                }
            }
        }

        // Also check source directories
        let src_dirs = [
            "src",
            repo_path
                .file_name()
                .unwrap_or_default()
                .to_str()
                .unwrap_or(""),
        ];
        for src_dir in &src_dirs {
            let src_path = repo_path.join(src_dir);
            if src_path.exists() && src_path.is_dir() {
                if let Ok(metadata) = std::fs::metadata(&src_path) {
                    if let Ok(modified) = metadata.modified() {
                        if modified > latest_time {
                            latest_time = modified;
                        }
                    }
                }
            }
        }

        Ok(latest_time)
    }

    /// Generate a unique identifier for a repository based on its path
    /// This is more reliable than extracting package names from setup.py/pyproject.toml
    fn get_repository_identifier(&self, repo_path: &Path) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Use absolute path to ensure consistency
        let abs_path = repo_path
            .canonicalize()
            .unwrap_or_else(|_| repo_path.to_path_buf());
        abs_path.to_string_lossy().hash(&mut hasher);

        // Also include the directory name for better debugging
        let dir_name = repo_path.file_name().unwrap_or_default().to_string_lossy();

        format!("repo_{}_{:x}", dir_name, hasher.finish())
    }

    /// Mark repository as installed by creating a marker file
    async fn mark_repository_installed(&self, env_path: &Path, repo_path: &Path) -> Result<()> {
        let repo_identifier = self.get_repository_identifier(repo_path);
        let marker_dir = env_path.join(".snp_installs");
        fs::create_dir_all(&marker_dir).await?;

        let marker_path = marker_dir.join(format!("{repo_identifier}.marker"));
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        fs::write(
            &marker_path,
            format!(
                "Installed at: {timestamp}\nRepository: {}",
                repo_path.display()
            ),
        )
        .await?;

        tracing::debug!("Created install marker at: {:?}", marker_path);
        Ok(())
    }
}

impl Default for PythonLanguageConfig {
    fn default() -> Self {
        Self {
            default_python_version: None,
            prefer_system_python: false,
            venv_backend: VenvBackend::Auto,
            pip_config: PipConfig::default(),
            cache_environments: true,
            max_environment_age: Duration::from_secs(7 * 24 * 3600), // 7 days
        }
    }
}

impl Default for PipConfig {
    fn default() -> Self {
        Self {
            index_url: None,
            extra_index_urls: vec![],
            trusted_hosts: vec![],
            timeout: Duration::from_secs(60),
            retries: 3,
            use_cache: true,
        }
    }
}

impl Default for PythonDependencyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonDependencyManager {
    pub fn new() -> Self {
        Self {
            config: PipConfig::default(),
        }
    }

    /// Install dependencies using pip
    pub async fn install_dependencies(
        &self,
        env_path: &Path,
        dependencies: &[Dependency],
    ) -> Result<InstallationResult> {
        let pip_executable = if cfg!(windows) {
            env_path.join("Scripts").join("pip.exe")
        } else {
            env_path.join("bin").join("pip")
        };

        if !pip_executable.exists() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!("pip executable not found at {}", pip_executable.display()),
                recovery_suggestion: Some("Reinstall pip in virtual environment".to_string()),
            }));
        }
        let mut installed = Vec::new();
        let mut failed = Vec::new();
        let start_time = Instant::now();

        for dep in dependencies {
            match self.install_single_dependency(&pip_executable, dep).await {
                Ok(mut package) => {
                    package.install_path = env_path.to_path_buf();
                    installed.push(package);
                }
                Err(e) => failed.push((dep.clone(), e.to_string())),
            }
        }

        Ok(InstallationResult {
            installed,
            failed,
            skipped: Vec::new(),
            duration: start_time.elapsed(),
        })
    }

    /// Install single dependency
    async fn install_single_dependency(
        &self,
        pip_executable: &Path,
        dependency: &Dependency,
    ) -> Result<InstalledPackage> {
        let mut args = vec!["install".to_string()];

        // Add index URLs if configured
        if let Some(ref index_url) = self.config.index_url {
            args.push("--index-url".to_string());
            args.push(index_url.clone());
        }

        for extra_url in &self.config.extra_index_urls {
            args.push("--extra-index-url".to_string());
            args.push(extra_url.clone());
        }

        // Add dependency specification
        let dep_spec = self.format_dependency_spec(dependency);
        args.push(dep_spec);

        let output = TokioCommand::new(pip_executable)
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if output.status.success() {
            Ok(InstalledPackage {
                name: dependency.name.clone(),
                version: self.extract_installed_version(&output.stdout)?,
                source: dependency.source.clone(),
                install_path: PathBuf::new(), // Will be set by caller
                metadata: HashMap::new(),
            })
        } else {
            Err(SnpError::from(
                LanguageError::DependencyInstallationFailed {
                    dependency: dependency.name.clone(),
                    language: "python".to_string(),
                    error: String::from_utf8_lossy(&output.stderr).to_string(),
                    installation_log: Some(String::from_utf8_lossy(&output.stdout).to_string()),
                },
            ))
        }
    }

    /// Format dependency specification for pip
    fn format_dependency_spec(&self, dependency: &Dependency) -> String {
        match &dependency.version_spec {
            VersionSpec::Any => dependency.name.clone(),
            VersionSpec::Exact(version) => format!("{}=={}", dependency.name, version),
            VersionSpec::Range { min, max } => {
                let mut spec = dependency.name.clone();
                if let Some(min_ver) = min {
                    spec.push_str(&format!(">={min_ver}"));
                }
                if let Some(max_ver) = max {
                    if min.is_some() {
                        spec.push(',');
                    }
                    spec.push_str(&format!("<={max_ver}"));
                }
                spec
            }
            VersionSpec::Compatible(version) => format!("{}~={}", dependency.name, version),
            VersionSpec::Exclude(_) => {
                // pip doesn't support exclude directly, use compatible version
                dependency.name.clone()
            }
        }
    }

    /// Extract installed version from pip output
    fn extract_installed_version(&self, _stdout: &[u8]) -> Result<String> {
        // This is a simplified version extraction - can be enhanced
        Ok("unknown".to_string())
    }

    /// Create a Python virtual environment in the specified directory
    async fn create_python_venv(&self, python_info: &PythonInfo, env_path: &Path) -> Result<()> {
        // If environment already exists and is valid, return success
        if env_path.exists() && self.is_valid_environment(env_path).await.unwrap_or(false) {
            return Ok(());
        }

        // Remove existing directory if it exists but is invalid
        if env_path.exists() {
            std::fs::remove_dir_all(env_path)?;
        }

        // Create the virtual environment using python -m venv
        let output = TokioCommand::new(&python_info.executable_path)
            .args(["-m", "venv", env_path.to_str().unwrap()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "python".to_string(),
                error: format!("Failed to create virtual environment: {stderr}"),
                recovery_suggestion: Some("Ensure python3-venv is installed".to_string()),
            }));
        }

        Ok(())
    }

    /// Check if a Python environment is valid
    async fn is_valid_environment(&self, env_path: &Path) -> Result<bool> {
        let python_exe = if cfg!(windows) {
            env_path.join("Scripts").join("python.exe")
        } else {
            env_path.join("bin").join("python")
        };

        Ok(python_exe.exists())
    }
}

#[async_trait]
impl Language for PythonLanguagePlugin {
    fn language_name(&self) -> &str {
        "python"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["py", "pyi", "pyx", "pxd", "pyw"]
    }

    fn detection_patterns(&self) -> &[&str] {
        &[
            r"^#!/usr/bin/env python",
            r"^#!/usr/bin/python",
            r"^#!/usr/local/bin/python",
            r"^#!.*python[0-9]*$",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        // Find suitable Python interpreter
        let python_info = self
            .find_suitable_python(config.language_version.as_deref().unwrap_or("python3"))
            .await?;

        // Generate environment ID
        let environment_id = self.generate_environment_id(&python_info, config);

        // Use a default cache directory if not specified (this should be provided by the environment manager)
        let cache_root = std::env::var("SNP_HOME")
            .or_else(|_| std::env::var("XDG_CACHE_HOME").map(|p| format!("{p}/snp")))
            .unwrap_or_else(|_| {
                format!("{}/.cache/snp", std::env::var("HOME").unwrap_or_default())
            });
        let env_path = PathBuf::from(cache_root)
            .join("environments")
            .join(&environment_id);

        // Ensure the environment directory exists
        std::fs::create_dir_all(&env_path)?;

        // Create virtual environment in the directory
        let python_exe = env_path.join("bin").join("python");
        tracing::debug!("Checking for Python executable at: {:?}", python_exe);
        if !python_exe.exists() {
            tracing::debug!("Creating Python virtual environment at: {:?}", env_path);
            tracing::debug!(
                "Using Python interpreter: {:?}",
                python_info.executable_path
            );
            let output = TokioCommand::new(&python_info.executable_path)
                .args(["-m", "venv", env_path.to_str().unwrap()])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .await?;

            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                tracing::error!("Virtual environment creation failed: {}", stderr);
                return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "python".to_string(),
                    error: format!("Failed to create virtual environment: {stderr}"),
                    recovery_suggestion: Some("Ensure python3-venv is installed".to_string()),
                }));
            } else {
                tracing::debug!("Virtual environment created successfully");
            }
        } else {
            tracing::debug!("Python executable already exists, skipping venv creation");
        }

        // Get Python executable
        tracing::debug!("Getting Python executable from environment: {:?}", env_path);
        let python_exe = self.get_python_executable(&env_path)?;
        tracing::debug!("Found Python executable: {:?}", python_exe);

        // Get environment variables
        tracing::debug!("Getting environment variables for: {:?}", env_path);
        let environment_variables = self.get_python_environment_variables(&env_path)?;
        tracing::debug!("Got {} environment variables", environment_variables.len());

        // Install repository if specified
        if let Some(ref repo_path) = config.repository_path {
            tracing::debug!("Installing repository from path: {:?}", repo_path);
            self.install_repository(&env_path, repo_path).await?;
        }

        // Install additional dependencies if specified
        tracing::debug!(
            "Checking for additional dependencies: {:?}",
            config.additional_dependencies
        );
        let installed_dependencies = if !config.additional_dependencies.is_empty() {
            tracing::debug!(
                "Installing {} additional dependencies",
                config.additional_dependencies.len()
            );
            let deps = self
                .resolve_dependencies(&config.additional_dependencies)
                .await?;
            let _install_result = self
                .dependency_manager
                .install_dependencies(&env_path, &deps)
                .await?;
            deps
        } else {
            tracing::debug!("No additional dependencies to install");
            Vec::new()
        };

        let dependency_count = installed_dependencies.len();

        Ok(LanguageEnvironment {
            language: "python".to_string(),
            environment_id,
            root_path: env_path.clone(),
            executable_path: python_exe,
            environment_variables,
            installed_dependencies,
            metadata: super::environment::EnvironmentMetadata {
                created_at: std::time::SystemTime::now(),
                last_used: std::time::SystemTime::now(),
                usage_count: 0,
                size_bytes: 0,
                dependency_count,
                language_version: python_info.version.to_string(),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        })
    }

    async fn install_dependencies(
        &self,
        env: &LanguageEnvironment,
        dependencies: &[Dependency],
    ) -> Result<()> {
        self.dependency_manager
            .install_dependencies(&env.root_path, dependencies)
            .await?;
        Ok(())
    }

    async fn cleanup_environment(&self, env: &LanguageEnvironment) -> Result<()> {
        // Remove virtual environment directory
        if env.root_path.exists() {
            tokio::fs::remove_dir_all(&env.root_path).await?;
        }
        Ok(())
    }

    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        tracing::debug!("Python plugin execute_hook called for hook: {}", hook.id);
        tracing::debug!("Environment root_path: {:?}", env.root_path);
        tracing::debug!("Environment executable_path: {:?}", env.executable_path);

        let command = self.build_command(hook, env, files)?;
        tracing::debug!(
            "Built command: executable={:?}, args={:?}",
            command.executable,
            command.arguments
        );

        let start_time = Instant::now();

        // Verify executable exists and is accessible (with retry for race conditions)
        let executable_path = PathBuf::from(&command.executable);
        if !command.executable.starts_with("__module__") && !executable_path.is_absolute() {
            // For relative paths, resolve against venv bin directory
            let resolved_path = env.root_path.join("bin").join(&command.executable);
            self.verify_executable_with_retry(&resolved_path, 3).await?;
        } else if !command.executable.starts_with("__module__") && executable_path.is_absolute() {
            // For absolute paths, verify directly
            self.verify_executable_with_retry(&executable_path, 3)
                .await?;
        }
        // Note: __module__ entries are handled by build_command and don't need file verification

        // Build tokio command
        let mut tokio_cmd = TokioCommand::new(&command.executable);
        tokio_cmd.args(&command.arguments);
        tokio_cmd.envs(&command.environment);

        if let Some(ref cwd) = command.working_directory {
            tokio_cmd.current_dir(cwd);
        }

        tokio_cmd.stdout(Stdio::piped());
        tokio_cmd.stderr(Stdio::piped());

        tracing::debug!("About to execute command for hook: {}", hook.id);

        // Execute with timeout
        let output = if let Some(timeout) = command.timeout {
            match tokio::time::timeout(timeout, tokio_cmd.output()).await {
                Ok(output) => output?,
                Err(_) => {
                    return Ok(HookExecutionResult {
                        hook_id: hook.id.clone(),
                        success: false,
                        skipped: false,
                        skip_reason: None,
                        exit_code: Some(-1),
                        duration: start_time.elapsed(),
                        files_processed: files.to_vec(),
                        files_modified: vec![],
                        stdout: String::new(),
                        stderr: "Command timed out".to_string(),
                        error: Some(crate::error::HookExecutionError::ExecutionFailed {
                            hook_id: hook.id.clone(),
                            exit_code: -1,
                            stdout: String::new(),
                            stderr: "Command timed out".to_string(),
                        }),
                    });
                }
            }
        } else {
            tokio_cmd.output().await?
        };

        tracing::debug!("Command execution completed for hook: {}", hook.id);
        tracing::debug!("Command exit status: {:?}", output.status);

        let duration = start_time.elapsed();

        Ok(HookExecutionResult {
            hook_id: hook.id.clone(),
            success: output.status.success(),
            skipped: false,
            skip_reason: None,
            exit_code: output.status.code(),
            duration,
            files_processed: files.to_vec(),
            files_modified: vec![], // Could be enhanced to detect file modifications
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            error: if output.status.success() {
                None
            } else {
                Some(crate::error::HookExecutionError::ExecutionFailed {
                    hook_id: hook.id.clone(),
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            },
        })
    }

    fn build_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        tracing::debug!(
            "Building command for hook: {}, entry: {}",
            hook.id,
            hook.entry
        );
        let python_exe = self.get_python_executable(&env.root_path)?;
        tracing::debug!("Python executable: {:?}", python_exe);
        let mut command = Command::new(python_exe.to_string_lossy());

        // Handle different entry types
        if hook.entry == "python" {
            // Direct Python command - use arguments as-is
            tracing::debug!("Using direct Python command path");
            command.args(&hook.args);
        } else if hook.entry.starts_with("python -m") {
            // Module execution
            tracing::debug!("Using Python module execution path");
            let module_parts: Vec<&str> = hook.entry.split_whitespace().collect();
            if module_parts.len() >= 3 {
                command.arg("-m").arg(module_parts[2]);
            }
            command.args(&hook.args);
        } else {
            // Assume it's a script or command available in the environment
            tracing::debug!(
                "Using script/command execution path - looking for: {}",
                hook.entry
            );

            // Use the improved executable finder
            let executable_path = self
                .find_executable_in_venv(&env.root_path, &hook.entry)
                .unwrap_or_else(|_| PathBuf::from(&hook.entry));

            // Check if it's a module marker
            if let Some(module_name) = executable_path.to_string_lossy().strip_prefix("__module__")
            {
                tracing::debug!("Running as Python module: {}", module_name);
                command = Command::new(python_exe.to_string_lossy());
                command.arg("-m").arg(module_name);
                command.args(&hook.args);
            } else {
                tracing::debug!("Using executable path: {:?}", executable_path);
                command = Command::new(executable_path.to_string_lossy());
                command.args(&hook.args);
            }
        }

        // Add file arguments if not disabled
        if hook.pass_filenames {
            for file in files {
                command.arg(file.to_string_lossy().as_ref());
            }
        }

        // Set environment variables from virtual environment
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

        // Set timeout
        command.timeout(Duration::from_secs(300)); // 5 minutes default

        Ok(command)
    }
    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>> {
        let mut resolved = Vec::new();
        for dep_str in dependencies {
            match self.parse_dependency_spec(dep_str) {
                Ok(dep) => resolved.push(dep),
                Err(_) => {
                    // If parsing fails, create a basic dependency
                    resolved.push(Dependency {
                        name: dep_str.clone(),
                        version_spec: VersionSpec::Any,
                        source: DependencySource::Registry { registry_url: None },
                        extras: Vec::new(),
                        metadata: HashMap::new(),
                    });
                }
            }
        }
        Ok(resolved)
    }

    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency> {
        self.parse_dependency_spec(dep_spec)
    }

    fn get_dependency_manager(&self) -> &dyn DependencyManager {
        &self.dependency_manager
    }

    fn get_environment_info(&self, env: &LanguageEnvironment) -> EnvironmentInfo {
        EnvironmentInfo {
            environment_id: env.environment_id.clone(),
            language: env.language.clone(),
            language_version: "python".to_string(),
            created_at: env.metadata.created_at,
            last_used: env.metadata.last_used,
            usage_count: env.metadata.usage_count,
            size_bytes: env.metadata.size_bytes,
            dependency_count: env.installed_dependencies.len(),
            is_healthy: true,
        }
    }

    fn validate_environment(&self, _env: &LanguageEnvironment) -> Result<ValidationReport> {
        Ok(ValidationReport {
            is_healthy: true,
            issues: vec![],
            recommendations: vec![],
            performance_score: 1.0,
        })
    }

    fn default_config(&self) -> LanguageConfig {
        LanguageConfig::default()
    }

    fn validate_config(&self, _config: &LanguageConfig) -> Result<()> {
        Ok(())
    }
}

impl PythonLanguagePlugin {
    /// Verify executable exists and is accessible with retry logic
    async fn verify_executable_with_retry(
        &self,
        executable_path: &Path,
        max_retries: u32,
    ) -> Result<()> {
        for attempt in 1..=max_retries {
            if executable_path.exists() {
                // Additional check: try to get metadata to ensure the file is actually accessible
                match std::fs::metadata(executable_path) {
                    Ok(metadata) => {
                        if metadata.is_file() {
                            tracing::debug!(
                                "Executable verified on attempt {}: {:?}",
                                attempt,
                                executable_path
                            );
                            return Ok(());
                        }
                    }
                    Err(e) => {
                        tracing::debug!(
                            "Metadata check failed on attempt {} for {:?}: {}",
                            attempt,
                            executable_path,
                            e
                        );
                    }
                }
            }

            if attempt < max_retries {
                let delay_ms = std::cmp::min(100 * attempt, 500); // Exponential backoff up to 500ms
                tracing::debug!(
                    "Executable not found on attempt {}, waiting {}ms before retry: {:?}",
                    attempt,
                    delay_ms,
                    executable_path
                );
                tokio::time::sleep(tokio::time::Duration::from_millis(delay_ms.into())).await;
            }
        }

        Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
            language: "python".to_string(),
            error: format!(
                "Executable not found after {} attempts: {}",
                max_retries,
                executable_path.display()
            ),
            recovery_suggestion: Some(
                "The executable may not be properly installed in the virtual environment"
                    .to_string(),
            ),
        }))
    }
}

#[async_trait]
impl DependencyManager for PythonDependencyManager {
    async fn install(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<InstallationResult> {
        self.install_dependencies(&env.root_path, dependencies)
            .await
    }

    async fn resolve(&self, _dependencies: &[Dependency]) -> Result<Vec<ResolvedDependency>> {
        // Basic implementation - can be enhanced
        Ok(vec![])
    }

    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<Vec<bool>> {
        // Basic implementation - assume not installed
        Ok(vec![false; dependencies.len()])
    }

    async fn update(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<UpdateResult> {
        Ok(UpdateResult {
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
        Ok(())
    }

    async fn list_installed(&self, _env: &LanguageEnvironment) -> Result<Vec<InstalledPackage>> {
        Ok(vec![])
    }
}
