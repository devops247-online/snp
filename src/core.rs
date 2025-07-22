// Core data structures for SNP
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Result, SnpError};

/// Represents the different stages where hooks can be executed
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Stage {
    PreCommit,
    PreMergeCommit,
    PrePush,
    PrepareCommitMsg,
    CommitMsg,
    PostCommit,
    PostCheckout,
    PostMerge,
    PreRebase,
    PostRewrite,
    Manual,
}

impl Stage {
    /// Convert from string representation
    #[allow(clippy::should_implement_trait, clippy::empty_line_after_outer_attr)]

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "pre-commit" => Ok(Stage::PreCommit),
            "pre-merge-commit" => Ok(Stage::PreMergeCommit),
            "pre-push" => Ok(Stage::PrePush),
            "prepare-commit-msg" => Ok(Stage::PrepareCommitMsg),
            "commit-msg" => Ok(Stage::CommitMsg),
            "post-commit" => Ok(Stage::PostCommit),
            "post-checkout" => Ok(Stage::PostCheckout),
            "post-merge" => Ok(Stage::PostMerge),
            "pre-rebase" => Ok(Stage::PreRebase),
            "post-rewrite" => Ok(Stage::PostRewrite),
            "manual" => Ok(Stage::Manual),
            // Legacy aliases
            "commit" => Ok(Stage::PreCommit),
            "push" => Ok(Stage::PrePush),
            _ => Err(SnpError::Config(format!("Unknown stage: {s}"))),
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Stage::PreCommit => "pre-commit",
            Stage::PreMergeCommit => "pre-merge-commit",
            Stage::PrePush => "pre-push",
            Stage::PrepareCommitMsg => "prepare-commit-msg",
            Stage::CommitMsg => "commit-msg",
            Stage::PostCommit => "post-commit",
            Stage::PostCheckout => "post-checkout",
            Stage::PostMerge => "post-merge",
            Stage::PreRebase => "pre-rebase",
            Stage::PostRewrite => "post-rewrite",
            Stage::Manual => "manual",
        }
    }

    /// Get all supported stages
    pub fn all() -> Vec<Stage> {
        vec![
            Stage::PreCommit,
            Stage::PreMergeCommit,
            Stage::PrePush,
            Stage::PrepareCommitMsg,
            Stage::CommitMsg,
            Stage::PostCommit,
            Stage::PostCheckout,
            Stage::PostMerge,
            Stage::PreRebase,
            Stage::PostRewrite,
            Stage::Manual,
        ]
    }
}

/// Represents different types of repositories
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Repository {
    /// Remote git repository
    Remote { url: String, rev: Option<String> },
    /// Local filesystem repository
    Local { path: PathBuf },
    /// Meta repository (built-in hooks)
    Meta,
}

impl Repository {
    /// Create a new remote repository
    pub fn remote(url: impl Into<String>, rev: Option<impl Into<String>>) -> Self {
        Repository::Remote {
            url: url.into(),
            rev: rev.map(|r| r.into()),
        }
    }

    /// Create a new local repository
    pub fn local(path: impl Into<PathBuf>) -> Self {
        Repository::Local { path: path.into() }
    }

    /// Create a meta repository
    pub fn meta() -> Self {
        Repository::Meta
    }

    /// Check if this is a remote repository
    pub fn is_remote(&self) -> bool {
        matches!(self, Repository::Remote { .. })
    }

    /// Check if this is a local repository
    pub fn is_local(&self) -> bool {
        matches!(self, Repository::Local { .. })
    }

    /// Check if this is a meta repository
    pub fn is_meta(&self) -> bool {
        matches!(self, Repository::Meta)
    }

    /// Get the repository identifier (URL for remote, path for local, "meta" for meta)
    pub fn identifier(&self) -> String {
        match self {
            Repository::Remote { url, .. } => url.clone(),
            Repository::Local { path } => path.display().to_string(),
            Repository::Meta => "meta".to_string(),
        }
    }

    /// Validate the repository configuration
    pub fn validate(&self) -> Result<()> {
        match self {
            Repository::Remote { url, .. } => {
                if url.is_empty() {
                    return Err(SnpError::Config(
                        "Remote repository URL cannot be empty".to_string(),
                    ));
                }
                // Basic URL validation
                if !url.contains("://") && !url.contains('@') {
                    return Err(SnpError::Config(format!("Invalid repository URL: {url}")));
                }
            }
            Repository::Local { path } => {
                if !path.exists() {
                    return Err(SnpError::Config(format!(
                        "Local repository path does not exist: {}",
                        path.display()
                    )));
                }
            }
            Repository::Meta => {
                // Meta repositories are always valid
            }
        }
        Ok(())
    }
}

/// File filter for matching files against patterns
#[derive(Debug, Clone)]
pub struct FileFilter {
    pub files_pattern: Option<Regex>,
    pub exclude_pattern: Option<Regex>,
    pub types: Vec<String>,
    pub exclude_types: Vec<String>,
}

impl FileFilter {
    /// Create a new file filter
    pub fn new() -> Self {
        Self {
            files_pattern: None,
            exclude_pattern: None,
            types: Vec::new(),
            exclude_types: Vec::new(),
        }
    }

    /// Set the files pattern
    pub fn with_files_pattern(mut self, pattern: &str) -> Result<Self> {
        self.files_pattern =
            Some(Regex::new(pattern).map_err(|e| {
                SnpError::Config(format!("Invalid files pattern '{pattern}': {e}"))
            })?);
        Ok(self)
    }

    /// Set the exclude pattern
    pub fn with_exclude_pattern(mut self, pattern: &str) -> Result<Self> {
        self.exclude_pattern =
            Some(Regex::new(pattern).map_err(|e| {
                SnpError::Config(format!("Invalid exclude pattern '{pattern}': {e}"))
            })?);
        Ok(self)
    }

    /// Add file types
    pub fn with_types(mut self, types: Vec<String>) -> Self {
        self.types = types;
        self
    }

    /// Add exclude types
    pub fn with_exclude_types(mut self, exclude_types: Vec<String>) -> Self {
        self.exclude_types = exclude_types;
        self
    }

    /// Check if a file matches this filter
    pub fn matches(&self, file_path: &str) -> bool {
        // Check exclude pattern first
        if let Some(ref exclude) = self.exclude_pattern {
            if exclude.is_match(file_path) {
                return false;
            }
        }

        // Check files pattern
        if let Some(ref files) = self.files_pattern {
            if !files.is_match(file_path) {
                return false;
            }
        }

        // TODO: Implement type-based filtering
        // For now, if no patterns are set, match everything
        true
    }
}

impl Default for FileFilter {
    fn default() -> Self {
        Self::new()
    }
}

/// Enhanced Hook structure with runtime capabilities
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hook {
    /// Unique identifier for the hook
    pub id: String,
    /// Human-readable name (optional)
    pub name: Option<String>,
    /// Command to execute
    pub entry: String,
    /// Programming language environment
    pub language: String,
    /// File patterns to include (raw string for serialization)
    pub files: Option<String>,
    /// File patterns to exclude (raw string for serialization)
    pub exclude: Option<String>,
    /// File types to include
    pub types: Vec<String>,
    /// File types to exclude
    pub exclude_types: Vec<String>,
    /// Additional dependencies to install
    pub additional_dependencies: Vec<String>,
    /// Additional arguments to pass to the hook
    pub args: Vec<String>,
    /// Whether to always run this hook, even if no files match
    pub always_run: bool,
    /// Whether to fail fast on first error
    pub fail_fast: bool,
    /// Whether to pass filenames to the hook
    pub pass_filenames: bool,
    /// Stages where this hook should run
    pub stages: Vec<Stage>,
    /// Whether to show verbose output
    pub verbose: bool,
    /// Minimum version required
    pub minimum_pre_commit_version: Option<String>,
}

impl Hook {
    /// Create a new hook with required fields
    pub fn new(
        id: impl Into<String>,
        entry: impl Into<String>,
        language: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: None,
            entry: entry.into(),
            language: language.into(),
            files: None,
            exclude: None,
            types: Vec::new(),
            exclude_types: Vec::new(),
            additional_dependencies: Vec::new(),
            args: Vec::new(),
            always_run: false,
            fail_fast: false,
            pass_filenames: true,
            stages: vec![Stage::PreCommit], // Default stage
            verbose: false,
            minimum_pre_commit_version: None,
        }
    }

    /// Builder pattern methods
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_files(mut self, files: impl Into<String>) -> Self {
        self.files = Some(files.into());
        self
    }

    pub fn with_exclude(mut self, exclude: impl Into<String>) -> Self {
        self.exclude = Some(exclude.into());
        self
    }

    pub fn with_types(mut self, types: Vec<String>) -> Self {
        self.types = types;
        self
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_stages(mut self, stages: Vec<Stage>) -> Self {
        self.stages = stages;
        self
    }

    pub fn always_run(mut self, always_run: bool) -> Self {
        self.always_run = always_run;
        self
    }

    pub fn fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }

    pub fn pass_filenames(mut self, pass_filenames: bool) -> Self {
        self.pass_filenames = pass_filenames;
        self
    }

    /// Validate the hook configuration
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(SnpError::Config("Hook ID cannot be empty".to_string()));
        }

        if self.entry.is_empty() {
            return Err(SnpError::Config(format!(
                "Hook '{}' has empty entry",
                self.id
            )));
        }

        // Validate file patterns
        if let Some(ref files) = self.files {
            Regex::new(files).map_err(|e| {
                SnpError::Config(format!("Hook '{}' has invalid files pattern: {e}", self.id))
            })?;
        }

        if let Some(ref exclude) = self.exclude {
            Regex::new(exclude).map_err(|e| {
                SnpError::Config(format!(
                    "Hook '{}' has invalid exclude pattern: {e}",
                    self.id
                ))
            })?;
        }

        if self.stages.is_empty() {
            return Err(SnpError::Config(format!(
                "Hook '{}' must have at least one stage",
                self.id
            )));
        }

        Ok(())
    }

    /// Get a file filter for this hook
    pub fn file_filter(&self) -> Result<FileFilter> {
        let mut filter = FileFilter::new();

        if let Some(ref files) = self.files {
            filter = filter.with_files_pattern(files)?;
        }

        if let Some(ref exclude) = self.exclude {
            filter = filter.with_exclude_pattern(exclude)?;
        }

        filter = filter
            .with_types(self.types.clone())
            .with_exclude_types(self.exclude_types.clone());

        Ok(filter)
    }

    /// Check if this hook should run for the given stage
    pub fn runs_for_stage(&self, stage: &Stage) -> bool {
        self.stages.contains(stage)
    }

    /// Get the full command to execute
    pub fn command(&self) -> Vec<String> {
        let mut cmd = vec![self.entry.clone()];
        cmd.extend(self.args.clone());
        cmd
    }
}

/// Execution context for running hooks
#[derive(Debug, Clone)]
pub struct ExecutionContext {
    /// Files to process
    pub files: Vec<PathBuf>,
    /// Current stage being executed
    pub stage: Stage,
    /// Whether to show verbose output
    pub verbose: bool,
    /// Whether to show diff on failure
    pub show_diff_on_failure: bool,
    /// Environment variables
    pub environment: HashMap<String, String>,
    /// Working directory
    pub working_directory: PathBuf,
    /// Whether to use color output
    pub color: bool,
}

impl ExecutionContext {
    /// Create a new execution context
    pub fn new(stage: Stage) -> Self {
        Self {
            files: Vec::new(),
            stage,
            verbose: false,
            show_diff_on_failure: false,
            environment: HashMap::new(),
            working_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            color: true,
        }
    }

    /// Builder pattern methods
    pub fn with_files(mut self, files: Vec<PathBuf>) -> Self {
        self.files = files;
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn with_show_diff(mut self, show_diff: bool) -> Self {
        self.show_diff_on_failure = show_diff;
        self
    }

    pub fn with_environment(mut self, env: HashMap<String, String>) -> Self {
        self.environment = env;
        self
    }

    pub fn with_working_directory(mut self, dir: PathBuf) -> Self {
        self.working_directory = dir;
        self
    }

    pub fn with_color(mut self, color: bool) -> Self {
        self.color = color;
        self
    }

    /// Add an environment variable
    pub fn add_env(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.environment.insert(key.into(), value.into());
    }

    /// Get filtered files for a specific hook
    pub fn filtered_files(&self, hook: &Hook) -> Result<Vec<PathBuf>> {
        if hook.always_run {
            return Ok(self.files.clone());
        }

        let filter = hook.file_filter()?;
        let filtered: Vec<PathBuf> = self
            .files
            .iter()
            .filter(|path| {
                let path_str = path.to_string_lossy();
                filter.matches(&path_str)
            })
            .cloned()
            .collect();

        Ok(filtered)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stage_conversion() {
        assert_eq!(Stage::from_str("pre-commit").unwrap(), Stage::PreCommit);
        assert_eq!(Stage::from_str("commit").unwrap(), Stage::PreCommit); // Legacy alias
        assert_eq!(Stage::PreCommit.as_str(), "pre-commit");

        assert!(Stage::from_str("invalid-stage").is_err());
    }

    #[test]
    fn test_stage_all() {
        let stages = Stage::all();
        assert!(stages.len() >= 10);
        assert!(stages.contains(&Stage::PreCommit));
        assert!(stages.contains(&Stage::PrePush));
    }

    #[test]
    fn test_repository_creation() {
        let remote = Repository::remote("https://github.com/test/test", Some("v1.0.0"));
        assert!(remote.is_remote());
        assert!(!remote.is_local());
        assert!(!remote.is_meta());
        assert_eq!(remote.identifier(), "https://github.com/test/test");

        let local = Repository::local("/path/to/local");
        assert!(local.is_local());

        let meta = Repository::meta();
        assert!(meta.is_meta());
        assert_eq!(meta.identifier(), "meta");
    }

    #[test]
    fn test_repository_validation() {
        // Valid remote repository
        let remote = Repository::remote("https://github.com/test/test", Option::<&str>::None);
        assert!(remote.validate().is_ok());

        // Invalid empty URL
        let invalid = Repository::Remote {
            url: String::new(),
            rev: None,
        };
        assert!(invalid.validate().is_err());

        // Meta repository is always valid
        let meta = Repository::meta();
        assert!(meta.validate().is_ok());
    }

    #[test]
    fn test_file_filter_creation() {
        let filter = FileFilter::new()
            .with_files_pattern(r"\.py$")
            .unwrap()
            .with_exclude_pattern(r"test_.*\.py$")
            .unwrap()
            .with_types(vec!["python".to_string()]);

        assert!(filter.matches("script.py"));
        assert!(!filter.matches("test_script.py"));
        assert!(!filter.matches("script.js"));
    }

    #[test]
    fn test_file_filter_invalid_regex() {
        let result = FileFilter::new().with_files_pattern("[invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_hook_creation_and_validation() {
        let hook = Hook::new("black", "black --check", "python")
            .with_name("Black formatter")
            .with_files(r"\.py$".to_string())
            .with_args(vec!["--line-length".to_string(), "88".to_string()])
            .pass_filenames(true)
            .fail_fast(false);

        assert_eq!(hook.id, "black");
        assert_eq!(hook.name, Some("Black formatter".to_string()));
        assert_eq!(hook.entry, "black --check");
        assert_eq!(hook.language, "python");
        assert!(hook.pass_filenames);
        assert!(!hook.fail_fast);
        assert_eq!(hook.stages, vec![Stage::PreCommit]);

        assert!(hook.validate().is_ok());
    }

    #[test]
    fn test_hook_validation_errors() {
        // Empty ID
        let hook = Hook::new("", "test", "system");
        assert!(hook.validate().is_err());

        // Empty entry
        let hook = Hook::new("test", "", "system");
        assert!(hook.validate().is_err());

        // Invalid files pattern
        let hook = Hook::new("test", "test", "system").with_files("[invalid".to_string());
        assert!(hook.validate().is_err());

        // No stages
        let mut hook = Hook::new("test", "test", "system");
        hook.stages.clear();
        assert!(hook.validate().is_err());
    }

    #[test]
    fn test_hook_runs_for_stage() {
        let hook =
            Hook::new("test", "test", "system").with_stages(vec![Stage::PreCommit, Stage::PrePush]);

        assert!(hook.runs_for_stage(&Stage::PreCommit));
        assert!(hook.runs_for_stage(&Stage::PrePush));
        assert!(!hook.runs_for_stage(&Stage::PostCommit));
    }

    #[test]
    fn test_hook_command() {
        let hook = Hook::new("black", "black", "python")
            .with_args(vec!["--check".to_string(), "--diff".to_string()]);

        let cmd = hook.command();
        assert_eq!(cmd, vec!["black", "--check", "--diff"]);
    }

    #[test]
    fn test_hook_file_filter() {
        let hook = Hook::new("test", "test", "system")
            .with_files(r"\.py$".to_string())
            .with_exclude(r"test_.*\.py$".to_string());

        let filter = hook.file_filter().unwrap();
        assert!(filter.matches("script.py"));
        assert!(!filter.matches("test_script.py"));
    }

    #[test]
    fn test_execution_context_creation() {
        let ctx = ExecutionContext::new(Stage::PreCommit)
            .with_files(vec![PathBuf::from("test.py")])
            .with_verbose(true)
            .with_show_diff(true);

        assert_eq!(ctx.stage, Stage::PreCommit);
        assert_eq!(ctx.files, vec![PathBuf::from("test.py")]);
        assert!(ctx.verbose);
        assert!(ctx.show_diff_on_failure);
    }

    #[test]
    fn test_execution_context_environment() {
        let mut ctx = ExecutionContext::new(Stage::PreCommit);
        ctx.add_env("TEST_VAR", "test_value");

        assert_eq!(
            ctx.environment.get("TEST_VAR"),
            Some(&"test_value".to_string())
        );
    }

    #[test]
    fn test_execution_context_filtered_files() {
        let hook = Hook::new("test", "test", "system").with_files(r"\.py$".to_string());

        let ctx = ExecutionContext::new(Stage::PreCommit).with_files(vec![
            PathBuf::from("test.py"),
            PathBuf::from("test.js"),
            PathBuf::from("another.py"),
        ]);

        let filtered = ctx.filtered_files(&hook).unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&PathBuf::from("test.py")));
        assert!(filtered.contains(&PathBuf::from("another.py")));
        assert!(!filtered.contains(&PathBuf::from("test.js")));
    }

    #[test]
    fn test_execution_context_always_run() {
        let hook = Hook::new("test", "test", "system")
            .with_files(r"\.py$".to_string())
            .always_run(true);

        let ctx =
            ExecutionContext::new(Stage::PreCommit).with_files(vec![PathBuf::from("test.js")]); // Doesn't match pattern

        let filtered = ctx.filtered_files(&hook).unwrap();
        assert_eq!(filtered.len(), 1); // Should return all files due to always_run
    }

    // Additional creation tests (targeting 30+ total)
    #[test]
    fn test_hook_builder_pattern_comprehensive() {
        let hook = Hook::new("comprehensive-test", "echo test", "system")
            .with_name("Comprehensive Test Hook")
            .with_files(r"\.(rs|py)$".to_string())
            .with_exclude(r"test_.*".to_string())
            .with_types(vec!["rust".to_string(), "python".to_string()])
            .with_args(vec!["--check".to_string(), "--verbose".to_string()])
            .with_stages(vec![Stage::PreCommit, Stage::PrePush])
            .always_run(false)
            .fail_fast(true)
            .pass_filenames(true);

        assert_eq!(hook.id, "comprehensive-test");
        assert_eq!(hook.name, Some("Comprehensive Test Hook".to_string()));
        assert_eq!(hook.types, vec!["rust", "python"]);
        assert_eq!(hook.args, vec!["--check", "--verbose"]);
        assert_eq!(hook.stages, vec![Stage::PreCommit, Stage::PrePush]);
        assert!(hook.fail_fast);
        assert!(hook.pass_filenames);
        assert!(!hook.always_run);
    }

    #[test]
    fn test_repository_remote_with_revision() {
        let repo = Repository::remote("https://github.com/rust-lang/rust", Some("1.70.0"));
        match repo {
            Repository::Remote { url, rev } => {
                assert_eq!(url, "https://github.com/rust-lang/rust");
                assert_eq!(rev, Some("1.70.0".to_string()));
            }
            _ => panic!("Expected remote repository"),
        }
    }

    #[test]
    fn test_repository_remote_without_revision() {
        let repo = Repository::remote("https://github.com/rust-lang/rust", Option::<&str>::None);
        match repo {
            Repository::Remote { url, rev } => {
                assert_eq!(url, "https://github.com/rust-lang/rust");
                assert_eq!(rev, None);
            }
            _ => panic!("Expected remote repository"),
        }
    }

    #[test]
    fn test_multiple_stage_parsing() {
        let stages = ["pre-commit", "pre-push", "commit-msg", "manual"];
        let parsed: std::result::Result<Vec<Stage>, _> =
            stages.iter().map(|s| Stage::from_str(s)).collect();
        let parsed = parsed.unwrap();

        assert_eq!(parsed.len(), 4);
        assert!(parsed.contains(&Stage::PreCommit));
        assert!(parsed.contains(&Stage::PrePush));
        assert!(parsed.contains(&Stage::CommitMsg));
        assert!(parsed.contains(&Stage::Manual));
    }

    #[test]
    fn test_file_filter_complex_patterns() {
        let filter = FileFilter::new()
            .with_files_pattern(r"\.(py|rs|js)$")
            .unwrap()
            .with_exclude_pattern(r"(test_|__pycache__|target/)")
            .unwrap();

        assert!(filter.matches("script.py"));
        assert!(filter.matches("main.rs"));
        assert!(filter.matches("app.js"));
        assert!(!filter.matches("test_script.py"));
        assert!(!filter.matches("__pycache__/module.py"));
        assert!(!filter.matches("target/debug/app"));
    }

    #[test]
    fn test_execution_context_with_environment() {
        let mut env = HashMap::new();
        env.insert("RUST_LOG".to_string(), "debug".to_string());
        env.insert("PATH".to_string(), "/usr/bin:/bin".to_string());

        let ctx = ExecutionContext::new(Stage::PreCommit)
            .with_environment(env.clone())
            .with_working_directory(PathBuf::from("/tmp"));

        assert_eq!(ctx.environment, env);
        assert_eq!(ctx.working_directory, PathBuf::from("/tmp"));
    }

    #[test]
    fn test_hook_with_additional_dependencies() {
        let hook = Hook::new("black", "black", "python")
            .with_args(vec!["--check".to_string()])
            .with_types(vec!["python".to_string()]);

        let mut hook_with_deps = hook.clone();
        hook_with_deps.additional_dependencies =
            vec!["click>=8.0.0".to_string(), "toml".to_string()];

        assert_eq!(hook_with_deps.additional_dependencies.len(), 2);
        assert!(hook_with_deps
            .additional_dependencies
            .contains(&"click>=8.0.0".to_string()));
    }

    #[test]
    fn test_hook_multiple_stages() {
        let hook = Hook::new("test", "test", "system").with_stages(vec![
            Stage::PreCommit,
            Stage::PrePush,
            Stage::CommitMsg,
        ]);

        assert!(hook.runs_for_stage(&Stage::PreCommit));
        assert!(hook.runs_for_stage(&Stage::PrePush));
        assert!(hook.runs_for_stage(&Stage::CommitMsg));
        assert!(!hook.runs_for_stage(&Stage::PostCommit));
    }

    #[test]
    fn test_execution_context_file_filtering_multiple_patterns() {
        let hook = Hook::new("multi-pattern", "test", "system")
            .with_files(r"\.(py|rs)$".to_string())
            .with_exclude(r"test_.*".to_string());

        let ctx = ExecutionContext::new(Stage::PreCommit).with_files(vec![
            PathBuf::from("main.py"),
            PathBuf::from("lib.rs"),
            PathBuf::from("test_main.py"),
            PathBuf::from("test_lib.rs"),
            PathBuf::from("script.js"),
        ]);

        let filtered = ctx.filtered_files(&hook).unwrap();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.contains(&PathBuf::from("main.py")));
        assert!(filtered.contains(&PathBuf::from("lib.rs")));
    }

    // Serialization tests (targeting 10+)
    #[test]
    fn test_stage_serialization() {
        let stage = Stage::PreCommit;
        let serialized = serde_json::to_string(&stage).unwrap();
        let deserialized: Stage = serde_json::from_str(&serialized).unwrap();
        assert_eq!(stage, deserialized);
    }

    #[test]
    fn test_repository_serialization() {
        let repo = Repository::remote("https://github.com/test/test", Some("v1.0.0"));
        let serialized = serde_json::to_string(&repo).unwrap();
        let deserialized: Repository = serde_json::from_str(&serialized).unwrap();
        assert_eq!(repo, deserialized);
    }

    #[test]
    fn test_hook_serialization() {
        let hook = Hook::new("test", "echo test", "system")
            .with_name("Test Hook")
            .with_args(vec!["--verbose".to_string()]);

        let serialized = serde_json::to_string(&hook).unwrap();
        let deserialized: Hook = serde_json::from_str(&serialized).unwrap();
        assert_eq!(hook, deserialized);
    }

    #[test]
    fn test_stage_vec_serialization() {
        let stages = vec![Stage::PreCommit, Stage::PrePush, Stage::CommitMsg];
        let serialized = serde_json::to_string(&stages).unwrap();
        let deserialized: Vec<Stage> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(stages, deserialized);
    }

    #[test]
    fn test_repository_local_serialization() {
        let repo = Repository::local("/path/to/local");
        let serialized = serde_json::to_string(&repo).unwrap();
        let deserialized: Repository = serde_json::from_str(&serialized).unwrap();
        assert_eq!(repo, deserialized);
    }

    #[test]
    fn test_repository_meta_serialization() {
        let repo = Repository::meta();
        let serialized = serde_json::to_string(&repo).unwrap();
        let deserialized: Repository = serde_json::from_str(&serialized).unwrap();
        assert_eq!(repo, deserialized);
    }

    #[test]
    fn test_hook_with_optional_fields_serialization() {
        let mut hook = Hook::new("test", "test", "system");
        hook.name = Some("Optional Name".to_string());
        hook.files = Some(r"\.py$".to_string());
        hook.verbose = true;

        let serialized = serde_json::to_string(&hook).unwrap();
        let deserialized: Hook = serde_json::from_str(&serialized).unwrap();
        assert_eq!(hook, deserialized);
    }

    #[test]
    fn test_complex_hook_serialization() {
        let hook = Hook::new("complex", "complex-command", "python")
            .with_name("Complex Hook")
            .with_files(r"\.(py|pyi)$".to_string())
            .with_exclude(r"test_.*".to_string())
            .with_types(vec!["python".to_string(), "pyi".to_string()])
            .with_args(vec!["--arg1".to_string(), "--arg2=value".to_string()])
            .with_stages(vec![Stage::PreCommit, Stage::PrePush])
            .always_run(false)
            .fail_fast(true)
            .pass_filenames(true);

        let serialized = serde_json::to_string(&hook).unwrap();
        let deserialized: Hook = serde_json::from_str(&serialized).unwrap();
        assert_eq!(hook, deserialized);
    }

    #[test]
    fn test_repository_collection_serialization() {
        let repos = vec![
            Repository::remote("https://github.com/test1/repo1", Some("v1.0")),
            Repository::local("/local/repo"),
            Repository::meta(),
        ];

        let serialized = serde_json::to_string(&repos).unwrap();
        let deserialized: Vec<Repository> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(repos, deserialized);
    }

    #[test]
    fn test_hook_collection_serialization() {
        let hooks = vec![
            Hook::new("hook1", "command1", "python"),
            Hook::new("hook2", "command2", "rust").with_name("Named Hook"),
            Hook::new("hook3", "command3", "system").always_run(true),
        ];

        let serialized = serde_json::to_string(&hooks).unwrap();
        let deserialized: Vec<Hook> = serde_json::from_str(&serialized).unwrap();
        assert_eq!(hooks, deserialized);
    }

    // Edge cases and error conditions (targeting 15+)
    #[test]
    fn test_stage_invalid_conversions() {
        let invalid_stages = vec!["invalid", "", "pre-", "commit-", "random-stage"];
        for invalid in invalid_stages {
            assert!(
                Stage::from_str(invalid).is_err(),
                "Stage '{invalid}' should be invalid"
            );
        }
    }

    #[test]
    fn test_repository_validation_edge_cases() {
        // Empty URL
        let repo = Repository::Remote {
            url: "".to_string(),
            rev: None,
        };
        assert!(repo.validate().is_err());

        // Invalid URL format
        let repo = Repository::Remote {
            url: "not-a-url".to_string(),
            rev: None,
        };
        assert!(repo.validate().is_err());

        // Valid SSH URL
        let repo = Repository::Remote {
            url: "git@github.com:user/repo.git".to_string(),
            rev: None,
        };
        assert!(repo.validate().is_ok());
    }

    #[test]
    fn test_file_filter_edge_cases() {
        // Empty patterns should match everything
        let filter = FileFilter::new();
        assert!(filter.matches("any-file.txt"));
        assert!(filter.matches(""));
        assert!(filter.matches("path/to/file.ext"));

        // Only exclude pattern
        let filter = FileFilter::new().with_exclude_pattern(r"\.tmp$").unwrap();
        assert!(filter.matches("file.txt"));
        assert!(!filter.matches("file.tmp"));
    }

    #[test]
    fn test_hook_validation_empty_stages() {
        let mut hook = Hook::new("test", "test", "system");
        hook.stages.clear();
        assert!(hook.validate().is_err());
    }

    #[test]
    fn test_hook_validation_empty_id() {
        let hook = Hook::new("", "test-command", "system");
        assert!(hook.validate().is_err());
    }

    #[test]
    fn test_hook_validation_empty_entry() {
        let hook = Hook::new("test-id", "", "system");
        assert!(hook.validate().is_err());
    }

    #[test]
    fn test_hook_invalid_regex_patterns() {
        // Invalid files pattern
        let result = Hook::new("test", "test", "system")
            .with_files("[unclosed".to_string())
            .validate();
        assert!(result.is_err());

        // Invalid exclude pattern
        let result = Hook::new("test", "test", "system")
            .with_exclude("*invalid*regex".to_string())
            .validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_execution_context_empty_files() {
        let ctx = ExecutionContext::new(Stage::PreCommit);
        let hook = Hook::new("test", "test", "system");

        let filtered = ctx.filtered_files(&hook).unwrap();
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_execution_context_no_matching_files() {
        let hook = Hook::new("python-only", "test", "python").with_files(r"\.py$".to_string());

        let ctx = ExecutionContext::new(Stage::PreCommit).with_files(vec![
            PathBuf::from("test.rs"),
            PathBuf::from("test.js"),
            PathBuf::from("README.md"),
        ]);

        let filtered = ctx.filtered_files(&hook).unwrap();
        assert!(filtered.is_empty());
    }

    #[test]
    fn test_file_filter_complex_exclude_patterns() {
        let filter = FileFilter::new()
            .with_exclude_pattern(r"(\.git/|__pycache__/|\.pytest_cache/|target/debug/)")
            .unwrap();

        assert!(!filter.matches(".git/config"));
        assert!(!filter.matches("__pycache__/module.pyc"));
        assert!(!filter.matches(".pytest_cache/readme.md"));
        assert!(!filter.matches("target/debug/binary"));
        assert!(filter.matches("src/main.rs"));
    }

    #[test]
    fn test_hook_command_with_empty_args() {
        let hook = Hook::new("test", "echo", "system");
        let cmd = hook.command();
        assert_eq!(cmd, vec!["echo"]);
    }

    #[test]
    fn test_hook_command_with_multiple_args() {
        let hook = Hook::new("test", "rustfmt", "rust").with_args(vec![
            "--check".to_string(),
            "--edition".to_string(),
            "2021".to_string(),
        ]);

        let cmd = hook.command();
        assert_eq!(cmd, vec!["rustfmt", "--check", "--edition", "2021"]);
    }

    #[test]
    fn test_repository_identifier_edge_cases() {
        let remote = Repository::remote("", Option::<&str>::None);
        assert_eq!(remote.identifier(), "");

        let local = Repository::local("");
        assert_eq!(local.identifier(), "");

        let meta = Repository::meta();
        assert_eq!(meta.identifier(), "meta");
    }

    #[test]
    fn test_execution_context_builder_defaults() {
        let ctx = ExecutionContext::new(Stage::PreCommit);

        assert_eq!(ctx.stage, Stage::PreCommit);
        assert!(ctx.files.is_empty());
        assert!(!ctx.verbose);
        assert!(!ctx.show_diff_on_failure);
        assert!(ctx.environment.is_empty());
        assert!(ctx.color);
    }

    #[test]
    fn test_stage_all_completeness() {
        let all_stages = Stage::all();

        // Verify we have all expected stages
        assert!(all_stages.contains(&Stage::PreCommit));
        assert!(all_stages.contains(&Stage::PreMergeCommit));
        assert!(all_stages.contains(&Stage::PrePush));
        assert!(all_stages.contains(&Stage::PrepareCommitMsg));
        assert!(all_stages.contains(&Stage::CommitMsg));
        assert!(all_stages.contains(&Stage::PostCommit));
        assert!(all_stages.contains(&Stage::PostCheckout));
        assert!(all_stages.contains(&Stage::PostMerge));
        assert!(all_stages.contains(&Stage::PreRebase));
        assert!(all_stages.contains(&Stage::PostRewrite));
        assert!(all_stages.contains(&Stage::Manual));

        // Verify all stages can round-trip through string conversion
        for stage in &all_stages {
            let stage_str = stage.as_str();
            let parsed = Stage::from_str(stage_str).unwrap();
            assert_eq!(*stage, parsed);
        }
    }

    #[test]
    fn test_hook_file_filter_integration() {
        let hook = Hook::new("integration-test", "test", "system")
            .with_files(r"\.(rs|py)$".to_string())
            .with_exclude(r"test_.*".to_string());

        let filter = hook.file_filter().unwrap();

        // Should match Rust and Python files
        assert!(filter.matches("main.rs"));
        assert!(filter.matches("script.py"));

        // Should not match test files
        assert!(!filter.matches("test_main.rs"));
        assert!(!filter.matches("test_script.py"));

        // Should not match other file types
        assert!(!filter.matches("config.toml"));
        assert!(!filter.matches("README.md"));
    }
}
