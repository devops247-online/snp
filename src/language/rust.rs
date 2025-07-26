// Rust language plugin for Rust-based hooks and tools
// This plugin provides comprehensive Rust toolchain management, Cargo dependency handling,
// and hook execution for Rust-based pre-commit hooks and development tools.

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
    EnvironmentConfig, EnvironmentInfo, LanguageEnvironment, ValidationReport,
};
use super::traits::{Command, Language, LanguageConfig, LanguageError};

/// Rust language plugin for Rust-based hooks and tools
pub struct RustLanguagePlugin {
    config: RustLanguageConfig,
    toolchain_cache: HashMap<String, RustToolchainInfo>,
    workspace_cache: HashMap<PathBuf, CargoWorkspace>,
    dependency_manager: RustDependencyManager,
}

#[derive(Debug, Clone)]
pub struct RustLanguageConfig {
    pub default_toolchain: Option<String>,
    pub prefer_system_rust: bool,
    pub enable_incremental_compilation: bool,
    pub shared_target_dir: Option<PathBuf>,
    pub cargo_config: CargoConfig,
    pub rustup_config: RustupConfig,
}

#[derive(Debug, Clone)]
pub struct CargoConfig {
    pub offline_mode: bool,
    pub parallel_jobs: Option<usize>,
    pub target_dir: Option<PathBuf>,
    pub registry_url: Option<String>,
    pub build_profile: BuildProfile,
}

#[derive(Debug, Clone)]
pub enum BuildProfile {
    Debug,
    Release,
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct RustupConfig {
    pub auto_install_toolchains: bool,
    pub auto_install_components: bool,
    pub preferred_toolchain: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RustToolchainInfo {
    pub name: String,
    pub rust_version: semver::Version,
    pub rustc_path: PathBuf,
    pub cargo_path: PathBuf,
    pub toolchain_path: Option<PathBuf>,
    pub components: Vec<RustComponent>,
    pub targets: Vec<String>,
    pub is_default: bool,
    pub source: RustInstallationSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RustInstallationSource {
    Rustup { channel: String },
    System,
    Manual(PathBuf),
}

#[derive(Debug, Clone)]
pub struct RustComponent {
    pub name: String,
    pub installed: bool,
    pub available: bool,
}

#[derive(Debug, Clone)]
pub struct CargoWorkspace {
    pub root_path: PathBuf,
    pub workspace_type: WorkspaceType,
    pub members: Vec<CargoPackage>,
    pub cargo_toml_path: PathBuf,
    pub cargo_lock_path: Option<PathBuf>,
    pub target_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WorkspaceType {
    Single,  // Single package (no workspace)
    Virtual, // Virtual workspace (no root package)
    Mixed,   // Workspace with root package
}

#[derive(Debug, Clone)]
pub struct CargoPackage {
    pub name: String,
    pub version: semver::Version,
    pub path: PathBuf,
    pub cargo_toml_path: PathBuf,
    pub dependencies: Vec<CargoDependency>,
    pub dev_dependencies: Vec<CargoDependency>,
    pub build_dependencies: Vec<CargoDependency>,
    pub features: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct CargoDependency {
    pub name: String,
    pub version_req: semver::VersionReq,
    pub source: DependencySource,
    pub features: Vec<String>,
    pub optional: bool,
}

#[derive(Debug, Clone)]
pub enum DependencySource {
    Registry(String),
    Git { url: String, rev: Option<String> },
    Path(PathBuf),
}

#[derive(Debug, Clone)]
pub struct RustEnvironment {
    pub toolchain: RustToolchainInfo,
    pub workspace: CargoWorkspace,
    pub target_dir: PathBuf,
    pub cargo_config: CargoConfigFile,
}

#[derive(Debug, Clone)]
pub struct CargoConfigFile {
    pub content: HashMap<String, toml::Value>,
    pub path: Option<PathBuf>,
}

/// Rust dependency manager with Cargo integration
pub struct RustDependencyManager {
    config: DependencyManagerConfig,
}

impl Default for RustLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}

impl RustLanguagePlugin {
    pub fn new() -> Self {
        Self {
            config: RustLanguageConfig::default(),
            toolchain_cache: HashMap::new(),
            workspace_cache: HashMap::new(),
            dependency_manager: RustDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    pub fn with_config(config: RustLanguageConfig) -> Self {
        Self {
            config,
            toolchain_cache: HashMap::new(),
            workspace_cache: HashMap::new(),
            dependency_manager: RustDependencyManager {
                config: DependencyManagerConfig::default(),
            },
        }
    }

    /// Detect available Rust toolchains
    pub async fn detect_rust_toolchains(&mut self) -> Result<Vec<RustToolchainInfo>> {
        let mut toolchains = Vec::new();

        // Check for rustup-managed toolchains
        if let Ok(rustup_toolchains) = self.detect_rustup_toolchains().await {
            toolchains.extend(rustup_toolchains);
        }

        // Check for system Rust installation
        if let Ok(system_rust) = self.detect_system_rust().await {
            toolchains.push(system_rust);
        }

        // Sort by version (newest first)
        toolchains.sort_by(|a, b| b.rust_version.cmp(&a.rust_version));
        Ok(toolchains)
    }

    async fn detect_rustup_toolchains(&self) -> Result<Vec<RustToolchainInfo>> {
        let rustup_path = which::which("rustup")?;
        let output = TokioCommand::new(&rustup_path)
            .args(["toolchain", "list"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "rust".to_string(),
                error: format!(
                    "rustup toolchain list failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
                recovery_suggestion: Some("Check rustup installation".to_string()),
            }));
        }

        let mut toolchains = Vec::new();
        let output_str = String::from_utf8_lossy(&output.stdout);

        for line in output_str.lines() {
            if let Some(toolchain_info) = self.parse_rustup_toolchain_line(line).await? {
                toolchains.push(toolchain_info);
            }
        }

        Ok(toolchains)
    }

    async fn parse_rustup_toolchain_line(&self, line: &str) -> Result<Option<RustToolchainInfo>> {
        let line = line.trim();
        if line.is_empty() {
            return Ok(None);
        }

        // Parse line like "stable-x86_64-unknown-linux-gnu (default)"
        let (name, is_default) = if let Some(stripped) = line.strip_suffix(" (default)") {
            (stripped, true)
        } else {
            (line, false)
        };

        // Get toolchain info using rustup
        let rustup_path = which::which("rustup")?;
        let output = TokioCommand::new(&rustup_path)
            .args(["run", name, "rustc", "--version"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        if !output.status.success() {
            return Ok(None);
        }

        let version_str = String::from_utf8_lossy(&output.stdout);
        let rust_version = self.parse_rust_version(&version_str)?;

        // Get toolchain path
        let toolchain_path_output = TokioCommand::new(&rustup_path)
            .args(["run", name, "rustc", "--print", "sysroot"])
            .stdout(Stdio::piped())
            .output()
            .await?;

        let toolchain_path = if toolchain_path_output.status.success() {
            let path_str = String::from_utf8_lossy(&toolchain_path_output.stdout);
            let trimmed_path = path_str.trim();
            Some(PathBuf::from(trimmed_path))
        } else {
            None
        };

        let rustc_path = if let Some(ref tp) = toolchain_path {
            tp.join("bin")
                .join(if cfg!(windows) { "rustc.exe" } else { "rustc" })
        } else {
            which::which("rustc")?
        };

        let cargo_path = if let Some(ref tp) = toolchain_path {
            tp.join("bin")
                .join(if cfg!(windows) { "cargo.exe" } else { "cargo" })
        } else {
            which::which("cargo")?
        };

        Ok(Some(RustToolchainInfo {
            name: name.to_string(),
            rust_version,
            rustc_path,
            cargo_path,
            toolchain_path,
            components: self.detect_toolchain_components(name).await?,
            targets: self.detect_toolchain_targets(name).await?,
            is_default,
            source: RustInstallationSource::Rustup {
                channel: name.to_string(),
            },
        }))
    }

    async fn detect_system_rust(&self) -> Result<RustToolchainInfo> {
        let rustc_path = which::which("rustc")?;
        let cargo_path = which::which("cargo")?;

        let version_output = TokioCommand::new(&rustc_path)
            .arg("--version")
            .stdout(Stdio::piped())
            .output()
            .await?;

        let version_str = String::from_utf8_lossy(&version_output.stdout);
        let rust_version = self.parse_rust_version(&version_str)?;

        Ok(RustToolchainInfo {
            name: "system".to_string(),
            rust_version,
            rustc_path,
            cargo_path,
            toolchain_path: None,
            components: self.detect_system_components().await?,
            targets: self.detect_system_targets().await?,
            is_default: false,
            source: RustInstallationSource::System,
        })
    }

    fn parse_rust_version(&self, version_str: &str) -> Result<semver::Version> {
        // Parse version from output like "rustc 1.70.0 (90c541806 2023-05-31)"
        let version_regex = regex::Regex::new(r"rustc (\d+\.\d+\.\d+)")?;

        if let Some(captures) = version_regex.captures(version_str) {
            if let Some(version_match) = captures.get(1) {
                return Ok(semver::Version::parse(version_match.as_str())?);
            }
        }

        Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
            language: "rust".to_string(),
            error: format!("Could not parse Rust version from: {version_str}"),
            recovery_suggestion: Some("Check Rust installation".to_string()),
        }))
    }

    async fn detect_toolchain_components(&self, toolchain: &str) -> Result<Vec<RustComponent>> {
        if let Ok(rustup_path) = which::which("rustup") {
            let output = TokioCommand::new(&rustup_path)
                .args(["component", "list", "--toolchain", toolchain])
                .stdout(Stdio::piped())
                .output()
                .await?;

            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let mut components = Vec::new();

                for line in output_str.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }

                    let (name, installed) =
                        if let Some(stripped) = line.strip_suffix(" (installed)") {
                            (stripped, true)
                        } else {
                            (line, false)
                        };

                    components.push(RustComponent {
                        name: name.to_string(),
                        installed,
                        available: true,
                    });
                }

                return Ok(components);
            }
        }

        // Fallback for system installation
        Ok(vec![])
    }

    async fn detect_toolchain_targets(&self, toolchain: &str) -> Result<Vec<String>> {
        if let Ok(rustup_path) = which::which("rustup") {
            let output = TokioCommand::new(&rustup_path)
                .args(["target", "list", "--toolchain", toolchain])
                .stdout(Stdio::piped())
                .output()
                .await?;

            if output.status.success() {
                let output_str = String::from_utf8_lossy(&output.stdout);
                let targets: Vec<String> = output_str
                    .lines()
                    .map(|line| {
                        if let Some(stripped) = line.strip_suffix(" (installed)") {
                            stripped.to_string()
                        } else {
                            line.trim().to_string()
                        }
                    })
                    .filter(|line| !line.is_empty())
                    .collect();

                return Ok(targets);
            }
        }

        // Fallback
        Ok(vec![])
    }

    async fn detect_system_components(&self) -> Result<Vec<RustComponent>> {
        // For system installation, we check if common tools are available
        let common_tools = ["clippy", "rustfmt", "rust-analyzer"];
        let mut components = Vec::new();

        for tool in &common_tools {
            let tool_name = if cfg!(windows) {
                format!("{tool}.exe")
            } else {
                tool.to_string()
            };

            let installed = which::which(&tool_name).is_ok();
            components.push(RustComponent {
                name: tool.to_string(),
                installed,
                available: true,
            });
        }

        Ok(components)
    }

    async fn detect_system_targets(&self) -> Result<Vec<String>> {
        // For system installation, we just return the current target
        Ok(vec![format!(
            "{}-{}-{}",
            std::env::consts::ARCH,
            "unknown",
            std::env::consts::OS
        )])
    }

    /// Detect Cargo workspace configuration
    pub async fn detect_cargo_workspace(&mut self, project_path: &Path) -> Result<CargoWorkspace> {
        // Check cache first
        if let Some(cached) = self.workspace_cache.get(project_path) {
            return Ok(cached.clone());
        }

        let workspace = self.analyze_cargo_workspace(project_path).await?;

        // Cache the result
        self.workspace_cache
            .insert(project_path.to_path_buf(), workspace.clone());
        Ok(workspace)
    }

    async fn analyze_cargo_workspace(&self, project_path: &Path) -> Result<CargoWorkspace> {
        let cargo_toml_path = self.find_cargo_toml(project_path)?;
        let cargo_toml = self.parse_cargo_toml(&cargo_toml_path)?;

        let workspace_type = if cargo_toml.get("workspace").is_some() {
            if cargo_toml.get("package").is_some() {
                WorkspaceType::Mixed
            } else {
                WorkspaceType::Virtual
            }
        } else {
            WorkspaceType::Single
        };

        let members = match workspace_type {
            WorkspaceType::Single => {
                vec![self.parse_cargo_package(&cargo_toml_path)?]
            }
            WorkspaceType::Virtual | WorkspaceType::Mixed => {
                self.discover_workspace_members(&cargo_toml_path, &cargo_toml)
                    .await?
            }
        };

        Ok(CargoWorkspace {
            root_path: cargo_toml_path.parent().unwrap().to_path_buf(),
            workspace_type,
            members,
            cargo_toml_path: cargo_toml_path.clone(),
            cargo_lock_path: self.find_cargo_lock(&cargo_toml_path),
            target_dir: self.determine_target_dir(&cargo_toml_path)?,
        })
    }

    fn find_cargo_toml(&self, project_path: &Path) -> Result<PathBuf> {
        let mut current = project_path.to_path_buf();

        loop {
            let cargo_toml = current.join("Cargo.toml");
            if cargo_toml.exists() {
                return Ok(cargo_toml);
            }

            if let Some(parent) = current.parent() {
                current = parent.to_path_buf();
            } else {
                break;
            }
        }

        Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
            language: "rust".to_string(),
            error: "Cargo.toml not found in project hierarchy".to_string(),
            recovery_suggestion: Some("Initialize Rust project with 'cargo init'".to_string()),
        }))
    }

    fn parse_cargo_toml(&self, path: &Path) -> Result<toml::Value> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    fn parse_cargo_package(&self, cargo_toml_path: &Path) -> Result<CargoPackage> {
        let cargo_toml = self.parse_cargo_toml(cargo_toml_path)?;

        let package = cargo_toml.get("package").ok_or_else(|| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "rust".to_string(),
                error: "No [package] section in Cargo.toml".to_string(),
                recovery_suggestion: Some("Add [package] section to Cargo.toml".to_string()),
            })
        })?;

        let name = package
            .get("name")
            .and_then(|n| n.as_str())
            .ok_or_else(|| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "rust".to_string(),
                    error: "No package name in Cargo.toml".to_string(),
                    recovery_suggestion: None,
                })
            })?;

        let version_str = package
            .get("version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "rust".to_string(),
                    error: "No package version in Cargo.toml".to_string(),
                    recovery_suggestion: None,
                })
            })?;

        let version = semver::Version::parse(version_str)?;

        Ok(CargoPackage {
            name: name.to_string(),
            version,
            path: cargo_toml_path.parent().unwrap().to_path_buf(),
            cargo_toml_path: cargo_toml_path.to_path_buf(),
            dependencies: self.parse_dependencies(&cargo_toml, "dependencies")?,
            dev_dependencies: self.parse_dependencies(&cargo_toml, "dev-dependencies")?,
            build_dependencies: self.parse_dependencies(&cargo_toml, "build-dependencies")?,
            features: self.parse_features(&cargo_toml)?,
        })
    }

    async fn discover_workspace_members(
        &self,
        cargo_toml_path: &Path,
        cargo_toml: &toml::Value,
    ) -> Result<Vec<CargoPackage>> {
        let workspace = cargo_toml.get("workspace").ok_or_else(|| {
            SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "rust".to_string(),
                error: "No workspace section found".to_string(),
                recovery_suggestion: None,
            })
        })?;

        let members = workspace
            .get("members")
            .and_then(|m| m.as_array())
            .ok_or_else(|| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "rust".to_string(),
                    error: "No workspace members found".to_string(),
                    recovery_suggestion: None,
                })
            })?;

        let workspace_root = cargo_toml_path.parent().unwrap();
        let mut packages = Vec::new();

        for member in members {
            if let Some(member_path) = member.as_str() {
                let member_dir = workspace_root.join(member_path);
                let member_cargo_toml = member_dir.join("Cargo.toml");

                if member_cargo_toml.exists() {
                    packages.push(self.parse_cargo_package(&member_cargo_toml)?);
                }
            }
        }

        Ok(packages)
    }

    fn parse_dependencies(
        &self,
        cargo_toml: &toml::Value,
        section: &str,
    ) -> Result<Vec<CargoDependency>> {
        let mut dependencies = Vec::new();

        if let Some(deps) = cargo_toml.get(section).and_then(|d| d.as_table()) {
            for (name, spec) in deps {
                let dependency = self.parse_dependency_spec(name, spec)?;
                dependencies.push(dependency);
            }
        }

        Ok(dependencies)
    }

    fn parse_dependency_spec(&self, name: &str, spec: &toml::Value) -> Result<CargoDependency> {
        match spec {
            toml::Value::String(version) => Ok(CargoDependency {
                name: name.to_string(),
                version_req: semver::VersionReq::parse(version)?,
                source: DependencySource::Registry("crates.io".to_string()),
                features: vec![],
                optional: false,
            }),
            toml::Value::Table(table) => {
                let version_req =
                    if let Some(version) = table.get("version").and_then(|v| v.as_str()) {
                        semver::VersionReq::parse(version)?
                    } else {
                        semver::VersionReq::STAR
                    };

                let source = if let Some(git) = table.get("git").and_then(|g| g.as_str()) {
                    let rev = table
                        .get("rev")
                        .and_then(|r| r.as_str())
                        .map(|s| s.to_string());
                    DependencySource::Git {
                        url: git.to_string(),
                        rev,
                    }
                } else if let Some(path) = table.get("path").and_then(|p| p.as_str()) {
                    DependencySource::Path(PathBuf::from(path))
                } else {
                    DependencySource::Registry("crates.io".to_string())
                };

                let features = table
                    .get("features")
                    .and_then(|f| f.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    })
                    .unwrap_or_default();

                let optional = table
                    .get("optional")
                    .and_then(|o| o.as_bool())
                    .unwrap_or(false);

                Ok(CargoDependency {
                    name: name.to_string(),
                    version_req,
                    source,
                    features,
                    optional,
                })
            }
            _ => Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "rust".to_string(),
                error: format!("Invalid dependency specification for {name}"),
                recovery_suggestion: None,
            })),
        }
    }

    fn parse_features(&self, cargo_toml: &toml::Value) -> Result<HashMap<String, Vec<String>>> {
        let mut features = HashMap::new();

        if let Some(features_table) = cargo_toml.get("features").and_then(|f| f.as_table()) {
            for (feature_name, feature_deps) in features_table {
                let deps = if let Some(array) = feature_deps.as_array() {
                    array
                        .iter()
                        .filter_map(|v| v.as_str())
                        .map(|s| s.to_string())
                        .collect()
                } else {
                    vec![]
                };

                features.insert(feature_name.clone(), deps);
            }
        }

        Ok(features)
    }

    fn find_cargo_lock(&self, cargo_toml_path: &Path) -> Option<PathBuf> {
        let cargo_lock = cargo_toml_path.parent()?.join("Cargo.lock");
        if cargo_lock.exists() {
            Some(cargo_lock)
        } else {
            None
        }
    }

    fn determine_target_dir(&self, cargo_toml_path: &Path) -> Result<PathBuf> {
        // Check environment variable first
        if let Ok(target_dir) = std::env::var("CARGO_TARGET_DIR") {
            return Ok(PathBuf::from(target_dir));
        }

        // Check config
        if let Some(ref target_dir) = self.config.cargo_config.target_dir {
            return Ok(target_dir.clone());
        }

        // Default to workspace/target
        Ok(cargo_toml_path.parent().unwrap().join("target"))
    }

    /// Find suitable Rust toolchain for configuration
    pub async fn find_suitable_toolchain(
        &mut self,
        config: &EnvironmentConfig,
    ) -> Result<RustToolchainInfo> {
        let available_toolchains = self.detect_rust_toolchains().await?;

        if available_toolchains.is_empty() {
            return Err(SnpError::from(LanguageError::EnvironmentSetupFailed {
                language: "rust".to_string(),
                error: "No Rust toolchains found".to_string(),
                recovery_suggestion: Some("Install Rust from https://rustup.rs/".to_string()),
            }));
        }

        // Find toolchain based on version requirement
        if let Some(version) = &config.version {
            match version.as_str() {
                "system" => {
                    // Look for system installation
                    if let Some(system_toolchain) = available_toolchains
                        .iter()
                        .find(|t| t.source == RustInstallationSource::System)
                    {
                        return Ok(system_toolchain.clone());
                    }
                }
                "stable" | "beta" | "nightly" => {
                    // Look for rustup toolchain
                    if let Some(toolchain) = available_toolchains.iter()
                        .find(|t| matches!(&t.source, RustInstallationSource::Rustup { channel } if channel.starts_with(version)))
                    {
                        return Ok(toolchain.clone());
                    }
                }
                _ => {
                    // Try to parse as version requirement
                    if let Ok(version_req) = semver::VersionReq::parse(version) {
                        if let Some(toolchain) = available_toolchains
                            .iter()
                            .find(|t| version_req.matches(&t.rust_version))
                        {
                            return Ok(toolchain.clone());
                        }
                    } else {
                        // If version parsing fails, return error for invalid version
                        return Err(SnpError::from(LanguageError::UnsupportedVersion {
                            language: "rust".to_string(),
                            version: version.clone(),
                            supported_versions: vec![
                                "stable".to_string(),
                                "beta".to_string(),
                                "nightly".to_string(),
                                "system".to_string(),
                            ],
                        }));
                    }
                }
            }
        }

        // Fallback to default or first available
        let default_toolchain = available_toolchains
            .iter()
            .find(|t| t.is_default)
            .or_else(|| available_toolchains.first())
            .unwrap();

        Ok(default_toolchain.clone())
    }

    /// Setup Rust environment for hook execution
    pub async fn setup_rust_environment(
        &mut self,
        toolchain: &RustToolchainInfo,
        workspace: &CargoWorkspace,
        dependencies: &[String],
    ) -> Result<RustEnvironment> {
        // Install additional dependencies if specified
        if !dependencies.is_empty() {
            self.install_rust_dependencies(toolchain, workspace, dependencies)
                .await?;
        }

        // Ensure required components are available
        self.ensure_rust_components(toolchain, &["clippy", "rustfmt"])
            .await?;

        Ok(RustEnvironment {
            toolchain: toolchain.clone(),
            workspace: workspace.clone(),
            target_dir: self.get_target_directory(workspace)?,
            cargo_config: self.load_cargo_config(workspace)?,
        })
    }

    async fn install_rust_dependencies(
        &self,
        toolchain: &RustToolchainInfo,
        workspace: &CargoWorkspace,
        dependencies: &[String],
    ) -> Result<()> {
        for dep in dependencies {
            if dep.starts_with("cli:") {
                // Install as binary crate
                let dep_name = dep.strip_prefix("cli:").unwrap();
                let output = TokioCommand::new(&toolchain.cargo_path)
                    .args(["install", dep_name])
                    .current_dir(&workspace.root_path)
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(SnpError::from(
                        LanguageError::DependencyInstallationFailed {
                            dependency: dep.clone(),
                            language: "rust".to_string(),
                            error: String::from_utf8_lossy(&output.stderr).to_string(),
                            installation_log: Some(
                                String::from_utf8_lossy(&output.stdout).to_string(),
                            ),
                        },
                    ));
                }
            } else {
                // Add as regular dependency
                let output = TokioCommand::new(&toolchain.cargo_path)
                    .args(["add", dep])
                    .current_dir(&workspace.root_path)
                    .output()
                    .await?;

                if !output.status.success() {
                    return Err(SnpError::from(
                        LanguageError::DependencyInstallationFailed {
                            dependency: dep.clone(),
                            language: "rust".to_string(),
                            error: String::from_utf8_lossy(&output.stderr).to_string(),
                            installation_log: Some(
                                String::from_utf8_lossy(&output.stdout).to_string(),
                            ),
                        },
                    ));
                }
            }
        }
        Ok(())
    }

    async fn ensure_rust_components(
        &self,
        toolchain: &RustToolchainInfo,
        components: &[&str],
    ) -> Result<()> {
        if let RustInstallationSource::Rustup { .. } = &toolchain.source {
            let rustup_path = which::which("rustup")?;

            for component in components {
                // Check if component is already installed
                if toolchain
                    .components
                    .iter()
                    .any(|c| c.name == *component && c.installed)
                {
                    continue;
                }

                // Install component
                let output = TokioCommand::new(&rustup_path)
                    .args([
                        "component",
                        "add",
                        component,
                        "--toolchain",
                        &toolchain.name,
                    ])
                    .output()
                    .await?;

                if !output.status.success() {
                    // Don't fail if component installation fails, just warn
                    eprintln!(
                        "Warning: Failed to install component {}: {}",
                        component,
                        String::from_utf8_lossy(&output.stderr)
                    );
                }
            }
        }
        Ok(())
    }

    fn get_target_directory(&self, workspace: &CargoWorkspace) -> Result<PathBuf> {
        if let Some(ref target_dir) = self.config.cargo_config.target_dir {
            Ok(target_dir.clone())
        } else {
            Ok(workspace.target_dir.clone())
        }
    }

    fn load_cargo_config(&self, workspace: &CargoWorkspace) -> Result<CargoConfigFile> {
        let config_path = workspace.root_path.join(".cargo").join("config.toml");

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)?;
            let parsed: HashMap<String, toml::Value> = toml::from_str(&content)?;

            Ok(CargoConfigFile {
                content: parsed,
                path: Some(config_path),
            })
        } else {
            Ok(CargoConfigFile {
                content: HashMap::new(),
                path: None,
            })
        }
    }
}

impl Default for RustLanguageConfig {
    fn default() -> Self {
        Self {
            default_toolchain: None,
            prefer_system_rust: false,
            enable_incremental_compilation: true,
            shared_target_dir: None,
            cargo_config: CargoConfig::default(),
            rustup_config: RustupConfig::default(),
        }
    }
}

impl Default for CargoConfig {
    fn default() -> Self {
        Self {
            offline_mode: false,
            parallel_jobs: None,
            target_dir: None,
            registry_url: None,
            build_profile: BuildProfile::Debug,
        }
    }
}

impl Default for RustupConfig {
    fn default() -> Self {
        Self {
            auto_install_toolchains: true,
            auto_install_components: true,
            preferred_toolchain: None,
        }
    }
}

#[async_trait]
impl Language for RustLanguagePlugin {
    fn language_name(&self) -> &str {
        "rust"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["rs", "toml"] // Cargo.toml is also relevant
    }

    fn detection_patterns(&self) -> &[&str] {
        &[
            "Cargo.toml",
            "Cargo.lock",
            r"^#!/usr/bin/env cargo",
            r"^#!/usr/bin/rustc",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        // Find suitable Rust toolchain
        let mut plugin = self.clone(); // Clone to avoid borrow issues
        let toolchain = plugin.find_suitable_toolchain(config).await?;

        // Detect Cargo workspace
        let workspace_root = config
            .working_directory
            .as_ref()
            .unwrap_or(&std::env::current_dir()?)
            .clone();
        let workspace = plugin.detect_cargo_workspace(&workspace_root).await?;

        // Setup Rust environment
        let _rust_env = plugin
            .setup_rust_environment(&toolchain, &workspace, &config.additional_dependencies)
            .await?;

        // Build environment variables
        let mut environment_variables = HashMap::new();

        // Set CARGO_HOME
        if let Some(cargo_home) = dirs::home_dir().map(|h| h.join(".cargo")) {
            environment_variables
                .insert("CARGO_HOME".to_string(), cargo_home.display().to_string());
        }

        // Set RUSTUP_HOME
        if let Some(rustup_home) = dirs::home_dir().map(|h| h.join(".rustup")) {
            environment_variables
                .insert("RUSTUP_HOME".to_string(), rustup_home.display().to_string());
        }

        // Set RUSTUP_TOOLCHAIN if not system
        if let Some(version) = &config.version {
            if version != "system" {
                let toolchain_name = match version.as_str() {
                    "stable" | "beta" | "nightly" => version.clone(),
                    _ => version.clone(),
                };
                environment_variables.insert("RUSTUP_TOOLCHAIN".to_string(), toolchain_name);
            }
        }

        // Set CARGO_TARGET_DIR if configured
        if let Some(ref target_dir) = self.config.cargo_config.target_dir {
            environment_variables.insert(
                "CARGO_TARGET_DIR".to_string(),
                target_dir.display().to_string(),
            );
        }

        // Add config environment variables
        for (key, value) in &config.environment_variables {
            environment_variables.insert(key.clone(), value.clone());
        }

        // Add toolchain bin to PATH
        let mut path_dirs = vec![];
        if let Some(ref toolchain_path) = toolchain.toolchain_path {
            path_dirs.push(toolchain_path.join("bin"));
        }

        // Add existing PATH
        if let Ok(existing_path) = std::env::var("PATH") {
            let separator = if cfg!(windows) { ";" } else { ":" };
            for path in existing_path.split(separator) {
                path_dirs.push(PathBuf::from(path));
            }
        }

        let new_path = path_dirs
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(if cfg!(windows) { ";" } else { ":" });
        environment_variables.insert("PATH".to_string(), new_path);

        Ok(LanguageEnvironment {
            language: "rust".to_string(),
            environment_id: format!("rust-{}-env", uuid::Uuid::new_v4()),
            root_path: workspace.root_path.clone(),
            executable_path: toolchain.cargo_path.clone(),
            environment_variables,
            installed_dependencies: vec![], // Will be populated during dependency installation
            metadata: super::environment::EnvironmentMetadata {
                created_at: std::time::SystemTime::now(),
                last_used: std::time::SystemTime::now(),
                usage_count: 0,
                size_bytes: 0,
                dependency_count: 0,
                language_version: toolchain.rust_version.to_string(),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        })
    }

    async fn install_dependencies(
        &self,
        env: &LanguageEnvironment,
        dependencies: &[Dependency],
    ) -> Result<()> {
        // Convert dependencies to string format and install
        let dep_specs: Vec<String> = dependencies
            .iter()
            .map(|d| format!("{}:{}", d.name, d.version_spec))
            .collect();

        let mut plugin = self.clone();
        let toolchain = plugin
            .detect_rust_toolchains()
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                SnpError::from(LanguageError::EnvironmentSetupFailed {
                    language: "rust".to_string(),
                    error: "No Rust toolchain available".to_string(),
                    recovery_suggestion: Some("Install Rust".to_string()),
                })
            })?;

        let workspace = plugin.detect_cargo_workspace(&env.root_path).await?;

        plugin
            .install_rust_dependencies(&toolchain, &workspace, &dep_specs)
            .await
    }

    async fn cleanup_environment(&self, _env: &LanguageEnvironment) -> Result<()> {
        // Rust environments are mostly stateless, minimal cleanup needed
        Ok(())
    }

    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        let command = self.build_command(hook, env, files)?;
        let executor = RustHookExecutor::new(&self.config);
        let mut result = executor.execute(command).await?;

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
        let mut command = Command::new(&hook.entry);

        // Add hook arguments
        for arg in &hook.args {
            command.arg(arg);
        }

        // Add file arguments if enabled
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
        command.current_dir(&env.root_path);

        // Set timeout
        command.timeout(Duration::from_secs(300)); // 5 minutes default

        Ok(command)
    }

    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>> {
        let mut resolved = Vec::new();

        for dep_str in dependencies {
            let dependency = self.parse_dependency(dep_str)?;
            resolved.push(dependency);
        }

        Ok(resolved)
    }

    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency> {
        // Parse specs like "serde:1.0.0", "cli:cargo-audit", "anyhow"
        if dep_spec.starts_with("cli:") {
            let name = dep_spec.strip_prefix("cli:").unwrap();
            return Ok(Dependency::new(name));
        }

        let parts: Vec<&str> = dep_spec.split(':').collect();
        let name = parts[0];

        let mut dependency = Dependency::new(name);

        if parts.len() > 1 {
            dependency = dependency.with_version(parts[1]);
        }

        // Parse features if present (serde:1.0.0:derive,std)
        if parts.len() > 2 {
            let features: Vec<String> = parts[2].split(',').map(|s| s.to_string()).collect();
            dependency = dependency.with_features(features);
        }

        Ok(dependency)
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
            is_healthy: true, // TODO: Implement health check
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

/// Rust hook executor with Cargo integration
pub struct RustHookExecutor {
    config: RustLanguageConfig,
}

impl RustHookExecutor {
    pub fn new(config: &RustLanguageConfig) -> Self {
        Self {
            config: config.clone(),
        }
    }

    pub async fn execute(&self, command: Command) -> Result<HookExecutionResult> {
        let start_time = Instant::now();

        // Setup Rust-specific environment variables
        let mut env_vars = command.environment.clone();
        self.setup_rust_environment(&mut env_vars, &command.working_directory);

        let mut tokio_cmd = TokioCommand::new(&command.executable);
        tokio_cmd.args(&command.arguments);
        tokio_cmd.envs(&env_vars);

        if let Some(ref cwd) = command.working_directory {
            tokio_cmd.current_dir(cwd);
        }

        tokio_cmd.stdout(Stdio::piped());
        tokio_cmd.stderr(Stdio::piped());

        let output = tokio_cmd.output().await?;
        let duration = start_time.elapsed();

        Ok(HookExecutionResult {
            hook_id: "rust".to_string(), // Will be set by caller
            success: output.status.success(),
            skipped: false,
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
                    hook_id: "rust".to_string(),
                    exit_code: output.status.code().unwrap_or(-1),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                })
            },
        })
    }

    fn setup_rust_environment(
        &self,
        env_vars: &mut HashMap<String, String>,
        _working_dir: &Option<PathBuf>,
    ) {
        // Set CARGO_TARGET_DIR if configured
        if let Some(ref target_dir) = self.config.cargo_config.target_dir {
            env_vars.insert(
                "CARGO_TARGET_DIR".to_string(),
                target_dir.display().to_string(),
            );
        }

        // Set CARGO_HOME if not already set
        if !env_vars.contains_key("CARGO_HOME") {
            if let Some(cargo_home) = dirs::home_dir().map(|h| h.join(".cargo")) {
                env_vars.insert("CARGO_HOME".to_string(), cargo_home.display().to_string());
            }
        }

        // Set RUSTUP_HOME if not already set
        if !env_vars.contains_key("RUSTUP_HOME") {
            if let Some(rustup_home) = dirs::home_dir().map(|h| h.join(".rustup")) {
                env_vars.insert("RUSTUP_HOME".to_string(), rustup_home.display().to_string());
            }
        }

        // Set incremental compilation if enabled
        if self.config.enable_incremental_compilation {
            env_vars.insert("CARGO_INCREMENTAL".to_string(), "1".to_string());
        }
    }
}

// Make RustLanguagePlugin cloneable for use in async contexts
impl Clone for RustLanguagePlugin {
    fn clone(&self) -> Self {
        Self {
            config: self.config.clone(),
            toolchain_cache: self.toolchain_cache.clone(),
            workspace_cache: self.workspace_cache.clone(),
            dependency_manager: RustDependencyManager {
                config: self.dependency_manager.config.clone(),
            },
        }
    }
}

// Implement dependency manager for Rust
#[async_trait]
impl DependencyManager for RustDependencyManager {
    async fn install(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<super::dependency::InstallationResult> {
        let start_time = Instant::now();
        let mut installed = Vec::new();
        let mut failed = Vec::new();

        for dependency in dependencies {
            // Try to install the dependency using cargo
            let dep_spec = format!("{}:{}", dependency.name, dependency.version_spec);

            let result = TokioCommand::new("cargo")
                .args(["add", &dep_spec])
                .current_dir(&env.root_path)
                .output()
                .await;

            match result {
                Ok(output) if output.status.success() => {
                    installed.push(super::dependency::InstalledPackage {
                        name: dependency.name.clone(),
                        version: dependency.version_spec.to_string(),
                        source: super::dependency::DependencySource::registry(),
                        install_path: env.root_path.clone(),
                        metadata: HashMap::new(),
                    });
                }
                _ => {
                    failed.push(dependency.clone());
                }
            }
        }

        Ok(super::dependency::InstallationResult {
            installed,
            failed: failed
                .into_iter()
                .map(|d| (d, "Installation failed".to_string()))
                .collect(),
            skipped: vec![],
            duration: start_time.elapsed(),
        })
    }

    async fn resolve(
        &self,
        dependencies: &[Dependency],
    ) -> Result<Vec<super::dependency::ResolvedDependency>> {
        // For Rust, dependency resolution is handled by Cargo
        let resolved: Vec<super::dependency::ResolvedDependency> = dependencies
            .iter()
            .map(|dep| super::dependency::ResolvedDependency {
                dependency: dep.clone(),
                resolved_version: dep.version_spec.to_string(),
                dependencies: vec![], // Cargo handles transitive dependencies
                conflicts: vec![],
            })
            .collect();

        Ok(resolved)
    }

    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<Vec<bool>> {
        // Check if dependencies are in Cargo.toml
        let cargo_toml_path = env.root_path.join("Cargo.toml");

        if !cargo_toml_path.exists() {
            return Ok(vec![false; dependencies.len()]);
        }

        let cargo_toml_content = std::fs::read_to_string(&cargo_toml_path)?;
        let cargo_toml: toml::Value = toml::from_str(&cargo_toml_content)?;

        let mut results = Vec::new();

        for dependency in dependencies {
            let installed = cargo_toml
                .get("dependencies")
                .and_then(|deps| deps.get(&dependency.name))
                .is_some();
            results.push(installed);
        }

        Ok(results)
    }

    async fn update(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<super::dependency::UpdateResult> {
        let start_time = Instant::now();
        let mut updated = Vec::new();
        let mut failed = Vec::new();

        // Update dependencies using cargo update
        for dependency in dependencies {
            let result = TokioCommand::new("cargo")
                .args(["update", &dependency.name])
                .current_dir(&env.root_path)
                .output()
                .await;

            match result {
                Ok(output) if output.status.success() => {
                    updated.push(super::dependency::InstalledPackage {
                        name: dependency.name.clone(),
                        version: dependency.version_spec.to_string(),
                        source: super::dependency::DependencySource::registry(),
                        install_path: env.root_path.clone(),
                        metadata: HashMap::new(),
                    });
                }
                _ => {
                    failed.push(dependency.clone());
                }
            }
        }

        Ok(super::dependency::UpdateResult {
            updated: updated
                .into_iter()
                .map(|pkg| (pkg, "unknown".to_string()))
                .collect(),
            failed: failed
                .into_iter()
                .map(|d| (d, "Update failed".to_string()))
                .collect(),
            no_update_available: vec![],
            duration: start_time.elapsed(),
        })
    }

    async fn uninstall(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<()> {
        // Remove dependencies using cargo remove
        for dependency in dependencies {
            let _result = TokioCommand::new("cargo")
                .args(["remove", &dependency.name])
                .current_dir(&env.root_path)
                .output()
                .await;
            // Ignore errors for uninstall
        }

        Ok(())
    }

    async fn list_installed(
        &self,
        env: &LanguageEnvironment,
    ) -> Result<Vec<super::dependency::InstalledPackage>> {
        let cargo_toml_path = env.root_path.join("Cargo.toml");

        if !cargo_toml_path.exists() {
            return Ok(vec![]);
        }

        let cargo_toml_content = std::fs::read_to_string(&cargo_toml_path)?;
        let cargo_toml: toml::Value = toml::from_str(&cargo_toml_content)?;

        let mut installed = Vec::new();

        if let Some(dependencies) = cargo_toml.get("dependencies").and_then(|d| d.as_table()) {
            for (name, version_spec) in dependencies {
                let version = match version_spec {
                    toml::Value::String(v) => v.clone(),
                    toml::Value::Table(t) => t
                        .get("version")
                        .and_then(|v| v.as_str())
                        .unwrap_or("*")
                        .to_string(),
                    _ => "*".to_string(),
                };

                installed.push(super::dependency::InstalledPackage {
                    name: name.clone(),
                    version,
                    source: super::dependency::DependencySource::registry(),
                    install_path: env.root_path.clone(),
                    metadata: HashMap::new(),
                });
            }
        }

        Ok(installed)
    }
}
