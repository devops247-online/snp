// Dependency management framework for language plugins

use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::error::Result;

use super::environment::LanguageEnvironment;

/// Dependency specification and management
#[derive(Debug, Clone, PartialEq)]
pub struct Dependency {
    pub name: String,
    pub version_spec: VersionSpec,
    pub source: DependencySource,
    pub extras: Vec<String>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VersionSpec {
    Any,
    Exact(String),
    Range {
        min: Option<String>,
        max: Option<String>,
    },
    Compatible(String), // ~= operator
    Exclude(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum DependencySource {
    Registry {
        registry_url: Option<String>,
    },
    Git {
        url: String,
        reference: Option<String>,
    },
    Path {
        path: PathBuf,
    },
    Url {
        url: String,
    },
}

/// Dependency manager trait for language-specific operations
#[async_trait]
pub trait DependencyManager: Send + Sync {
    /// Install dependencies in the given environment
    async fn install(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<InstallationResult>;

    /// Resolve dependency tree and check for conflicts
    async fn resolve(&self, dependencies: &[Dependency]) -> Result<Vec<ResolvedDependency>>;

    /// Check if dependencies are already installed
    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<Vec<bool>>;

    /// Update dependencies to latest compatible versions
    async fn update(
        &self,
        dependencies: &[Dependency],
        env: &LanguageEnvironment,
    ) -> Result<UpdateResult>;

    /// Uninstall dependencies
    async fn uninstall(&self, dependencies: &[Dependency], env: &LanguageEnvironment)
        -> Result<()>;

    /// Get installed package information
    async fn list_installed(&self, env: &LanguageEnvironment) -> Result<Vec<InstalledPackage>>;
}

#[derive(Debug)]
pub struct ResolvedDependency {
    pub dependency: Dependency,
    pub resolved_version: String,
    pub dependencies: Vec<Dependency>, // Transitive dependencies
    pub conflicts: Vec<DependencyConflict>,
}

#[derive(Debug)]
pub struct DependencyConflict {
    pub package_name: String,
    pub required_by: Vec<String>,
    pub conflicting_versions: Vec<String>,
    pub resolution_suggestion: Option<String>,
}

#[derive(Debug)]
pub struct InstallationResult {
    pub installed: Vec<InstalledPackage>,
    pub failed: Vec<(Dependency, String)>,
    pub skipped: Vec<Dependency>,
    pub duration: Duration,
}

#[derive(Debug)]
pub struct UpdateResult {
    pub updated: Vec<(InstalledPackage, String)>, // (package, old_version)
    pub failed: Vec<(Dependency, String)>,
    pub no_update_available: Vec<InstalledPackage>,
    pub duration: Duration,
}

#[derive(Debug, Clone)]
pub struct InstalledPackage {
    pub name: String,
    pub version: String,
    pub source: DependencySource,
    pub install_path: PathBuf,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct DependencyManagerConfig {
    pub registry_urls: Vec<String>,
    pub timeout: Duration,
    pub retry_attempts: u32,
    pub offline_mode: bool,
    pub verify_checksums: bool,
}

impl Default for DependencyManagerConfig {
    fn default() -> Self {
        Self {
            registry_urls: Vec::new(),
            timeout: Duration::from_secs(300), // 5 minutes
            retry_attempts: 3,
            offline_mode: false,
            verify_checksums: true,
        }
    }
}

impl Dependency {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version_spec: VersionSpec::Any,
            source: DependencySource::Registry { registry_url: None },
            extras: Vec::new(),
            metadata: HashMap::new(),
        }
    }

    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version_spec = VersionSpec::Exact(version.into());
        self
    }

    pub fn with_version_range(mut self, min: Option<String>, max: Option<String>) -> Self {
        self.version_spec = VersionSpec::Range { min, max };
        self
    }

    pub fn with_source(mut self, source: DependencySource) -> Self {
        self.source = source;
        self
    }

    pub fn with_extras(mut self, extras: Vec<String>) -> Self {
        self.extras = extras;
        self
    }

    pub fn parse_spec(spec: &str) -> Result<Self> {
        // Simple parsing for now - can be extended
        if let Some((name, version)) = spec.split_once("==") {
            Ok(Self::new(name.trim()).with_version(version.trim()))
        } else if let Some((name, version)) = spec.split_once(">=") {
            Ok(Self::new(name.trim()).with_version_range(Some(version.trim().to_string()), None))
        } else {
            Ok(Self::new(spec.trim()))
        }
    }
}

impl VersionSpec {
    pub fn matches(&self, version: &str) -> bool {
        match self {
            VersionSpec::Any => true,
            VersionSpec::Exact(v) => v == version,
            VersionSpec::Range { min, max } => {
                let mut matches = true;
                if let Some(min_ver) = min {
                    matches &= version >= min_ver.as_str();
                }
                if let Some(max_ver) = max {
                    matches &= version <= max_ver.as_str();
                }
                matches
            }
            VersionSpec::Compatible(v) => {
                // Simple compatible version check - can be made more sophisticated
                version.starts_with(v.as_str())
            }
            VersionSpec::Exclude(versions) => !versions.contains(&version.to_string()),
        }
    }
}

impl DependencySource {
    pub fn registry() -> Self {
        Self::Registry { registry_url: None }
    }

    pub fn registry_with_url(url: impl Into<String>) -> Self {
        Self::Registry {
            registry_url: Some(url.into()),
        }
    }

    pub fn git(url: impl Into<String>) -> Self {
        Self::Git {
            url: url.into(),
            reference: None,
        }
    }

    pub fn git_with_ref(url: impl Into<String>, reference: impl Into<String>) -> Self {
        Self::Git {
            url: url.into(),
            reference: Some(reference.into()),
        }
    }

    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self::Path { path: path.into() }
    }

    pub fn url(url: impl Into<String>) -> Self {
        Self::Url { url: url.into() }
    }
}

/// Mock dependency manager for testing
pub struct MockDependencyManager {
    pub installed_packages: Vec<InstalledPackage>,
}

impl MockDependencyManager {
    pub fn new() -> Self {
        Self {
            installed_packages: Vec::new(),
        }
    }
}

impl Default for MockDependencyManager {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl DependencyManager for MockDependencyManager {
    async fn install(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<InstallationResult> {
        let installed = dependencies
            .iter()
            .map(|dep| InstalledPackage {
                name: dep.name.clone(),
                version: "1.0.0".to_string(),
                source: dep.source.clone(),
                install_path: PathBuf::from("/mock/install/path"),
                metadata: HashMap::new(),
            })
            .collect();

        Ok(InstallationResult {
            installed,
            failed: Vec::new(),
            skipped: Vec::new(),
            duration: Duration::from_millis(100),
        })
    }

    async fn resolve(&self, dependencies: &[Dependency]) -> Result<Vec<ResolvedDependency>> {
        Ok(dependencies
            .iter()
            .map(|dep| ResolvedDependency {
                dependency: dep.clone(),
                resolved_version: "1.0.0".to_string(),
                dependencies: Vec::new(),
                conflicts: Vec::new(),
            })
            .collect())
    }

    async fn check_installed(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<Vec<bool>> {
        Ok(dependencies
            .iter()
            .map(|dep| {
                self.installed_packages
                    .iter()
                    .any(|pkg| pkg.name == dep.name)
            })
            .collect())
    }

    async fn update(
        &self,
        dependencies: &[Dependency],
        _env: &LanguageEnvironment,
    ) -> Result<UpdateResult> {
        Ok(UpdateResult {
            updated: Vec::new(),
            failed: Vec::new(),
            no_update_available: dependencies
                .iter()
                .map(|dep| InstalledPackage {
                    name: dep.name.clone(),
                    version: "1.0.0".to_string(),
                    source: dep.source.clone(),
                    install_path: PathBuf::from("/mock/install/path"),
                    metadata: HashMap::new(),
                })
                .collect(),
            duration: Duration::from_millis(50),
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
        Ok(self.installed_packages.clone())
    }
}
