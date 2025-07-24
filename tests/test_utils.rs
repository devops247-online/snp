// Shared test utilities for command tests

use snp::error::{Result, SnpError};
use snp::git::GitRepository;
use std::fs;
use tempfile::{tempdir, TempDir};

/// Helper function to create a temporary git repository for testing
#[allow(dead_code)]
pub async fn create_temp_git_repo() -> Result<(TempDir, GitRepository)> {
    let temp_dir = tempdir().map_err(|e| {
        SnpError::Cli(Box::new(snp::error::CliError::RuntimeError {
            message: format!("Failed to create temp directory: {e}"),
        }))
    })?;

    // Initialize a git repository using git2 directly
    let repo = git2::Repository::init(temp_dir.path())?;

    // Create a basic commit
    fs::write(temp_dir.path().join("README.md"), "# Test Repository")?;

    // Add file to index
    let mut index = repo.index()?;
    index.add_path(std::path::Path::new("README.md"))?;
    let tree_id = index.write_tree()?;
    index.write()?;

    // Create initial commit
    let tree = repo.find_tree(tree_id)?;
    let signature = git2::Signature::now("Test User", "test@example.com")?;
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        "Initial commit",
        &tree,
        &[],
    )?;

    let git_repo = GitRepository::open(temp_dir.path())?;
    Ok((temp_dir, git_repo))
}

/// Helper function to create a sample pre-commit config
#[allow(dead_code)]
pub fn create_sample_config_content() -> String {
    r#"repos:
- repo: https://github.com/pre-commit/pre-commit-hooks
  rev: v4.0.0
  hooks:
  - id: trailing-whitespace
  - id: end-of-file-fixer
"#
    .to_string()
}
