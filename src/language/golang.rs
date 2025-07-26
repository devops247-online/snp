use crate::core::Hook;
use crate::error::{Result, SnpError};
use crate::execution::HookExecutionResult;

use super::dependency::{
    Dependency, DependencyManager, DependencyManagerConfig, InstallationResult, InstalledPackage,
    ResolvedDependency, UpdateResult,
};
use super::environment::{
    EnvironmentConfig, EnvironmentInfo, EnvironmentMetadata, LanguageEnvironment, ValidationReport,
};
use super::traits::{Command, Language, LanguageConfig};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use thiserror::Error;
use tokio::process::Command as TokioCommand;

/// Type alias for Go module parsing result to reduce complexity
type GoModuleParseResult = (
    Option<String>,
    Option<semver::Version>,
    Vec<GoDependency>,
    Vec<ReplaceDirective>,
);

/// Go language plugin for Go-based hooks and tools
#[derive(Debug, Clone)]
pub struct GoLanguagePlugin {
    pub config: GoLanguageConfig,
    pub toolchain_cache: HashMap<String, GoToolchainInfo>,
    pub module_cache: HashMap<PathBuf, GoModule>,
    dependency_manager: GoDependencyManager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoLanguageConfig {
    pub default_go_version: Option<String>,
    pub prefer_system_go: bool,
    pub enable_module_mode: bool,
    pub goproxy_url: Option<String>,
    pub gosumdb: Option<String>,
    pub build_config: GoBuildConfig,
    pub cache_config: GoCacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoBuildConfig {
    pub build_cache: bool,
    pub cgo_enabled: bool,
    pub race_detector: bool,
    pub optimization_level: OptimizationLevel,
    pub build_tags: Vec<String>,
    pub ldflags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OptimizationLevel {
    None,
    Default,
    Size,
    Speed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoCacheConfig {
    pub module_cache_dir: Option<PathBuf>,
    pub build_cache_dir: Option<PathBuf>,
    pub max_cache_size: Option<u64>,
    pub cache_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct GoToolchainInfo {
    pub go_executable: PathBuf,
    pub version: semver::Version,
    pub goroot: PathBuf,
    pub gopath: Option<PathBuf>,
    pub goos: String,
    pub goarch: String,
    pub tools: Vec<GoTool>,
    pub source: GoInstallationSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GoInstallationSource {
    System,
    GoVersionManager { name: String, version: String },
    Manual(PathBuf),
}

#[derive(Debug, Clone)]
pub struct GoTool {
    pub name: String,
    pub available: bool,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct GoModule {
    pub root_path: PathBuf,
    pub module_mode: ModuleMode,
    pub go_mod_path: Option<PathBuf>,
    pub go_sum_path: Option<PathBuf>,
    pub go_work_path: Option<PathBuf>,
    pub module_name: Option<String>,
    pub go_version: Option<semver::Version>,
    pub dependencies: Vec<GoDependency>,
    pub replace_directives: Vec<ReplaceDirective>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ModuleMode {
    Module,    // go.mod present
    Workspace, // go.work present
    Gopath,    // Legacy GOPATH mode
    Auto,      // Auto-detect based on environment
}

#[derive(Debug, Clone)]
pub struct GoDependency {
    pub module_path: String,
    pub version: String,
    pub indirect: bool,
    pub replace: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReplaceDirective {
    pub old_path: String,
    pub old_version: Option<String>,
    pub new_path: String,
    pub new_version: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GoEnvironment {
    pub toolchain: GoToolchainInfo,
    pub module: GoModule,
    pub gopath: PathBuf,
    pub gocache: PathBuf,
    pub environment_vars: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct GoDependencyManager {
    #[allow(dead_code)]
    config: DependencyManagerConfig,
}

#[derive(Debug, Error)]
pub enum GoError {
    #[error("Go installation not found")]
    GoNotFound {
        searched_paths: Vec<PathBuf>,
        min_version: Option<semver::Version>,
    },

    #[error("go.mod not found or invalid: {path}")]
    InvalidGoMod { path: PathBuf, error: String },

    #[error("Dependency installation failed: {package}")]
    DependencyInstallationFailed { package: String, error: String },

    #[error("Tool installation failed: {tool}")]
    ToolInstallationFailed { tool: String, error: String },

    #[error("Go version {found} does not meet requirement {required}")]
    VersionMismatch {
        found: semver::Version,
        required: semver::VersionReq,
    },

    #[error("Version parsing failed from: {output}")]
    VersionParsingFailed { output: String },

    #[error("Environment parsing failed from: {output}")]
    EnvironmentParsingFailed { output: String },
}

impl Default for GoLanguageConfig {
    fn default() -> Self {
        Self {
            default_go_version: None,
            prefer_system_go: true,
            enable_module_mode: true,
            goproxy_url: Some("https://proxy.golang.org".to_string()),
            gosumdb: Some("sum.golang.org".to_string()),
            build_config: GoBuildConfig::default(),
            cache_config: GoCacheConfig::default(),
        }
    }
}

impl Default for GoBuildConfig {
    fn default() -> Self {
        Self {
            build_cache: true,
            cgo_enabled: true,
            race_detector: false,
            optimization_level: OptimizationLevel::Default,
            build_tags: Vec::new(),
            ldflags: Vec::new(),
        }
    }
}

impl Default for GoCacheConfig {
    fn default() -> Self {
        Self {
            module_cache_dir: None,
            build_cache_dir: None,
            max_cache_size: Some(1024 * 1024 * 1024), // 1GB
            cache_ttl: Duration::from_secs(3600),     // 1 hour
        }
    }
}

impl GoLanguagePlugin {
    pub fn new() -> Self {
        Self {
            config: GoLanguageConfig::default(),
            toolchain_cache: HashMap::new(),
            module_cache: HashMap::new(),
            dependency_manager: GoDependencyManager::new(),
        }
    }

    /// Detect available Go installations
    pub async fn detect_go_installations(&mut self) -> Result<Vec<GoToolchainInfo>> {
        let mut installations = Vec::new();

        // Check for system Go installation
        if let Ok(go_path) = which::which("go") {
            if let Ok(info) = self.analyze_go_installation(&go_path).await {
                installations.push(info);
            }
        }

        // Check for Go version manager installations (g, gvm, etc.)
        installations.extend(self.detect_go_version_managers().await?);

        // Sort by version (newest first)
        installations.sort_by(|a, b| b.version.cmp(&a.version));
        Ok(installations)
    }

    /// Detect Go module configuration
    pub async fn detect_go_module(&mut self, project_path: &Path) -> Result<GoModule> {
        // For now, disable caching to avoid test issues where files change
        // TODO: Implement proper cache invalidation based on file modification times
        let module = self.analyze_go_module(project_path).await?;

        // Cache the result
        self.module_cache
            .insert(project_path.to_path_buf(), module.clone());
        Ok(module)
    }

    /// Setup Go environment for hook execution
    pub async fn setup_go_environment(
        &mut self,
        toolchain: &GoToolchainInfo,
        module: &GoModule,
        dependencies: &[String],
    ) -> Result<GoEnvironment> {
        // Install additional dependencies if specified
        if !dependencies.is_empty() {
            self.install_go_dependencies(toolchain, module, dependencies)
                .await?;
        }

        // Ensure required tools are available
        self.ensure_go_tools(toolchain, &["gofmt", "go", "vet"])
            .await?;

        Ok(GoEnvironment {
            toolchain: toolchain.clone(),
            module: module.clone(),
            gopath: self.determine_gopath(toolchain, module)?,
            gocache: self.determine_gocache(toolchain)?,
            environment_vars: self.build_go_environment_vars(toolchain, module)?,
        })
    }

    pub async fn analyze_go_installation(&self, go_path: &Path) -> Result<GoToolchainInfo> {
        // Get Go version
        let version_output = TokioCommand::new(go_path)
            .args(["version"])
            .output()
            .await?;

        let version_str = String::from_utf8_lossy(&version_output.stdout);
        let version = self.parse_go_version(&version_str)?;

        // Get Go environment info
        let env_output = TokioCommand::new(go_path)
            .args(["env", "GOROOT", "GOPATH", "GOOS", "GOARCH"])
            .output()
            .await?;

        let env_str = String::from_utf8_lossy(&env_output.stdout);
        let (goroot, gopath, goos, goarch) = self.parse_go_env(&env_str)?;

        // Detect available tools
        let tools = self.detect_go_tools(go_path).await?;

        Ok(GoToolchainInfo {
            go_executable: go_path.to_path_buf(),
            version,
            goroot,
            gopath,
            goos,
            goarch,
            tools,
            source: GoInstallationSource::System,
        })
    }

    fn parse_go_version(&self, version_str: &str) -> Result<semver::Version> {
        // Parse version from output like "go version go1.20.5 linux/amd64"
        let version_regex = regex::Regex::new(r"go version go(\d+\.\d+(?:\.\d+)?)")?;

        if let Some(captures) = version_regex.captures(version_str) {
            if let Some(version_match) = captures.get(1) {
                return Ok(semver::Version::parse(version_match.as_str())?);
            }
        }

        Err(SnpError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse Go version from: {version_str}"),
        )))
    }

    fn parse_go_env(&self, env_str: &str) -> Result<(PathBuf, Option<PathBuf>, String, String)> {
        let lines: Vec<&str> = env_str.lines().collect();

        if lines.len() < 4 {
            return Err(SnpError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to parse Go environment from: {env_str}"),
            )));
        }

        let goroot = PathBuf::from(lines[0]);
        let gopath = if lines[1].is_empty() {
            None
        } else {
            Some(PathBuf::from(lines[1]))
        };
        let goos = lines[2].to_string();
        let goarch = lines[3].to_string();

        Ok((goroot, gopath, goos, goarch))
    }

    async fn detect_go_tools(&self, go_path: &Path) -> Result<Vec<GoTool>> {
        let mut tools = Vec::new();

        // Common Go tools to check for
        let tool_names = [
            "gofmt", "go", "vet", "build", "test", "install", "get", "mod", "generate", "clean",
            "env", "version", "help",
        ];

        for tool_name in &tool_names {
            let available = TokioCommand::new(go_path)
                .args(["help", tool_name])
                .output()
                .await
                .map(|output| output.status.success())
                .unwrap_or(false);

            tools.push(GoTool {
                name: tool_name.to_string(),
                available,
                path: if available {
                    Some(go_path.to_path_buf())
                } else {
                    None
                },
            });
        }

        Ok(tools)
    }

    async fn detect_go_version_managers(&self) -> Result<Vec<GoToolchainInfo>> {
        // For now, we'll implement basic detection
        // This can be expanded to detect g, gvm, etc.
        Ok(vec![])
    }

    async fn analyze_go_module(&self, project_path: &Path) -> Result<GoModule> {
        let go_mod_path = self.find_go_mod(project_path)?;
        let go_sum_path = self.find_go_sum(project_path);
        let go_work_path = self.find_go_work(project_path);

        let module_mode = if go_work_path.is_some() {
            ModuleMode::Workspace
        } else if go_mod_path.is_some() {
            ModuleMode::Module
        } else {
            ModuleMode::Gopath
        };

        let (module_name, go_version, dependencies, replace_directives) =
            if let Some(ref mod_path) = go_mod_path {
                self.parse_go_mod(mod_path)?
            } else {
                (None, None, Vec::new(), Vec::new())
            };

        Ok(GoModule {
            root_path: project_path.to_path_buf(),
            module_mode,
            go_mod_path,
            go_sum_path,
            go_work_path,
            module_name,
            go_version,
            dependencies,
            replace_directives,
        })
    }

    fn find_go_mod(&self, path: &Path) -> Result<Option<PathBuf>> {
        let go_mod = path.join("go.mod");
        if go_mod.exists() {
            Ok(Some(go_mod))
        } else {
            Ok(None)
        }
    }

    fn find_go_sum(&self, path: &Path) -> Option<PathBuf> {
        let go_sum = path.join("go.sum");
        if go_sum.exists() {
            Some(go_sum)
        } else {
            None
        }
    }

    fn find_go_work(&self, path: &Path) -> Option<PathBuf> {
        let go_work = path.join("go.work");
        if go_work.exists() {
            Some(go_work)
        } else {
            None
        }
    }

    fn parse_go_mod(&self, go_mod_path: &Path) -> Result<GoModuleParseResult> {
        let content = std::fs::read_to_string(go_mod_path)?;

        let mut module_name = None;
        let mut go_version = None;
        let mut dependencies = Vec::new();
        let mut replace_directives = Vec::new();
        let mut in_require_block = false;

        for line in content.lines() {
            let line = line.trim();

            if line.starts_with("module ") {
                module_name = Some(line.strip_prefix("module ").unwrap().trim().to_string());
            } else if line.starts_with("go ") {
                let version_str = line.strip_prefix("go ").unwrap().trim();
                go_version = semver::Version::parse(&format!("{version_str}.0")).ok();
            } else if line.starts_with("require (") {
                in_require_block = true;
            } else if line.starts_with("require ") {
                if let Some(dep) = self.parse_require_line(line)? {
                    dependencies.push(dep);
                }
            } else if in_require_block {
                if line == ")" {
                    in_require_block = false;
                } else if !line.is_empty() {
                    // Parse dependency line within require block
                    if let Some(dep) = self.parse_dependency_line(line)? {
                        dependencies.push(dep);
                    }
                }
            } else if line.starts_with("replace ") {
                if let Some(replace) = self.parse_replace_line(line)? {
                    replace_directives.push(replace);
                }
            }
        }

        Ok((module_name, go_version, dependencies, replace_directives))
    }

    fn parse_require_line(&self, line: &str) -> Result<Option<GoDependency>> {
        // Parse lines like "require github.com/example/lib v1.0.0 // indirect"
        let line = line.strip_prefix("require ").unwrap().trim();

        // Handle multi-line requires
        if line == "(" || line == ")" {
            return Ok(None);
        }

        self.parse_dependency_line(line)
    }

    fn parse_dependency_line(&self, line: &str) -> Result<Option<GoDependency>> {
        // Parse dependency lines like "github.com/example/lib v1.0.0 // indirect"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 {
            let module_path = parts[0].to_string();
            let version = parts[1].to_string();
            let indirect = line.contains("// indirect");

            Ok(Some(GoDependency {
                module_path,
                version,
                indirect,
                replace: None,
            }))
        } else {
            Ok(None)
        }
    }

    fn parse_replace_line(&self, line: &str) -> Result<Option<ReplaceDirective>> {
        // Parse lines like "replace github.com/old/lib => github.com/new/lib v1.0.0"
        let line = line.strip_prefix("replace ").unwrap().trim();

        if let Some((old_part, new_part)) = line.split_once(" => ") {
            let old_parts: Vec<&str> = old_part.split_whitespace().collect();
            let new_parts: Vec<&str> = new_part.split_whitespace().collect();

            let old_path = old_parts[0].to_string();
            let old_version = if old_parts.len() > 1 {
                Some(old_parts[1].to_string())
            } else {
                None
            };

            let new_path = new_parts[0].to_string();
            let new_version = if new_parts.len() > 1 {
                Some(new_parts[1].to_string())
            } else {
                None
            };

            Ok(Some(ReplaceDirective {
                old_path,
                old_version,
                new_path,
                new_version,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn install_go_dependencies(
        &self,
        toolchain: &GoToolchainInfo,
        module: &GoModule,
        dependencies: &[String],
    ) -> Result<()> {
        for dep in dependencies {
            let output = TokioCommand::new(&toolchain.go_executable)
                .args(["install", dep])
                .current_dir(&module.root_path)
                .output()
                .await?;

            if !output.status.success() {
                return Err(SnpError::Io(std::io::Error::other(format!(
                    "Go dependency installation failed for {}: {}",
                    dep,
                    String::from_utf8_lossy(&output.stderr)
                ))));
            }
        }
        Ok(())
    }

    async fn ensure_go_tools(&self, _toolchain: &GoToolchainInfo, _tools: &[&str]) -> Result<()> {
        // For now, assume tools are available
        // This could be expanded to actually check and install tools
        Ok(())
    }

    fn determine_gopath(&self, toolchain: &GoToolchainInfo, _module: &GoModule) -> Result<PathBuf> {
        if let Some(ref gopath) = toolchain.gopath {
            Ok(gopath.clone())
        } else {
            // Default GOPATH
            if let Some(home) = dirs::home_dir() {
                Ok(home.join("go"))
            } else {
                Ok(PathBuf::from("/tmp/go"))
            }
        }
    }

    fn determine_gocache(&self, _toolchain: &GoToolchainInfo) -> Result<PathBuf> {
        if let Some(ref cache_dir) = self.config.cache_config.build_cache_dir {
            Ok(cache_dir.clone())
        } else if let Some(cache_dir) = dirs::cache_dir() {
            Ok(cache_dir.join("go-build"))
        } else {
            Ok(PathBuf::from("/tmp/go-cache"))
        }
    }

    fn build_go_environment_vars(
        &self,
        toolchain: &GoToolchainInfo,
        _module: &GoModule,
    ) -> Result<HashMap<String, String>> {
        let mut env_vars = HashMap::new();

        // Set GOROOT
        env_vars.insert(
            "GOROOT".to_string(),
            toolchain.goroot.to_string_lossy().to_string(),
        );

        // Set GOPATH
        if let Some(ref gopath) = toolchain.gopath {
            env_vars.insert("GOPATH".to_string(), gopath.to_string_lossy().to_string());
        }

        // Set GOPROXY if configured
        if let Some(ref goproxy) = self.config.goproxy_url {
            env_vars.insert("GOPROXY".to_string(), goproxy.clone());
        }

        // Set GOSUMDB if configured
        if let Some(ref gosumdb) = self.config.gosumdb {
            env_vars.insert("GOSUMDB".to_string(), gosumdb.clone());
        }

        // Enable module mode if configured
        if self.config.enable_module_mode {
            env_vars.insert("GO111MODULE".to_string(), "on".to_string());
        }

        // Set CGO_ENABLED based on config
        env_vars.insert(
            "CGO_ENABLED".to_string(),
            if self.config.build_config.cgo_enabled {
                "1"
            } else {
                "0"
            }
            .to_string(),
        );

        // Set GOTOOLCHAIN to local to prevent automatic switching
        env_vars.insert("GOTOOLCHAIN".to_string(), "local".to_string());

        Ok(env_vars)
    }
}

impl Default for GoDependencyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GoDependencyManager {
    pub fn new() -> Self {
        Self {
            config: DependencyManagerConfig::default(),
        }
    }
}

#[async_trait]
impl DependencyManager for GoDependencyManager {
    async fn install(
        &self,
        _dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<InstallationResult> {
        // Basic implementation - could be expanded
        Ok(InstallationResult {
            installed: vec![],
            failed: vec![],
            skipped: vec![],
            duration: Duration::from_millis(0),
        })
    }

    async fn resolve(&self, _dependencies: &[Dependency]) -> Result<Vec<ResolvedDependency>> {
        Ok(vec![])
    }

    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<Vec<bool>> {
        Ok(vec![true; dependencies.len()])
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

#[async_trait]
impl Language for GoLanguagePlugin {
    fn language_name(&self) -> &str {
        "golang"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["go", "mod", "sum", "work"]
    }

    fn detection_patterns(&self) -> &[&str] {
        &[
            "go.mod",
            "go.sum",
            "go.work",
            r"^#!/usr/bin/env go",
            r"^package\s+\w+",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        // Find suitable Go installation
        let mut plugin_clone = self.clone();
        let installations = plugin_clone.detect_go_installations().await?;

        if installations.is_empty() {
            return Err(SnpError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No Go installation found",
            )));
        }

        let toolchain = &installations[0];

        // Detect Go module
        let current_dir = std::env::current_dir()?;
        let working_dir = config.working_directory.as_ref().unwrap_or(&current_dir);
        let module = plugin_clone.detect_go_module(working_dir).await?;

        // Setup Go environment
        let go_env = plugin_clone
            .setup_go_environment(toolchain, &module, &config.additional_dependencies)
            .await?;

        Ok(LanguageEnvironment {
            language: "golang".to_string(),
            environment_id: format!("golang-{}", toolchain.version),
            root_path: go_env.module.root_path.clone(),
            executable_path: go_env.toolchain.go_executable.clone(),
            environment_variables: go_env.environment_vars.clone(),
            installed_dependencies: config
                .additional_dependencies
                .iter()
                .map(Dependency::new)
                .collect(),
            metadata: EnvironmentMetadata {
                created_at: SystemTime::now(),
                last_used: SystemTime::now(),
                usage_count: 0,
                size_bytes: 0,
                dependency_count: config.additional_dependencies.len(),
                language_version: toolchain.version.to_string(),
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        })
    }

    async fn install_dependencies(
        &self,
        _env: &LanguageEnvironment,
        _dependencies: &[Dependency],
    ) -> Result<()> {
        // Basic implementation - dependencies are installed during environment setup
        Ok(())
    }

    async fn cleanup_environment(&self, _env: &LanguageEnvironment) -> Result<()> {
        // Go environment cleanup if needed
        Ok(())
    }

    async fn execute_hook(
        &self,
        hook: &Hook,
        env: &LanguageEnvironment,
        files: &[PathBuf],
    ) -> Result<HookExecutionResult> {
        let command = self.build_command(hook, env, files)?;

        let start_time = std::time::Instant::now();

        // Execute command
        let mut cmd = TokioCommand::new(&command.executable);
        cmd.args(&command.arguments);
        cmd.envs(&command.environment);

        if let Some(ref cwd) = command.working_directory {
            cmd.current_dir(cwd);
        }

        let output = cmd.output().await?;
        let duration = start_time.elapsed();
        let success = output.status.success();
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        Ok(HookExecutionResult {
            hook_id: hook.id.clone(),
            success,
            skipped: false,
            exit_code: output.status.code(),
            duration,
            files_processed: files.to_vec(),
            files_modified: vec![], // Would need to be determined by the specific hook
            stdout,
            stderr,
            error: if success {
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
        let entry_parts: Vec<&str> = hook.entry.split_whitespace().collect();

        let executable = if entry_parts.is_empty() {
            env.executable_path.to_string_lossy().to_string()
        } else {
            entry_parts[0].to_string()
        };

        let mut arguments = Vec::new();

        // Determine working directory - use the directory of the first file if available
        // or fall back to env.root_path
        let working_directory = if !files.is_empty() {
            if let Some(parent) = files[0].parent() {
                parent.to_path_buf()
            } else {
                env.root_path.clone()
            }
        } else {
            env.root_path.clone()
        };

        // Add entry arguments (after the first part which is the executable)
        if entry_parts.len() > 1 {
            arguments.extend(entry_parts[1..].iter().map(|s| s.to_string()));
        }

        // Add hook arguments
        arguments.extend(hook.args.clone());

        // Add files as arguments - convert to relative paths if working directory is set
        // Filter out files that are the main executable (e.g., main.go in "go run main.go")
        // but keep files that are meant to be processed by the program
        for file in files {
            let filename = file
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();

            // Skip adding the file if it's already the main program file in a "go run" command
            // For "go run main.go", don't add main.go again
            // For "go run .", add all files as arguments
            let is_main_program_file = entry_parts.len() >= 3
                && entry_parts[0] == "go"
                && entry_parts[1] == "run"
                && entry_parts[2] != "."
                && entry_parts[2] == filename;

            if !is_main_program_file {
                let file_arg = if let Ok(relative) = file.strip_prefix(&working_directory) {
                    relative.to_string_lossy().to_string()
                } else {
                    file.to_string_lossy().to_string()
                };
                arguments.push(file_arg);
            }
        }

        Ok(Command {
            executable,
            arguments,
            environment: env.environment_variables.clone(),
            working_directory: Some(working_directory),
            timeout: Some(Duration::from_secs(300)), // 5 minutes default
        })
    }

    async fn resolve_dependencies(&self, dependencies: &[String]) -> Result<Vec<Dependency>> {
        let mut resolved = Vec::new();
        for dep_str in dependencies {
            resolved.push(Dependency::new(dep_str));
        }
        Ok(resolved)
    }

    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency> {
        Ok(Dependency::new(dep_spec))
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

impl Default for GoLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}
