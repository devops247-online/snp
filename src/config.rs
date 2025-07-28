// Configuration handling for SNP
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::{ConfigError, Result, SnpError};
use crate::file_change_detector::FileChangeDetectorConfig;
use crate::storage::Store;

#[derive(Debug, Deserialize, Serialize)]
pub struct Config {
    pub repos: Vec<Repository>,
    pub default_install_hook_types: Option<Vec<String>>,
    pub default_language_version: Option<HashMap<String, String>>,
    pub default_stages: Option<Vec<String>>,
    pub files: Option<String>,
    pub exclude: Option<String>,
    pub fail_fast: Option<bool>,
    pub minimum_pre_commit_version: Option<String>,
    pub ci: Option<serde_yaml::Value>,
    pub incremental: Option<IncrementalConfig>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Repository {
    pub repo: String,
    pub rev: Option<String>,
    pub hooks: Vec<Hook>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Hook {
    pub id: String,
    pub name: Option<String>,
    #[serde(default)]
    pub entry: String,
    #[serde(default)]
    pub language: String,
    pub files: Option<String>,
    pub exclude: Option<String>,
    pub types: Option<Vec<String>>,
    pub exclude_types: Option<Vec<String>>,
    pub additional_dependencies: Option<Vec<String>>,
    pub args: Option<Vec<String>>,
    pub always_run: Option<bool>,
    pub fail_fast: Option<bool>,
    pub pass_filenames: Option<bool>,
    pub stages: Option<Vec<String>>,
    pub verbose: Option<bool>,
    pub depends_on: Option<Vec<String>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct IncrementalConfig {
    pub enabled: Option<bool>,
    pub cache_size: Option<usize>,
    pub hash_large_files: Option<bool>,
    pub watch_filesystem: Option<bool>,
    pub force_refresh: Option<bool>,
}

impl IncrementalConfig {
    pub fn to_detector_config(&self) -> FileChangeDetectorConfig {
        FileChangeDetectorConfig::new()
            .with_watch_filesystem(self.watch_filesystem.unwrap_or(true))
            .with_cache_size(self.cache_size.unwrap_or(10000))
            .with_hash_large_files(self.hash_large_files.unwrap_or(false))
            .with_force_refresh(self.force_refresh.unwrap_or(false))
    }
}

impl Config {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        // Validate file path
        if !path.exists() {
            return Err(SnpError::Config(Box::new(ConfigError::NotFound {
                path: path.clone(),
                suggestion: Some(
                    "Create a .pre-commit-config.yaml file in your repository root".to_string(),
                ),
            })));
        }

        if !path.is_file() {
            return Err(SnpError::Config(Box::new(ConfigError::InvalidValue {
                message: "Configuration path is not a file".to_string(),
                field: "config_path".to_string(),
                value: path.display().to_string(),
                expected: "file path".to_string(),
                file_path: Some(path.clone()),
                line: None,
            })));
        }

        let content = std::fs::read_to_string(path).map_err(SnpError::Io)?;

        Self::from_yaml_with_context(&content, Some(path))
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        Self::from_yaml_with_context(yaml, None)
    }

    fn from_yaml_with_context(yaml: &str, _file_path: Option<&PathBuf>) -> Result<Self> {
        let config: Config = serde_yaml::from_str(yaml).map_err(|e| {
            let mut config_error = *Box::<ConfigError>::from(e);
            if let ConfigError::InvalidYaml {
                ref mut file_path, ..
            } = config_error
            {
                *file_path = file_path.clone();
            }
            SnpError::Config(Box::new(config_error))
        })?;

        // Validate the parsed configuration
        config.validate()?;
        Ok(config)
    }

    fn validate(&self) -> Result<()> {
        // Validate that we have at least one repository (unless explicitly empty)
        if self.repos.is_empty() {
            // Empty repos is allowed but log a warning
            tracing::warn!("Configuration contains no repositories");
        }

        // Validate each repository
        for (repo_idx, repo) in self.repos.iter().enumerate() {
            repo.validate(repo_idx)?;
        }

        // Validate global file patterns
        if let Some(ref files_pattern) = self.files {
            if let Err(e) = regex::Regex::new(files_pattern) {
                return Err(SnpError::Config(Box::new(ConfigError::InvalidRegex {
                    pattern: files_pattern.clone(),
                    field: "files".to_string(),
                    error: e.to_string(),
                    file_path: None,
                    line: None,
                })));
            }
        }

        if let Some(ref exclude_pattern) = self.exclude {
            if let Err(e) = regex::Regex::new(exclude_pattern) {
                return Err(SnpError::Config(Box::new(ConfigError::InvalidRegex {
                    pattern: exclude_pattern.clone(),
                    field: "exclude".to_string(),
                    error: e.to_string(),
                    file_path: None,
                    line: None,
                })));
            }
        }

        // Validate global settings
        if let Some(ref min_version) = self.minimum_pre_commit_version {
            if min_version.is_empty() {
                return Err(SnpError::Config(Box::new(ConfigError::MissingField {
                    field: "minimum_pre_commit_version".to_string(),
                    file_path: None,
                    line: None,
                })));
            }
            // Basic semantic version validation
            if !is_valid_version(min_version) {
                return Err(SnpError::Config(Box::new(ConfigError::InvalidValue {
                    message: "Invalid version format".to_string(),
                    field: "minimum_pre_commit_version".to_string(),
                    value: min_version.clone(),
                    expected: "semantic version (e.g., 1.0.0)".to_string(),
                    file_path: None,
                    line: None,
                })));
            }
        }

        // Validate default language versions
        if let Some(ref versions) = self.default_language_version {
            for (language, version) in versions {
                if version.is_empty() {
                    return Err(SnpError::Config(Box::new(ConfigError::InvalidValue {
                        message: format!("Language version for '{language}' cannot be empty"),
                        field: format!("default_language_version.{language}"),
                        value: "".to_string(),
                        expected: "non-empty version string".to_string(),
                        file_path: None,
                        line: None,
                    })));
                }
                if !is_valid_language(language) {
                    tracing::warn!("Unknown language in default_language_version: '{language}'");
                }
            }
        }

        // Validate default stages
        if let Some(ref stages) = self.default_stages {
            for stage in stages {
                if !is_valid_stage(stage) {
                    return Err(SnpError::Config(Box::new(ConfigError::InvalidValue {
                        message: format!("Invalid stage in default_stages: '{stage}'"),
                        field: "default_stages".to_string(),
                        value: stage.clone(),
                        expected: "valid git hook stage (pre-commit, pre-push, etc.)".to_string(),
                        file_path: None,
                        line: None,
                    })));
                }
            }
        }

        // Validate default install hook types
        if let Some(ref hook_types) = self.default_install_hook_types {
            for hook_type in hook_types {
                if !is_valid_hook_type(hook_type) {
                    return Err(SnpError::Config(Box::new(ConfigError::InvalidValue {
                        message: format!(
                            "Invalid hook type in default_install_hook_types: '{hook_type}'"
                        ),
                        field: "default_install_hook_types".to_string(),
                        value: hook_type.clone(),
                        expected: "valid git hook type (pre-commit, pre-push, etc.)".to_string(),
                        file_path: None,
                        line: None,
                    })));
                }
            }
        }

        Ok(())
    }

    /// Load configuration with external repository hooks resolved
    pub async fn from_file_with_resolved_hooks(path: &PathBuf) -> Result<Self> {
        let mut config = Self::from_file(path)?;
        config.resolve_external_hooks().await?;
        Ok(config)
    }

    /// Resolve external repository hooks by fetching their definitions
    async fn resolve_external_hooks(&mut self) -> Result<()> {
        tracing::debug!("Starting external hook resolution");
        let store = Arc::new(Store::new().map_err(|e| {
            tracing::error!("Failed to create store: {}", e);
            e
        })?);

        // Collect external repositories (skip local and meta)
        let external_repos: Vec<(usize, &Repository)> = self
            .repos
            .iter()
            .enumerate()
            .filter(|(_, repo)| repo.repo != "local" && repo.repo != "meta")
            .collect();

        if external_repos.is_empty() {
            tracing::debug!("No external repositories to process");
            return Ok(());
        }

        tracing::debug!(
            "Processing {} external repositories in parallel",
            external_repos.len()
        );

        // Clone all repositories in parallel
        let clone_tasks: Vec<_> = external_repos
            .iter()
            .map(|(i, repo)| {
                let store = Arc::clone(&store);
                let repo_url = repo.repo.clone();
                let repo_rev = repo.rev.as_deref().unwrap_or("HEAD").to_string();
                let repo_index = *i;

                tokio::spawn(async move {
                    tracing::debug!("Processing repository {}: {}", repo_index, repo_url);

                    // Clone/fetch the external repository
                    tracing::debug!("Cloning repository: {}", repo_url);
                    let repo_path = store
                        .clone_repository(&repo_url, &repo_rev, &[])
                        .await
                        .map_err(|e| {
                            tracing::error!("Failed to clone repository {}: {}", repo_url, e);
                            e
                        })?;

                    tracing::debug!("Repository cloned to: {:?}", repo_path);

                    // Load hook definitions from the repository
                    tracing::debug!("Loading hook definitions from: {:?}", repo_path);
                    let hook_definitions = Config::load_repository_hook_definitions_static(
                        &repo_path,
                    )
                    .map_err(|e| {
                        tracing::error!(
                            "Failed to load hook definitions from {:?}: {}",
                            repo_path,
                            e
                        );
                        e
                    })?;

                    tracing::debug!("Loaded {} hook definitions", hook_definitions.len());

                    Ok::<(usize, HashMap<String, Hook>), crate::error::SnpError>((
                        repo_index,
                        hook_definitions,
                    ))
                })
            })
            .collect();

        // Wait for all clone operations to complete
        let clone_results = futures::future::try_join_all(clone_tasks)
            .await
            .map_err(|e| {
                crate::error::SnpError::Storage(Box::new(
                    crate::error::StorageError::ConcurrencyConflict {
                        operation: "parallel_repository_cloning".to_string(),
                        error: format!("Task join error: {e}"),
                        retry_suggested: true,
                    },
                ))
            })?;

        // Process results and merge hook definitions
        for clone_result in clone_results {
            let (repo_index, hook_definitions) = clone_result?;

            // Merge user config with repository definitions
            for user_hook in &mut self.repos[repo_index].hooks {
                if let Some(repo_hook_def) = hook_definitions.get(&user_hook.id) {
                    tracing::debug!("Merging definition for hook: {}", user_hook.id);
                    // Merge repository definition with user overrides
                    Config::merge_hook_definition_static(user_hook, repo_hook_def);
                }
            }
        }

        tracing::debug!("External hook resolution completed");
        Ok(())
    }

    /// Load hook definitions from a repository's .pre-commit-hooks.yaml file (static version)
    fn load_repository_hook_definitions_static(repo_path: &Path) -> Result<HashMap<String, Hook>> {
        let hooks_file = repo_path.join(".pre-commit-hooks.yaml");

        if !hooks_file.exists() {
            return Err(SnpError::Config(Box::new(ConfigError::NotFound {
                path: hooks_file,
                suggestion: Some(
                    "Repository must contain a .pre-commit-hooks.yaml file".to_string(),
                ),
            })));
        }

        let hooks_content = std::fs::read_to_string(&hooks_file)?;
        let hook_definitions: Vec<serde_yaml::Value> = serde_yaml::from_str(&hooks_content)
            .map_err(|e| SnpError::Config(Box::<ConfigError>::from(e)))?;

        let mut hooks_map = HashMap::new();

        for hook_def in hook_definitions {
            let hook_id = hook_def.get("id").and_then(|v| v.as_str()).ok_or_else(|| {
                SnpError::Config(Box::new(ConfigError::ValidationFailed {
                    message: "Hook missing required 'id' field".to_string(),
                    file_path: Some(hooks_file.clone()),
                    errors: vec![],
                }))
            })?;

            let entry = hook_def
                .get("entry")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    SnpError::Config(Box::new(ConfigError::ValidationFailed {
                        message: format!("Hook '{hook_id}' missing required 'entry' field"),
                        file_path: Some(hooks_file.clone()),
                        errors: vec![],
                    }))
                })?;

            let language = hook_def
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("system");

            let hook = Hook {
                id: hook_id.to_string(),
                name: hook_def
                    .get("name")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                entry: entry.to_string(),
                language: language.to_string(),
                files: hook_def
                    .get("files")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                exclude: hook_def
                    .get("exclude")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                types: hook_def
                    .get("types")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    }),
                exclude_types: hook_def
                    .get("exclude_types")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    }),
                additional_dependencies: hook_def
                    .get("additional_dependencies")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    }),
                args: hook_def
                    .get("args")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    }),
                always_run: hook_def.get("always_run").and_then(|v| v.as_bool()),
                fail_fast: hook_def.get("fail_fast").and_then(|v| v.as_bool()),
                pass_filenames: hook_def.get("pass_filenames").and_then(|v| v.as_bool()),
                stages: hook_def
                    .get("stages")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    }),
                verbose: hook_def.get("verbose").and_then(|v| v.as_bool()),
                depends_on: hook_def
                    .get("depends_on")
                    .and_then(|v| v.as_sequence())
                    .map(|seq| {
                        seq.iter()
                            .filter_map(|v| v.as_str())
                            .map(|s| s.to_string())
                            .collect()
                    }),
            };

            hooks_map.insert(hook_id.to_string(), hook);
        }

        Ok(hooks_map)
    }

    /// Merge repository hook definition with user configuration (static version)
    fn merge_hook_definition_static(user_hook: &mut Hook, repo_hook: &Hook) {
        // Always use repository's entry if user doesn't have one
        if user_hook.entry.is_empty() {
            user_hook.entry = repo_hook.entry.clone();
        }

        // Always use repository's language if user doesn't have one
        if user_hook.language.is_empty() {
            user_hook.language = repo_hook.language.clone();
        }

        // Use repository defaults for missing optional fields, but allow user overrides
        if user_hook.name.is_none() && repo_hook.name.is_some() {
            user_hook.name = repo_hook.name.clone();
        }

        if user_hook.files.is_none() && repo_hook.files.is_some() {
            user_hook.files = repo_hook.files.clone();
        }

        if user_hook.exclude.is_none() && repo_hook.exclude.is_some() {
            user_hook.exclude = repo_hook.exclude.clone();
        }

        if user_hook.types.is_none() && repo_hook.types.is_some() {
            user_hook.types = repo_hook.types.clone();
        }

        if user_hook.exclude_types.is_none() && repo_hook.exclude_types.is_some() {
            user_hook.exclude_types = repo_hook.exclude_types.clone();
        }

        if user_hook.additional_dependencies.is_none()
            && repo_hook.additional_dependencies.is_some()
        {
            user_hook.additional_dependencies = repo_hook.additional_dependencies.clone();
        }

        if user_hook.always_run.is_none() && repo_hook.always_run.is_some() {
            user_hook.always_run = repo_hook.always_run;
        }

        if user_hook.fail_fast.is_none() && repo_hook.fail_fast.is_some() {
            user_hook.fail_fast = repo_hook.fail_fast;
        }

        if user_hook.pass_filenames.is_none() && repo_hook.pass_filenames.is_some() {
            user_hook.pass_filenames = repo_hook.pass_filenames;
        }

        if user_hook.stages.is_none() && repo_hook.stages.is_some() {
            user_hook.stages = repo_hook.stages.clone();
        }

        if user_hook.verbose.is_none() && repo_hook.verbose.is_some() {
            user_hook.verbose = repo_hook.verbose;
        }

        // For args, merge repository args with user args (user args take precedence)
        if user_hook.args.is_none() && repo_hook.args.is_some() {
            user_hook.args = repo_hook.args.clone();
        }
    }
}

impl Repository {
    fn validate(&self, repo_idx: usize) -> Result<()> {
        // Validate repository URL/type
        if self.repo.is_empty() {
            return Err(SnpError::Config(Box::new(ConfigError::MissingField {
                field: format!("repos[{repo_idx}].repo"),
                file_path: None,
                line: None,
            })));
        }

        // Validate revision for non-local/meta repos
        if !matches!(self.repo.as_str(), "local" | "meta") && self.rev.is_none() {
            return Err(SnpError::Config(Box::new(ConfigError::MissingField {
                field: format!("repos[{repo_idx}].rev"),
                file_path: None,
                line: None,
            })));
        }

        // Validate hooks
        if self.hooks.is_empty() {
            return Err(SnpError::Config(Box::new(ConfigError::ValidationFailed {
                message: format!("Repository {repo_idx} has no hooks defined"),
                file_path: None,
                errors: vec![format!(
                    "Repository at index {} must define at least one hook",
                    repo_idx
                )],
            })));
        }

        for (hook_idx, hook) in self.hooks.iter().enumerate() {
            hook.validate(repo_idx, hook_idx)?;
        }

        Ok(())
    }
}

impl Hook {
    fn validate(&self, repo_idx: usize, hook_idx: usize) -> Result<()> {
        // Validate hook ID
        if self.id.is_empty() {
            return Err(SnpError::Config(Box::new(ConfigError::MissingField {
                field: format!("repos[{repo_idx}].hooks[{hook_idx}].id"),
                file_path: None,
                line: None,
            })));
        }

        // Validate file patterns are valid regex if provided
        if let Some(ref files_pattern) = self.files {
            if let Err(e) = regex::Regex::new(files_pattern) {
                return Err(SnpError::Config(Box::new(ConfigError::InvalidRegex {
                    pattern: files_pattern.clone(),
                    field: format!("repos[{repo_idx}].hooks[{hook_idx}].files"),
                    error: e.to_string(),
                    file_path: None,
                    line: None,
                })));
            }
        }

        if let Some(ref exclude_pattern) = self.exclude {
            if let Err(e) = regex::Regex::new(exclude_pattern) {
                return Err(SnpError::Config(Box::new(ConfigError::InvalidRegex {
                    pattern: exclude_pattern.clone(),
                    field: format!("repos[{repo_idx}].hooks[{hook_idx}].exclude"),
                    error: e.to_string(),
                    file_path: None,
                    line: None,
                })));
            }
        }

        // Validate language is known
        if !self.language.is_empty() && !is_valid_language(&self.language) {
            tracing::warn!(
                "Hook '{}' uses unknown language: '{}'",
                self.id,
                self.language
            );
        }

        // Validate stages if provided
        if let Some(ref stages) = self.stages {
            for stage in stages {
                if !is_valid_stage(stage) {
                    return Err(SnpError::Config(Box::new(ConfigError::InvalidValue {
                        message: format!("Hook '{}' has invalid stage: '{}'", self.id, stage),
                        field: "stages".to_string(),
                        value: stage.clone(),
                        expected: "valid git hook stage (pre-commit, pre-push, etc.)".to_string(),
                        file_path: None,
                        line: None,
                    })));
                }
            }
        }

        // Validate types if provided
        if let Some(ref types) = self.types {
            for type_name in types {
                if !is_valid_file_type(type_name) {
                    tracing::warn!("Hook '{}' uses unknown file type: '{type_name}'", self.id);
                }
            }
        }

        if let Some(ref exclude_types) = self.exclude_types {
            for type_name in exclude_types {
                if !is_valid_file_type(type_name) {
                    tracing::warn!(
                        "Hook '{}' uses unknown exclude file type: '{type_name}'",
                        self.id
                    );
                }
            }
        }

        Ok(())
    }
}

// Helper function to validate languages
fn is_valid_language(language: &str) -> bool {
    matches!(
        language,
        "python"
            | "node"
            | "ruby"
            | "rust"
            | "go"
            | "java"
            | "swift"
            | "system"
            | "script"
            | "docker"
            | "docker_image"
            | "conda"
            | "coursier"
            | "dart"
            | "dotnet"
            | "fail"
            | "haskell"
            | "julia"
            | "lua"
            | "perl"
            | "pygrep"
            | "r"
            | "" // empty string is valid (default)
    )
}

// Helper function to validate stages
fn is_valid_stage(stage: &str) -> bool {
    matches!(
        stage,
        "commit"
            | "merge-commit"
            | "prepare-commit-msg"
            | "commit-msg"
            | "post-commit"
            | "manual"
            | "post-checkout"
            | "post-merge"
            | "pre-commit"
            | "pre-merge-commit"
            | "pre-push"
            | "pre-rebase"
            | "push"
            | "post-rewrite"
    )
}

// Helper function to validate hook types
fn is_valid_hook_type(hook_type: &str) -> bool {
    matches!(
        hook_type,
        "pre-commit"
            | "pre-merge-commit"
            | "pre-push"
            | "prepare-commit-msg"
            | "commit-msg"
            | "post-commit"
            | "post-checkout"
            | "post-merge"
            | "pre-rebase"
            | "post-rewrite"
    )
}

// Helper function to validate file types
fn is_valid_file_type(file_type: &str) -> bool {
    matches!(
        file_type,
        "file"
            | "directory"
            | "symlink"
            | "executable"
            | "text"
            | "binary"
            | "python"
            | "pyi"
            | "javascript"
            | "typescript"
            | "jsx"
            | "tsx"
            | "json"
            | "yaml"
            | "toml"
            | "xml"
            | "html"
            | "css"
            | "scss"
            | "sass"
            | "markdown"
            | "rst"
            | "dockerfile"
            | "makefile"
            | "shell"
            | "bash"
            | "zsh"
            | "fish"
            | "powershell"
            | "batch"
            | "ruby"
            | "perl"
            | "php"
            | "java"
            | "kotlin"
            | "scala"
            | "groovy"
            | "clojure"
            | "rust"
            | "go"
            | "c"
            | "cpp"
            | "cxx"
            | "cc"
            | "h"
            | "hpp"
            | "hxx"
            | "objective-c"
            | "swift"
            | "dart"
            | "lua"
            | "r"
            | "julia"
            | "haskell"
            | "erlang"
            | "elixir"
            | "elm"
            | "fsharp"
            | "csharp"
            | "vb"
            | "sql"
            | "image"
            | "video"
            | "audio"
            | "archive"
            | "compressed"
            | "font"
            | "csv"
            | "tsv"
            | "log"
            | "ini"
            | "cfg"
            | "conf"
            | "properties"
            | "env"
    )
}

// Helper function to validate version strings
fn is_valid_version(version: &str) -> bool {
    // Basic semantic version validation (major.minor.patch)
    let version_regex =
        regex::Regex::new(r"^\d+(\.\d+)*(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$").unwrap();
    version_regex.is_match(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic_config() {
        let yaml = r#"
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    entry: black
    language: python
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.repos.len(), 1);
        assert_eq!(config.repos[0].repo, "https://github.com/psf/black");
        assert_eq!(config.repos[0].hooks.len(), 1);
        assert_eq!(config.repos[0].hooks[0].id, "black");
    }

    #[test]
    fn test_parse_invalid_yaml() {
        let yaml = "invalid: yaml: content:";
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_hook_defaults() {
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let hook = &config.repos[0].hooks[0];

        // Test that optional fields can be None
        assert!(hook.name.is_none());
        assert!(hook.always_run.is_none());
        assert!(hook.pass_filenames.is_none());
    }

    #[test]
    fn test_parse_complex_config() {
        let yaml = r#"
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    entry: black --check --diff
    language: python
    types: [python]
    args: [--line-length=88]
    exclude: ^tests/
    additional_dependencies: [click>=8.0.0]
- repo: https://github.com/pycqa/flake8
  rev: 6.0.0
  hooks:
  - id: flake8
    entry: flake8
    language: python
    types: [python]
    always_run: false
    pass_filenames: true
    fail_fast: true
    verbose: true
default_install_hook_types: [pre-commit, pre-push]
default_language_version:
  python: python3.9
  node: 18.16.0
default_stages: [commit, push]
files: ^src/
exclude: ^(tests/|docs/)
fail_fast: true
minimum_pre_commit_version: 3.0.0
"#;
        let config = Config::from_yaml(yaml).unwrap();

        // Test repos parsing
        assert_eq!(config.repos.len(), 2);
        assert_eq!(config.repos[0].repo, "https://github.com/psf/black");
        assert_eq!(config.repos[1].repo, "https://github.com/pycqa/flake8");

        // Test hook details
        let black_hook = &config.repos[0].hooks[0];
        assert_eq!(black_hook.id, "black");
        assert_eq!(black_hook.entry, "black --check --diff");
        assert_eq!(black_hook.language, "python");
        assert_eq!(black_hook.types.as_ref().unwrap()[0], "python");
        assert_eq!(black_hook.args.as_ref().unwrap()[0], "--line-length=88");
        assert_eq!(black_hook.exclude.as_ref().unwrap(), "^tests/");

        // Test global settings
        assert_eq!(config.default_install_hook_types.as_ref().unwrap().len(), 2);
        assert_eq!(
            config
                .default_language_version
                .as_ref()
                .unwrap()
                .get("python")
                .unwrap(),
            "python3.9"
        );
        assert_eq!(config.files.as_ref().unwrap(), "^src/");
        assert_eq!(config.exclude.as_ref().unwrap(), "^(tests/|docs/)");
        assert!(config.fail_fast.unwrap());
        assert_eq!(config.minimum_pre_commit_version.as_ref().unwrap(), "3.0.0");
    }

    #[test]
    fn test_parse_local_repo() {
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: local-script
    name: Run local script
    entry: ./scripts/check.sh
    language: system
    files: \\.sh$
    pass_filenames: false
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.repos[0].repo, "local");
        assert!(config.repos[0].rev.is_none());

        let hook = &config.repos[0].hooks[0];
        assert_eq!(hook.id, "local-script");
        assert_eq!(hook.name.as_ref().unwrap(), "Run local script");
        assert_eq!(hook.entry, "./scripts/check.sh");
        assert_eq!(hook.language, "system");
        assert!(!hook.pass_filenames.unwrap());
    }

    #[test]
    fn test_parse_meta_repo() {
        let yaml = r#"
repos:
- repo: meta
  hooks:
  - id: check-hooks-apply
  - id: check-useless-excludes
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.repos[0].repo, "meta");
        assert!(config.repos[0].rev.is_none());
        assert_eq!(config.repos[0].hooks.len(), 2);
        assert_eq!(config.repos[0].hooks[0].id, "check-hooks-apply");
        assert_eq!(config.repos[0].hooks[1].id, "check-useless-excludes");
    }

    #[test]
    fn test_parse_empty_config() {
        let yaml = r#"
repos: []
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.repos.len(), 0);
    }

    #[test]
    fn test_parse_minimal_config() {
        let yaml = r#"
repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.4.0
  hooks:
  - id: trailing-whitespace
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert_eq!(config.repos.len(), 1);
        assert_eq!(config.repos[0].hooks.len(), 1);
        assert_eq!(config.repos[0].hooks[0].id, "trailing-whitespace");

        // Test that minimal hook only has required fields
        let hook = &config.repos[0].hooks[0];
        assert!(hook.entry.is_empty()); // entry is required but can be empty
        assert!(hook.language.is_empty()); // language is required but can be empty
    }

    #[test]
    fn test_parse_missing_required_fields() {
        // Missing repos field
        let yaml = r#"
default_stages: [commit]
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());

        // Missing hook id
        let yaml = r#"
repos:
- repo: https://github.com/test/test
  rev: v1.0.0
  hooks:
  - entry: test
    language: system
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_invalid_yaml_syntax() {
        let invalid_yamls = vec![
            "repos: [unclosed",
            "repos:\n  - invalid: :",
            "repos:\n- repo:\n    invalid_indent",
            "invalid: yaml: structure: bad",
        ];

        for yaml in invalid_yamls {
            let result = Config::from_yaml(yaml);
            assert!(result.is_err(), "Expected error for YAML: {yaml}");
        }
    }

    #[test]
    fn test_parse_config_with_ci_integration() {
        let yaml = r#"
repos:
- repo: https://github.com/psf/black
  rev: 23.1.0
  hooks:
  - id: black
    entry: black
    language: python
ci:
  autofix_commit_msg: 'style: auto fixes from pre-commit.ci'
  autofix_prs: true
  autoupdate_branch: ''
  autoupdate_commit_msg: 'chore: pre-commit autoupdate'
  autoupdate_schedule: weekly
  skip: []
  submodules: false
"#;
        let config = Config::from_yaml(yaml).unwrap();
        assert!(config.ci.is_some());
        // CI field is stored as generic Value for flexibility
    }

    #[test]
    fn test_hook_types_and_exclude_types() {
        let yaml = r#"
repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.4.0
  hooks:
  - id: check-added-large-files
    entry: check-added-large-files
    language: system
    types: [file]
    exclude_types: [image, video]
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let hook = &config.repos[0].hooks[0];

        assert_eq!(hook.types.as_ref().unwrap()[0], "file");
        assert_eq!(hook.exclude_types.as_ref().unwrap().len(), 2);
        assert_eq!(hook.exclude_types.as_ref().unwrap()[0], "image");
        assert_eq!(hook.exclude_types.as_ref().unwrap()[1], "video");
    }

    #[test]
    fn test_hook_stages() {
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
    stages: [pre-commit, pre-push, commit-msg]
"#;
        let config = Config::from_yaml(yaml).unwrap();
        let hook = &config.repos[0].hooks[0];

        let stages = hook.stages.as_ref().unwrap();
        assert_eq!(stages.len(), 3);
        assert!(stages.contains(&"pre-commit".to_string()));
        assert!(stages.contains(&"pre-push".to_string()));
        assert!(stages.contains(&"commit-msg".to_string()));
    }

    #[test]
    fn test_validation_invalid_regex_patterns() {
        // Invalid files pattern in global config
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
files: "[unclosed"
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(
            error_str.contains("Invalid regex pattern")
                || error_str.contains("invalid regex pattern")
        );

        // Invalid exclude pattern in hook
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
    exclude: "*invalid*regex"
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(
            error_str.contains("Invalid regex pattern")
                || error_str.contains("invalid exclude regex pattern")
        );
    }

    #[test]
    fn test_validation_invalid_versions() {
        let yaml = r#"
repos:
- repo: https://github.com/test/test
  rev: v1.0.0
  hooks:
  - id: test-hook
    entry: test
    language: python
minimum_pre_commit_version: "not.a.version.format!"
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
        let error = result.unwrap_err();
        let error_str = error.to_string();
        assert!(
            error_str.contains("Invalid version format")
                || error_str.contains("not a valid version format")
        );
    }

    #[test]
    fn test_validation_invalid_stages() {
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
    stages: [pre-commit, invalid-stage]
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("invalid stage"));
    }

    #[test]
    fn test_validation_invalid_hook_types() {
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
default_install_hook_types: [pre-commit, invalid-hook-type]
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Invalid hook type"));
    }

    #[test]
    fn test_validation_empty_language_version() {
        let yaml = r#"
repos:
- repo: local
  hooks:
  - id: test-hook
    entry: test
    language: system
default_language_version:
  python: ""
"#;
        let result = Config::from_yaml(yaml);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cannot be empty"));
    }

    #[test]
    fn test_validation_valid_version_formats() {
        let valid_versions = vec![
            "1.0.0",
            "1.2.3-alpha",
            "2.0.0-beta.1",
            "1.0.0+build.1",
            "10.2.1-rc.1+build.123",
        ];

        for version in valid_versions {
            let yaml = format!(
                r#"
repos:
- repo: https://github.com/test/test
  rev: v1.0.0
  hooks:
  - id: test-hook
    entry: test
    language: python
minimum_pre_commit_version: "{version}"
"#
            );
            let result = Config::from_yaml(&yaml);
            assert!(result.is_ok(), "Version {version} should be valid");
        }
    }
}
