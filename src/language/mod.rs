// Language trait architecture for SNP
// This module provides the foundational trait-based architecture for language plugins

pub mod base;
pub mod dependency;
pub mod docker;
pub mod environment;
pub mod executor;
pub mod golang;
pub mod nodejs;
pub mod python;
pub mod registry;
pub mod ruby;
pub mod rust;
pub mod system;
pub mod traits;

// Re-export main types for easier access
pub use base::{BaseLanguagePlugin, CommandBuilder};
pub use dependency::{
    Dependency, DependencyConflict, DependencyManager, DependencyManagerConfig, DependencySource,
    InstallationResult, InstalledPackage, ResolvedDependency, UpdateResult, VersionSpec,
};
pub use docker::DockerLanguagePlugin;
pub use environment::{
    CacheStrategy, EnvironmentConfig, EnvironmentInfo, EnvironmentManager, EnvironmentMetadata,
    IsolationLevel, LanguageEnvironment, ValidationIssue, ValidationReport,
};
pub use executor::{ExecutionCache, LanguageHookExecutor};
pub use golang::GoLanguagePlugin;
pub use nodejs::NodejsLanguagePlugin;
pub use python::PythonLanguagePlugin;
pub use registry::{LanguageRegistry, PluginMetadata};
pub use ruby::RubyLanguagePlugin;
pub use rust::RustLanguagePlugin;
pub use system::SystemLanguagePlugin;
pub use traits::{Language, LanguageError};

// Version information for language trait architecture
pub const LANGUAGE_TRAIT_VERSION: &str = "0.1.0";
