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

/// Ruby language plugin for Ruby-based hooks and tools
#[derive(Debug, Clone)]
pub struct RubyLanguagePlugin {
    pub config: RubyLanguageConfig,
    pub version_manager_cache: HashMap<String, RubyVersionManager>,
    pub bundler_cache: HashMap<PathBuf, BundlerInfo>,
    dependency_manager: RubyDependencyManager,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubyLanguageConfig {
    pub default_ruby_version: Option<String>,
    pub prefer_system_ruby: bool,
    pub enable_bundler: bool,
    pub bundler_config: BundlerConfig,
    pub gem_config: GemConfig,
    pub cache_config: RubyCacheConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BundlerConfig {
    pub ignore_system_config: bool,
    pub deployment_mode: bool,
    pub without_groups: Vec<String>,
    pub with_groups: Vec<String>,
    pub jobs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GemConfig {
    pub no_document: bool,
    pub no_user_install: bool,
    pub install_dir: Option<PathBuf>,
    pub bindir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RubyCacheConfig {
    pub gem_cache_dir: Option<PathBuf>,
    pub bundler_cache_dir: Option<PathBuf>,
    pub max_cache_size: Option<u64>,
    pub cache_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct RubyInstallation {
    pub ruby_executable: PathBuf,
    pub gem_executable: PathBuf,
    pub version: String,
    pub platform: String,
    pub gem_home: Option<PathBuf>,
    pub gem_path: Vec<PathBuf>,
    pub source: RubyInstallationSource,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RubyInstallationSource {
    System,
    Rbenv { root: PathBuf },
    Rvm { root: PathBuf },
    ChRuby { root: PathBuf },
    Manual(PathBuf),
}

#[derive(Debug, Clone, PartialEq)]
pub enum RubyVersionManager {
    Rbenv(PathBuf),
    Rvm(PathBuf),
    ChRuby(PathBuf),
    System,
}

#[derive(Debug, Clone)]
pub struct BundlerInfo {
    pub gemfile_path: Option<PathBuf>,
    pub gemfile_lock_path: Option<PathBuf>,
    pub ruby_version: Option<String>,
    pub source: String,
    pub dependencies: Vec<GemDependency>,
    pub locked_dependencies: Vec<LockedGem>,
    pub bundler_version: Option<String>,
    pub groups: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GemDependency {
    pub name: String,
    pub version_requirement: String,
    pub groups: Vec<String>,
    pub platforms: Vec<String>,
    pub source: Option<String>,
    pub git: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub ref_: Option<String>,
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct LockedGem {
    pub name: String,
    pub version: String,
    pub dependencies: Vec<String>,
    pub source: String,
}

#[derive(Debug, Clone)]
pub struct RubyEnvironment {
    pub installation: RubyInstallation,
    pub bundler_info: Option<BundlerInfo>,
    pub gem_home: PathBuf,
    pub gem_path: Vec<PathBuf>,
    pub environment_vars: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct RubyDependencyManager {
    #[allow(dead_code)]
    config: DependencyManagerConfig,
}

#[derive(Debug, Error)]
pub enum RubyError {
    #[error("Ruby installation not found")]
    RubyNotFound {
        searched_paths: Vec<PathBuf>,
        min_version: Option<String>,
    },

    #[error("Gemfile not found or invalid: {path}")]
    InvalidGemfile { path: PathBuf, error: String },

    #[error("Gem installation failed: {gem}")]
    GemInstallationFailed {
        gem: String,
        version: Option<String>,
        error: String,
    },

    #[error("Bundler operation failed: {operation}")]
    BundlerFailed { operation: String, error: String },

    #[error("Ruby version {found} does not meet requirement {required}")]
    VersionMismatch { found: String, required: String },

    #[error("Version parsing failed: {input}")]
    VersionParsingFailed { input: String },

    #[error("Environment setup failed: {details}")]
    EnvironmentSetupFailed { details: String },
}

impl Default for RubyLanguageConfig {
    fn default() -> Self {
        Self {
            default_ruby_version: None,
            prefer_system_ruby: true,
            enable_bundler: true,
            bundler_config: BundlerConfig::default(),
            gem_config: GemConfig::default(),
            cache_config: RubyCacheConfig::default(),
        }
    }
}

impl Default for BundlerConfig {
    fn default() -> Self {
        Self {
            ignore_system_config: true,
            deployment_mode: false,
            without_groups: vec!["development".to_string(), "test".to_string()],
            with_groups: vec![],
            jobs: Some(4),
        }
    }
}

impl Default for GemConfig {
    fn default() -> Self {
        Self {
            no_document: true,
            no_user_install: true,
            install_dir: None,
            bindir: None,
        }
    }
}

impl Default for RubyCacheConfig {
    fn default() -> Self {
        Self {
            gem_cache_dir: None,
            bundler_cache_dir: None,
            max_cache_size: Some(1024 * 1024 * 512), // 512MB
            cache_ttl: Duration::from_secs(3600),    // 1 hour
        }
    }
}

impl RubyLanguagePlugin {
    pub fn new() -> Self {
        Self {
            config: RubyLanguageConfig::default(),
            version_manager_cache: HashMap::new(),
            bundler_cache: HashMap::new(),
            dependency_manager: RubyDependencyManager::new(),
        }
    }

    /// Detect available Ruby installations
    pub async fn detect_ruby_installations(&mut self) -> Result<Vec<RubyInstallation>> {
        let mut installations = Vec::new();

        // Check for system Ruby installation
        if let Ok(ruby_path) = which::which("ruby") {
            if let Ok(installation) = self.analyze_ruby_executable(&ruby_path).await {
                installations.push(installation);
            }
        }

        // Check for version manager installations
        let version_managers = self.detect_version_managers().await;
        for manager in version_managers {
            installations.extend(self.get_installations_from_manager(&manager).await);
        }

        // Sort by version (newest first)
        installations.sort_by(|a, b| {
            self.compare_versions(&b.version, &a.version)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(installations)
    }

    /// Detect Ruby version managers
    pub async fn detect_version_managers(&self) -> Vec<RubyVersionManager> {
        let mut managers = Vec::new();

        // Check for rbenv
        if let Ok(rbenv_root) = std::env::var("RBENV_ROOT") {
            let rbenv_path = PathBuf::from(rbenv_root);
            if rbenv_path.exists() {
                managers.push(RubyVersionManager::Rbenv(rbenv_path));
            }
        } else if let Some(home) = dirs::home_dir() {
            let rbenv_path = home.join(".rbenv");
            if rbenv_path.exists() {
                managers.push(RubyVersionManager::Rbenv(rbenv_path));
            }
        }

        // Check for RVM
        if let Ok(rvm_path) = std::env::var("rvm_path") {
            let rvm_root = PathBuf::from(rvm_path);
            if rvm_root.exists() {
                managers.push(RubyVersionManager::Rvm(rvm_root));
            }
        } else if let Some(home) = dirs::home_dir() {
            let rvm_path = home.join(".rvm");
            if rvm_path.exists() {
                managers.push(RubyVersionManager::Rvm(rvm_path));
            }
        }

        // Check for chruby
        if let Ok(chruby_path) = which::which("chruby") {
            if let Some(parent) = chruby_path.parent() {
                managers.push(RubyVersionManager::ChRuby(parent.to_path_buf()));
            }
        }

        // Always include system as fallback
        managers.push(RubyVersionManager::System);

        managers
    }

    /// Analyze a Ruby executable to get installation info
    pub async fn analyze_ruby_executable(&self, ruby_path: &Path) -> Result<RubyInstallation> {
        // Get Ruby version
        let version_output = TokioCommand::new(ruby_path).args(["-v"]).output().await?;

        let version_str = String::from_utf8_lossy(&version_output.stdout);
        let version = self.parse_ruby_version(&version_str)?;

        // Get Ruby platform
        let platform_output = TokioCommand::new(ruby_path)
            .args(["-e", "puts RUBY_PLATFORM"])
            .output()
            .await?;

        let platform = String::from_utf8_lossy(&platform_output.stdout)
            .trim()
            .to_string();

        // Find gem executable
        let gem_executable = if let Some(parent) = ruby_path.parent() {
            let gem_path = parent.join(if cfg!(windows) { "gem.exe" } else { "gem" });
            if gem_path.exists() {
                gem_path
            } else {
                which::which("gem").unwrap_or_else(|_| PathBuf::from("gem"))
            }
        } else {
            which::which("gem").unwrap_or_else(|_| PathBuf::from("gem"))
        };

        // Get gem environment info
        let (gem_home, gem_path) = self.get_gem_environment(ruby_path).await?;

        Ok(RubyInstallation {
            ruby_executable: ruby_path.to_path_buf(),
            gem_executable,
            version,
            platform,
            gem_home,
            gem_path,
            source: RubyInstallationSource::System, // Will be updated by caller if needed
        })
    }

    fn parse_ruby_version(&self, version_str: &str) -> Result<String> {
        // Parse version from output like "ruby 3.0.0p648 (2021-12-25 revision 12345) [x86_64-linux]"
        let version_regex = regex::Regex::new(r"ruby (\d+\.\d+\.\d+)")?;

        if let Some(captures) = version_regex.captures(version_str) {
            if let Some(version_match) = captures.get(1) {
                return Ok(version_match.as_str().to_string());
            }
        }

        Err(SnpError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("Failed to parse Ruby version from: {version_str}"),
        )))
    }

    async fn get_gem_environment(
        &self,
        ruby_path: &Path,
    ) -> Result<(Option<PathBuf>, Vec<PathBuf>)> {
        // Get GEM_HOME
        let gem_home_output = TokioCommand::new(ruby_path)
            .args(["-e", "puts Gem.user_dir rescue nil"])
            .output()
            .await?;

        let gem_home = if gem_home_output.status.success() {
            let path_string = String::from_utf8_lossy(&gem_home_output.stdout);
            let path_str = path_string.trim();
            if !path_str.is_empty() && path_str != "nil" {
                Some(PathBuf::from(path_str))
            } else {
                None
            }
        } else {
            None
        };

        // Get GEM_PATH
        let gem_path_output = TokioCommand::new(ruby_path)
            .args(["-e", "puts Gem.path.join(':')"])
            .output()
            .await?;

        let gem_path = if gem_path_output.status.success() {
            let path_string = String::from_utf8_lossy(&gem_path_output.stdout);
            let path_str = path_string.trim();
            if !path_str.is_empty() {
                path_str
                    .split(':')
                    .filter(|p| !p.is_empty())
                    .map(PathBuf::from)
                    .collect()
            } else {
                vec![]
            }
        } else {
            vec![]
        };

        Ok((gem_home, gem_path))
    }

    async fn get_installations_from_manager(
        &self,
        manager: &RubyVersionManager,
    ) -> Vec<RubyInstallation> {
        let mut installations = Vec::new();

        match manager {
            RubyVersionManager::Rbenv(root) => {
                let versions_dir = root.join("versions");
                if let Ok(entries) = std::fs::read_dir(versions_dir) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            let ruby_path = entry.path().join("bin").join("ruby");
                            if ruby_path.exists() {
                                if let Ok(mut installation) =
                                    self.analyze_ruby_executable(&ruby_path).await
                                {
                                    installation.source =
                                        RubyInstallationSource::Rbenv { root: root.clone() };
                                    installations.push(installation);
                                }
                            }
                        }
                    }
                }
            }
            RubyVersionManager::Rvm(root) => {
                let rubies_dir = root.join("rubies");
                if let Ok(entries) = std::fs::read_dir(rubies_dir) {
                    for entry in entries.flatten() {
                        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                            let ruby_path = entry.path().join("bin").join("ruby");
                            if ruby_path.exists() {
                                if let Ok(mut installation) =
                                    self.analyze_ruby_executable(&ruby_path).await
                                {
                                    installation.source =
                                        RubyInstallationSource::Rvm { root: root.clone() };
                                    installations.push(installation);
                                }
                            }
                        }
                    }
                }
            }
            RubyVersionManager::ChRuby(root) => {
                // chruby typically uses /opt/rubies or ~/.rubies
                let possible_dirs = vec![
                    PathBuf::from("/opt/rubies"),
                    dirs::home_dir().unwrap_or_default().join(".rubies"),
                ];

                for rubies_dir in possible_dirs {
                    if let Ok(entries) = std::fs::read_dir(rubies_dir) {
                        for entry in entries.flatten() {
                            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                                let ruby_path = entry.path().join("bin").join("ruby");
                                if ruby_path.exists() {
                                    if let Ok(mut installation) =
                                        self.analyze_ruby_executable(&ruby_path).await
                                    {
                                        installation.source =
                                            RubyInstallationSource::ChRuby { root: root.clone() };
                                        installations.push(installation);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            RubyVersionManager::System => {
                // System manager is handled in detect_ruby_installations
            }
        }

        installations
    }

    fn compare_versions(&self, a: &str, b: &str) -> Option<std::cmp::Ordering> {
        // Simple version comparison - could be enhanced with proper semver
        let parse_version =
            |v: &str| -> Vec<u32> { v.split('.').filter_map(|part| part.parse().ok()).collect() };

        let version_a = parse_version(a);
        let version_b = parse_version(b);

        Some(version_a.cmp(&version_b))
    }

    /// Analyze Bundler information from a project directory
    pub async fn analyze_bundler_info(&mut self, project_path: &Path) -> Result<BundlerInfo> {
        // Check cache first
        if let Some(cached) = self.bundler_cache.get(project_path) {
            return Ok(cached.clone());
        }

        let gemfile_path = self.find_gemfile(project_path);
        let gemfile_lock_path = self.find_gemfile_lock(project_path);

        let mut bundler_info = BundlerInfo {
            gemfile_path: gemfile_path.clone(),
            gemfile_lock_path: gemfile_lock_path.clone(),
            ruby_version: None,
            source: "https://rubygems.org".to_string(),
            dependencies: vec![],
            locked_dependencies: vec![],
            bundler_version: None,
            groups: vec!["default".to_string()],
        };

        // Parse Gemfile if it exists
        if let Some(ref gemfile) = gemfile_path {
            bundler_info = self.parse_gemfile(gemfile, bundler_info).await?;
        }

        // Parse Gemfile.lock if it exists
        if let Some(ref lockfile) = gemfile_lock_path {
            bundler_info = self.parse_gemfile_lock(lockfile, bundler_info).await?;
        }

        // Cache the result
        self.bundler_cache
            .insert(project_path.to_path_buf(), bundler_info.clone());

        Ok(bundler_info)
    }

    fn find_gemfile(&self, path: &Path) -> Option<PathBuf> {
        let gemfile = path.join("Gemfile");
        if gemfile.exists() {
            Some(gemfile)
        } else {
            None
        }
    }

    fn find_gemfile_lock(&self, path: &Path) -> Option<PathBuf> {
        let lockfile = path.join("Gemfile.lock");
        if lockfile.exists() {
            Some(lockfile)
        } else {
            None
        }
    }

    async fn parse_gemfile(
        &self,
        gemfile_path: &Path,
        mut info: BundlerInfo,
    ) -> Result<BundlerInfo> {
        let content = tokio::fs::read_to_string(gemfile_path).await?;
        let mut current_groups = vec!["default".to_string()];
        let mut group_stack = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip comments and empty lines
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse ruby version
            if line.starts_with("ruby ") {
                if let Some(version) = self.extract_quoted_value(line, "ruby ") {
                    info.ruby_version = Some(version);
                }
            }
            // Parse source
            else if line.starts_with("source ") {
                if let Some(source) = self.extract_quoted_value(line, "source ") {
                    info.source = source;
                }
            }
            // Parse group blocks
            else if line.starts_with("group ") {
                let groups = self.parse_group_line(line);
                group_stack.push(current_groups.clone());
                current_groups = groups;
            }
            // Handle end of group block
            else if line == "end" && !group_stack.is_empty() {
                current_groups = group_stack.pop().unwrap();
            }
            // Parse gem dependencies
            else if line.starts_with("gem ") {
                if let Some(mut gem_dep) = self.parse_gem_line(line)? {
                    gem_dep.groups = current_groups.clone();
                    info.dependencies.push(gem_dep);
                }
            }
        }

        Ok(info)
    }

    async fn parse_gemfile_lock(
        &self,
        lockfile_path: &Path,
        mut info: BundlerInfo,
    ) -> Result<BundlerInfo> {
        let content = tokio::fs::read_to_string(lockfile_path).await?;
        let mut in_specs = false;
        let mut _in_dependencies = false;

        for original_line in content.lines() {
            let line = original_line.trim();

            if line == "GEM" {
                continue;
            } else if line.starts_with("remote:") {
                if let Some(source) = line.strip_prefix("remote: ") {
                    info.source = source.trim().to_string();
                }
            } else if line == "specs:" {
                in_specs = true;
                _in_dependencies = false;
            } else if line == "DEPENDENCIES" {
                in_specs = false;
                _in_dependencies = true;
            } else if line.starts_with("RUBY VERSION") {
                // Skip to next line for actual version
                continue;
            } else if line.starts_with("ruby ") && info.ruby_version.is_none() {
                if let Some(version) = self.extract_version_from_ruby_line(line) {
                    info.ruby_version = Some(version);
                }
            } else if line == "BUNDLED WITH" {
                // Skip to next line for bundler version
                continue;
            } else if in_specs
                && original_line.starts_with("    ")
                && line.contains('(')
                && line.contains(')')
            {
                // Parse locked gem - use original line to check indentation
                if let Some(locked_gem) = self.parse_locked_gem_line(original_line)? {
                    info.locked_dependencies.push(locked_gem);
                }
            } else if !line.is_empty()
                && line.chars().all(|c| c.is_ascii_digit() || c == '.')
                && info.bundler_version.is_none()
            {
                // This is likely the bundler version (line after "BUNDLED WITH")
                info.bundler_version = Some(line.to_string());
            }
        }

        Ok(info)
    }

    fn parse_group_line(&self, line: &str) -> Vec<String> {
        // Parse lines like: group :development, :test do
        if let Some(rest) = line.strip_prefix("group ") {
            let groups_part = rest.trim_end_matches(" do").trim();
            let groups = groups_part
                .split(',')
                .map(|g| g.trim())
                .filter_map(|g| {
                    if let Some(stripped) = g.strip_prefix(':') {
                        Some(stripped.to_string())
                    } else if g.starts_with('\'') || g.starts_with('"') {
                        self.extract_quoted_string(g)
                    } else {
                        Some(g.to_string())
                    }
                })
                .collect();
            groups
        } else {
            vec!["default".to_string()]
        }
    }

    fn extract_quoted_value(&self, line: &str, prefix: &str) -> Option<String> {
        if let Some(rest) = line.strip_prefix(prefix) {
            let rest = rest.trim();
            if let Some(quoted) = rest.strip_prefix('\'') {
                if let Some(end) = quoted.find('\'') {
                    return Some(quoted[..end].to_string());
                }
            } else if let Some(quoted) = rest.strip_prefix('"') {
                if let Some(end) = quoted.find('"') {
                    return Some(quoted[..end].to_string());
                }
            }
        }
        None
    }

    fn parse_gem_line(&self, line: &str) -> Result<Option<GemDependency>> {
        // Parse lines like: gem 'rails', '~> 7.0.0', group: :production
        if let Some(rest) = line.strip_prefix("gem ") {
            let parts: Vec<&str> = rest.split(',').map(|s| s.trim()).collect();
            if !parts.is_empty() {
                let name = self.extract_gem_name(parts[0])?;
                let mut version_requirement = "".to_string();
                let groups = vec!["default".to_string()]; // Will be overridden by caller

                // Parse version and options
                for part in &parts[1..] {
                    if part.starts_with('\'') || part.starts_with('"') {
                        // This is a version requirement
                        if let Some(version) = self.extract_quoted_string(part) {
                            version_requirement = version;
                        }
                    }
                    // Note: Group parsing is handled at the Gemfile level, not individual gem level
                }

                return Ok(Some(GemDependency {
                    name,
                    version_requirement,
                    groups,
                    platforms: vec![],
                    source: None,
                    git: None,
                    branch: None,
                    tag: None,
                    ref_: None,
                    path: None,
                }));
            }
        }
        Ok(None)
    }

    fn extract_gem_name(&self, part: &str) -> Result<String> {
        if let Some(name) = self.extract_quoted_string(part) {
            Ok(name)
        } else {
            Err(SnpError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("Failed to extract gem name from: {part}"),
            )))
        }
    }

    fn extract_quoted_string(&self, s: &str) -> Option<String> {
        let s = s.trim();
        if let Some(quoted) = s.strip_prefix('\'') {
            if let Some(end) = quoted.find('\'') {
                return Some(quoted[..end].to_string());
            }
        } else if let Some(quoted) = s.strip_prefix('"') {
            if let Some(end) = quoted.find('"') {
                return Some(quoted[..end].to_string());
            }
        }
        None
    }

    fn extract_version_from_ruby_line(&self, line: &str) -> Option<String> {
        // Parse lines like "   ruby 3.0.0p648"
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 2 && parts[0] == "ruby" {
            // Extract just the version number without patch info
            let version_with_patch = parts[1];
            if let Some(pos) = version_with_patch.find('p') {
                Some(version_with_patch[..pos].to_string())
            } else {
                Some(version_with_patch.to_string())
            }
        } else {
            None
        }
    }

    fn parse_locked_gem_line(&self, line: &str) -> Result<Option<LockedGem>> {
        // Parse lines like "    actionpack (7.0.4)"
        let line = line.trim();
        if let Some(open_paren) = line.find('(') {
            let name = line[..open_paren].trim().to_string();
            if let Some(close_paren) = line.find(')') {
                let version = line[open_paren + 1..close_paren].trim().to_string();
                return Ok(Some(LockedGem {
                    name,
                    version,
                    dependencies: vec![], // Would need more parsing for full dependency info
                    source: "rubygems".to_string(),
                }));
            }
        }
        Ok(None)
    }

    /// Install gem dependencies for a project
    pub async fn install_gem_dependencies(
        &self,
        env: &LanguageEnvironment,
        additional_deps: &[String],
        project_path: &Path,
    ) -> Result<()> {
        // Check for gemspec files
        let gemspec_files: Vec<PathBuf> = std::fs::read_dir(project_path)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("gemspec") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        // Build gems from gemspecs
        for gemspec in &gemspec_files {
            let output = TokioCommand::new("gem")
                .args(["build", &gemspec.to_string_lossy()])
                .current_dir(project_path)
                .envs(&env.environment_variables)
                .output()
                .await?;

            if !output.status.success() {
                return Err(SnpError::Io(std::io::Error::other(format!(
                    "Gem build failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ))));
            }
        }

        // Install built gems and additional dependencies
        let gem_files: Vec<PathBuf> = std::fs::read_dir(project_path)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("gem") {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        let gem_home = env
            .environment_variables
            .get("GEM_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| project_path.join(".gem"));

        let gem_bin = gem_home.join("bin");

        let mut install_args = vec![
            "install".to_string(),
            "--no-document".to_string(),
            "--no-user-install".to_string(),
            "--install-dir".to_string(),
            gem_home.to_string_lossy().to_string(),
            "--bindir".to_string(),
            gem_bin.to_string_lossy().to_string(),
        ];

        // Add gem files
        for gem_file in gem_files {
            install_args.push(gem_file.to_string_lossy().to_string());
        }

        // Add additional dependencies
        install_args.extend(additional_deps.iter().cloned());

        if !install_args.iter().any(|arg| arg.ends_with(".gem")) && additional_deps.is_empty() {
            // Nothing to install
            return Ok(());
        }

        let output = TokioCommand::new("gem")
            .args(&install_args)
            .current_dir(project_path)
            .envs(&env.environment_variables)
            .output()
            .await?;

        if !output.status.success() {
            return Err(SnpError::Io(std::io::Error::other(format!(
                "Gem installation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            ))));
        }

        Ok(())
    }

    pub fn parse_version_requirement(&self, req: &str) -> Result<String> {
        // Simple version requirement validation
        let req = req.trim();
        if req.is_empty() {
            return Err(SnpError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Empty version requirement",
            )));
        }

        // Basic validation for common patterns
        let valid_patterns = [
            regex::Regex::new(r"^~>\s*\d+\.\d+(\.\d+)?")?,
            regex::Regex::new(r"^>=\s*\d+\.\d+(\.\d+)?")?,
            regex::Regex::new(r"^>\s*\d+\.\d+(\.\d+)?")?,
            regex::Regex::new(r"^<=\s*\d+\.\d+(\.\d+)?")?,
            regex::Regex::new(r"^<\s*\d+\.\d+(\.\d+)?")?,
            regex::Regex::new(r"^=\s*\d+\.\d+(\.\d+)?")?,
            regex::Regex::new(r"^\d+\.\d+(\.\d+)?$")?,
        ];

        for pattern in &valid_patterns {
            if pattern.is_match(req) {
                return Ok(req.to_string());
            }
        }

        Err(SnpError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("Invalid version requirement: {req}"),
        )))
    }
}

impl Default for RubyDependencyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl RubyDependencyManager {
    pub fn new() -> Self {
        Self {
            config: DependencyManagerConfig::default(),
        }
    }
}

#[async_trait]
impl DependencyManager for RubyDependencyManager {
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
impl Language for RubyLanguagePlugin {
    fn language_name(&self) -> &str {
        "ruby"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["rb", "ruby", "gemspec"]
    }

    fn detection_patterns(&self) -> &[&str] {
        &[
            "Gemfile",
            "Gemfile.lock",
            r"^#!/usr/bin/env ruby",
            r"^#!/usr/bin/ruby",
            r"\.gemspec$",
        ]
    }

    async fn setup_environment(&self, config: &EnvironmentConfig) -> Result<LanguageEnvironment> {
        // Find suitable Ruby installation
        let mut plugin_clone = self.clone();
        let installations = plugin_clone.detect_ruby_installations().await?;

        if installations.is_empty() {
            return Err(SnpError::Io(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "No Ruby installation found",
            )));
        }

        // Select the best installation based on config
        let installation = if let Some(version) = &config.language_version {
            let installations_clone = installations.clone();
            installations_clone
                .into_iter()
                .find(|inst| inst.version.starts_with(version) || version == "system")
                .unwrap_or_else(|| installations.into_iter().next().unwrap())
        } else {
            installations.into_iter().next().unwrap()
        };

        // Setup gem environment
        let current_dir = std::env::current_dir()?;
        let working_dir = config.working_directory.as_ref().unwrap_or(&current_dir);

        // Create isolated gem environment
        let environment_id = format!("ruby-{}", installation.version);
        let gem_home = working_dir.join(".snp").join("ruby").join(&environment_id);
        std::fs::create_dir_all(&gem_home)?;

        let gem_bin = gem_home.join("bin");
        std::fs::create_dir_all(&gem_bin)?;

        // Build environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert(
            "GEM_HOME".to_string(),
            gem_home.to_string_lossy().to_string(),
        );
        env_vars.insert("GEM_PATH".to_string(), String::new()); // Unset GEM_PATH
        env_vars.insert("BUNDLE_IGNORE_CONFIG".to_string(), "1".to_string());

        // Add gem bin to PATH
        if let Some(current_path) = std::env::var_os("PATH") {
            let mut paths = vec![gem_bin.to_string_lossy().to_string()];
            if let Some(path_str) = current_path.to_str() {
                paths.push(path_str.to_string());
            }
            env_vars.insert("PATH".to_string(), paths.join(":"));
        }

        // Add custom environment variables
        for (key, value) in &config.environment_variables {
            env_vars.insert(key.clone(), value.clone());
        }

        Ok(LanguageEnvironment {
            language: "ruby".to_string(),
            environment_id,
            root_path: gem_home.clone(),
            executable_path: installation.ruby_executable,
            environment_variables: env_vars,
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
                language_version: installation.version,
                platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            },
        })
    }

    async fn install_dependencies(
        &self,
        env: &LanguageEnvironment,
        dependencies: &[Dependency],
    ) -> Result<()> {
        let dep_names: Vec<String> = dependencies.iter().map(|d| d.name.clone()).collect();

        let working_dir = env.root_path.parent().unwrap_or(&env.root_path);
        self.install_gem_dependencies(env, &dep_names, working_dir)
            .await
    }

    async fn cleanup_environment(&self, _env: &LanguageEnvironment) -> Result<()> {
        // Ruby environment cleanup if needed
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
            skip_reason: None,
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

        // Determine working directory
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
        for file in files {
            let file_arg = if let Ok(relative) = file.strip_prefix(&working_directory) {
                relative.to_string_lossy().to_string()
            } else {
                file.to_string_lossy().to_string()
            };
            arguments.push(file_arg);
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
            resolved.push(self.parse_dependency(dep_str)?);
        }
        Ok(resolved)
    }

    fn parse_dependency(&self, dep_spec: &str) -> Result<Dependency> {
        // Parse gem specifications like "rails >= 7.0.0"
        let parts: Vec<&str> = dep_spec.split_whitespace().collect();
        if parts.is_empty() {
            return Err(SnpError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Empty dependency specification",
            )));
        }

        let name = parts[0].to_string();
        let version_spec = if parts.len() > 2 {
            let version_req = format!("{} {}", parts[1], parts[2]);
            if version_req.starts_with("~>") {
                super::dependency::VersionSpec::Compatible(
                    version_req
                        .strip_prefix("~> ")
                        .unwrap_or(&version_req)
                        .to_string(),
                )
            } else {
                super::dependency::VersionSpec::Exact(version_req)
            }
        } else if parts.len() > 1 {
            let version_req = parts[1].to_string();
            if version_req.starts_with("~>") {
                super::dependency::VersionSpec::Compatible(
                    version_req
                        .strip_prefix("~> ")
                        .unwrap_or(&version_req)
                        .to_string(),
                )
            } else {
                super::dependency::VersionSpec::Exact(version_req)
            }
        } else {
            super::dependency::VersionSpec::Any
        };

        Ok(Dependency {
            name,
            version_spec,
            source: super::dependency::DependencySource::Registry {
                registry_url: Some("https://rubygems.org".to_string()),
            },
            extras: vec![],
            metadata: HashMap::new(),
        })
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

impl Default for RubyLanguagePlugin {
    fn default() -> Self {
        Self::new()
    }
}
