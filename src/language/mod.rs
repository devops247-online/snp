// Language trait architecture for SNP
// This module provides the foundational trait-based architecture for language plugins

pub mod base;
pub mod dependency;
pub mod environment;
pub mod executor;
pub mod python;
pub mod registry;
pub mod rust;
pub mod system;
pub mod traits;

// Re-export main types for easier access
pub use base::{BaseLanguagePlugin, CommandBuilder};
pub use dependency::{
    Dependency, DependencyConflict, DependencyManager, DependencyManagerConfig, DependencySource,
    InstallationResult, InstalledPackage, ResolvedDependency, UpdateResult, VersionSpec,
};
pub use environment::{
    CacheStrategy, EnvironmentConfig, EnvironmentInfo, EnvironmentManager, EnvironmentMetadata,
    IsolationLevel, LanguageEnvironment, ValidationIssue, ValidationReport,
};
pub use executor::{ExecutionCache, LanguageHookExecutor};
pub use python::PythonLanguagePlugin;
pub use registry::{LanguageRegistry, PluginMetadata};
pub use rust::RustLanguagePlugin;
pub use system::SystemLanguagePlugin;
pub use traits::{Language, LanguageError};

// Version information for language trait architecture
pub const LANGUAGE_TRAIT_VERSION: &str = "0.1.0";
