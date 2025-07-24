// Sample configuration command implementation
// Generates sample .pre-commit-config.yaml files

use crate::error::Result;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::info;

#[derive(Debug, Clone)]
pub struct SampleConfigConfig {
    pub language: Option<String>,
    pub hook_type: Option<String>,
    pub output_file: Option<PathBuf>,
}

#[derive(Debug)]
pub struct SampleConfigResult {
    pub config_generated: String,
    pub language_detected: Option<String>,
    pub output_location: String,
}

pub async fn execute_sample_config_command(
    config: &SampleConfigConfig,
) -> Result<SampleConfigResult> {
    info!("Generating sample configuration");

    // Detect project language if not specified
    let detected_language = if let Some(ref lang) = config.language {
        Some(lang.clone())
    } else {
        let detection_dir = if let Some(ref output_file) = config.output_file {
            output_file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        } else {
            std::env::current_dir()?
        };
        detect_project_language_in_dir(&detection_dir).await?
    };

    // Generate configuration based on language and hook type
    let config_content = generate_config_content(&detected_language, &config.hook_type).await?;

    // Determine output location
    let output_location = if let Some(ref output_file) = config.output_file {
        // Write to file
        if let Some(parent) = output_file.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(output_file, &config_content)?;
        output_file.to_string_lossy().to_string()
    } else {
        // Output to stdout
        println!("{config_content}");
        "stdout".to_string()
    };

    Ok(SampleConfigResult {
        config_generated: config_content,
        language_detected: detected_language,
        output_location,
    })
}

async fn detect_project_language_in_dir(dir: &Path) -> Result<Option<String>> {
    // Check for common language indicators
    if has_files(
        dir,
        &["setup.py", "pyproject.toml", "requirements.txt", "Pipfile"],
    ) {
        return Ok(Some("python".to_string()));
    }

    if has_files(dir, &["Cargo.toml", "Cargo.lock"]) {
        return Ok(Some("rust".to_string()));
    }

    if has_files(dir, &["package.json", "yarn.lock", "npm-shrinkwrap.json"]) {
        return Ok(Some("node".to_string()));
    }

    if has_files(dir, &["go.mod", "go.sum"]) {
        return Ok(Some("go".to_string()));
    }

    if has_files(dir, &["pom.xml", "build.gradle", "build.gradle.kts"]) {
        return Ok(Some("java".to_string()));
    }

    if has_files(dir, &["Gemfile", "Gemfile.lock", "*.gemspec"]) {
        return Ok(Some("ruby".to_string()));
    }

    // Check for source files if no manifest files found
    if has_source_files(dir, &["*.py"]) {
        return Ok(Some("python".to_string()));
    }

    if has_source_files(dir, &["*.rs"]) {
        return Ok(Some("rust".to_string()));
    }

    if has_source_files(dir, &["*.js", "*.ts", "*.jsx", "*.tsx"]) {
        return Ok(Some("node".to_string()));
    }

    if has_source_files(dir, &["*.go"]) {
        return Ok(Some("go".to_string()));
    }

    Ok(None)
}

fn has_files(dir: &Path, patterns: &[&str]) -> bool {
    for pattern in patterns {
        if pattern.contains('*') {
            // Handle glob patterns
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if glob_match(pattern, name) {
                            return true;
                        }
                    }
                }
            }
        } else if dir.join(pattern).exists() {
            return true;
        }
    }
    false
}

fn has_source_files(dir: &Path, patterns: &[&str]) -> bool {
    fn check_recursively(dir: &Path, patterns: &[&str], depth: usize) -> bool {
        if depth > 3 {
            // Limit recursion depth
            return false;
        }

        if let Ok(entries) = fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Ok(file_type) = entry.file_type() {
                    if file_type.is_file() {
                        if let Some(name) = entry.file_name().to_str() {
                            for pattern in patterns {
                                if glob_match(pattern, name) {
                                    return true;
                                }
                            }
                        }
                    } else if file_type.is_dir() {
                        let dir_name = entry.file_name();
                        // Skip common directories that shouldn't be checked
                        if ![
                            "target",
                            "node_modules",
                            ".git",
                            "__pycache__",
                            ".venv",
                            "venv",
                        ]
                        .contains(&dir_name.to_str().unwrap_or(""))
                            && check_recursively(&entry.path(), patterns, depth + 1)
                        {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    check_recursively(dir, patterns, 0)
}

fn glob_match(pattern: &str, name: &str) -> bool {
    // Simple glob matching for file extensions
    if let Some(ext) = pattern.strip_prefix("*.") {
        return name.ends_with(&format!(".{ext}"));
    }
    pattern == name
}

async fn generate_config_content(
    language: &Option<String>,
    hook_type: &Option<String>,
) -> Result<String> {
    let mut config = ConfigBuilder::new();

    // Add language-specific hooks
    if let Some(lang) = language {
        config.add_language_hooks(lang);
    } else {
        config.add_default_hooks();
    }

    // Modify for specific hook types (stages)
    if let Some(stage) = hook_type {
        config.set_stage(stage);
    }

    config.build()
}

struct ConfigBuilder {
    repos: Vec<RepoConfig>,
    default_stages: Option<Vec<String>>,
}

#[derive(Clone)]
struct RepoConfig {
    repo_url: String,
    rev: String,
    hooks: Vec<HookConfig>,
}

#[derive(Clone)]
struct HookConfig {
    id: String,
    name: Option<String>,
    stages: Option<Vec<String>>,
    args: Option<Vec<String>>,
    files: Option<String>,
}

impl ConfigBuilder {
    fn new() -> Self {
        Self {
            repos: Vec::new(),
            default_stages: None,
        }
    }

    fn add_language_hooks(&mut self, language: &str) {
        match language {
            "python" => self.add_python_hooks(),
            "rust" => self.add_rust_hooks(),
            "node" | "javascript" | "typescript" => self.add_node_hooks(),
            "go" => self.add_go_hooks(),
            "java" => self.add_java_hooks(),
            "ruby" => self.add_ruby_hooks(),
            _ => self.add_default_hooks(),
        }
    }

    fn add_default_hooks(&mut self) {
        // Add standard pre-commit hooks
        let repo = RepoConfig {
            repo_url: "https://github.com/pre-commit/pre-commit-hooks".to_string(),
            rev: "v4.4.0".to_string(),
            hooks: vec![
                HookConfig {
                    id: "trailing-whitespace".to_string(),
                    name: None,
                    stages: None,
                    args: None,
                    files: None,
                },
                HookConfig {
                    id: "end-of-file-fixer".to_string(),
                    name: None,
                    stages: None,
                    args: None,
                    files: None,
                },
                HookConfig {
                    id: "check-yaml".to_string(),
                    name: None,
                    stages: None,
                    args: None,
                    files: None,
                },
                HookConfig {
                    id: "check-added-large-files".to_string(),
                    name: None,
                    stages: None,
                    args: None,
                    files: None,
                },
            ],
        };
        self.repos.push(repo);
    }

    fn add_python_hooks(&mut self) {
        // Add standard hooks first
        self.add_default_hooks();

        // Add Python-specific hooks
        let python_repo = RepoConfig {
            repo_url: "https://github.com/psf/black".to_string(),
            rev: "23.3.0".to_string(),
            hooks: vec![HookConfig {
                id: "black".to_string(),
                name: Some("Black Code Formatter".to_string()),
                stages: None,
                args: Some(vec!["--line-length=88".to_string()]),
                files: None,
            }],
        };
        self.repos.push(python_repo);

        let flake8_repo = RepoConfig {
            repo_url: "https://github.com/pycqa/flake8".to_string(),
            rev: "6.0.0".to_string(),
            hooks: vec![HookConfig {
                id: "flake8".to_string(),
                name: Some("Flake8 Linter".to_string()),
                stages: None,
                args: Some(vec!["--max-line-length=88".to_string()]),
                files: None,
            }],
        };
        self.repos.push(flake8_repo);
    }

    fn add_rust_hooks(&mut self) {
        // Add standard hooks first
        self.add_default_hooks();

        // Add Rust-specific hooks
        let rust_repo = RepoConfig {
            repo_url: "https://github.com/doublify/pre-commit-rust".to_string(),
            rev: "v1.0".to_string(),
            hooks: vec![
                HookConfig {
                    id: "fmt".to_string(),
                    name: Some("Rust Format".to_string()),
                    stages: None,
                    args: Some(vec!["--".to_string(), "--check".to_string()]),
                    files: None,
                },
                HookConfig {
                    id: "clippy".to_string(),
                    name: Some("Rust Clippy".to_string()),
                    stages: None,
                    args: Some(vec![
                        "--".to_string(),
                        "--deny".to_string(),
                        "warnings".to_string(),
                    ]),
                    files: None,
                },
            ],
        };
        self.repos.push(rust_repo);
    }

    fn add_node_hooks(&mut self) {
        // Add standard hooks first
        self.add_default_hooks();

        // Add Node.js-specific hooks
        let prettier_repo = RepoConfig {
            repo_url: "https://github.com/pre-commit/mirrors-prettier".to_string(),
            rev: "v3.0.0".to_string(),
            hooks: vec![HookConfig {
                id: "prettier".to_string(),
                name: Some("Prettier".to_string()),
                stages: None,
                args: None,
                files: Some(r"\.(js|jsx|ts|tsx|json|css|md)$".to_string()),
            }],
        };
        self.repos.push(prettier_repo);

        let eslint_repo = RepoConfig {
            repo_url: "https://github.com/pre-commit/mirrors-eslint".to_string(),
            rev: "v8.0.0".to_string(),
            hooks: vec![HookConfig {
                id: "eslint".to_string(),
                name: Some("ESLint".to_string()),
                stages: None,
                args: Some(vec!["--fix".to_string()]),
                files: Some(r"\.(js|jsx|ts|tsx)$".to_string()),
            }],
        };
        self.repos.push(eslint_repo);
    }

    fn add_go_hooks(&mut self) {
        // Add standard hooks first
        self.add_default_hooks();

        // Add Go-specific hooks
        let go_repo = RepoConfig {
            repo_url: "https://github.com/dnephin/pre-commit-golang".to_string(),
            rev: "v0.5.1".to_string(),
            hooks: vec![
                HookConfig {
                    id: "go-fmt".to_string(),
                    name: Some("Go Format".to_string()),
                    stages: None,
                    args: None,
                    files: None,
                },
                HookConfig {
                    id: "go-vet-mod".to_string(),
                    name: Some("Go Vet".to_string()),
                    stages: None,
                    args: None,
                    files: None,
                },
                HookConfig {
                    id: "golangci-lint-mod".to_string(),
                    name: Some("GolangCI Lint".to_string()),
                    stages: None,
                    args: None,
                    files: None,
                },
            ],
        };
        self.repos.push(go_repo);
    }

    fn add_java_hooks(&mut self) {
        // Add standard hooks first
        self.add_default_hooks();

        // Add Java-specific hooks (example with google-java-format)
        let java_repo = RepoConfig {
            repo_url: "https://github.com/macisamuele/language-formatters-pre-commit-hooks"
                .to_string(),
            rev: "v2.10.0".to_string(),
            hooks: vec![HookConfig {
                id: "pretty-format-java".to_string(),
                name: Some("Java Formatter".to_string()),
                stages: None,
                args: Some(vec!["--autofix".to_string()]),
                files: Some(r"\.java$".to_string()),
            }],
        };
        self.repos.push(java_repo);
    }

    fn add_ruby_hooks(&mut self) {
        // Add standard hooks first
        self.add_default_hooks();

        // Add Ruby-specific hooks
        let ruby_repo = RepoConfig {
            repo_url: "https://github.com/rubocop/rubocop".to_string(),
            rev: "v1.50.0".to_string(),
            hooks: vec![HookConfig {
                id: "rubocop".to_string(),
                name: Some("RuboCop".to_string()),
                stages: None,
                args: Some(vec!["--auto-correct".to_string()]),
                files: None,
            }],
        };
        self.repos.push(ruby_repo);
    }

    fn set_stage(&mut self, stage: &str) {
        let stages = match stage {
            "pre-push" => vec!["pre-push".to_string()],
            "pre-commit" => vec!["pre-commit".to_string()],
            "commit-msg" => vec!["commit-msg".to_string()],
            "post-commit" => vec!["post-commit".to_string()],
            "post-checkout" => vec!["post-checkout".to_string()],
            "post-merge" => vec!["post-merge".to_string()],
            _ => vec![stage.to_string()],
        };

        self.default_stages = Some(stages.clone());

        // Apply stages to all hooks that don't have stages specified
        for repo in &mut self.repos {
            for hook in &mut repo.hooks {
                if hook.stages.is_none() {
                    hook.stages = Some(stages.clone());
                }
            }
        }
    }

    fn build(&self) -> Result<String> {
        let mut yaml_content = String::new();

        // Add header comment
        yaml_content.push_str("# See https://pre-commit.com for more information\n");
        yaml_content.push_str("# See https://pre-commit.com/hooks.html for more hooks\n");

        // Add default stages if specified
        if let Some(ref stages) = self.default_stages {
            if stages.len() == 1 && stages[0] != "pre-commit" {
                yaml_content.push_str(&format!("default_stages: [{}]\n", stages[0]));
            } else if stages.len() > 1 {
                yaml_content.push_str(&format!("default_stages: {stages:?}\n"));
            }
        }

        yaml_content.push_str("repos:\n");

        for repo in &self.repos {
            yaml_content.push_str(&format!("-   repo: {}\n", repo.repo_url));
            yaml_content.push_str(&format!("    rev: {}\n", repo.rev));
            yaml_content.push_str("    hooks:\n");

            for hook in &repo.hooks {
                yaml_content.push_str(&format!("    -   id: {}\n", hook.id));

                if let Some(ref name) = hook.name {
                    yaml_content.push_str(&format!("        name: {name}\n"));
                }

                if let Some(ref args) = hook.args {
                    yaml_content.push_str("        args: ");
                    yaml_content.push_str(&format!("{args:?}\n"));
                }

                if let Some(ref files) = hook.files {
                    yaml_content.push_str(&format!("        files: {files}\n"));
                }

                if let Some(ref stages) = hook.stages {
                    if stages.len() == 1 {
                        yaml_content.push_str(&format!("        stages: [{}]\n", stages[0]));
                    } else {
                        yaml_content.push_str(&format!("        stages: {stages:?}\n"));
                    }
                }
            }
        }

        Ok(yaml_content)
    }
}
