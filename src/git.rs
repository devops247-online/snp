// Git operations module for SNP
// Provides comprehensive Git integration including repository detection,
// file enumeration, and hook installation

use crate::error::{GitError, Result, SnpError};
use git2::{DiffOptions, Oid, Repository, Status, StatusOptions};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use tracing::{debug, info, warn};

/// Configuration for Git operations
#[derive(Debug, Clone)]
pub struct GitConfig {
    pub respect_gitignore: bool,
    pub include_untracked: bool,
    pub recurse_submodules: bool,
    pub diff_context_lines: u32,
    pub max_file_size: Option<u64>,
}

impl Default for GitConfig {
    fn default() -> Self {
        Self {
            respect_gitignore: true,
            include_untracked: false,
            recurse_submodules: false,
            diff_context_lines: 3,
            max_file_size: Some(10 * 1024 * 1024), // 10MB
        }
    }
}

/// Index cache for improved Git performance
#[derive(Debug)]
pub struct IndexCache {
    file_statuses: HashMap<PathBuf, CachedFileStatus>,
    last_update: SystemTime,
    cache_ttl: Duration,
}

#[derive(Debug, Clone)]
pub struct CachedFileStatus {
    pub status: FileChangeType,
    pub size: u64,
    pub mtime: SystemTime,
    pub checksum: Option<String>,
}

/// Staged file information
#[derive(Debug, Clone)]
pub struct StagedFile {
    pub path: PathBuf,
    pub status: FileChangeType,
    pub old_path: Option<PathBuf>, // For renames
    pub size: Option<u64>,
    pub is_binary: bool,
    pub last_modified: Option<SystemTime>,
}

/// File change classification
#[derive(Debug, Clone, PartialEq)]
pub enum FileChangeType {
    Added,
    Modified,
    Deleted,
    Renamed { from: PathBuf },
    Copied { from: PathBuf },
    TypeChanged, // e.g., file -> symlink
}

/// Repository status information
#[derive(Debug)]
pub struct RepositoryStatus {
    pub staged_files: Vec<StagedFile>,
    pub unstaged_files: Vec<ModifiedFile>,
    pub untracked_files: Vec<PathBuf>,
    pub ignored_files: Vec<PathBuf>,
    pub conflicted_files: Vec<PathBuf>,
    pub is_dirty: bool,
    pub ahead_behind: Option<(usize, usize)>, // (ahead, behind) from upstream
}

/// Modified file information
#[derive(Debug, Clone)]
pub struct ModifiedFile {
    pub path: PathBuf,
    pub status: FileChangeType,
    pub size: Option<u64>,
    pub is_binary: bool,
}

/// File processing modes
#[derive(Debug, Clone)]
pub enum ProcessingMode {
    StagedOnly,
    AllFiles,
    ModifiedSinceCommit { commit: String },
    ModifiedSinceTime { since: SystemTime },
    FileList { files: Vec<PathBuf> },
}

/// File filtering configuration
#[derive(Debug, Clone)]
pub struct FileFilter {
    pub include_patterns: Vec<String>,
    pub exclude_patterns: Vec<String>,
    pub file_types: Vec<String>,
    pub languages: Vec<String>,
    pub max_file_size: Option<u64>,
    pub respect_gitignore: bool,
}

/// Git diff wrapper with enhanced functionality
#[derive(Debug)]
pub struct GitDiff {
    stats: DiffStats,
    #[allow(dead_code)]
    config: DiffConfig,
    patch_content: String,
}

#[derive(Debug, Clone)]
pub struct DiffConfig {
    pub context_lines: u32,
    pub interhunk_lines: u32,
    pub max_file_size: Option<u64>,
    pub ignore_whitespace: bool,
    pub show_binary: bool,
}

#[derive(Debug, Default)]
pub struct DiffStats {
    pub files_changed: usize,
    pub insertions: usize,
    pub deletions: usize,
    pub binary_files: usize,
}

/// Individual file diff information
#[derive(Debug)]
pub struct FileDiff {
    pub old_path: Option<PathBuf>,
    pub new_path: Option<PathBuf>,
    pub change_type: FileChangeType,
    pub hunks: Vec<DiffHunk>,
    pub is_binary: bool,
    pub similarity: Option<u32>, // For renames/copies
}

#[derive(Debug)]
pub struct DiffHunk {
    pub old_start: u32,
    pub old_lines: u32,
    pub new_start: u32,
    pub new_lines: u32,
    pub header: String,
    pub lines: Vec<DiffLine>,
}

#[derive(Debug)]
pub struct DiffLine {
    pub content: String,
    pub line_type: DiffLineType,
    pub old_line_no: Option<u32>,
    pub new_line_no: Option<u32>,
}

#[derive(Debug, PartialEq)]
pub enum DiffLineType {
    Context,
    Addition,
    Deletion,
    NoNewlineAtEof,
}

/// Git repository wrapper providing SNP-specific Git operations
pub struct GitRepository {
    repo: Repository,
    root_path: PathBuf,
    #[allow(dead_code)]
    config: GitConfig,
    #[allow(dead_code)]
    index_cache: Option<IndexCache>,
    file_cache: std::sync::Mutex<HashMap<String, (Vec<PathBuf>, SystemTime)>>,
}

impl GitRepository {
    /// Discover and open a Git repository from the current directory or its parents
    pub fn discover() -> Result<Self> {
        Self::discover_from_path(".")
    }

    /// Discover and open a Git repository from a specific path
    pub fn discover_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::discover_from_path_with_config(path, GitConfig::default())
    }

    /// Discover and open a Git repository with custom configuration
    pub fn discover_from_path_with_config<P: AsRef<Path>>(
        path: P,
        config: GitConfig,
    ) -> Result<Self> {
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

        Ok(GitRepository {
            repo,
            root_path,
            config,
            index_cache: None,
            file_cache: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// Open an existing repository at the given path with default config
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let repo = Repository::open(path.as_ref()).map_err(|_e| {
            SnpError::Git(Box::new(GitError::RepositoryNotFound {
                path: path.as_ref().to_path_buf(),
                suggestion: Some("Ensure the path points to a valid git repository".to_string()),
            }))
        })?;

        let workdir = repo.workdir().ok_or_else(|| {
            SnpError::Git(Box::new(GitError::RepositoryNotFound {
                path: path.as_ref().to_path_buf(),
                suggestion: Some(
                    "Bare repositories are not supported for pre-commit hooks".to_string(),
                ),
            }))
        })?;

        let root_path = workdir.to_path_buf();

        Ok(GitRepository {
            repo,
            root_path,
            config: GitConfig::default(),
            index_cache: None,
            file_cache: std::sync::Mutex::new(HashMap::new()),
        })
    }

    /// Create a new GitRepository instance (alias for discover_from_path)
    pub async fn new(path: PathBuf) -> Result<Self> {
        Self::discover_from_path(path)
    }

    /// Get the root path of the Git repository
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }

    /// Get the root path of the Git repository (alias for compatibility)
    pub fn path(&self) -> &Path {
        &self.root_path
    }

    /// Get the .git directory path
    pub fn git_dir(&self) -> &Path {
        self.repo.path()
    }

    /// Check if this is a bare repository
    pub fn is_bare(&self) -> bool {
        self.repo.is_bare()
    }

    /// Get the head commit OID
    pub fn head_commit(&self) -> Result<git2::Oid> {
        let head = self.repo.head().map_err(|_e| {
            SnpError::Git(Box::new(GitError::InvalidReference {
                reference: "HEAD".to_string(),
                repository: self.root_path.display().to_string(),
                suggestion: Some("Repository may be in a detached HEAD state".to_string()),
            }))
        })?;

        head.target().ok_or_else(|| {
            SnpError::Git(Box::new(GitError::InvalidReference {
                reference: "HEAD".to_string(),
                repository: self.root_path.display().to_string(),
                suggestion: Some("Unable to resolve HEAD reference".to_string()),
            }))
        })
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

    /// Get enhanced staged files information
    pub fn staged_files_detailed(&self) -> Result<Vec<StagedFile>> {
        debug!("Getting detailed staged files information");

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

            if status_flags.intersects(
                Status::INDEX_NEW
                    | Status::INDEX_MODIFIED
                    | Status::INDEX_RENAMED
                    | Status::INDEX_TYPECHANGE,
            ) && !status_flags.contains(Status::INDEX_DELETED)
            {
                let file_path = self.root_path.join(path);

                // Determine change type
                let change_type = if status_flags.contains(Status::INDEX_NEW) {
                    FileChangeType::Added
                } else if status_flags.contains(Status::INDEX_MODIFIED) {
                    FileChangeType::Modified
                } else if status_flags.contains(Status::INDEX_RENAMED) {
                    // For renames, we need to get the old path
                    // This is a simplified version - full implementation would extract old path
                    FileChangeType::Renamed {
                        from: file_path.clone(),
                    }
                } else if status_flags.contains(Status::INDEX_TYPECHANGE) {
                    FileChangeType::TypeChanged
                } else {
                    FileChangeType::Modified // Default fallback
                };

                // Get file metadata
                let (size, last_modified, is_binary) = if file_path.exists() {
                    let metadata = std::fs::metadata(&file_path).ok();
                    let size = metadata.as_ref().map(|m| m.len());
                    let last_modified = metadata.and_then(|m| m.modified().ok());
                    let is_binary = self.is_binary_file(&file_path)?;
                    (size, last_modified, is_binary)
                } else {
                    (None, None, false)
                };

                staged_files.push(StagedFile {
                    path: file_path,
                    status: change_type,
                    old_path: None, // TODO: Implement proper old path detection for renames
                    size,
                    is_binary,
                    last_modified,
                });
            }
        }

        debug!("Found {} detailed staged files", staged_files.len());
        Ok(staged_files)
    }

    /// Get files staged for commit (for pre-commit hooks)
    pub fn staged_files(&self) -> Result<Vec<PathBuf>> {
        // Check cache first (with 1 second TTL)
        const CACHE_TTL: Duration = Duration::from_secs(1);
        let cache_key = "staged_files";

        if let Ok(cache) = self.file_cache.lock() {
            if let Some((cached_files, cached_time)) = cache.get(cache_key) {
                if cached_time.elapsed().unwrap_or(CACHE_TTL) < CACHE_TTL {
                    debug!("Using cached staged files ({} files)", cached_files.len());
                    return Ok(cached_files.clone());
                }
            }
        }

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

        // Update cache
        if let Ok(mut cache) = self.file_cache.lock() {
            cache.insert(
                cache_key.to_string(),
                (staged_files.clone(), SystemTime::now()),
            );
        }

        Ok(staged_files)
    }

    /// Get all tracked files in the repository
    pub fn all_files(&self) -> Result<Vec<PathBuf>> {
        // Check cache first (with 10 second TTL for all files since they change less frequently)
        const CACHE_TTL: Duration = Duration::from_secs(10);
        let cache_key = "all_files";

        if let Ok(cache) = self.file_cache.lock() {
            if let Some((cached_files, cached_time)) = cache.get(cache_key) {
                if cached_time.elapsed().unwrap_or(CACHE_TTL) < CACHE_TTL {
                    debug!("Using cached all files ({} files)", cached_files.len());
                    return Ok(cached_files.clone());
                }
            }
        }

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

        // Update cache
        if let Ok(mut cache) = self.file_cache.lock() {
            cache.insert(cache_key.to_string(), (files.clone(), SystemTime::now()));
        }

        Ok(files)
    }

    /// Get comprehensive repository status
    pub fn repository_status(&self) -> Result<RepositoryStatus> {
        debug!("Getting comprehensive repository status");

        let staged_files = self.staged_files_detailed()?;

        // Get unstaged changes and untracked files
        let mut status_opts = StatusOptions::new();
        status_opts.include_untracked(true);
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

        let mut unstaged_files = Vec::new();
        let mut untracked_files = Vec::new();

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

            let file_path = self.root_path.join(path);

            if status_flags.contains(Status::WT_NEW) {
                untracked_files.push(file_path);
            } else if status_flags
                .intersects(Status::WT_MODIFIED | Status::WT_DELETED | Status::WT_TYPECHANGE)
            {
                let change_type = if status_flags.contains(Status::WT_MODIFIED) {
                    FileChangeType::Modified
                } else if status_flags.contains(Status::WT_DELETED) {
                    FileChangeType::Deleted
                } else {
                    FileChangeType::TypeChanged
                };

                let (size, is_binary) = if file_path.exists() {
                    let size = std::fs::metadata(&file_path).ok().map(|m| m.len());
                    let is_binary = self.is_binary_file(&file_path)?;
                    (size, is_binary)
                } else {
                    (None, false)
                };

                unstaged_files.push(ModifiedFile {
                    path: file_path,
                    status: change_type,
                    size,
                    is_binary,
                });
            }
        }

        let conflicted_files = self.conflicted_files()?.into_iter().collect();
        let is_dirty =
            !staged_files.is_empty() || !unstaged_files.is_empty() || !untracked_files.is_empty();

        Ok(RepositoryStatus {
            staged_files,
            unstaged_files,
            untracked_files,
            ignored_files: Vec::new(), // TODO: Implement ignored files detection
            conflicted_files,
            is_dirty,
            ahead_behind: None, // TODO: Implement ahead/behind detection
        })
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

    /// Generate diffs for staged files
    pub fn diff_staged(&self) -> Result<GitDiff> {
        debug!("Generating diff for staged files");

        let head_tree = if let Ok(head_commit) = self.head_commit() {
            Some(self.repo.find_commit(head_commit)?.tree()?)
        } else {
            None
        };

        let mut diff_opts = DiffOptions::new();
        diff_opts.context_lines(self.config.diff_context_lines);

        let diff = self.repo.diff_tree_to_index(
            head_tree.as_ref(),
            Some(&self.repo.index()?),
            Some(&mut diff_opts),
        )?;

        // Calculate stats
        let diff_stats = diff.stats()?;
        let stats = DiffStats {
            files_changed: diff.deltas().len(),
            insertions: diff_stats.insertions(),
            deletions: diff_stats.deletions(),
            ..Default::default()
        };

        // Generate patch content
        let mut patch_content = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            match line.origin() {
                '+' | '-' | ' ' => {
                    patch_content.push(line.origin());
                    patch_content
                        .push_str(std::str::from_utf8(line.content()).unwrap_or("<invalid utf8>"));
                }
                _ => {}
            }
            true
        })?;

        Ok(GitDiff {
            stats,
            config: DiffConfig {
                context_lines: self.config.diff_context_lines,
                interhunk_lines: 0,
                max_file_size: self.config.max_file_size,
                ignore_whitespace: false,
                show_binary: false,
            },
            patch_content,
        })
    }

    /// Generate diffs for working tree
    pub fn diff_working_tree(&self) -> Result<GitDiff> {
        debug!("Generating diff for working tree");

        let index = self.repo.index()?;

        let mut diff_opts = DiffOptions::new();
        diff_opts.context_lines(self.config.diff_context_lines);

        let diff = self
            .repo
            .diff_index_to_workdir(Some(&index), Some(&mut diff_opts))?;

        let diff_stats = diff.stats()?;
        let stats = DiffStats {
            files_changed: diff.deltas().len(),
            insertions: diff_stats.insertions(),
            deletions: diff_stats.deletions(),
            ..Default::default()
        };

        let mut patch_content = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            match line.origin() {
                '+' | '-' | ' ' => {
                    patch_content.push(line.origin());
                    patch_content
                        .push_str(std::str::from_utf8(line.content()).unwrap_or("<invalid utf8>"));
                }
                _ => {}
            }
            true
        })?;

        Ok(GitDiff {
            stats,
            config: DiffConfig {
                context_lines: self.config.diff_context_lines,
                interhunk_lines: 0,
                max_file_size: self.config.max_file_size,
                ignore_whitespace: false,
                show_binary: false,
            },
            patch_content,
        })
    }

    /// Generate diff for a specific file
    pub fn diff_file(&self, path: &Path, staged: bool) -> Result<Option<String>> {
        let relative_path = path.strip_prefix(&self.root_path).map_err(|_| {
            SnpError::Git(Box::new(GitError::CommandFailed {
                command: "git diff".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!(
                    "Path {} is not within repository {}",
                    path.display(),
                    self.root_path.display()
                ),
                working_dir: Some(self.root_path.clone()),
            }))
        })?;

        let mut diff_opts = DiffOptions::new();
        diff_opts.context_lines(self.config.diff_context_lines);
        diff_opts.pathspec(relative_path);

        let diff = if staged {
            let head_tree = if let Ok(head_commit) = self.head_commit() {
                Some(self.repo.find_commit(head_commit)?.tree()?)
            } else {
                None
            };

            self.repo.diff_tree_to_index(
                head_tree.as_ref(),
                Some(&self.repo.index()?),
                Some(&mut diff_opts),
            )?
        } else {
            let index = self.repo.index()?;
            self.repo
                .diff_index_to_workdir(Some(&index), Some(&mut diff_opts))?
        };

        if diff.deltas().len() == 0 {
            return Ok(None);
        }

        let mut patch_content = String::new();
        diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
            match line.origin() {
                '+' | '-' | ' ' => {
                    patch_content.push(line.origin());
                    patch_content
                        .push_str(std::str::from_utf8(line.content()).unwrap_or("<invalid utf8>"));
                }
                _ => {}
            }
            true
        })?;

        Ok(Some(patch_content))
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

    /// Check if a file is binary
    pub fn is_binary_file(&self, path: &Path) -> Result<bool> {
        if !path.exists() {
            return Ok(false);
        }

        // Simple binary detection: read first 8KB and check for null bytes
        let mut file = std::fs::File::open(path).map_err(SnpError::Io)?;

        use std::io::Read;
        let mut buffer = [0u8; 8192];
        let bytes_read = file.read(&mut buffer).map_err(SnpError::Io)?;

        // Check for null bytes - common indicator of binary files
        Ok(buffer[..bytes_read].contains(&0))
    }
}

/// File processor for hook execution
pub struct FileProcessor {
    git_repo: Option<GitRepository>,
    filter_cache: HashMap<String, Vec<PathBuf>>,
}

impl Default for FileProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl FileProcessor {
    pub fn new() -> Self {
        Self {
            git_repo: None,
            filter_cache: HashMap::new(),
        }
    }

    pub fn with_git_repository(mut self, repo: GitRepository) -> Self {
        self.git_repo = Some(repo);
        self
    }

    /// Main file processing method for hooks
    pub fn process_for_hooks(
        &mut self,
        hooks: &[crate::core::Hook],
        mode: ProcessingMode,
    ) -> Result<HashMap<String, Vec<PathBuf>>> {
        let mut results = HashMap::new();

        let discovered_files = self.discover_files(mode)?;

        for hook in hooks {
            let filter = FileFilter::from_hook(hook)?;
            let filtered_files = self.filter_files(&discovered_files, &filter)?;
            results.insert(hook.id.clone(), filtered_files);
        }

        Ok(results)
    }

    /// Get files for a specific hook
    pub fn get_files_for_hook(
        &mut self,
        hook: &crate::core::Hook,
        mode: ProcessingMode,
    ) -> Result<Vec<PathBuf>> {
        let discovered_files = self.discover_files(mode)?;
        let filter = FileFilter::from_hook(hook)?;
        self.filter_files(&discovered_files, &filter)
    }

    /// Discover files based on processing mode
    pub fn discover_files(&self, mode: ProcessingMode) -> Result<Vec<PathBuf>> {
        match mode {
            ProcessingMode::StagedOnly => {
                if let Some(ref git_repo) = self.git_repo {
                    git_repo.staged_files()
                } else {
                    Err(SnpError::Git(Box::new(GitError::RepositoryNotFound {
                        path: std::env::current_dir()
                            .unwrap_or_else(|_| std::path::PathBuf::from(".")),
                        suggestion: Some(
                            "Git repository required for staged files mode".to_string(),
                        ),
                    })))
                }
            }
            ProcessingMode::AllFiles => {
                if let Some(ref git_repo) = self.git_repo {
                    git_repo.all_files()
                } else {
                    // Fallback to current directory traversal
                    self.discover_files_in_directory(&std::env::current_dir()?)
                }
            }
            ProcessingMode::ModifiedSinceCommit { ref commit } => {
                if let Some(ref git_repo) = self.git_repo {
                    git_repo.changed_files(commit, "HEAD")
                } else {
                    Err(SnpError::Git(Box::new(GitError::RepositoryNotFound {
                        path: std::env::current_dir()
                            .unwrap_or_else(|_| std::path::PathBuf::from(".")),
                        suggestion: Some(
                            "Git repository required for commit comparison".to_string(),
                        ),
                    })))
                }
            }
            ProcessingMode::ModifiedSinceTime { since: _ } => {
                // For now, fall back to all files - could be enhanced to check file mtimes
                self.discover_files(ProcessingMode::AllFiles)
            }
            ProcessingMode::FileList { ref files } => Ok(files.clone()),
        }
    }

    /// Filter files based on criteria
    pub fn filter_files(&self, files: &[PathBuf], filter: &FileFilter) -> Result<Vec<PathBuf>> {
        let mut filtered_files = Vec::new();

        for file in files {
            if filter.matches(file)? {
                filtered_files.push(file.clone());
            }
        }

        Ok(filtered_files)
    }

    /// Warm cache for multiple hooks
    pub fn warm_cache(&mut self, hooks: &[crate::core::Hook]) -> Result<()> {
        // Pre-compute common file lists
        let all_files = self.discover_files(ProcessingMode::AllFiles)?;
        let staged_files = self
            .discover_files(ProcessingMode::StagedOnly)
            .unwrap_or_default();

        self.filter_cache.insert("all_files".to_string(), all_files);
        self.filter_cache
            .insert("staged_files".to_string(), staged_files);

        for hook in hooks {
            let cache_key = format!("hook_{}", hook.id);
            if !self.filter_cache.contains_key(&cache_key) {
                let files = self.get_files_for_hook(hook, ProcessingMode::AllFiles)?;
                self.filter_cache.insert(cache_key, files);
            }
        }

        Ok(())
    }

    /// Clear file cache
    pub fn clear_cache(&mut self) {
        self.filter_cache.clear();
    }

    /// Discover files in a directory (fallback when no git repo)
    fn discover_files_in_directory(&self, dir: &Path) -> Result<Vec<PathBuf>> {
        use walkdir::WalkDir;

        let mut files = Vec::new();

        for entry in WalkDir::new(dir)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                files.push(entry.path().to_path_buf());
            }
        }

        Ok(files)
    }
}

/// Implementation for FileFilter
impl Default for FileFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl FileFilter {
    pub fn new() -> Self {
        Self {
            include_patterns: Vec::new(),
            exclude_patterns: Vec::new(),
            file_types: Vec::new(),
            languages: Vec::new(),
            max_file_size: None,
            respect_gitignore: true,
        }
    }

    pub fn from_hook(hook: &crate::core::Hook) -> Result<Self> {
        let mut filter = Self::new();

        if let Some(ref files_pattern) = hook.files {
            filter.include_patterns.push(files_pattern.clone());
        }

        if let Some(ref exclude_pattern) = hook.exclude {
            filter.exclude_patterns.push(exclude_pattern.clone());
        }

        filter.file_types = hook.types.clone();
        filter.languages.push(hook.language.clone());

        Ok(filter)
    }

    pub fn matches(&self, file: &Path) -> Result<bool> {
        use regex::Regex;

        let file_str = file.to_string_lossy();

        // Check include patterns
        if !self.include_patterns.is_empty() {
            let mut matches_include = false;
            for pattern in &self.include_patterns {
                if let Ok(regex) = Regex::new(pattern) {
                    if regex.is_match(&file_str) {
                        matches_include = true;
                        break;
                    }
                }
            }
            if !matches_include {
                return Ok(false);
            }
        }

        // Check exclude patterns
        for pattern in &self.exclude_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                if regex.is_match(&file_str) {
                    return Ok(false);
                }
            }
        }

        // Check file size
        if let Some(max_size) = self.max_file_size {
            if let Ok(metadata) = std::fs::metadata(file) {
                if metadata.len() > max_size {
                    return Ok(false);
                }
            }
        }

        Ok(true)
    }

    pub fn is_empty(&self) -> bool {
        self.include_patterns.is_empty()
            && self.exclude_patterns.is_empty()
            && self.file_types.is_empty()
            && self.languages.is_empty()
    }
}

/// Implementation for GitDiff
impl GitDiff {
    pub fn stats(&self) -> &DiffStats {
        &self.stats
    }

    pub fn format_patch(&self) -> Result<String> {
        Ok(self.patch_content.clone())
    }

    pub fn format_stats(&self) -> String {
        format!(
            "{} files changed, {} insertions(+), {} deletions(-)",
            self.stats.files_changed, self.stats.insertions, self.stats.deletions
        )
    }

    pub fn format_summary(&self) -> String {
        format!(
            "Diff summary: {} files, +{} -{}",
            self.stats.files_changed, self.stats.insertions, self.stats.deletions
        )
    }

    /// Check if the diff is empty (no changes)
    pub fn is_empty(&self) -> bool {
        self.stats.files_changed == 0 && self.stats.insertions == 0 && self.stats.deletions == 0
    }
}

/// Implementation for IndexCache
impl IndexCache {
    pub fn new(cache_ttl: Duration) -> Self {
        Self {
            file_statuses: HashMap::new(),
            last_update: SystemTime::now(),
            cache_ttl,
        }
    }

    pub fn get_status(&self, path: &Path) -> Option<&CachedFileStatus> {
        self.file_statuses.get(path)
    }

    pub fn update_status(&mut self, path: PathBuf, status: CachedFileStatus) {
        self.file_statuses.insert(path, status);
    }

    pub fn invalidate(&mut self, path: &Path) {
        self.file_statuses.remove(path);
    }

    pub fn is_stale(&self) -> bool {
        self.last_update.elapsed().unwrap_or(Duration::from_secs(0)) > self.cache_ttl
    }

    pub fn refresh(&mut self, _repo: &GitRepository) -> Result<()> {
        // TODO: Implement cache refresh logic
        self.last_update = SystemTime::now();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Hook;
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

    fn create_test_repo_with_commit() -> (TempDir, GitRepository) {
        let (temp_dir, git_repo) = create_test_repo();
        let repo_path = temp_dir.path();

        // Create initial file and commit
        std::fs::write(repo_path.join("initial.txt"), "initial content").unwrap();
        Command::new("git")
            .args(["add", "initial.txt"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to add initial file");

        Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(repo_path)
            .output()
            .expect("Failed to create initial commit");

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

    // ===== New Tests for Staged Files Processing =====

    #[test]
    fn test_git_integration() {
        // Test Git repository detection and initialization
        let (_temp_dir, git_repo) = create_test_repo();
        assert!(git_repo.root_path().exists());
        assert!(!git_repo.is_bare());

        // Test staged file discovery with various Git states
        let staged = git_repo.staged_files_detailed().unwrap();
        assert!(staged.is_empty());

        // Test different file statuses
        let status = git_repo.repository_status().unwrap();
        assert!(!status.is_dirty);
    }

    #[test]
    fn test_file_change_detection() {
        let (temp_dir, git_repo) = create_test_repo_with_commit();
        let repo_path = temp_dir.path();

        // Test detection of modified files
        std::fs::write(repo_path.join("modified.txt"), "new content").unwrap();
        Command::new("git")
            .args(["add", "modified.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let staged = git_repo.staged_files_detailed().unwrap();
        assert_eq!(staged.len(), 1);
        assert_eq!(staged[0].status, FileChangeType::Added);

        // Test change type classification
        std::fs::write(repo_path.join("initial.txt"), "modified initial content").unwrap();
        Command::new("git")
            .args(["add", "initial.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let staged = git_repo.staged_files_detailed().unwrap();
        assert_eq!(staged.len(), 2);
    }

    #[test]
    fn test_file_filtering() {
        let (_temp_dir, git_repo) = create_test_repo();
        let processor = FileProcessor::new().with_git_repository(git_repo);

        // Test pattern-based filtering
        let filter = FileFilter {
            include_patterns: vec![r"\.rs$".to_string()],
            exclude_patterns: vec![r"test_.*".to_string()],
            file_types: vec![],
            languages: vec![],
            max_file_size: None,
            respect_gitignore: true,
        };

        let test_files = vec![
            PathBuf::from("main.rs"),
            PathBuf::from("test_main.rs"),
            PathBuf::from("script.py"),
        ];

        let filtered = processor.filter_files(&test_files, &filter).unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].ends_with("main.rs"));
    }

    #[test]
    fn test_diff_generation() {
        let (temp_dir, git_repo) = create_test_repo_with_commit();
        let repo_path = temp_dir.path();

        // Create changes for diff
        std::fs::write(repo_path.join("new_file.txt"), "new content").unwrap();
        Command::new("git")
            .args(["add", "new_file.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Test diff generation
        let diff = git_repo.diff_staged().unwrap();
        assert!(diff.stats().files_changed > 0);
        assert!(!diff.format_patch().unwrap().is_empty());
        assert!(!diff.format_stats().is_empty());
    }

    #[test]
    fn test_performance_optimization() {
        let (temp_dir, git_repo) = create_test_repo();
        let repo_path = temp_dir.path();
        let processor = FileProcessor::new().with_git_repository(git_repo);

        // Create many files
        for i in 0..100 {
            let file_path = repo_path.join(format!("file_{i}.txt"));
            std::fs::write(&file_path, format!("content {i}")).unwrap();
            Command::new("git")
                .args(["add", &format!("file_{i}.txt")])
                .current_dir(repo_path)
                .output()
                .unwrap();
        }

        let start = std::time::Instant::now();
        let _files = processor
            .discover_files(ProcessingMode::StagedOnly)
            .unwrap();
        let duration = start.elapsed();

        // Performance target: should complete within reasonable time
        assert!(
            duration.as_millis() < 1000,
            "File discovery took too long: {}ms",
            duration.as_millis()
        );
    }

    #[test]
    fn test_edge_cases() {
        // Test empty repository
        let (_temp_dir, git_repo) = create_test_repo();
        let staged = git_repo.staged_files_detailed().unwrap();
        assert!(staged.is_empty());

        // Test repository status
        let status = git_repo.repository_status().unwrap();
        assert!(!status.is_dirty);
        assert!(status.untracked_files.is_empty());

        // Test file processor with no git repo
        let processor = FileProcessor::new();
        let result = processor.discover_files(ProcessingMode::StagedOnly);
        assert!(result.is_err());
    }

    #[test]
    fn test_file_processor_integration() {
        let (temp_dir, git_repo) = create_test_repo_with_commit();
        let repo_path = temp_dir.path();
        let mut processor = FileProcessor::new().with_git_repository(git_repo);

        // Create test files
        std::fs::write(repo_path.join("script.py"), "print('hello')").unwrap();
        std::fs::write(repo_path.join("main.rs"), "fn main() {}").unwrap();
        Command::new("git")
            .args(["add", "."])
            .current_dir(repo_path)
            .output()
            .unwrap();

        // Test hook-specific file processing
        let hook = Hook::new("test", "echo", "system").with_files(r"\.py$".to_string());

        let files = processor
            .get_files_for_hook(&hook, ProcessingMode::StagedOnly)
            .unwrap();
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("script.py"));
    }

    #[test]
    fn test_cache_functionality() {
        let (_temp_dir, git_repo) = create_test_repo();
        let mut processor = FileProcessor::new().with_git_repository(git_repo);

        let hooks = vec![
            Hook::new("hook1", "echo", "system"),
            Hook::new("hook2", "echo", "system"),
        ];

        // Test cache warming
        processor.warm_cache(&hooks).unwrap();
        assert!(!processor.filter_cache.is_empty());

        // Test cache clearing
        processor.clear_cache();
        assert!(processor.filter_cache.is_empty());
    }

    #[test]
    fn test_git_config_and_caching() {
        let config = GitConfig {
            respect_gitignore: true,
            include_untracked: false,
            recurse_submodules: false,
            diff_context_lines: 5,
            max_file_size: Some(1024),
        };

        let temp_dir = TempDir::new().unwrap();
        let repo_path = temp_dir.path();

        Command::new("git")
            .args(["init"])
            .current_dir(repo_path)
            .output()
            .unwrap();

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

        let git_repo = GitRepository::discover_from_path_with_config(repo_path, config).unwrap();
        assert_eq!(git_repo.config.diff_context_lines, 5);
        assert_eq!(git_repo.config.max_file_size, Some(1024));
    }

    #[test]
    fn test_binary_file_detection() {
        let (temp_dir, git_repo) = create_test_repo();
        let repo_path = temp_dir.path();

        // Create a text file
        let text_file = repo_path.join("text.txt");
        std::fs::write(&text_file, "This is plain text").unwrap();
        assert!(!git_repo.is_binary_file(&text_file).unwrap());

        // Create a binary file (with null bytes)
        let binary_file = repo_path.join("binary.dat");
        std::fs::write(&binary_file, b"Binary\x00content\x00here").unwrap();
        assert!(git_repo.is_binary_file(&binary_file).unwrap());
    }

    #[test]
    fn test_file_filter_matches() {
        let filter = FileFilter {
            include_patterns: vec![r"\.rs$".to_string()],
            exclude_patterns: vec![r"test_.*".to_string()],
            file_types: vec![],
            languages: vec![],
            max_file_size: Some(1024),
            respect_gitignore: true,
        };

        // Should match .rs files
        assert!(filter.matches(&PathBuf::from("main.rs")).unwrap());

        // Should not match excluded test files
        assert!(!filter.matches(&PathBuf::from("test_main.rs")).unwrap());

        // Should not match non-.rs files
        assert!(!filter.matches(&PathBuf::from("script.py")).unwrap());
    }

    #[test]
    fn test_processing_modes() {
        let (temp_dir, git_repo) = create_test_repo_with_commit();
        let repo_path = temp_dir.path();
        let processor = FileProcessor::new().with_git_repository(git_repo);

        // Test StagedOnly mode
        std::fs::write(repo_path.join("staged.txt"), "staged content").unwrap();
        Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(repo_path)
            .output()
            .unwrap();

        let staged_files = processor
            .discover_files(ProcessingMode::StagedOnly)
            .unwrap();
        assert!(!staged_files.is_empty());

        // Test AllFiles mode
        let all_files = processor.discover_files(ProcessingMode::AllFiles).unwrap();
        assert!(!all_files.is_empty());

        // Test FileList mode
        let file_list = vec![PathBuf::from("test.txt")];
        let listed_files = processor
            .discover_files(ProcessingMode::FileList {
                files: file_list.clone(),
            })
            .unwrap();
        assert_eq!(listed_files, file_list);
    }
}
