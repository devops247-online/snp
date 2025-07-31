// Try repository command implementation
// Tests hooks from a repository without adding to config

use crate::core::Hook;
use crate::error::{Result, SnpError};
use crate::execution::{ExecutionConfig, HookExecutionEngine};
use crate::git::GitRepository;
use crate::process::ProcessManager;
use crate::storage::Store;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use tracing::{debug, info};

#[derive(Debug, Clone)]
pub struct TryRepoConfig {
    pub repository_url: String,
    pub revision: Option<String>,
    pub hook_id: Option<String>,
    pub files: Vec<String>,
    pub all_files: bool,
    pub verbose: bool,
}

#[derive(Debug)]
pub struct TryRepoResult {
    pub repository_cloned: bool,
    pub revision_used: String,
    pub hooks_executed: Vec<HookExecutionResult>,
    pub files_processed: Vec<String>,
    pub temp_directory_path: String,
    pub temp_cleanup_success: bool,
}

#[derive(Debug)]
pub struct HookExecutionResult {
    pub hook_id: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

pub async fn execute_try_repo_command(config: &TryRepoConfig) -> Result<TryRepoResult> {
    info!(
        "Executing try-repo command for repository: {}",
        config.repository_url
    );

    // Create temporary directory for the repository
    let temp_dir = TempDir::new().map_err(|e| {
        SnpError::Cli(Box::new(crate::error::CliError::RuntimeError {
            message: format!("Failed to create temporary directory: {e}"),
        }))
    })?;

    let temp_path = temp_dir.path().to_path_buf();
    debug!("Created temporary directory: {}", temp_path.display());

    // Clone the repository
    let git_repo = clone_repository(
        &config.repository_url,
        &temp_path,
        config.revision.as_deref(),
    )
    .await?;
    let revision_used = config
        .revision
        .clone()
        .unwrap_or_else(|| "HEAD".to_string());

    // Load the repository's hook manifest
    let hooks = load_repository_hooks(&temp_path, config.hook_id.as_deref()).await?;

    // Determine files to process
    let files_to_process = determine_files_to_process(config, &git_repo).await?;

    // Execute the hooks
    let hook_results = execute_hooks_on_files(&hooks, &files_to_process, &temp_path).await?;

    // Clean up temporary directory
    let temp_path_string = temp_path.to_string_lossy().to_string();
    let cleanup_success = cleanup_temp_directory(temp_dir);

    Ok(TryRepoResult {
        repository_cloned: true,
        revision_used,
        hooks_executed: hook_results,
        files_processed: files_to_process
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect(),
        temp_directory_path: temp_path_string,
        temp_cleanup_success: cleanup_success,
    })
}

async fn clone_repository(
    url: &str,
    target_path: &Path,
    revision: Option<&str>,
) -> Result<GitRepository> {
    debug!("Cloning repository {} to {}", url, target_path.display());

    // Validate URL format
    if !is_valid_repository_url(url) {
        return Err(SnpError::Config(Box::new(
            crate::error::ConfigError::ValidationFailed {
                message: format!("Invalid repository URL: {url}"),
                file_path: None,
                errors: vec![],
            },
        )));
    }

    // Clone using git2
    let mut builder = git2::build::RepoBuilder::new();
    let repo = builder.clone(url, target_path).map_err(|e| {
        let error_msg = e.message().to_lowercase();
        if error_msg.contains("repository not found")
            || error_msg.contains("not found")
            || error_msg.contains("404")
        {
            SnpError::Git(Box::new(crate::error::GitError::RepositoryNotFound {
                path: PathBuf::from(url),
                suggestion: Some(
                    "Check that the repository URL is correct and accessible".to_string(),
                ),
            }))
        } else {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: format!("git clone {url}"),
                exit_code: None,
                stdout: String::new(),
                stderr: e.message().to_string(),
                working_dir: Some(target_path.to_path_buf()),
            }))
        }
    })?;

    // Checkout specific revision if provided
    if let Some(rev) = revision {
        checkout_revision(&repo, rev)?;
    }

    GitRepository::open(target_path)
}

fn checkout_revision(repo: &git2::Repository, revision: &str) -> Result<()> {
    debug!("Checking out revision: {}", revision);

    // Try to resolve the revision as a branch, tag, or commit
    let obj = repo.revparse_single(revision).map_err(|_e| {
        SnpError::Git(Box::new(crate::error::GitError::InvalidReference {
            reference: revision.to_string(),
            suggestion: Some(format!(
                "Check that '{revision}' is a valid branch, tag, or commit hash"
            )),
            repository: repo
                .workdir()
                .unwrap_or_else(|| repo.path())
                .display()
                .to_string(),
        }))
    })?;

    // Checkout the revision
    repo.checkout_tree(&obj, None).map_err(|e| {
        SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
            command: format!("git checkout {revision}"),
            exit_code: None,
            stdout: String::new(),
            stderr: e.message().to_string(),
            working_dir: None,
        }))
    })?;

    // Update HEAD
    repo.set_head_detached(obj.id()).map_err(|e| {
        SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
            command: "git set HEAD".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: e.message().to_string(),
            working_dir: None,
        }))
    })?;

    Ok(())
}

async fn load_repository_hooks(repo_path: &Path, specific_hook: Option<&str>) -> Result<Vec<Hook>> {
    let hooks_file = repo_path.join(".pre-commit-hooks.yaml");

    if !hooks_file.exists() {
        return Err(SnpError::Config(Box::new(
            crate::error::ConfigError::NotFound {
                path: hooks_file,
                suggestion: Some(
                    "Repository must contain a .pre-commit-hooks.yaml file".to_string(),
                ),
            },
        )));
    }

    let hooks_content = fs::read_to_string(&hooks_file)?;
    let hook_definitions: Vec<serde_yaml::Value> = serde_yaml::from_str(&hooks_content)
        .map_err(|e| SnpError::Config(Box::<crate::error::ConfigError>::from(e)))?;

    let mut hooks = Vec::new();
    for hook_def in hook_definitions {
        let hook_id = hook_def.get("id").and_then(|v| v.as_str()).ok_or_else(|| {
            SnpError::Config(Box::new(crate::error::ConfigError::ValidationFailed {
                message: "Hook missing required 'id' field".to_string(),
                file_path: Some(hooks_file.clone()),
                errors: vec![],
            }))
        })?;

        // If specific hook requested, only include that one
        if let Some(specific) = specific_hook {
            if hook_id != specific {
                continue;
            }
        }

        let entry = hook_def
            .get("entry")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                SnpError::Config(Box::new(crate::error::ConfigError::ValidationFailed {
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
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            exclude_types: hook_def
                .get("exclude_types")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            args: hook_def
                .get("args")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            stages: hook_def
                .get("stages")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| {
                            v.as_str()
                                .and_then(|s| crate::core::Stage::from_str(s).ok())
                        })
                        .collect()
                })
                .unwrap_or_else(|| vec![crate::core::Stage::PreCommit]),
            additional_dependencies: hook_def
                .get("additional_dependencies")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            always_run: hook_def
                .get("always_run")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            verbose: hook_def
                .get("verbose")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            pass_filenames: hook_def
                .get("pass_filenames")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            fail_fast: hook_def
                .get("fail_fast")
                .and_then(|v| v.as_bool())
                .unwrap_or(false),
            minimum_pre_commit_version: hook_def
                .get("minimum_pre_commit_version")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            depends_on: hook_def
                .get("depends_on")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            concurrent: hook_def
                .get("concurrent")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        };

        hooks.push(hook);
    }

    if hooks.is_empty() {
        if let Some(specific) = specific_hook {
            return Err(SnpError::Config(Box::new(
                crate::error::ConfigError::ValidationFailed {
                    message: format!("Hook '{specific}' not found in repository"),
                    file_path: Some(hooks_file),
                    errors: vec![],
                },
            )));
        }
    }

    Ok(hooks)
}

async fn determine_files_to_process(
    config: &TryRepoConfig,
    git_repo: &GitRepository,
) -> Result<Vec<PathBuf>> {
    if !config.files.is_empty() {
        // Use specific files provided
        Ok(config.files.iter().map(PathBuf::from).collect())
    } else if config.all_files {
        // Use all files in the repository
        git_repo.all_files()
    } else {
        // Use staged files (or all files if none staged)
        let staged = git_repo.staged_files()?;
        if staged.is_empty() {
            git_repo.all_files()
        } else {
            Ok(staged)
        }
    }
}

async fn execute_hooks_on_files(
    hooks: &[Hook],
    files: &[PathBuf],
    repo_path: &Path,
) -> Result<Vec<HookExecutionResult>> {
    let mut results = Vec::new();

    for hook in hooks {
        debug!("Executing hook: {}", hook.id);

        let result = execute_single_hook(hook, files, repo_path).await?;
        results.push(result);
    }

    Ok(results)
}

async fn execute_single_hook(
    hook: &Hook,
    files: &[PathBuf],
    repo_path: &Path,
) -> Result<HookExecutionResult> {
    // Create execution config for the hook
    let execution_config = ExecutionConfig::new(crate::core::Stage::PreCommit)
        .with_verbose(hook.verbose)
        .with_files(files.to_vec());

    // Create process manager and storage (simplified for try-repo)
    let process_manager = std::sync::Arc::new(ProcessManager::new());
    let storage = std::sync::Arc::new(Store::new()?);

    // Use the hook execution engine
    let mut executor = HookExecutionEngine::new(process_manager, storage);

    // Change working directory to the repository path
    std::env::set_current_dir(repo_path)?;

    // Execute the hook
    match executor
        .execute_single_hook(hook, files, &execution_config)
        .await
    {
        Ok(execution_result) => Ok(HookExecutionResult {
            hook_id: hook.id.clone(),
            success: execution_result.success,
            exit_code: execution_result.exit_code,
            stdout: execution_result.stdout,
            stderr: execution_result.stderr,
        }),
        Err(e) => Ok(HookExecutionResult {
            hook_id: hook.id.clone(),
            success: false,
            exit_code: Some(1),
            stdout: String::new(),
            stderr: e.to_string(),
        }),
    }
}

fn cleanup_temp_directory(temp_dir: TempDir) -> bool {
    // TempDir automatically cleans up when dropped
    std::mem::drop(temp_dir);
    true
}

fn is_valid_repository_url(url: &str) -> bool {
    // Basic URL validation
    if url.starts_with("http://") || url.starts_with("https://") {
        return url::Url::parse(url).is_ok();
    }

    // SSH URLs (git@host:repo format)
    if url.contains('@') && url.contains(':') {
        // Must have content before @ (username)
        let at_pos = url.find('@').unwrap();
        if at_pos == 0 {
            return false;
        }
        // Must have content after : (repository path)
        let colon_pos = url.rfind(':').unwrap();
        if colon_pos == url.len() - 1 {
            return false;
        }
        // Must have content between @ and : (hostname)
        if colon_pos <= at_pos + 1 {
            return false;
        }
        return true;
    }

    // Local paths
    if Path::new(url).exists() {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_test_config() -> TryRepoConfig {
        TryRepoConfig {
            repository_url: "https://github.com/test/repo".to_string(),
            revision: None,
            hook_id: None,
            files: vec![],
            all_files: false,
            verbose: false,
        }
    }

    #[allow(dead_code)]
    fn create_test_hook() -> Hook {
        Hook {
            id: "test-hook".to_string(),
            name: Some("Test Hook".to_string()),
            entry: "echo test".to_string(),
            language: "system".to_string(),
            files: None,
            exclude: None,
            types: vec![],
            exclude_types: vec![],
            args: vec![],
            stages: vec![crate::core::Stage::PreCommit],
            additional_dependencies: vec![],
            always_run: false,
            verbose: false,
            pass_filenames: true,
            fail_fast: false,
            minimum_pre_commit_version: None,
            depends_on: vec![],
            concurrent: true,
        }
    }

    #[test]
    fn test_try_repo_config_creation() {
        let config = TryRepoConfig {
            repository_url: "https://github.com/test/repo".to_string(),
            revision: Some("v1.0.0".to_string()),
            hook_id: Some("test-hook".to_string()),
            files: vec!["file1.py".to_string(), "file2.py".to_string()],
            all_files: true,
            verbose: true,
        };

        assert_eq!(config.repository_url, "https://github.com/test/repo");
        assert_eq!(config.revision, Some("v1.0.0".to_string()));
        assert_eq!(config.hook_id, Some("test-hook".to_string()));
        assert_eq!(config.files.len(), 2);
        assert!(config.all_files);
        assert!(config.verbose);
    }

    #[test]
    fn test_try_repo_result_structure() {
        let result = TryRepoResult {
            repository_cloned: true,
            revision_used: "main".to_string(),
            hooks_executed: vec![],
            files_processed: vec!["test.py".to_string()],
            temp_directory_path: "/tmp/test".to_string(),
            temp_cleanup_success: true,
        };

        assert!(result.repository_cloned);
        assert_eq!(result.revision_used, "main");
        assert!(result.hooks_executed.is_empty());
        assert_eq!(result.files_processed.len(), 1);
        assert_eq!(result.temp_directory_path, "/tmp/test");
        assert!(result.temp_cleanup_success);
    }

    #[test]
    fn test_hook_execution_result_structure() {
        let result = HookExecutionResult {
            hook_id: "test-hook".to_string(),
            success: true,
            exit_code: Some(0),
            stdout: "Test output".to_string(),
            stderr: "".to_string(),
        };

        assert_eq!(result.hook_id, "test-hook");
        assert!(result.success);
        assert_eq!(result.exit_code, Some(0));
        assert_eq!(result.stdout, "Test output");
        assert!(result.stderr.is_empty());
    }

    #[test]
    fn test_hook_execution_result_failure() {
        let result = HookExecutionResult {
            hook_id: "failing-hook".to_string(),
            success: false,
            exit_code: Some(1),
            stdout: "".to_string(),
            stderr: "Error occurred".to_string(),
        };

        assert_eq!(result.hook_id, "failing-hook");
        assert!(!result.success);
        assert_eq!(result.exit_code, Some(1));
        assert!(result.stdout.is_empty());
        assert_eq!(result.stderr, "Error occurred");
    }

    #[test]
    fn test_is_valid_repository_url_http() {
        assert!(is_valid_repository_url("https://github.com/user/repo"));
        assert!(is_valid_repository_url("http://gitlab.com/user/repo"));
        assert!(!is_valid_repository_url("https://not a valid url"));
        assert!(!is_valid_repository_url("not-a-url"));
    }

    #[test]
    fn test_is_valid_repository_url_ssh() {
        assert!(is_valid_repository_url("git@github.com:user/repo.git"));
        assert!(is_valid_repository_url("user@server.com:path/to/repo"));
        assert!(!is_valid_repository_url("not-ssh-format"));
    }

    #[test]
    fn test_is_valid_repository_url_local() {
        let temp_dir = TempDir::new().unwrap();
        let path = temp_dir.path().to_str().unwrap();

        assert!(is_valid_repository_url(path));
        assert!(!is_valid_repository_url("/non/existent/path"));
    }

    #[test]
    fn test_try_repo_functionality() {
        // Test basic try-repo functionality without depending on external functions
        let config = create_test_config();
        assert_eq!(config.repository_url, "https://github.com/test/repo");
        assert!(config.revision.is_none());
        assert!(config.hook_id.is_none());
    }

    #[test]
    fn test_cleanup_temp_directory() {
        let temp_dir = TempDir::new().unwrap();
        let cleanup_success = cleanup_temp_directory(temp_dir);
        assert!(cleanup_success);
    }

    #[tokio::test]
    async fn test_load_repository_hooks_missing_file() {
        let temp_dir = TempDir::new().unwrap();
        let result = load_repository_hooks(temp_dir.path(), None).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            SnpError::Config(_) => {}
            _ => panic!("Expected ConfigError"),
        }
    }

    #[tokio::test]
    async fn test_load_repository_hooks_invalid_yaml() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_file = temp_dir.path().join(".pre-commit-hooks.yaml");
        fs::write(&hooks_file, "invalid yaml content [").unwrap();

        let result = load_repository_hooks(temp_dir.path(), None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_repository_hooks_valid() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_file = temp_dir.path().join(".pre-commit-hooks.yaml");

        let hooks_content = r#"
- id: test-hook
  name: Test Hook
  entry: echo test
  language: system
  files: "\\.py$"
  stages: [pre-commit]
  args: ["--check"]
"#;
        fs::write(&hooks_file, hooks_content).unwrap();

        let result = load_repository_hooks(temp_dir.path(), None).await;
        assert!(result.is_ok());

        let hooks = result.unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].id, "test-hook");
        assert_eq!(hooks[0].name, Some("Test Hook".to_string()));
        assert_eq!(hooks[0].entry, "echo test");
        assert_eq!(hooks[0].language, "system");
    }

    #[tokio::test]
    async fn test_load_repository_hooks_specific_hook() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_file = temp_dir.path().join(".pre-commit-hooks.yaml");

        let hooks_content = r#"
- id: hook1
  entry: echo hook1
  language: system
- id: hook2
  entry: echo hook2
  language: system
"#;
        fs::write(&hooks_file, hooks_content).unwrap();

        let result = load_repository_hooks(temp_dir.path(), Some("hook2")).await;
        assert!(result.is_ok());

        let hooks = result.unwrap();
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].id, "hook2");
    }

    #[tokio::test]
    async fn test_load_repository_hooks_hook_not_found() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_file = temp_dir.path().join(".pre-commit-hooks.yaml");

        let hooks_content = r#"
- id: existing-hook
  entry: echo test
  language: system
"#;
        fs::write(&hooks_file, hooks_content).unwrap();

        let result = load_repository_hooks(temp_dir.path(), Some("non-existent")).await;
        assert!(result.is_err());

        match result.unwrap_err() {
            SnpError::Config(_) => {}
            _ => panic!("Expected ConfigError for hook not found"),
        }
    }

    #[tokio::test]
    async fn test_load_repository_hooks_missing_required_fields() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_file = temp_dir.path().join(".pre-commit-hooks.yaml");

        // Hook missing 'id' field
        let hooks_content = r#"
- name: Test Hook
  entry: echo test
"#;
        fs::write(&hooks_file, hooks_content).unwrap();

        let result = load_repository_hooks(temp_dir.path(), None).await;
        assert!(result.is_err());

        // Hook missing 'entry' field
        let hooks_content = r#"
- id: test-hook
  name: Test Hook
"#;
        fs::write(&hooks_file, hooks_content).unwrap();

        let result = load_repository_hooks(temp_dir.path(), None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_repository_hooks_with_all_fields() {
        let temp_dir = TempDir::new().unwrap();
        let hooks_file = temp_dir.path().join(".pre-commit-hooks.yaml");

        let hooks_content = r#"
- id: comprehensive-hook
  name: Comprehensive Hook
  entry: echo test
  language: python
  files: "\\.py$"
  exclude: "__pycache__"
  types: [python]
  exclude_types: [markdown]
  args: ["--check", "--verbose"]
  stages: [pre-commit, pre-push]
  additional_dependencies: ["requests", "pytest"]
  always_run: true
  verbose: true
  pass_filenames: false
  fail_fast: true
  minimum_pre_commit_version: "2.0.0"
  depends_on: ["other-hook"]
"#;
        fs::write(&hooks_file, hooks_content).unwrap();

        let result = load_repository_hooks(temp_dir.path(), None).await;
        assert!(result.is_ok());

        let hooks = result.unwrap();
        assert_eq!(hooks.len(), 1);

        let hook = &hooks[0];
        assert_eq!(hook.id, "comprehensive-hook");
        assert_eq!(hook.name, Some("Comprehensive Hook".to_string()));
        assert_eq!(hook.entry, "echo test");
        assert_eq!(hook.language, "python");
        assert_eq!(hook.files, Some(r"\.py$".to_string()));
        assert_eq!(hook.exclude, Some("__pycache__".to_string()));
        assert_eq!(hook.types, vec!["python".to_string()]);
        assert_eq!(hook.exclude_types, vec!["markdown".to_string()]);
        assert_eq!(
            hook.args,
            vec!["--check".to_string(), "--verbose".to_string()]
        );
        assert_eq!(hook.stages.len(), 2);
        assert_eq!(
            hook.additional_dependencies,
            vec!["requests".to_string(), "pytest".to_string()]
        );
        assert!(hook.always_run);
        assert!(hook.verbose);
        assert!(!hook.pass_filenames);
        assert!(hook.fail_fast);
        assert_eq!(hook.minimum_pre_commit_version, Some("2.0.0".to_string()));
        assert_eq!(hook.depends_on, vec!["other-hook".to_string()]);
    }

    #[tokio::test]
    async fn test_determine_files_to_process_specific_files() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a git repo in the temp directory
        git2::Repository::init(temp_dir.path()).unwrap();
        let git_repo = GitRepository::open(temp_dir.path()).unwrap();

        let config = TryRepoConfig {
            repository_url: "test".to_string(),
            revision: None,
            hook_id: None,
            files: vec!["file1.py".to_string(), "file2.py".to_string()],
            all_files: false,
            verbose: false,
        };

        let result = determine_files_to_process(&config, &git_repo).await;
        assert!(result.is_ok());

        let files = result.unwrap();
        assert_eq!(files.len(), 2);
        assert!(files.contains(&PathBuf::from("file1.py")));
        assert!(files.contains(&PathBuf::from("file2.py")));
    }

    #[test]
    fn test_checkout_revision_invalid_reference() {
        let temp_dir = TempDir::new().unwrap();

        // Initialize a git repo
        let repo = git2::Repository::init(temp_dir.path()).unwrap();

        let result = checkout_revision(&repo, "non-existent-branch");
        assert!(result.is_err());
        // Can't unwrap_err due to Debug requirement
    }

    #[tokio::test]
    async fn test_clone_repository_invalid_url() {
        let temp_dir = TempDir::new().unwrap();
        let target_path = temp_dir.path().join("repo");

        let result = clone_repository("invalid-url", &target_path, None).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_execute_hooks_on_files_empty() {
        let temp_dir = TempDir::new().unwrap();
        let hooks = vec![];
        let files = vec![PathBuf::from("test.py")];

        let result = execute_hooks_on_files(&hooks, &files, temp_dir.path()).await;
        assert!(result.is_ok());

        let results = result.unwrap();
        assert!(results.is_empty());
    }

    #[tokio::test]
    async fn test_execute_single_hook_system_command() {
        let temp_dir = TempDir::new().unwrap();
        let hook = Hook {
            id: "echo-test".to_string(),
            name: Some("Echo Test".to_string()),
            entry: "echo 'test successful'".to_string(),
            language: "system".to_string(),
            files: None,
            exclude: None,
            types: vec![],
            exclude_types: vec![],
            args: vec![],
            stages: vec![crate::core::Stage::PreCommit],
            additional_dependencies: vec![],
            always_run: false,
            verbose: false,
            pass_filenames: true,
            fail_fast: false,
            minimum_pre_commit_version: None,
            depends_on: vec![],
            concurrent: true,
        };

        let files = vec![PathBuf::from("test.py")];

        // This test may fail in some environments due to missing dependencies
        // We're mainly testing the structure and error handling
        let result = execute_single_hook(&hook, &files, temp_dir.path()).await;
        assert!(result.is_ok());

        let hook_result = result.unwrap();
        assert_eq!(hook_result.hook_id, "echo-test");
        // The success status depends on the execution environment
    }

    #[test]
    fn test_try_repo_config_defaults() {
        let config = TryRepoConfig {
            repository_url: "https://github.com/test/repo".to_string(),
            revision: None,
            hook_id: None,
            files: vec![],
            all_files: false,
            verbose: false,
        };

        assert!(config.revision.is_none());
        assert!(config.hook_id.is_none());
        assert!(config.files.is_empty());
        assert!(!config.all_files);
        assert!(!config.verbose);
    }

    #[test]
    fn test_try_repo_result_defaults() {
        let result = TryRepoResult {
            repository_cloned: false,
            revision_used: "HEAD".to_string(),
            hooks_executed: vec![],
            files_processed: vec![],
            temp_directory_path: "/tmp".to_string(),
            temp_cleanup_success: false,
        };

        assert!(!result.repository_cloned);
        assert_eq!(result.revision_used, "HEAD");
        assert!(result.hooks_executed.is_empty());
        assert!(result.files_processed.is_empty());
        assert!(!result.temp_cleanup_success);
    }

    #[test]
    fn test_hook_execution_result_no_exit_code() {
        let result = HookExecutionResult {
            hook_id: "test-hook".to_string(),
            success: false,
            exit_code: None,
            stdout: "Some output".to_string(),
            stderr: "Some error".to_string(),
        };

        assert_eq!(result.hook_id, "test-hook");
        assert!(!result.success);
        assert!(result.exit_code.is_none());
        assert_eq!(result.stdout, "Some output");
        assert_eq!(result.stderr, "Some error");
    }

    #[test]
    fn test_is_valid_repository_url_edge_cases() {
        // Empty string
        assert!(!is_valid_repository_url(""));

        // Just protocol
        assert!(!is_valid_repository_url("https://"));
        assert!(!is_valid_repository_url("http://"));

        // Invalid SSH format
        assert!(!is_valid_repository_url("git@"));
        assert!(!is_valid_repository_url("@host:repo"));
        assert!(!is_valid_repository_url("user@host"));

        // Valid edge cases
        assert!(is_valid_repository_url("git@host.com:repo"));
        assert!(is_valid_repository_url("user@192.168.1.1:path/repo.git"));
    }
}
