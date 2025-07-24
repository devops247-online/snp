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
        return true;
    }

    // Local paths
    if Path::new(url).exists() {
        return true;
    }

    false
}
