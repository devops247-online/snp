// Schema validation for SNP configuration files
use crate::config::{Config, Hook, Repository};
use crate::error::{ConfigError, Result, SnpError};
use ahash::{HashMap, HashMapExt, HashSet, HashSetExt};
use once_cell::sync::Lazy;
use regex::Regex;
use std::path::Path;
use std::time::Duration;

/// Schema validation configuration
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    pub strict_mode: bool,
    pub allow_deprecated: bool,
    pub check_performance_impact: bool,
    pub validate_file_existence: bool,
    pub max_hooks_per_repo: Option<usize>,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            strict_mode: false,
            allow_deprecated: true,
            check_performance_impact: true,
            validate_file_existence: false,
            max_hooks_per_repo: Some(100),
        }
    }
}

/// Validation result with detailed errors and warnings
#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<ValidationError>,
    pub warnings: Vec<ValidationWarning>,
    pub performance_metrics: PerformanceMetrics,
}

impl Default for ValidationResult {
    fn default() -> Self {
        Self::new()
    }
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
            performance_metrics: PerformanceMetrics::default(),
        }
    }

    pub fn add_error(&mut self, error: ValidationError) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: ValidationWarning) {
        self.warnings.push(warning);
    }

    pub fn merge(&mut self, other: ValidationResult) {
        if !other.is_valid {
            self.is_valid = false;
        }
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
        self.performance_metrics.merge(other.performance_metrics);
    }
}

/// Detailed validation errors with context
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub error_type: ValidationErrorType,
    pub message: String,
    pub field_path: String,
    pub line_number: Option<u32>,
    pub column_number: Option<u32>,
    pub suggestion: Option<String>,
    pub severity: ErrorSeverity,
}

impl ValidationError {
    pub fn new(
        error_type: ValidationErrorType,
        message: impl Into<String>,
        field_path: impl Into<String>,
    ) -> Self {
        Self {
            error_type,
            message: message.into(),
            field_path: field_path.into(),
            line_number: None,
            column_number: None,
            suggestion: None,
            severity: ErrorSeverity::Error,
        }
    }

    pub fn with_suggestion(mut self, suggestion: impl Into<String>) -> Self {
        self.suggestion = Some(suggestion.into());
        self
    }

    pub fn with_location(mut self, line: u32, column: u32) -> Self {
        self.line_number = Some(line);
        self.column_number = Some(column);
        self
    }

    pub fn with_severity(mut self, severity: ErrorSeverity) -> Self {
        self.severity = severity;
        self
    }
}

#[derive(Debug, Clone)]
pub enum ValidationErrorType {
    MissingRequiredField,
    InvalidFieldType,
    InvalidFieldValue,
    InvalidFormat,
    CrossReferenceError,
    PerformanceImpact,
    CompatibilityIssue,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ErrorSeverity {
    Error,
    Warning,
    Info,
}

/// Validation warnings
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub message: String,
    pub field_path: String,
    pub suggestion: Option<String>,
    pub warning_type: WarningType,
}

#[derive(Debug, Clone)]
pub enum WarningType {
    DeprecatedField,
    PerformanceImpact,
    Compatibility,
    UnknownLanguage,
    UnknownFileType,
    SensibleRegex,
}

/// Performance metrics and analysis
#[derive(Debug, Default)]
pub struct PerformanceMetrics {
    pub total_hooks: usize,
    pub estimated_file_count: usize,
    pub regex_complexity_score: f64,
    pub potential_bottlenecks: Vec<PerformanceWarning>,
}

impl PerformanceMetrics {
    pub fn merge(&mut self, other: PerformanceMetrics) {
        self.total_hooks += other.total_hooks;
        self.estimated_file_count += other.estimated_file_count;
        self.regex_complexity_score += other.regex_complexity_score;
        self.potential_bottlenecks
            .extend(other.potential_bottlenecks);
    }
}

#[derive(Debug)]
pub struct PerformanceWarning {
    pub hook_id: String,
    pub issue: PerformanceIssue,
    pub impact: PerformanceImpact,
    pub suggestion: String,
}

#[derive(Debug)]
pub enum PerformanceIssue {
    TooManyFiles { count: usize },
    ComplexRegex { pattern: String },
    ExpensiveHook { estimated_time: Duration },
    FrequentExecution { stage_count: usize },
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum PerformanceImpact {
    Low,
    Medium,
    High,
    Critical,
}

/// Main schema validator
pub struct SchemaValidator {
    config: ValidationConfig,
    supported_languages: HashSet<String>,
    supported_stages: HashSet<String>,
    hook_type_validators: HashMap<String, Box<dyn HookValidator>>,
    regex_cache: HashMap<String, Regex>,
}

impl SchemaValidator {
    pub fn new(config: ValidationConfig) -> Self {
        Self {
            config,
            supported_languages: get_supported_languages(),
            supported_stages: get_supported_stages(),
            hook_type_validators: HashMap::new(),
            regex_cache: HashMap::new(),
        }
    }

    pub fn with_supported_languages(mut self, languages: Vec<String>) -> Self {
        self.supported_languages = languages.into_iter().collect();
        self
    }

    // Main validation methods
    pub fn validate_config(&mut self, config: &Config) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate repositories
        for (repo_idx, repo) in config.repos.iter().enumerate() {
            let repo_result = self.validate_repository_detailed(repo, repo_idx);
            result.merge(repo_result);
        }

        // Validate global settings
        result.merge(self.validate_global_settings(config));

        // Cross-reference validation
        result.merge(self.validate_cross_references(config));

        result
    }

    pub fn validate_yaml(&mut self, yaml: &str) -> ValidationResult {
        match serde_yaml::from_str::<Config>(yaml) {
            Ok(config) => self.validate_config(&config),
            Err(e) => {
                let mut result = ValidationResult::new();
                result.add_error(ValidationError::new(
                    ValidationErrorType::InvalidFormat,
                    format!("YAML parsing failed: {e}"),
                    "root",
                ));
                result
            }
        }
    }

    pub fn validate_file(&mut self, path: &Path) -> ValidationResult {
        match std::fs::read_to_string(path) {
            Ok(content) => self.validate_yaml(&content),
            Err(e) => {
                let mut result = ValidationResult::new();
                result.add_error(ValidationError::new(
                    ValidationErrorType::InvalidFormat,
                    format!("Failed to read file: {e}"),
                    "file",
                ));
                result
            }
        }
    }

    // Partial validation for incremental checking
    pub fn validate_hook(&mut self, hook: &Hook, hook_idx: usize) -> ValidationResult {
        self.validate_hook_with_context(hook, hook_idx, None)
    }

    // Validate hook with repository context
    pub fn validate_hook_with_context(
        &mut self,
        hook: &Hook,
        hook_idx: usize,
        repo_type: Option<&str>,
    ) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate required fields
        if hook.id.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Hook ID cannot be empty",
                    format!("hooks[{hook_idx}].id"),
                )
                .with_suggestion("Provide a unique identifier for this hook"),
            );
        }

        // Entry is required except for:
        // - External repo hooks (get their entry from the repository's .pre-commit-hooks.yaml)
        // - Meta hooks (get their entry from the system)
        let is_external_repo = repo_type
            .map(|repo| {
                !repo.is_empty() && repo != "local" && repo != "meta" && self.is_valid_git_url(repo)
            })
            .unwrap_or(false);
        let is_meta_hook = repo_type == Some("meta");
        let requires_entry = !is_external_repo && !is_meta_hook;

        if hook.entry.is_empty() && requires_entry {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Hook entry cannot be empty",
                    format!("hooks[{hook_idx}].entry"),
                )
                .with_suggestion("Specify the command to execute for this hook"),
            );
        }

        // Validate language
        if !hook.language.is_empty() && !self.supported_languages.contains(&hook.language) {
            result.add_warning(ValidationWarning {
                message: format!("Unknown language: '{}'", hook.language),
                field_path: format!("hooks[{hook_idx}].language"),
                suggestion: Some(self.get_similar_languages(&hook.language)),
                warning_type: WarningType::UnknownLanguage,
            });
        }

        // Use custom hook validators if available
        if let Some(_validator) = self.hook_type_validators.get(&hook.language) {
            // Custom validation could be implemented here
            // For now, we just acknowledge that hook_type_validators exists
        }

        // Validate file patterns
        if let Some(ref files) = hook.files {
            if self.compile_regex(files).is_err() {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::InvalidFormat,
                        "Invalid files regex pattern",
                        format!("hooks[{hook_idx}].files"),
                    )
                    .with_suggestion("Use valid regex syntax for file patterns"),
                );
            }
        }

        if let Some(ref exclude) = hook.exclude {
            if self.compile_regex(exclude).is_err() {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::InvalidFormat,
                        "Invalid exclude regex pattern",
                        format!("hooks[{hook_idx}].exclude"),
                    )
                    .with_suggestion("Use valid regex syntax for exclude patterns"),
                );
            }
        }

        // Validate stages
        if let Some(ref stages) = hook.stages {
            if stages.is_empty() {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::MissingRequiredField,
                        "Hook must have at least one stage",
                        format!("hooks[{hook_idx}].stages"),
                    )
                    .with_suggestion("Add at least one stage (e.g., 'pre-commit')"),
                );
            } else {
                for stage in stages {
                    if !self.supported_stages.contains(stage) {
                        result.add_error(
                            ValidationError::new(
                                ValidationErrorType::InvalidFieldValue,
                                format!("Invalid stage: '{stage}'"),
                                format!("hooks[{hook_idx}].stages"),
                            )
                            .with_suggestion("Use valid git hook stages"),
                        );
                    }
                }
            }
        } else {
            // If no stages specified, warn that default will be used
            result.add_warning(ValidationWarning {
                message: "No stages specified, will use default 'pre-commit'".to_string(),
                field_path: format!("hooks[{hook_idx}].stages"),
                suggestion: Some("Explicitly specify stages for clarity".to_string()),
                warning_type: WarningType::Compatibility,
            });
        }

        result
    }

    pub fn validate_repository_detailed(
        &mut self,
        repo: &Repository,
        repo_idx: usize,
    ) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate repository URL
        if repo.repo.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Repository URL cannot be empty",
                    format!("repos[{repo_idx}].repo"),
                )
                .with_suggestion("Provide a valid repository URL"),
            );
        } else if repo.repo != "local" && repo.repo != "meta" && !self.is_valid_git_url(&repo.repo)
        {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::InvalidFieldValue,
                    format!("Invalid repository URL format: {}", repo.repo),
                    format!("repos[{repo_idx}].repo"),
                )
                .with_suggestion("Use valid URL format (https://, git@, etc.)"),
            );
        }

        // Validate revision for non-local/meta repos
        if repo.repo != "local" && repo.repo != "meta" {
            if let Some(ref rev) = repo.rev {
                if !self.is_valid_revision_format(rev) {
                    result.add_error(
                        ValidationError::new(
                            ValidationErrorType::InvalidFieldValue,
                            format!("Invalid revision format: {rev}"),
                            format!("repos[{repo_idx}].rev"),
                        )
                        .with_suggestion("Use valid SHA, tag, or branch name"),
                    );
                }
            } else {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::MissingRequiredField,
                        "Non-local repositories must specify a revision",
                        format!("repos[{repo_idx}].rev"),
                    )
                    .with_suggestion("Add a revision (SHA, tag, or branch name)"),
                );
            }
        }

        // Check for local path validation if enabled
        if repo.repo == "local" && self.config.validate_file_existence {
            // Local path validation could be implemented here
            // For now, just check that it's not an empty string since 'local' is the literal value
        }

        // Validate hooks exist
        if repo.hooks.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Repository must have at least one hook",
                    format!("repos[{repo_idx}].hooks"),
                )
                .with_suggestion("Add at least one hook to this repository"),
            );
        }

        // Validate each hook
        for (hook_idx, hook) in repo.hooks.iter().enumerate() {
            let hook_result = self.validate_hook_with_context(hook, hook_idx, Some(&repo.repo));
            result.merge(hook_result);
        }

        result
    }

    // Compatibility checking
    pub fn check_compatibility(&self, _config: &Config) -> CompatibilityReport {
        CompatibilityReport {
            compatible: true,
            issues: Vec::new(),
            suggestions: Vec::new(),
        }
    }

    pub fn suggest_migrations(&self, _config: &Config) -> Vec<MigrationSuggestion> {
        Vec::new()
    }

    // Helper methods
    fn validate_global_settings(&mut self, config: &Config) -> ValidationResult {
        let mut result = ValidationResult::new();

        // Validate global file patterns
        if let Some(ref files) = config.files {
            if self.compile_regex(files).is_err() {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::InvalidFormat,
                        "Invalid global files regex pattern",
                        "files",
                    )
                    .with_suggestion("Use valid regex syntax for global file patterns"),
                );
            }
        }

        if let Some(ref exclude) = config.exclude {
            if self.compile_regex(exclude).is_err() {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::InvalidFormat,
                        "Invalid global exclude regex pattern",
                        "exclude",
                    )
                    .with_suggestion("Use valid regex syntax for global exclude patterns"),
                );
            }
        }

        // Validate default_install_hook_types
        if let Some(ref hook_types) = config.default_install_hook_types {
            for hook_type in hook_types {
                if !self.is_valid_hook_type(hook_type) {
                    result.add_error(
                        ValidationError::new(
                            ValidationErrorType::InvalidFieldValue,
                            format!("Invalid hook type: '{hook_type}'"),
                            "default_install_hook_types",
                        )
                        .with_suggestion("Use valid hook types like 'pre-commit', 'pre-push'"),
                    );
                }
            }
        }

        // Validate default_stages
        if let Some(ref stages) = config.default_stages {
            for stage in stages {
                if !self.supported_stages.contains(stage) {
                    result.add_error(
                        ValidationError::new(
                            ValidationErrorType::InvalidFieldValue,
                            format!("Invalid default stage: '{stage}'"),
                            "default_stages",
                        )
                        .with_suggestion("Use valid git hook stages"),
                    );
                }
            }
        }

        // Validate default_language_version
        if let Some(ref lang_versions) = config.default_language_version {
            for (language, version) in lang_versions {
                if !self.supported_languages.contains(language) {
                    result.add_warning(ValidationWarning {
                        message: format!(
                            "Unknown language in default_language_version: '{language}'"
                        ),
                        field_path: format!("default_language_version.{language}"),
                        suggestion: Some("Use supported language names".to_string()),
                        warning_type: WarningType::UnknownLanguage,
                    });
                }
                if version.is_empty() {
                    result.add_error(
                        ValidationError::new(
                            ValidationErrorType::InvalidFieldValue,
                            format!("Empty version for language '{language}'"),
                            format!("default_language_version.{language}"),
                        )
                        .with_suggestion("Specify a valid version string"),
                    );
                }
            }
        }

        result
    }

    fn validate_cross_references(&mut self, config: &Config) -> ValidationResult {
        let mut result = ValidationResult::new();
        let mut hook_ids = HashSet::new();

        // Check for duplicate hook IDs across all repositories
        for (repo_idx, repo) in config.repos.iter().enumerate() {
            for (hook_idx, hook) in repo.hooks.iter().enumerate() {
                if hook_ids.contains(&hook.id) {
                    result.add_error(
                        ValidationError::new(
                            ValidationErrorType::CrossReferenceError,
                            format!("Duplicate hook ID: '{}'", hook.id),
                            format!("repos[{repo_idx}].hooks[{hook_idx}].id"),
                        )
                        .with_suggestion("Hook IDs must be unique across all repositories"),
                    );
                } else {
                    hook_ids.insert(hook.id.clone());
                }
            }

            // Special validation for repository types
            if repo.repo == "local" {
                // Local repositories should have entry points specified
                for (hook_idx, hook) in repo.hooks.iter().enumerate() {
                    if hook.entry.is_empty() {
                        result.add_error(
                            ValidationError::new(
                                ValidationErrorType::MissingRequiredField,
                                "Local hooks must specify an entry point",
                                format!("repos[{repo_idx}].hooks[{hook_idx}].entry"),
                            )
                            .with_suggestion("Specify the command to execute for this local hook"),
                        );
                    }
                }
            } else if repo.repo == "meta" {
                // Meta hooks should use known meta hook IDs
                for (hook_idx, hook) in repo.hooks.iter().enumerate() {
                    if !is_valid_meta_hook_id(&hook.id) {
                        result.add_warning(ValidationWarning {
                            message: format!("Unknown meta hook ID: '{}'", hook.id),
                            field_path: format!("repos[{repo_idx}].hooks[{hook_idx}].id"),
                            suggestion: Some(
                                "Use known meta hook IDs like 'check-hooks-apply'".to_string(),
                            ),
                            warning_type: WarningType::Compatibility,
                        });
                    }
                }
            }
        }

        result
    }

    fn compile_regex(&mut self, pattern: &str) -> Result<()> {
        if self.regex_cache.contains_key(pattern) {
            return Ok(());
        }

        match Regex::new(pattern) {
            Ok(regex) => {
                self.regex_cache.insert(pattern.to_string(), regex);
                Ok(())
            }
            Err(e) => Err(SnpError::Config(Box::new(ConfigError::InvalidRegex {
                pattern: pattern.to_string(),
                field: "pattern".to_string(),
                error: e.to_string(),
                file_path: None,
                line: None,
            }))),
        }
    }

    fn is_valid_git_url(&self, url: &str) -> bool {
        // Basic URL validation - can be enhanced
        url.contains("://") || url.contains('@') || url.starts_with("git@")
    }

    fn is_valid_revision_format(&self, rev: &str) -> bool {
        // Basic revision validation - SHA, tag, or branch name
        !rev.is_empty() && !rev.contains(' ')
    }

    fn get_similar_languages(&self, language: &str) -> String {
        // Simple similarity matching - can be enhanced with fuzzy matching
        let similar: Vec<_> = self
            .supported_languages
            .iter()
            .filter(|lang| {
                let lower_lang = lang.to_lowercase();
                let lower_input = language.to_lowercase();
                lower_lang.contains(&lower_input) || lower_input.contains(&lower_lang)
            })
            .take(3)
            .cloned()
            .collect();

        if similar.is_empty() {
            format!(
                "Available languages: {}",
                self.supported_languages
                    .iter()
                    .take(5)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            format!("Did you mean: {}?", similar.join(", "))
        }
    }

    fn is_valid_hook_type(&self, hook_type: &str) -> bool {
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
                | "manual"
                | "merge-commit"
        )
    }
}

/// Hook field validation trait
pub trait HookValidator: Send + Sync {
    fn validate(&self, hook: &Hook) -> Vec<ValidationError>;
    fn get_validation_hint(&self) -> String;
}

/// Repository validation results
#[derive(Debug)]
pub struct CompatibilityReport {
    pub compatible: bool,
    pub issues: Vec<String>,
    pub suggestions: Vec<String>,
}

#[derive(Debug)]
pub struct MigrationSuggestion {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    pub reason: String,
}

// Static supported languages and stages
static SUPPORTED_LANGUAGES: Lazy<HashSet<String>> = Lazy::new(|| {
    [
        "python",
        "node",
        "ruby",
        "rust",
        "go",
        "java",
        "swift",
        "system",
        "script",
        "docker",
        "docker_image",
        "conda",
        "coursier",
        "dart",
        "dotnet",
        "fail",
        "haskell",
        "julia",
        "lua",
        "perl",
        "pygrep",
        "r",
    ]
    .iter()
    .map(|&s| s.to_string())
    .collect()
});

static SUPPORTED_STAGES: Lazy<HashSet<String>> = Lazy::new(|| {
    [
        "pre-commit",
        "pre-merge-commit",
        "pre-push",
        "prepare-commit-msg",
        "commit-msg",
        "post-commit",
        "post-checkout",
        "post-merge",
        "pre-rebase",
        "post-rewrite",
        "manual",
        "merge-commit",
    ]
    .iter()
    .map(|&s| s.to_string())
    .collect()
});

fn get_supported_languages() -> HashSet<String> {
    SUPPORTED_LANGUAGES.clone()
}

fn get_supported_stages() -> HashSet<String> {
    SUPPORTED_STAGES.clone()
}

fn is_valid_meta_hook_id(hook_id: &str) -> bool {
    matches!(
        hook_id,
        "check-hooks-apply" | "check-useless-excludes" | "identity"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_schema_validation() {
        // Test valid minimal configuration validation
        let mut validator = SchemaValidator::new(ValidationConfig::default());

        let config = Config {
            repos: vec![],
            default_install_hook_types: None,
            default_language_version: None,
            default_stages: None,
            files: None,
            exclude: None,
            fail_fast: None,
            minimum_pre_commit_version: None,
            ci: None,
        };

        let result = validator.validate_config(&config);
        assert!(result.is_valid);

        // Test required field presence checking
        // Empty repos configuration should be valid (though warned about)
        assert!(result.errors.is_empty());

        // Test basic type validation
        let invalid_config = Config {
            repos: vec![],
            default_install_hook_types: Some(vec!["invalid-hook-type".to_string()]),
            default_language_version: None,
            default_stages: None,
            files: Some("[invalid-regex".to_string()),
            exclude: None,
            fail_fast: None,
            minimum_pre_commit_version: None,
            ci: None,
        };

        let invalid_result = validator.validate_config(&invalid_config);
        assert!(!invalid_result.is_valid);
        assert!(!invalid_result.errors.is_empty());
    }

    #[test]
    fn test_hook_field_validation() {
        // Test hook id uniqueness and format
        let mut validator = SchemaValidator::new(ValidationConfig::default());

        // Create a hook with empty ID - should fail
        let hook = Hook {
            id: "".to_string(), // Empty ID should fail
            name: None,
            entry: "test".to_string(),
            language: "system".to_string(),
            files: None,
            exclude: None,
            types: None,
            exclude_types: None,
            additional_dependencies: None,
            args: None,
            always_run: None,
            fail_fast: None,
            pass_filenames: None,
            stages: None,
            verbose: None,
            depends_on: None,
        };
        let result = validator.validate_hook(&hook, 0);

        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test valid hook
        let valid_hook = Hook {
            id: "black".to_string(),
            name: Some("Black formatter".to_string()),
            entry: "black --check".to_string(),
            language: "python".to_string(),
            files: Some(r"\.py$".to_string()),
            exclude: None,
            types: Some(vec!["python".to_string()]),
            exclude_types: None,
            additional_dependencies: None,
            args: Some(vec!["--line-length=88".to_string()]),
            always_run: Some(false),
            fail_fast: Some(false),
            pass_filenames: Some(true),
            stages: Some(vec!["pre-commit".to_string()]),
            verbose: Some(false),
            depends_on: None,
        };
        let valid_result = validator.validate_hook(&valid_hook, 0);
        assert!(valid_result.is_valid);

        // Test hook with invalid regex
        let invalid_regex_hook = Hook {
            id: "test".to_string(),
            name: None,
            entry: "test".to_string(),
            language: "system".to_string(),
            files: Some("[invalid-regex".to_string()),
            exclude: None,
            types: None,
            exclude_types: None,
            additional_dependencies: None,
            args: None,
            always_run: None,
            fail_fast: None,
            pass_filenames: None,
            stages: Some(vec!["pre-commit".to_string()]),
            verbose: None,
            depends_on: None,
        };
        let invalid_regex_result = validator.validate_hook(&invalid_regex_hook, 0);
        assert!(!invalid_regex_result.is_valid);
        assert!(!invalid_regex_result.errors.is_empty());
    }

    #[test]
    fn test_repository_validation() {
        // Test URL format validation
        let mut validator = SchemaValidator::new(ValidationConfig::default());

        // Test invalid URL format
        let repo = Repository {
            repo: "invalid-url".to_string(),
            rev: None,
            hooks: vec![],
        };

        let result = validator.validate_repository_detailed(&repo, 0);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Test valid remote repository
        let valid_repo = Repository {
            repo: "https://github.com/psf/black".to_string(),
            rev: Some("23.1.0".to_string()),
            hooks: vec![Hook {
                id: "black".to_string(),
                name: None,
                entry: "black".to_string(),
                language: "python".to_string(),
                files: None,
                exclude: None,
                types: None,
                exclude_types: None,
                additional_dependencies: None,
                args: None,
                always_run: None,
                fail_fast: None,
                pass_filenames: None,
                stages: Some(vec!["pre-commit".to_string()]),
                verbose: None,
                depends_on: None,
            }],
        };

        let valid_result = validator.validate_repository_detailed(&valid_repo, 0);
        assert!(valid_result.is_valid);

        // Test local repository
        let local_repo = Repository {
            repo: "local".to_string(),
            rev: None,
            hooks: vec![Hook {
                id: "local-test".to_string(),
                name: None,
                entry: "./test.sh".to_string(),
                language: "system".to_string(),
                files: None,
                exclude: None,
                types: None,
                exclude_types: None,
                additional_dependencies: None,
                args: None,
                always_run: None,
                fail_fast: None,
                pass_filenames: None,
                stages: Some(vec!["pre-commit".to_string()]),
                verbose: None,
                depends_on: None,
            }],
        };

        let local_result = validator.validate_repository_detailed(&local_repo, 0);
        assert!(local_result.is_valid);
    }

    #[test]
    fn test_advanced_validation_rules() {
        // Test conditional field requirements
        let mut validator = SchemaValidator::new(ValidationConfig::default());

        // Test cross-hook dependency validation
        let config_with_duplicates = Config {
            repos: vec![
                Repository {
                    repo: "https://github.com/psf/black".to_string(),
                    rev: Some("23.1.0".to_string()),
                    hooks: vec![Hook {
                        id: "duplicate-id".to_string(),
                        name: None,
                        entry: "black".to_string(),
                        language: "python".to_string(),
                        files: None,
                        exclude: None,
                        types: None,
                        exclude_types: None,
                        additional_dependencies: None,
                        args: None,
                        always_run: None,
                        fail_fast: None,
                        pass_filenames: None,
                        stages: Some(vec!["pre-commit".to_string()]),
                        verbose: None,
                        depends_on: None,
                    }],
                },
                Repository {
                    repo: "https://github.com/pycqa/flake8".to_string(),
                    rev: Some("6.0.0".to_string()),
                    hooks: vec![Hook {
                        id: "duplicate-id".to_string(), // Duplicate ID
                        name: None,
                        entry: "flake8".to_string(),
                        language: "python".to_string(),
                        files: None,
                        exclude: None,
                        types: None,
                        exclude_types: None,
                        additional_dependencies: None,
                        args: None,
                        always_run: None,
                        fail_fast: None,
                        pass_filenames: None,
                        stages: Some(vec!["pre-commit".to_string()]),
                        verbose: None,
                        depends_on: None,
                    }],
                },
            ],
            default_install_hook_types: None,
            default_language_version: None,
            default_stages: None,
            files: None,
            exclude: None,
            fail_fast: None,
            minimum_pre_commit_version: None,
            ci: None,
        };

        let result = validator.validate_config(&config_with_duplicates);
        assert!(!result.is_valid);
        assert!(!result.errors.is_empty());

        // Check that the error mentions duplicate hook ID
        let has_duplicate_error = result
            .errors
            .iter()
            .any(|e| e.message.contains("Duplicate hook ID") && e.message.contains("duplicate-id"));
        assert!(has_duplicate_error);
    }

    #[test]
    fn test_error_reporting() {
        // Test detailed error messages with line numbers
        let error = ValidationError::new(
            ValidationErrorType::MissingRequiredField,
            "Test error",
            "test.field",
        );

        assert!(!error.message.is_empty());
        assert_eq!(error.message, "Test error");
        assert_eq!(error.field_path, "test.field");
        assert!(matches!(
            error.error_type,
            ValidationErrorType::MissingRequiredField
        ));

        // Test error with suggestion and location
        let detailed_error = ValidationError::new(
            ValidationErrorType::InvalidFormat,
            "Invalid regex pattern",
            "hooks[0].files",
        )
        .with_suggestion("Use valid regex syntax")
        .with_location(10, 5)
        .with_severity(ErrorSeverity::Error);

        assert_eq!(
            detailed_error.suggestion,
            Some("Use valid regex syntax".to_string())
        );
        assert_eq!(detailed_error.line_number, Some(10));
        assert_eq!(detailed_error.column_number, Some(5));
        assert_eq!(detailed_error.severity, ErrorSeverity::Error);

        // Test multiple error aggregation
        let mut result = ValidationResult::new();
        result.add_error(error);
        result.add_error(detailed_error);

        assert!(!result.is_valid);
        assert_eq!(result.errors.len(), 2);

        // Test warning creation
        result.add_warning(ValidationWarning {
            message: "Unknown language used".to_string(),
            field_path: "hooks[0].language".to_string(),
            suggestion: Some("Use supported language".to_string()),
            warning_type: WarningType::UnknownLanguage,
        });

        assert_eq!(result.warnings.len(), 1);
    }

    #[test]
    fn test_compatibility_validation() {
        // Test pre-commit version compatibility
        let validator = SchemaValidator::new(ValidationConfig::default());
        let config = Config {
            repos: vec![],
            default_install_hook_types: None,
            default_language_version: None,
            default_stages: None,
            files: None,
            exclude: None,
            fail_fast: None,
            minimum_pre_commit_version: Some("3.0.0".to_string()),
            ci: None,
        };

        let report = validator.check_compatibility(&config);
        assert!(report.compatible);
        assert!(report.issues.is_empty());

        // Test deprecated field detection
        let mut validator_strict = SchemaValidator::new(ValidationConfig {
            strict_mode: true,
            allow_deprecated: false,
            ..ValidationConfig::default()
        });

        let config_with_meta = Config {
            repos: vec![Repository {
                repo: "meta".to_string(),
                rev: None,
                hooks: vec![Hook {
                    id: "unknown-meta-hook".to_string(),
                    name: None,
                    entry: "".to_string(),
                    language: "".to_string(),
                    files: None,
                    exclude: None,
                    types: None,
                    exclude_types: None,
                    additional_dependencies: None,
                    args: None,
                    always_run: None,
                    fail_fast: None,
                    pass_filenames: None,
                    stages: Some(vec!["pre-commit".to_string()]),
                    verbose: None,
                    depends_on: None,
                }],
            }],
            default_install_hook_types: None,
            default_language_version: None,
            default_stages: None,
            files: None,
            exclude: None,
            fail_fast: None,
            minimum_pre_commit_version: None,
            ci: None,
        };

        let result = validator_strict.validate_config(&config_with_meta);
        // Should have warnings about unknown meta hook
        assert!(!result.warnings.is_empty());

        // Test migration suggestions
        let migrations = validator.suggest_migrations(&config);
        assert!(migrations.is_empty()); // No migrations needed for simple config
    }
}

/// Manifest hook structure for validation
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ManifestHook {
    pub id: String,
    pub name: String,
    pub entry: String,
    pub language: String,
    #[serde(default)]
    pub alias: Option<String>,
    #[serde(default)]
    pub files: Option<String>,
    #[serde(default)]
    pub exclude: Option<String>,
    #[serde(default)]
    pub types: Option<Vec<String>>,
    #[serde(default)]
    pub types_or: Option<Vec<String>>,
    #[serde(default)]
    pub exclude_types: Option<Vec<String>>,
    #[serde(default)]
    pub additional_dependencies: Option<Vec<String>>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
    #[serde(default)]
    pub always_run: Option<bool>,
    #[serde(default)]
    pub fail_fast: Option<bool>,
    #[serde(default)]
    pub pass_filenames: Option<bool>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub language_version: Option<String>,
    #[serde(default)]
    pub log_file: Option<String>,
    #[serde(default)]
    pub require_serial: Option<bool>,
    #[serde(default)]
    pub stages: Option<Vec<String>>,
    #[serde(default)]
    pub verbose: Option<bool>,
}

/// Manifest validation functions
pub fn validate_manifest_file(file_path: &str) -> Result<()> {
    let path = std::path::Path::new(file_path);

    if !path.exists() {
        return Err(SnpError::Config(Box::new(ConfigError::NotFound {
            path: path.to_path_buf(),
            suggestion: Some("Check that the manifest file exists".to_string()),
        })));
    }

    let content = std::fs::read_to_string(path).map_err(|e| {
        SnpError::Config(Box::new(ConfigError::ReadError {
            path: path.to_path_buf(),
            error: e.to_string(),
        }))
    })?;

    validate_manifest_content(&content)
}

pub fn validate_manifest_content(content: &str) -> Result<()> {
    // Parse YAML content
    let hooks: Vec<ManifestHook> = serde_yaml::from_str(content).map_err(|e| {
        SnpError::Config(Box::new(ConfigError::ParseError {
            file_path: None,
            error: format!("YAML parsing failed: {e}"),
            line: None,
            column: None,
        }))
    })?;

    // Validate manifest structure
    let mut validator = SchemaValidator::new(ValidationConfig::default());
    let validation_result = validate_manifest_hooks(&mut validator, &hooks)?;

    if !validation_result.is_valid {
        let error_messages: Vec<String> = validation_result
            .errors
            .iter()
            .map(|e| format!("{} (at {})", e.message, e.field_path))
            .collect();

        return Err(SnpError::Config(Box::new(ConfigError::ValidationError {
            errors: error_messages,
            file_path: None,
        })));
    }

    Ok(())
}

fn validate_manifest_hooks(
    validator: &mut SchemaValidator,
    hooks: &[ManifestHook],
) -> Result<ValidationResult> {
    let mut result = ValidationResult::new();
    let mut hook_ids = HashSet::new();

    // Validate each hook
    for (hook_idx, manifest_hook) in hooks.iter().enumerate() {
        // Check required fields
        if manifest_hook.id.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Hook ID cannot be empty",
                    format!("hooks[{hook_idx}].id"),
                )
                .with_suggestion("Provide a unique identifier for this hook"),
            );
        }

        if manifest_hook.name.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Hook name cannot be empty",
                    format!("hooks[{hook_idx}].name"),
                )
                .with_suggestion("Provide a descriptive name for this hook"),
            );
        }

        if manifest_hook.entry.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Hook entry cannot be empty",
                    format!("hooks[{hook_idx}].entry"),
                )
                .with_suggestion("Specify the command to execute for this hook"),
            );
        }

        if manifest_hook.language.is_empty() {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::MissingRequiredField,
                    "Hook language cannot be empty",
                    format!("hooks[{hook_idx}].language"),
                )
                .with_suggestion("Specify the language for this hook (e.g., 'python', 'system')"),
            );
        } else if !validator
            .supported_languages
            .contains(&manifest_hook.language)
        {
            result.add_warning(ValidationWarning {
                message: format!("Unknown language: '{}'", manifest_hook.language),
                field_path: format!("hooks[{hook_idx}].language"),
                suggestion: Some(validator.get_similar_languages(&manifest_hook.language)),
                warning_type: WarningType::UnknownLanguage,
            });
        }

        // Check for duplicate hook IDs
        if hook_ids.contains(&manifest_hook.id) {
            result.add_error(
                ValidationError::new(
                    ValidationErrorType::CrossReferenceError,
                    format!("Duplicate hook ID: '{}'", manifest_hook.id),
                    format!("hooks[{hook_idx}].id"),
                )
                .with_suggestion("Hook IDs must be unique within the manifest"),
            );
        } else {
            hook_ids.insert(manifest_hook.id.clone());
        }

        // Validate file patterns if present
        if let Some(ref files) = manifest_hook.files {
            if validator.compile_regex(files).is_err() {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::InvalidFormat,
                        "Invalid files regex pattern",
                        format!("hooks[{hook_idx}].files"),
                    )
                    .with_suggestion("Use valid regex syntax for file patterns"),
                );
            }
        }

        if let Some(ref exclude) = manifest_hook.exclude {
            if validator.compile_regex(exclude).is_err() {
                result.add_error(
                    ValidationError::new(
                        ValidationErrorType::InvalidFormat,
                        "Invalid exclude regex pattern",
                        format!("hooks[{hook_idx}].exclude"),
                    )
                    .with_suggestion("Use valid regex syntax for exclude patterns"),
                );
            }
        }

        // Validate stages if present
        if let Some(ref stages) = manifest_hook.stages {
            for stage in stages {
                if !validator.supported_stages.contains(stage) {
                    result.add_error(
                        ValidationError::new(
                            ValidationErrorType::InvalidFieldValue,
                            format!("Invalid stage: '{stage}'"),
                            format!("hooks[{hook_idx}].stages"),
                        )
                        .with_suggestion("Use valid git hook stages"),
                    );
                }
            }
        }

        // Validate types if present
        if let Some(ref types) = manifest_hook.types {
            for type_tag in types {
                // Basic type validation - could be enhanced with actual type checking
                if type_tag.is_empty() {
                    result.add_error(
                        ValidationError::new(
                            ValidationErrorType::InvalidFieldValue,
                            "Empty type tag",
                            format!("hooks[{hook_idx}].types"),
                        )
                        .with_suggestion("Provide valid type tags"),
                    );
                }
            }
        }
    }

    Ok(result)
}
