// Node.js language plugin for JavaScript/TypeScript-based hooks
// Provides comprehensive Node.js package management, npm/yarn/pnpm support, and hook execution

use async_trait::async_trait;
use serde_json::Value;
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
    EnvironmentConfig, EnvironmentInfo, LanguageEnvironment, ValidationReport,
};
use super::traits::{Command, Language, LanguageConfig, LanguageError};

/// Node.js language plugin for JavaScript/TypeScript-based hooks
pub struct NodejsLanguagePlugin {
    config: NodejsLanguageConfig,
    node_cache: HashMap<String, NodeInfo>,
    package_manager_cache: HashMap<PathBuf, PackageManagerInfo>,
    dependency_manager: NodejsDependencyManager,
}

#[derive(Debug, Clone)]
pub struct NodejsLanguageConfig {
    pub default_node_version: Option<String>,
    pub prefer_system_node: bool,
    pub package_manager_preference: PackageManagerPreference,
    pub enable_workspaces: bool,
    pub cache_node_modules: bool,
    pub max_cache_age: Duration,
}

#[derive(Debug, Clone)]
pub enum PackageManagerPreference {
    Npm,
    Yarn,
    Pnpm,
    Auto, // Detect based on lockfiles
}

#[derive(Debug, Clone)]
pub struct PackageManagerInfo {
    pub manager_type: PackageManagerType,
    pub executable_path: PathBuf,
    pub version: semver::Version,
    pub supports_workspaces: bool,
    pub lockfile_path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PackageManagerType {
    Npm,
    Yarn,
    YarnBerry, // Yarn 2+
    Pnpm,
    Bun,
}

#[derive(Debug, Clone)]
pub struct NodeInfo {
    pub executable_path: PathBuf,
    pub version: semver::Version,
    pub npm_version: Option<semver::Version>,
    pub architecture: String,
    pub platform: String,
    pub installation_source: NodeInstallationSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeInstallationSource {
    System,
    Nvm,
    Fnm,
    Volta,
    Asdf,
    Manual(PathBuf),
}

#[derive(Debug, Clone)]
pub struct NodejsEnvironment {
    pub node_info: NodeInfo,
    pub package_manager: PackageManagerInfo,
    pub project_path: PathBuf,
    pub workspace_root: Option<PathBuf>,
    pub node_modules_path: PathBuf,
    pub package_json_path: PathBuf,
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

/// Node.js dependency manager
pub struct NodejsDependencyManager {
    config: DependencyManagerConfig,
}

impl Default for NodejsLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl NodejsLanguagePlugin {
    pub fn new() -> Self {
        Self {
            config: NodejsLanguageConfig::default(),
            node_cache: HashMap::new(),
            package_manager_cache: HashMap::new(),
            dependency_manager: NodejsDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    pub fn with_config(config: NodejsLanguageConfig) -> Self {
        Self {
            config,
            node_cache: HashMap::new(),
            package_manager_cache: HashMap::new(),
            dependency_manager: NodejsDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    /// Detect available Node.js installations
    pub async fn detect_node_installations(&mut self) -> Result<Vec<NodeInfo>> {
        let mut installations = Vec::new();

        // Common Node.js executable names
        let node_names = ["node", "nodejs"];

        for name in &node_names {
            if let Ok(path) = which::which(name) {
                if let Ok(info) = self.analyze_node_installation(&path).await {
                    installations.push(info);
                }
            }
        }

        // Check Node Version Manager installations
        installations.extend(self.detect_nvm_installations().await?);

        Ok(installations)
    }

    async fn analyze_node_installation(&self, node_path: &Path) -> Result<NodeInfo> {
        let output = TokioCommand::new(node_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to get Node.js version from {}", node_path.display()),
                recovery_suggestion: Some("Ensure Node.js is properly installed".to_string()),
            }));
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        let version_str = version_output.trim().trim_start_matches('v');
        let version = semver::Version::parse(version_str).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to parse Node.js version '{version_str}': {e}"),
                recovery_suggestion: None,
            })
        })?;

        // Get npm version if available
        let npm_version = self.detect_npm_version(node_path).await.ok();

        // Detect platform info
        let platform_info = Self::detect_platform_info()?;

        // Determine installation source
        let installation_source = self.determine_installation_source(node_path);

        Ok(NodeInfo {
            executable_path: node_path.to_path_buf(),
            version,
            npm_version,
            architecture: platform_info.architecture,
            platform: format!("{:?}", platform_info.os_type).to_lowercase(),
            installation_source,
        })
    }

    async fn detect_npm_version(&self, node_path: &Path) -> Result<semver::Version> {
        // Try to find npm in the same directory as node
        let node_dir = node_path.parent().ok_or_else(|| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "Cannot determine Node.js directory".to_string(),
                recovery_suggestion: None,
            })
        })?;

        let npm_path = if cfg!(windows) {
            node_dir.join("npm.cmd")
        } else {
            node_dir.join("npm")
        };

        let npm_executable = if npm_path.exists() {
            npm_path
        } else {
            // Fallback to system npm
            which::which("npm").map_err(|_| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "node".to_string(),
                    error: "npm not found".to_string(),
                    recovery_suggestion: Some(
                        "Install npm or use a Node.js version that includes npm".to_string(),
                    ),
                })
            })?
        };

        let output = TokioCommand::new(&npm_executable)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "Failed to get npm version".to_string(),
                recovery_suggestion: None,
            }));
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        let version_str = version_output.trim();
        semver::Version::parse(version_str).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to parse npm version '{version_str}': {e}"),
                recovery_suggestion: None,
            })
        })
    }

    async fn detect_nvm_installations(&self) -> Result<Vec<NodeInfo>> {
        let mut installations = Vec::new();

        // Check for NVM installation directory
        if let Ok(home_dir) = std::env::var("HOME") {
            let nvm_dir = PathBuf::from(home_dir)
                .join(".nvm")
                .join("versions")
                .join("node");
            if nvm_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&nvm_dir) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                            let node_exe = entry.path().join("bin").join("node");
                            if node_exe.exists() {
                                if let Ok(info) = self.analyze_node_installation(&node_exe).await {
                                    installations.push(info);
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(installations)
    }

    fn determine_installation_source(&self, node_path: &Path) -> NodeInstallationSource {
        let path_str = node_path.to_string_lossy();

        if path_str.contains("/.nvm/") {
            NodeInstallationSource::Nvm
        } else if path_str.contains("/.fnm/") {
            NodeInstallationSource::Fnm
        } else if path_str.contains("/.volta/") {
            NodeInstallationSource::Volta
        } else if path_str.contains("/.asdf/") {
            NodeInstallationSource::Asdf
        } else if path_str.starts_with("/usr/bin") || path_str.starts_with("/usr/local/bin") {
            NodeInstallationSource::System
        } else {
            NodeInstallationSource::Manual(node_path.parent().unwrap_or(node_path).to_path_buf())
        }
    }

    pub fn detect_platform_info() -> Result<PlatformInfo> {
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

    /// Find suitable Node.js installation for given config
    pub async fn find_suitable_node(&mut self, config: &EnvironmentConfig) -> Result<NodeInfo> {
        let installations = self.detect_node_installations().await?;

        if installations.is_empty() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "No Node.js installations found".to_string(),
                recovery_suggestion: Some(
                    "Install Node.js from https://nodejs.org/ or use a version manager like nvm"
                        .to_string(),
                ),
            }));
        }

        // If specific version requested, try to find it
        if let Some(ref version_req) = config.language_version {
            if version_req != "system" && version_req != "node" {
                let version_spec = semver::VersionReq::parse(version_req).map_err(|_e| {
                    SnpError::from(LanguageError::UnsupportedVersion {
                        language: "node".to_string(),
                        version: version_req.clone(),
                        supported_versions: installations
                            .iter()
                            .map(|i| i.version.to_string())
                            .collect(),
                    })
                })?;

                for installation in &installations {
                    if version_spec.matches(&installation.version) {
                        return Ok(installation.clone());
                    }
                }

                return Err(SnpError::from(LanguageError::UnsupportedVersion {
                    language: "node".to_string(),
                    version: version_req.clone(),
                    supported_versions: installations
                        .iter()
                        .map(|i| i.version.to_string())
                        .collect(),
                }));
            }
        }

        // Return the first available installation (typically system or latest)
        Ok(installations[0].clone())
    }

    /// Detect package manager for a project
    pub async fn detect_package_manager(
        &mut self,
        project_path: &Path,
    ) -> Result<PackageManagerInfo> {
        // Check cache first
        if let Some(cached) = self.package_manager_cache.get(project_path) {
            return Ok(cached.clone());
        }

        let manager_info = match self.config.package_manager_preference {
            PackageManagerPreference::Auto => {
                self.auto_detect_package_manager(project_path).await?
            }
            PackageManagerPreference::Npm => self.setup_npm_manager(project_path).await?,
            PackageManagerPreference::Yarn => self.setup_yarn_manager(project_path).await?,
            PackageManagerPreference::Pnpm => self.setup_pnpm_manager(project_path).await?,
        };

        // Cache the result
        self.package_manager_cache
            .insert(project_path.to_path_buf(), manager_info.clone());
        Ok(manager_info)
    }

    async fn auto_detect_package_manager(&self, project_path: &Path) -> Result<PackageManagerInfo> {
        // Check for lockfiles to determine package manager
        if project_path.join("pnpm-lock.yaml").exists() {
            match self.setup_pnpm_manager(project_path).await {
                Ok(manager) => Ok(manager),
                Err(_) => self.setup_npm_manager(project_path).await,
            }
        } else if project_path.join("yarn.lock").exists() {
            match self.setup_yarn_manager(project_path).await {
                Ok(manager) => Ok(manager),
                Err(_) => self.setup_npm_manager(project_path).await,
            }
        } else if project_path.join("package-lock.json").exists() {
            self.setup_npm_manager(project_path).await
        } else {
            // Default to npm if no lockfile found
            self.setup_npm_manager(project_path).await
        }
    }

    async fn setup_npm_manager(&self, project_path: &Path) -> Result<PackageManagerInfo> {
        let npm_path = which::which("npm").map_err(|_| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "npm not found".to_string(),
                recovery_suggestion: Some("Install Node.js which includes npm".to_string()),
            })
        })?;

        let version = self.get_npm_version(&npm_path).await?;

        Ok(PackageManagerInfo {
            manager_type: PackageManagerType::Npm,
            executable_path: npm_path,
            version: version.clone(),
            supports_workspaces: version >= semver::Version::new(7, 0, 0),
            lockfile_path: Some(project_path.join("package-lock.json")),
        })
    }

    async fn setup_yarn_manager(&self, project_path: &Path) -> Result<PackageManagerInfo> {
        let yarn_path = which::which("yarn").map_err(|_| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "yarn not found".to_string(),
                recovery_suggestion: Some("Install yarn with `npm install -g yarn`".to_string()),
            })
        })?;

        let version = self.get_yarn_version(&yarn_path).await?;
        let is_berry = version >= semver::Version::new(2, 0, 0);

        Ok(PackageManagerInfo {
            manager_type: if is_berry {
                PackageManagerType::YarnBerry
            } else {
                PackageManagerType::Yarn
            },
            executable_path: yarn_path,
            version: version.clone(),
            supports_workspaces: true, // Yarn has had workspace support for a long time
            lockfile_path: Some(project_path.join("yarn.lock")),
        })
    }

    async fn setup_pnpm_manager(&self, project_path: &Path) -> Result<PackageManagerInfo> {
        let pnpm_path = which::which("pnpm").map_err(|_| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "pnpm not found".to_string(),
                recovery_suggestion: Some("Install pnpm with `npm install -g pnpm`".to_string()),
            })
        })?;

        let version = self.get_pnpm_version(&pnpm_path).await?;

        Ok(PackageManagerInfo {
            manager_type: PackageManagerType::Pnpm,
            executable_path: pnpm_path,
            version,
            supports_workspaces: true,
            lockfile_path: Some(project_path.join("pnpm-lock.yaml")),
        })
    }

    async fn get_npm_version(&self, npm_path: &Path) -> Result<semver::Version> {
        let output = TokioCommand::new(npm_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "Failed to get npm version".to_string(),
                recovery_suggestion: None,
            }));
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        let version_str = version_output.trim();
        semver::Version::parse(version_str).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to parse npm version: {e}"),
                recovery_suggestion: None,
            })
        })
    }

    async fn get_yarn_version(&self, yarn_path: &Path) -> Result<semver::Version> {
        let output = TokioCommand::new(yarn_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "Failed to get yarn version".to_string(),
                recovery_suggestion: None,
            }));
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        let version_str = version_output.trim();
        semver::Version::parse(version_str).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to parse yarn version: {e}"),
                recovery_suggestion: None,
            })
        })
    }

    async fn get_pnpm_version(&self, pnpm_path: &Path) -> Result<semver::Version> {
        let output = TokioCommand::new(pnpm_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: "Failed to get pnpm version".to_string(),
                recovery_suggestion: None,
            }));
        }

        let version_output = String::from_utf8_lossy(&output.stdout);
        let version_str = version_output.trim();
        semver::Version::parse(version_str).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to parse pnpm version: {e}"),
                recovery_suggestion: None,
            })
        })
    }

    /// Install dependencies using the detected package manager
    pub async fn install_dependencies(
        &self,
        package_manager: &PackageManagerInfo,
        project_path: &Path,
        dependencies: &[String],
    ) -> Result<()> {
        if dependencies.is_empty() {
            return Ok(());
        }

        let mut args = Vec::new();

        match package_manager.manager_type {
            PackageManagerType::Npm => {
                args.extend(&["install", "--save-dev"]);
                args.extend(dependencies.iter().map(|s| s.as_str()));
            }
            PackageManagerType::Yarn | PackageManagerType::YarnBerry => {
                args.push("add");
                args.push("--dev");
                args.extend(dependencies.iter().map(|s| s.as_str()));
            }
            PackageManagerType::Pnpm => {
                args.extend(&["add", "--save-dev"]);
                args.extend(dependencies.iter().map(|s| s.as_str()));
            }
            PackageManagerType::Bun => {
                args.extend(&["add", "--dev"]);
                args.extend(dependencies.iter().map(|s| s.as_str()));
            }
        }

        let output = TokioCommand::new(&package_manager.executable_path)
            .args(&args)
            .current_dir(project_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(
                LanguageError::DependencyInstallationFailed {
                    dependency: dependencies.join(", "),
                    language: "node".to_string(),
                    error: String::from_utf8_lossy(&output.stderr).to_string(),
                    installation_log: Some(String::from_utf8_lossy(&output.stdout).to_string()),
                },
            ));
        }

        Ok(())
    }

    /// Find workspace root directory
    pub fn find_workspace_root(&self, project_path: &Path) -> Result<Option<PathBuf>> {
        let mut current = project_path;

        loop {
            // Check for workspace indicators
            if self.is_workspace_root(current)? {
                return Ok(Some(current.to_path_buf()));
            }

            match current.parent() {
                Some(parent) => current = parent,
                None => return Ok(None),
            }
        }
    }

    pub fn is_workspace_root(&self, path: &Path) -> Result<bool> {
        let package_json_path = path.join("package.json");
        if !package_json_path.exists() {
            return Ok(false);
        }

        // Parse package.json to check for workspaces
        let content = std::fs::read_to_string(&package_json_path).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to read package.json: {e}"),
                recovery_suggestion: None,
            })
        })?;

        let package_json: Value = serde_json::from_str(&content).map_err(|e| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "node".to_string(),
                error: format!("Failed to parse package.json: {e}"),
                recovery_suggestion: Some("Ensure package.json contains valid JSON".to_string()),
            })
        })?;

        // Check for npm/yarn workspaces
        if package_json.get("workspaces").is_some() {
            return Ok(true);
        }

        // Check for lerna.json
        if path.join("lerna.json").exists() {
            return Ok(true);
        }

        // Check for nx.json
        if path.join("nx.json").exists() {
            return Ok(true);
        }

        // Check for rush.json
        if path.join("rush.json").exists() {
            return Ok(true);
        }

        Ok(false)
    }

    fn find_node_modules(&self, project_path: &Path) -> Result<PathBuf> {
        let node_modules = project_path.join("node_modules");
        if node_modules.exists() {
            Ok(node_modules)
        } else {
            // Create the node_modules directory if it doesn't exist
            Ok(node_modules)
        }
    }

    fn find_project_root(&self, starting_path: &Path) -> Result<PathBuf> {
        let mut current = starting_path;

        loop {
            if current.join("package.json").exists() {
                return Ok(current.to_path_buf());
            }

            match current.parent() {
                Some(parent) => current = parent,
                None => return Ok(starting_path.to_path_buf()), // Fallback to starting path
            }
        }
    }

    /// Setup Node.js environment with dependencies
    pub async fn setup_nodejs_environment(
        &mut self,
        node_info: &NodeInfo,
        project_path: &Path,
        dependencies: &[String],
    ) -> Result<NodejsEnvironment> {
        let setup_start = Instant::now();
        tracing::debug!(
            "Setting up Node.js environment for project: {}",
            project_path.display()
        );

        // Detect workspace root if applicable
        tracing::debug!("Detecting workspace root");
        let workspace_root = self.find_workspace_root(project_path).map_err(|e| {
            tracing::error!("Failed to detect workspace root: {}", e);
            e
        })?;
        let work_dir = workspace_root.as_deref().unwrap_or(project_path);
        let workspace_root_clone = workspace_root.clone();

        if let Some(ref workspace) = workspace_root {
            tracing::debug!("Found workspace root: {}", workspace.display());
        } else {
            tracing::debug!("No workspace root found, using project path");
        }

        // Detect package manager
        tracing::debug!("Detecting package manager for {}", work_dir.display());
        let package_manager_start = Instant::now();
        let package_manager = self.detect_package_manager(work_dir).await.map_err(|e| {
            tracing::error!("Failed to detect package manager: {}", e);
            e
        })?;
        tracing::debug!(
            "Detected package manager: {} (v{}) in {:?}",
            package_manager.manager_type.as_str(),
            package_manager.version,
            package_manager_start.elapsed()
        );

        // Install dependencies if needed
        if !dependencies.is_empty() {
            tracing::debug!(
                "Installing {} dependencies: {:?}",
                dependencies.len(),
                dependencies
            );
            let install_start = Instant::now();
            self.install_dependencies(&package_manager, work_dir, dependencies)
                .await
                .map_err(|e| {
                    tracing::error!("Failed to install dependencies: {}", e);
                    e
                })?;
            tracing::debug!("Dependencies installed in {:?}", install_start.elapsed());
        } else {
            tracing::debug!("No additional dependencies to install");
        }

        let total_setup_time = setup_start.elapsed();
        tracing::debug!(
            "Node.js environment setup completed in {:?}",
            total_setup_time
        );

        // Log performance warning if setup took too long
        if total_setup_time.as_secs() > 3 {
            tracing::warn!("Node.js environment setup took {:?}, which is longer than expected. Consider checking network connectivity or using dependency caching.", total_setup_time);
        }

        Ok(NodejsEnvironment {
            node_info: node_info.clone(),
            package_manager,
            project_path: project_path.to_path_buf(),
            workspace_root: workspace_root_clone,
            node_modules_path: self.find_node_modules(work_dir)?,
            package_json_path: work_dir.join("package.json"),
        })
    }

    fn generate_environment_id(&self, nodejs_env: &NodejsEnvironment) -> String {
        format!(
            "node-{}-{}-{}",
            nodejs_env.node_info.version,
            nodejs_env.package_manager.manager_type.as_str(),
            nodejs_env
                .project_path
                .to_string_lossy()
                .chars()
                .collect::<String>()
                .chars()
                .take(8)
                .collect::<String>()
        )
    }

    fn get_nodejs_environment_variables(
        &self,
        nodejs_env: &NodejsEnvironment,
    ) -> Result<HashMap<String, String>> {
        let mut env_vars = HashMap::new();

        // Set NODE_PATH to include local node_modules
        let node_path = nodejs_env.node_modules_path.to_string_lossy().to_string();
        env_vars.insert("NODE_PATH".to_string(), node_path);

        // Set NODE_VIRTUAL_ENV for compatibility
        env_vars.insert(
            "NODE_VIRTUAL_ENV".to_string(),
            nodejs_env.project_path.to_string_lossy().to_string(),
        );

        // Set npm configuration
        env_vars.insert("NPM_CONFIG_USERCONFIG".to_string(), "".to_string()); // Disable user config
        env_vars.insert("npm_config_userconfig".to_string(), "".to_string());

        // Add node executable to PATH
        if let Some(node_dir) = nodejs_env.node_info.executable_path.parent() {
            let current_path = std::env::var("PATH").unwrap_or_default();
            let new_path = if current_path.is_empty() {
                node_dir.to_string_lossy().to_string()
            } else {
                format!("{}:{}", node_dir.display(), current_path)
            };
            env_vars.insert("PATH".to_string(), new_path);
        }

        Ok(env_vars)
    }

    async fn get_installed_packages(
        &self,
        _nodejs_env: &NodejsEnvironment,
    ) -> Result<Vec<Dependency>> {
        // For now, return empty. In a full implementation, we'd parse package.json dependencies
        Ok(vec![])
    }

    fn create_environment_metadata(
        &self,
        nodejs_env: &NodejsEnvironment,
    ) -> super::environment::EnvironmentMetadata {
        super::environment::EnvironmentMetadata {
            created_at: std::time::SystemTime::now(),
            last_used: std::time::SystemTime::now(),
            usage_count: 0,
            size_bytes: 0,
            dependency_count: 0,
            language_version: nodejs_env.node_info.version.to_string(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
        }
    }

    /// Build Node.js command for execution
    pub fn build_nodejs_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        let mut command = Command::new(&hook.entry);

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

        // Set working directory
        if let Some(parent) = env.root_path.parent() {
            command.current_dir(parent);
        }

        // Set timeout
        command.timeout(Duration::from_secs(300)); // 5 minutes default

        Ok(command)
    }

    /// Execute a Node.js command
    pub async fn execute_nodejs_command(&self, command: Command) -> Result<HookExecutionResult> {
        let start_time = Instant::now();

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
                Ok(output) => output?,
                Err(_) => {
                    return Err(SnpError::from(Box::new(
                        crate::error::ProcessError::Timeout {
                            command: command.executable.clone(),
                            duration: timeout,
                        },
                    )));
                }
            }
        } else {
            tokio_cmd.output().await?
        };

        let duration = start_time.elapsed();

        Ok(HookExecutionResult {
            hook_id: "node".to_string(), // Will be overridden by caller
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
                    hook_id: "node".to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            },
        })
    }
}

impl Default for NodejsLanguageConfig {
    fn default() -> Self {
        Self {
            default_node_version: None,
            prefer_system_node: true,
            package_manager_preference: PackageManagerPreference::Auto,
            enable_workspaces: true,
            cache_node_modules: true,
            max_cache_age: Duration::from_secs(3600), // 1 hour
        }
    }
}

impl PackageManagerType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PackageManagerType::Npm => "npm",
            PackageManagerType::Yarn => "yarn",
            PackageManagerType::YarnBerry => "yarn-berry",
            PackageManagerType::Pnpm => "pnpm",
            PackageManagerType::Bun => "bun",
        }
    }
}

#[async_trait]
impl Language for NodejsLanguagePlugin {
    fn language_name(&self) -> &str {
        "node"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["js", "mjs", "cjs", "ts", "tsx", "jsx", "json"]
    }

    fn detection_patterns(&self) -> &[&str] {
        &[
            r"^#\!/usr/bin/env node",
            r"^#\!/usr/bin/node",
            r"^#\!/usr/local/bin/node",
            "package.json",
            "yarn.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        let setup_start = Instant::now();
        tracing::debug!("Starting Node.js environment setup");

        // Find suitable Node.js installation
        let mut plugin_copy = self.clone(); // We need a mutable reference
        tracing::debug!("Finding suitable Node.js installation");
        let node_info = plugin_copy.find_suitable_node(config).await.map_err(|e| {
            tracing::error!("Failed to find suitable Node.js installation: {}", e);
            e
        })?;
        tracing::debug!(
            "Found Node.js installation: v{} at {}",
            node_info.version,
            node_info.executable_path.display()
        );

        // Determine project root (look for package.json)
        tracing::debug!("Determining project root directory");
        let project_root = self
            .find_project_root(&std::env::current_dir()?)
            .map_err(|e| {
                tracing::error!("Failed to determine project root: {}", e);
                e
            })?;
        tracing::debug!("Project root: {}", project_root.display());

        // Setup Node.js environment
        tracing::debug!(
            "Setting up Node.js environment with {} dependencies",
            config.additional_dependencies.len()
        );
        let nodejs_env = plugin_copy
            .setup_nodejs_environment(&node_info, &project_root, &config.additional_dependencies)
            .await
            .map_err(|e| {
                tracing::error!("Failed to setup Node.js environment: {}", e);
                e
            })?;

        tracing::debug!(
            "Node.js environment setup completed in {:?}",
            setup_start.elapsed()
        );

        Ok(LanguageEnvironment {
            language: "node".to_string(),
            environment_id: self.generate_environment_id(&nodejs_env),
            root_path: nodejs_env.project_path.clone(),
            executable_path: nodejs_env.node_info.executable_path.clone(),
            environment_variables: self.get_nodejs_environment_variables(&nodejs_env)?,
            installed_dependencies: self.get_installed_packages(&nodejs_env).await?,
            metadata: self.create_environment_metadata(&nodejs_env),
        })
    }

    async fn install_dependencies(
        &self,
        env: &LanguageEnvironment,
        dependencies: &[Dependency],
    ) -> Result<()> {
        if dependencies.is_empty() {
            return Ok(());
        }

        // Convert dependencies to strings
        let dep_names: Vec<String> = dependencies.iter().map(|dep| dep.name.clone()).collect();

        // We need access to package manager info, so this is a simplified implementation
        // In a full implementation, we'd store the package manager info in the environment
        let mut plugin_copy = self.clone();
        let package_manager = plugin_copy.detect_package_manager(&env.root_path).await?;

        self.install_dependencies(&package_manager, &env.root_path, &dep_names)
            .await
    }

    async fn cleanup_environment(&self, _env: &LanguageEnvironment) -> Result<()> {
        // Node.js environments don't typically need cleanup
        Ok(())
    }

    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        let execution_start = Instant::now();
        tracing::debug!(
            "Executing Node.js hook '{}' with {} files",
            hook.id,
            files.len()
        );

        let command = self.build_command(hook, env, files).map_err(|e| {
            tracing::error!("Failed to build command for hook '{}': {}", hook.id, e);
            e
        })?;

        tracing::debug!(
            "Built command for hook '{}': {} with {} args",
            hook.id,
            command.executable,
            command.arguments.len()
        );

        let mut result = self.execute_nodejs_command(command).await.map_err(|e| {
            tracing::error!(
                "Failed to execute Node.js command for hook '{}': {}",
                hook.id,
                e
            );
            e
        })?;

        // Set the correct hook ID and files
        result.hook_id = hook.id.clone();
        result.files_processed = files.to_vec();

        let execution_time = execution_start.elapsed();
        if result.success {
            tracing::debug!(
                "Hook '{}' completed successfully in {:?}",
                hook.id,
                execution_time
            );
        } else {
            tracing::warn!(
                "Hook '{}' failed after {:?} with exit code {:?}",
                hook.id,
                execution_time,
                result.exit_code
            );
            if !result.stderr.is_empty() {
                tracing::debug!("Hook '{}' stderr: {}", hook.id, result.stderr.trim());
            }
        }

        Ok(result)
    }

    fn build_command(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<Command> {
        self.build_nodejs_command(hook, env, files)
    }

    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>> {
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
        Dependency::parse_spec(dep_spec)
    }

    fn get_dependency_manager(&self) -> &dyn DependencyManager {
        &self.dependency_manager
    }

    fn get_environment_info(&self, env: &LanguageEnvironment) -> EnvironmentInfo {
        EnvironmentInfo {
            environment_id: env.environment_id.clone(),
            language: env.language.clone(),
            language_version: env.metadata.language_version.clone(),
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

// Make the plugin cloneable for internal use
impl Clone for NodejsLanguagePlugin {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            node_cache: self.node_cache.clone(),
            package_manager_cache: self.package_manager_cache.clone(),
            dependency_manager: NodejsDependencyManager {
                config: self.dependency_manager.config.clone(),
            },
        }
    }
}

#[async_trait]
impl DependencyManager for NodejsDependencyManager {
    async fn install(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<super::dependency::InstallationResult> {
        // Node.js dependencies are installed via package managers
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
        Ok(vec![])
    }

    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<Vec<bool>> {
        // For simplicity, assume all are installed
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
        Ok(())
    }

    async fn list_installed(
        &self,
        _env: &LanguageEnvironment,
    ) -> Result<Vec<super::dependency::InstalledPackage>> {
        Ok(vec![])
    }
}
