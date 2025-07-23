// Git operations module for SNP
// Provides comprehensive Git integration including repository detection,
// file enumeration, and hook installation

use crate::error::{GitError, Result, SnpError};
use git2::{Oid, Repository, Status, StatusOptions};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Git repository wrapper providing SNP-specific Git operations
pub struct GitRepository {
    repo: Repository,
    root_path: PathBuf,
}

impl GitRepository {
    /// Discover and open a Git repository from the current directory or its parents
    pub fn discover() -> Result<Self> {
        Self::discover_from_path(".")
    }

    /// Discover and open a Git repository from a specific path
    pub fn discover_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        debug!("Discovering Git repository from path: {}", path.display());

        let repo = Repository::discover(path).map_err(|_e| {
            SnpError::Git(Box::new(GitError::RepositoryNotFound {
                path: path.to_path_buf(),
                suggestion: Some(
                    "Ensure you are in a Git repository directory or run 'git init' to create one"
                        .to_string(),
                ),
            }))
        })?;

        let workdir = repo.workdir().ok_or_else(|| {
            SnpError::Git(Box::new(GitError::RepositoryNotFound {
                path: path.to_path_buf(),
                suggestion: Some(
                    "Bare repositories are not supported for pre-commit hooks".to_string(),
                ),
            }))
        })?;

        let root_path = workdir.to_path_buf();
        info!("Found Git repository at: {}", root_path.display());

        Ok(GitRepository { repo, root_path })
    }

    /// Get the root path of the Git repository
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get the .git directory path
    pub fn git_dir(&self) -> &Path {
        self.repo.path()
    }

    /// Get the path to a specific Git hook
    pub fn hook_path(&self, hook_type: &str) -> PathBuf {
        self.git_dir().join("hooks").join(hook_type)
    }

    /// Check if we're currently in a merge conflict state
    pub fn is_merge_conflict(&self) -> Result<bool> {
        let git_dir = self.git_dir();
        let merge_msg = git_dir.join("MERGE_MSG");
        let merge_head = git_dir.join("MERGE_HEAD");

        Ok(merge_msg.exists() && merge_head.exists())
    }

    /// Get the current branch name
    pub fn current_branch(&self) -> Result<String> {
        let head = self.repo.head().map_err(|_e| {
            SnpError::Git(Box::new(GitError::InvalidReference {
                reference: "HEAD".to_string(),
                repository: self.root_path.display().to_string(),
                suggestion: Some("Repository may be in a detached HEAD state".to_string()),
            }))
        })?;

        if let Some(name) = head.shorthand() {
            Ok(name.to_string())
        } else {
            // In detached HEAD state, return the commit SHA
            let oid = head.target().ok_or_else(|| {
                SnpError::Git(Box::new(GitError::InvalidReference {
                    reference: "HEAD".to_string(),
                    repository: self.root_path.display().to_string(),
                    suggestion: Some("Unable to resolve HEAD reference".to_string()),
                }))
            })?;
            Ok(oid.to_string())
        }
    }

    /// Get files staged for commit (for pre-commit hooks)
    pub fn staged_files(&self) -> Result<Vec<PathBuf>> {
        debug!("Getting staged files for pre-commit");

        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(false);
        status_opts.include_ignored(false);

        let statuses = self.repo.statuses(Some(&mut status_opts)).map_err(|e| {
            SnpError::Git(Box::new(GitError::CommandFailed {
                command: "git status".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: e.message().to_string(),
                working_dir: Some(self.root_path.clone()),
            }))
        })?;

        let mut staged_files = Vec::new();

        for status in statuses.iter() {
            let status_flags = status.status();
            let path = status.path().ok_or_else(|| {
                SnpError::Git(Box::new(GitError::CommandFailed {
                    command: "git status".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "Invalid UTF-8 in file path".to_string(),
                    working_dir: Some(self.root_path.clone()),
                }))
            })?;

            // Include files that are staged (added, modified, renamed, copied, but not deleted)
            // This matches the pre-commit behavior with --diff-filter=ACMRTUXB
            if status_flags.intersects(
                Status::INDEX_NEW
                    | Status::INDEX_MODIFIED
                    | Status::INDEX_RENAMED
                    | Status::INDEX_TYPECHANGE,
            ) && !status_flags.contains(Status::INDEX_DELETED)
            {
                let file_path = self.root_path.join(path);
                staged_files.push(file_path);
            }
        }

        debug!("Found {} staged files", staged_files.len());
        Ok(staged_files)
    }

    /// Get all tracked files in the repository
    pub fn all_files(&self) -> Result<Vec<PathBuf>> {
        debug!("Getting all tracked files");

        let index = self.repo.index().map_err(|e| {
            SnpError::Git(Box::new(GitError::CommandFailed {
                command: "git ls-files".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: e.message().to_string(),
                working_dir: Some(self.root_path.clone()),
            }))
        })?;

        let mut files = Vec::new();
        for entry in index.iter() {
            let path = std::str::from_utf8(&entry.path).map_err(|_| {
                SnpError::Git(Box::new(GitError::CommandFailed {
                    command: "git ls-files".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "Invalid UTF-8 in file path".to_string(),
                    working_dir: Some(self.root_path.clone()),
                }))
            })?;

            let file_path = self.root_path.join(path);
            files.push(file_path);
        }

        debug!("Found {} tracked files", files.len());
        Ok(files)
    }

    /// Get changed files between two references (for pre-push hooks)
    pub fn changed_files(&self, old_ref: &str, new_ref: &str) -> Result<Vec<PathBuf>> {
        debug!("Getting changed files between {} and {}", old_ref, new_ref);

        let old_oid = self.resolve_reference(old_ref)?;
        let new_oid = self.resolve_reference(new_ref)?;

        let old_tree = self.repo.find_commit(old_oid)?.tree()?;
        let new_tree = self.repo.find_commit(new_oid)?.tree()?;

        let diff = self
            .repo
            .diff_tree_to_tree(Some(&old_tree), Some(&new_tree), None)
            .map_err(|e| {
                SnpError::Git(Box::new(GitError::CommandFailed {
                    command: format!("git diff {old_ref}..{new_ref}"),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: e.message().to_string(),
                    working_dir: Some(self.root_path.clone()),
                }))
            })?;

        let mut changed_files = Vec::new();
        diff.foreach(
            &mut |delta, _| {
                if let Some(path) = delta.new_file().path() {
                    let file_path = self.root_path.join(path);
                    changed_files.push(file_path);
                }
                true
            },
            None,
            None,
            None,
        )
        .map_err(|e| {
            SnpError::Git(Box::new(GitError::CommandFailed {
                command: format!("git diff {old_ref}..{new_ref}"),
                exit_code: None,
                stdout: String::new(),
                stderr: e.message().to_string(),
                working_dir: Some(self.root_path.clone()),
            }))
        })?;

        debug!("Found {} changed files", changed_files.len());
        Ok(changed_files)
    }

    /// Install a Git hook with the given content
    pub fn install_hook(&self, hook_type: &str, content: &str) -> Result<()> {
        let hook_path = self.hook_path(hook_type);
        let hooks_dir = hook_path.parent().unwrap();

        debug!("Installing {} hook at {}", hook_type, hook_path.display());

        // Create hooks directory if it doesn't exist
        if !hooks_dir.exists() {
            fs::create_dir_all(hooks_dir).map_err(|_e| {
                SnpError::Git(Box::new(GitError::PermissionDenied {
                    operation: format!("create hooks directory: {}", hooks_dir.display()),
                    path: hooks_dir.to_path_buf(),
                    suggestion: Some("Check directory permissions".to_string()),
                }))
            })?;
        }

        // Backup existing hook if it exists and is not ours
        if hook_path.exists() {
            let existing_content = fs::read_to_string(&hook_path).map_err(|_e| {
                SnpError::Git(Box::new(GitError::PermissionDenied {
                    operation: format!("read existing hook: {}", hook_path.display()),
                    path: hook_path.clone(),
                    suggestion: Some("Check file permissions".to_string()),
                }))
            })?;

            // Check if this is already our hook
            if !existing_content.contains("# Generated by SNP") {
                let backup_path = hook_path.with_extension("legacy");
                warn!(
                    "Backing up existing {} hook to {}",
                    hook_type,
                    backup_path.display()
                );

                fs::rename(&hook_path, &backup_path).map_err(|_e| {
                    SnpError::Git(Box::new(GitError::PermissionDenied {
                        operation: format!("backup existing hook to: {}", backup_path.display()),
                        path: hook_path.clone(),
                        suggestion: Some("Check file permissions".to_string()),
                    }))
                })?;
            }
        }

        // Write the new hook
        fs::write(&hook_path, content).map_err(|_e| {
            SnpError::Git(Box::new(GitError::PermissionDenied {
                operation: format!("write hook file: {}", hook_path.display()),
                path: hook_path.clone(),
                suggestion: Some("Check file permissions".to_string()),
            }))
        })?;

        // Make the hook executable
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&hook_path)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&hook_path, perms).map_err(|_e| {
                SnpError::Git(Box::new(GitError::PermissionDenied {
                    operation: format!("set executable permissions: {}", hook_path.display()),
                    path: hook_path.clone(),
                    suggestion: Some("Check file permissions".to_string()),
                }))
            })?;
        }

        info!("Successfully installed {} hook", hook_type);
        Ok(())
    }

    /// Generate hook script content for SNP
    pub fn generate_hook_script(&self, hook_type: &str) -> String {
        format!(
            r#"#!/usr/bin/env bash
# Generated by SNP (Shell Not Pass)
#
# This hook was installed by SNP to run pre-commit checks.
# To skip this hook, use --no-verify or set SKIP=hook_name
# To temporarily disable SNP, set PRE_COMMIT=0

# Exit on any error
set -e

# Skip if PRE_COMMIT is set to 0
if [ "$PRE_COMMIT" = "0" ]; then
    exit 0
fi

# Find SNP binary
SNP_BIN=""
if command -v snp >/dev/null 2>&1; then
    SNP_BIN="snp"
elif [ -f "./target/release/snp" ]; then
    SNP_BIN="./target/release/snp"
elif [ -f "./target/debug/snp" ]; then
    SNP_BIN="./target/debug/snp"
else
    echo "Error: SNP binary not found. Please install SNP or build it locally."
    exit 1
fi

# Run SNP with the appropriate hook type
exec "$SNP_BIN" run --hook-type="{hook_type}" "$@"
"#
        )
    }

    /// Resolve a reference (branch, tag, or commit) to an OID
    fn resolve_reference(&self, reference: &str) -> Result<Oid> {
        self.repo
            .revparse_single(reference)
            .map(|obj| obj.id())
            .map_err(|_e| {
                SnpError::Git(Box::new(GitError::InvalidReference {
                    reference: reference.to_string(),
                    repository: self.root_path.display().to_string(),
                    suggestion: Some("Check that the reference exists and is valid".to_string()),
                }))
            })
    }

    /// Check if the repository has core.hooksPath configured
    pub fn has_core_hooks_path(&self) -> Result<bool> {
        let config = self.repo.config().map_err(|e| {
            SnpError::Git(Box::new(GitError::CommandFailed {
                command: "git config core.hooksPath".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: e.message().to_string(),
                working_dir: Some(self.root_path.clone()),
            }))
        })?;

        match config.get_string("core.hooksPath") {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    /// Get conflicted files during a merge
    pub fn conflicted_files(&self) -> Result<HashSet<PathBuf>> {
        if !self.is_merge_conflict()? {
            return Ok(HashSet::new());
        }

        let mut conflicted = HashSet::new();

        // Read MERGE_MSG to get files that were in conflict
        let merge_msg_path = self.git_dir().join("MERGE_MSG");
        if let Ok(merge_msg) = fs::read(&merge_msg_path) {
            let conflicted_from_msg = self.parse_merge_msg_for_conflicts(&merge_msg)?;
            conflicted.extend(conflicted_from_msg);
        }

        // Get additional changed files from the merge
        if let Ok(head_oid) = self.resolve_reference("HEAD") {
            if let Ok(merge_head_oid) = self.resolve_reference("MERGE_HEAD") {
                // Get tree hash equivalent
                let head_commit = self.repo.find_commit(head_oid)?;
                let merge_head_commit = self.repo.find_commit(merge_head_oid)?;

                let merge_base = self
                    .repo
                    .merge_base(head_oid, merge_head_oid)
                    .map_err(|e| {
                        SnpError::Git(Box::new(GitError::CommandFailed {
                            command: "git merge-base".to_string(),
                            exit_code: None,
                            stdout: String::new(),
                            stderr: e.message().to_string(),
                            working_dir: Some(self.root_path.clone()),
                        }))
                    })?;

                let base_commit = self.repo.find_commit(merge_base)?;
                let base_tree = base_commit.tree()?;
                let head_tree = head_commit.tree()?;
                let merge_tree = merge_head_commit.tree()?;

                // Find files changed in the merge
                let diff_head =
                    self.repo
                        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)?;
                let diff_merge =
                    self.repo
                        .diff_tree_to_tree(Some(&base_tree), Some(&merge_tree), None)?;

                let mut collect_paths = |diff: git2::Diff| -> Result<()> {
                    diff.foreach(
                        &mut |delta, _| {
                            if let Some(path) = delta.new_file().path() {
                                conflicted.insert(self.root_path.join(path));
                            }
                            true
                        },
                        None,
                        None,
                        None,
                    )
                    .map_err(|e| {
                        SnpError::Git(Box::new(GitError::CommandFailed {
                            command: "git diff".to_string(),
                            exit_code: None,
                            stdout: String::new(),
                            stderr: e.message().to_string(),
                            working_dir: Some(self.root_path.clone()),
                        }))
                    })?;
                    Ok(())
                };

                collect_paths(diff_head)?;
                collect_paths(diff_merge)?;
            }
        }

        Ok(conflicted)
    }

    /// Parse MERGE_MSG file to extract conflicted file names
    fn parse_merge_msg_for_conflicts(&self, merge_msg: &[u8]) -> Result<Vec<PathBuf>> {
        let merge_msg_str = String::from_utf8_lossy(merge_msg);
        let mut conflicted_files = Vec::new();

        for line in merge_msg_str.lines() {
            // Conflicted files start with tabs in MERGE_MSG
            if line.starts_with('\t') || line.starts_with("#\t") {
                let cleaned_line = line.trim_start_matches('#').trim_start_matches('\t').trim();
                if !cleaned_line.is_empty() {
                    conflicted_files.push(self.root_path.join(cleaned_line));
                }
            }
        }

        Ok(conflicted_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::TempDir;

    fn create_test_repo() -> (TempDir, GitRepository) {
        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        // Initialize git repo
        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Configure git for testing
        Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let git_repo = GitRepository::discover_from_path(repo_path).unwrap();
        (temp_dir, git_repo)
    }

    #[test]
    fn test_repository_discovery_fails_outside_git() {
        let temp_dir = TempDir::new().unwrap();
        let result = GitRepository::discover_from_path(temp_dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_repository_discovery_succeeds_in_git() {
        let (_temp_dir, git_repo) = create_test_repo();
        assert!(git_repo.root_path().exists());
        assert!(git_repo.git_dir().exists());
    }

    #[test]
    fn test_hook_path_generation() {
        let (_temp_dir, git_repo) = create_test_repo();
        let hook_path = git_repo.hook_path("pre-commit");
        assert!(hook_path.ends_with("hooks/pre-commit"));
    }

    #[test]
    fn test_current_branch() {
        let (_temp_dir, git_repo) = create_test_repo();
        // New repos don't have a current branch until first commit
        let _result = git_repo.current_branch();
        // This might fail in a new repo, which is expected
        // We'll test this more thoroughly in integration tests
    }

    #[test]
    fn test_is_merge_conflict_false_initially() {
        let (_temp_dir, git_repo) = create_test_repo();
        assert!(!git_repo.is_merge_conflict().unwrap());
    }

    #[test]
    fn test_staged_files_empty_initially() {
        let (_temp_dir, git_repo) = create_test_repo();
        let staged = git_repo.staged_files().unwrap();
        assert!(staged.is_empty());
    }

    #[test]
    fn test_hook_script_generation() {
        let (_temp_dir, git_repo) = create_test_repo();
        let script = git_repo.generate_hook_script("pre-commit");
        assert!(script.contains("#!/usr/bin/env bash"));
        assert!(script.contains("Generated by SNP"));
        assert!(script.contains("pre-commit"));
    }

    #[test]
    fn test_has_core_hooks_path() {
        let (_temp_dir, git_repo) = create_test_repo();
        // Initially should be false
        assert!(!git_repo.has_core_hooks_path().unwrap());
    }
}
