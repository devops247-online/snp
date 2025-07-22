// Configuration handling for SNP
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

use crate::error::{Result, SnpError};

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
}

impl Config {
    pub fn from_file(path: &PathBuf) -> Result<Self> {
        // Validate file path
        if !path.exists() {
            return Err(SnpError::Config(format!("Configuration file not found: {}", path.display())));
        }
        
        if !path.is_file() {
            return Err(SnpError::Config(format!("Configuration path is not a file: {}", path.display())));
        }

        let content = std::fs::read_to_string(path)
            .map_err(|e| SnpError::Config(format!("Failed to read config file {}: {e}", path.display())))?;
        
        Self::from_yaml_with_context(&content, Some(path))
    }

    pub fn from_yaml(yaml: &str) -> Result<Self> {
        Self::from_yaml_with_context(yaml, None)
    }

    fn from_yaml_with_context(yaml: &str, file_path: Option<&PathBuf>) -> Result<Self> {
        let config: Config = serde_yaml::from_str(yaml)
            .map_err(|e| {
                let context = if let Some(path) = file_path {
                    format!(" in file {}", path.display())
                } else {
                    String::new()
                };
                SnpError::Config(format!("YAML parsing error{}: {}", context, e))
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
            if let Err(_) = regex::Regex::new(files_pattern) {
                return Err(SnpError::Config(format!(
                    "Global 'files' has invalid regex pattern: '{}'",
                    files_pattern
                )));
            }
        }

        if let Some(ref exclude_pattern) = self.exclude {
            if let Err(_) = regex::Regex::new(exclude_pattern) {
                return Err(SnpError::Config(format!(
                    "Global 'exclude' has invalid regex pattern: '{}'",
                    exclude_pattern
                )));
            }
        }

        // Validate global settings
        if let Some(ref min_version) = self.minimum_pre_commit_version {
            if min_version.is_empty() {
                return Err(SnpError::Config("minimum_pre_commit_version cannot be empty".to_string()));
            }
            // Basic semantic version validation
            if !is_valid_version(min_version) {
                return Err(SnpError::Config(format!(
                    "minimum_pre_commit_version '{}' is not a valid version format",
                    min_version
                )));
            }
        }

        // Validate default language versions
        if let Some(ref versions) = self.default_language_version {
            for (language, version) in versions {
                if version.is_empty() {
                    return Err(SnpError::Config(format!("Language version for '{}' cannot be empty", language)));
                }
                if !is_valid_language(language) {
                    tracing::warn!("Unknown language in default_language_version: '{}'", language);
                }
            }
        }

        // Validate default stages
        if let Some(ref stages) = self.default_stages {
            for stage in stages {
                if !is_valid_stage(stage) {
                    return Err(SnpError::Config(format!(
                        "Invalid stage in default_stages: '{}'",
                        stage
                    )));
                }
            }
        }

        // Validate default install hook types
        if let Some(ref hook_types) = self.default_install_hook_types {
            for hook_type in hook_types {
                if !is_valid_hook_type(hook_type) {
                    return Err(SnpError::Config(format!(
                        "Invalid hook type in default_install_hook_types: '{}'",
                        hook_type
                    )));
                }
            }
        }

        Ok(())
    }
}

impl Repository {
    fn validate(&self, repo_idx: usize) -> Result<()> {
        // Validate repository URL/type
        if self.repo.is_empty() {
            return Err(SnpError::Config(format!("Repository {} has empty repo field", repo_idx)));
        }

        // Validate revision for non-local/meta repos
        if !matches!(self.repo.as_str(), "local" | "meta") && self.rev.is_none() {
            return Err(SnpError::Config(format!(
                "Repository {} '{}' is missing required 'rev' field",
                repo_idx, self.repo
            )));
        }

        // Validate hooks
        if self.hooks.is_empty() {
            return Err(SnpError::Config(format!("Repository {} has no hooks defined", repo_idx)));
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
            return Err(SnpError::Config(format!(
                "Hook {} in repository {} has empty id",
                hook_idx, repo_idx
            )));
        }

        // Validate file patterns are valid regex if provided
        if let Some(ref files_pattern) = self.files {
            if let Err(_) = regex::Regex::new(files_pattern) {
                return Err(SnpError::Config(format!(
                    "Hook '{}' has invalid files regex pattern: '{}'",
                    self.id, files_pattern
                )));
            }
        }

        if let Some(ref exclude_pattern) = self.exclude {
            if let Err(_) = regex::Regex::new(exclude_pattern) {
                return Err(SnpError::Config(format!(
                    "Hook '{}' has invalid exclude regex pattern: '{}'",
                    self.id, exclude_pattern
                )));
            }
        }

        // Validate language is known
        if !self.language.is_empty() && !is_valid_language(&self.language) {
            tracing::warn!("Hook '{}' uses unknown language: '{}'", self.id, self.language);
        }

        // Validate stages if provided
        if let Some(ref stages) = self.stages {
            for stage in stages {
                if !is_valid_stage(stage) {
                    return Err(SnpError::Config(format!(
                        "Hook '{}' has invalid stage: '{}'",
                        self.id, stage
                    )));
                }
            }
        }

        // Validate types if provided
        if let Some(ref types) = self.types {
            for type_name in types {
                if !is_valid_file_type(type_name) {
                    tracing::warn!("Hook '{}' uses unknown file type: '{}'", self.id, type_name);
                }
            }
        }

        if let Some(ref exclude_types) = self.exclude_types {
            for type_name in exclude_types {
                if !is_valid_file_type(type_name) {
                    tracing::warn!("Hook '{}' uses unknown exclude file type: '{}'", self.id, type_name);
                }
            }
        }

        Ok(())
    }
}

// Helper function to validate languages
fn is_valid_language(language: &str) -> bool {
    matches!(language, 
        "python" | "node" | "ruby" | "rust" | "go" | "java" | "swift" | 
        "system" | "script" | "docker" | "docker_image" | "conda" | 
        "coursier" | "dart" | "dotnet" | "fail" | "haskell" | "julia" | 
        "lua" | "perl" | "pygrep" | "r" | "" // empty string is valid (default)
    )
}

// Helper function to validate stages
fn is_valid_stage(stage: &str) -> bool {
    matches!(stage,
        "commit" | "merge-commit" | "prepare-commit-msg" | "commit-msg" |
        "post-commit" | "manual" | "post-checkout" | "post-merge" |
        "pre-commit" | "pre-merge-commit" | "pre-push" | "pre-rebase" |
        "push" | "post-rewrite"
    )
}

// Helper function to validate hook types
fn is_valid_hook_type(hook_type: &str) -> bool {
    matches!(hook_type,
        "pre-commit" | "pre-merge-commit" | "pre-push" | "prepare-commit-msg" |
        "commit-msg" | "post-commit" | "post-checkout" | "post-merge" |
        "pre-rebase" | "post-rewrite"
    )
}

// Helper function to validate file types
fn is_valid_file_type(file_type: &str) -> bool {
    matches!(file_type,
        "file" | "directory" | "symlink" | "executable" | "text" | "binary" |
        "python" | "pyi" | "javascript" | "typescript" | "jsx" | "tsx" |
        "json" | "yaml" | "toml" | "xml" | "html" | "css" | "scss" | "sass" |
        "markdown" | "rst" | "dockerfile" | "makefile" | "shell" | "bash" |
        "zsh" | "fish" | "powershell" | "batch" | "ruby" | "perl" | "php" |
        "java" | "kotlin" | "scala" | "groovy" | "clojure" | "rust" | "go" |
        "c" | "cpp" | "cxx" | "cc" | "h" | "hpp" | "hxx" | "objective-c" |
        "swift" | "dart" | "lua" | "r" | "julia" | "haskell" | "erlang" |
        "elixir" | "elm" | "fsharp" | "csharp" | "vb" | "sql" | "image" |
        "video" | "audio" | "archive" | "compressed" | "font" | "csv" |
        "tsv" | "log" | "ini" | "cfg" | "conf" | "properties" | "env"
    )
}

// Helper function to validate version strings
fn is_valid_version(version: &str) -> bool {
    // Basic semantic version validation (major.minor.patch)
    let version_regex = regex::Regex::new(r"^\d+(\.\d+)*(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$").unwrap();
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
        assert_eq!(config.default_language_version.as_ref().unwrap().get("python").unwrap(), "python3.9");
        assert_eq!(config.files.as_ref().unwrap(), "^src/");
        assert_eq!(config.exclude.as_ref().unwrap(), "^(tests/|docs/)");
        assert_eq!(config.fail_fast.unwrap(), true);
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
        assert_eq!(hook.pass_filenames.unwrap(), false);
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
            assert!(result.is_err(), "Expected error for YAML: {}", yaml);
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
        assert!(result.unwrap_err().to_string().contains("invalid regex pattern"));

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
        assert!(result.unwrap_err().to_string().contains("invalid exclude regex pattern"));
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
        assert!(result.unwrap_err().to_string().contains("not a valid version format"));
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
        assert!(result.unwrap_err().to_string().contains("Invalid hook type"));
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
            let yaml = format!(r#"
repos:
- repo: https://github.com/test/test
  rev: v1.0.0
  hooks:
  - id: test-hook
    entry: test
    language: python
minimum_pre_commit_version: "{}"
"#, version);
            let result = Config::from_yaml(&yaml);
            assert!(result.is_ok(), "Version {} should be valid", version);
        }
    }
}
