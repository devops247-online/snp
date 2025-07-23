// Autoupdate command implementation for SNP
// Updates repository references in .pre-commit-config.yaml to their latest available versions

use crate::config::{Config, Repository};
use crate::error::{Result, SnpError};
use crate::git::GitRepository;
use regex::Regex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};

/// Configuration for autoupdate command
#[derive(Debug, Clone)]
pub struct AutoupdateConfig {
    pub bleeding_edge: bool,
    pub freeze: bool,
    pub specific_repos: Vec<String>,
    pub jobs: u32,
    pub dry_run: bool,
}

impl Default for AutoupdateConfig {
    fn default() -> Self {
        Self {
            bleeding_edge: false,
            freeze: false,
            specific_repos: Vec::new(),
            jobs: 1,
            dry_run: false,
        }
    }
}

/// Repository version information
#[derive(Debug, Clone)]
pub struct RepoVersion {
    pub revision: String,
    pub tag: Option<String>,
    pub version: Option<semver::Version>,
    pub commit_date: chrono::DateTime<chrono::Utc>,
    pub is_prerelease: bool,
    pub commit_hash: Option<String>,
}

impl RepoVersion {
    pub fn new(revision: String, tag: Option<String>) -> Self {
        let version = tag.as_ref().and_then(|t| {
            // Try to parse semantic version from tag
            let clean_tag = t.trim_start_matches('v');
            semver::Version::parse(clean_tag).ok()
        });

        let is_prerelease = version.as_ref().is_some_and(|v| !v.pre.is_empty());

        Self {
            revision,
            tag,
            version,
            commit_date: chrono::Utc::now(),
            is_prerelease,
            commit_hash: None,
        }
    }

    pub fn from_string(rev_str: &str) -> Self {
        // Determine if this is a tag or commit hash based on format
        let is_likely_hash = rev_str.chars().all(|c| c.is_ascii_hexdigit()) && rev_str.len() >= 7;

        if is_likely_hash {
            Self::new(rev_str.to_string(), None)
        } else {
            Self::new(rev_str.to_string(), Some(rev_str.to_string()))
        }
    }
}

/// Update strategy for repositories
#[derive(Debug, Clone)]
pub enum UpdateStrategy {
    LatestTag,
    LatestCommit,
    LatestStable,
    MajorVersion(u32),
    MinorVersion(u32, u32),
}

/// Update policy configuration
#[derive(Debug, Clone)]
pub struct UpdatePolicy {
    pub strategy: UpdateStrategy,
    pub allow_prereleases: bool,
    pub version_pattern: Option<Regex>,
    pub blacklisted_versions: Vec<String>,
    pub minimum_version: Option<semver::Version>,
}

impl Default for UpdatePolicy {
    fn default() -> Self {
        Self {
            strategy: UpdateStrategy::LatestTag,
            allow_prereleases: false,
            version_pattern: None,
            blacklisted_versions: Vec::new(),
            minimum_version: None,
        }
    }
}

/// Information about a repository update
#[derive(Debug, Clone)]
pub struct UpdateInfo {
    pub repository_url: String,
    pub current_version: RepoVersion,
    pub latest_version: RepoVersion,
    pub update_available: bool,
    pub breaking_changes: bool,
}

/// Result of a repository update
#[derive(Debug, Clone)]
pub struct UpdateResult {
    pub repository_url: String,
    pub old_version: RepoVersion,
    pub new_version: RepoVersion,
    pub success: bool,
    pub error: Option<String>,
}

/// Repository version resolver
pub struct RepositoryVersionResolver {
    git_ops: Option<GitRepository>,
    cache: HashMap<String, (RepoVersion, std::time::Instant)>,
    cache_ttl: Duration,
    semaphore: Arc<Semaphore>,
}

impl RepositoryVersionResolver {
    pub fn new() -> Self {
        Self {
            git_ops: None,
            cache: HashMap::new(),
            cache_ttl: Duration::from_secs(300),     // 5 minutes
            semaphore: Arc::new(Semaphore::new(10)), // Max 10 concurrent requests
        }
    }

    pub fn with_git_repo(mut self, git_repo: GitRepository) -> Self {
        self.git_ops = Some(git_repo);
        self
    }

    /// Get latest version for repository
    pub async fn get_latest_version(
        &mut self,
        repo_url: &str,
        strategy: UpdateStrategy,
    ) -> Result<RepoVersion> {
        let cache_key = format!("{repo_url}:{strategy:?}");

        // Check cache first
        if let Some((cached_version, cached_time)) = self.cache.get(&cache_key) {
            if cached_time.elapsed() < self.cache_ttl {
                debug!("Using cached version for {}", repo_url);
                return Ok(cached_version.clone());
            }
        }

        let _permit = self.semaphore.acquire().await.map_err(|_| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: "acquire semaphore".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: "Failed to acquire rate limiting semaphore".to_string(),
                working_dir: None,
            }))
        })?;

        let version = self
            .resolve_version_from_repository(repo_url, &strategy)
            .await?;

        // Cache the result
        self.cache
            .insert(cache_key, (version.clone(), std::time::Instant::now()));

        Ok(version)
    }

    /// Resolve version from repository using various methods
    async fn resolve_version_from_repository(
        &self,
        repo_url: &str,
        strategy: &UpdateStrategy,
    ) -> Result<RepoVersion> {
        debug!(
            "Resolving version for {} with strategy {:?}",
            repo_url, strategy
        );

        // Handle local and meta repositories
        if repo_url == "local" || repo_url == "meta" {
            return Err(SnpError::Config(Box::new(
                crate::error::ConfigError::ValidationFailed {
                    message: format!("Cannot update {repo_url} repository"),
                    file_path: None,
                    errors: vec![format!("{} repositories are not updatable", repo_url)],
                },
            )));
        }

        // For now, create a mock implementation that will be replaced with real Git operations
        // This allows tests to fail initially (Red phase) and then be implemented (Green phase)

        // TODO: Implement actual repository resolution logic
        // This should:
        // 1. Clone the repository to a temporary location
        // 2. Fetch all tags and commits
        // 3. Apply the update strategy to select the appropriate version
        // 4. Return the selected version information

        // Mock implementation for now
        match strategy {
            UpdateStrategy::LatestTag => {
                // Mock latest tag resolution
                Ok(RepoVersion::new(
                    "v1.0.0".to_string(),
                    Some("v1.0.0".to_string()),
                ))
            }
            UpdateStrategy::LatestCommit => {
                // Mock latest commit resolution
                Ok(RepoVersion::new("abcdef123456".to_string(), None))
            }
            _ => {
                // Mock other strategies
                Ok(RepoVersion::new(
                    "v1.0.0".to_string(),
                    Some("v1.0.0".to_string()),
                ))
            }
        }
    }

    /// List all available versions for a repository
    pub async fn list_versions(&mut self, repo_url: &str) -> Result<Vec<RepoVersion>> {
        debug!("Listing versions for {}", repo_url);

        // TODO: Implement actual version listing
        // This should return all available tags and their associated commit information

        // Mock implementation for now
        Ok(vec![
            RepoVersion::new("v0.9.0".to_string(), Some("v0.9.0".to_string())),
            RepoVersion::new("v1.0.0".to_string(), Some("v1.0.0".to_string())),
        ])
    }

    /// Check if update is available
    pub async fn check_update_available(
        &mut self,
        repo_url: &str,
        current_version: &str,
    ) -> Result<UpdateInfo> {
        debug!(
            "Checking update availability for {} (current: {})",
            repo_url, current_version
        );

        let current = RepoVersion::from_string(current_version);
        let latest = self
            .get_latest_version(repo_url, UpdateStrategy::LatestTag)
            .await?;

        let update_available = current.revision != latest.revision;
        let breaking_changes = self.detect_breaking_changes(&current, &latest);

        Ok(UpdateInfo {
            repository_url: repo_url.to_string(),
            current_version: current,
            latest_version: latest,
            update_available,
            breaking_changes,
        })
    }

    /// Detect if there are breaking changes between versions
    fn detect_breaking_changes(&self, current: &RepoVersion, latest: &RepoVersion) -> bool {
        if let (Some(current_ver), Some(latest_ver)) = (&current.version, &latest.version) {
            // Major version change indicates breaking changes
            current_ver.major < latest_ver.major
        } else {
            // If we can't parse versions, assume no breaking changes
            false
        }
    }
}

/// Configuration updater for YAML files
pub struct ConfigurationUpdater {
    #[allow(dead_code)]
    preserve_formatting: bool,
    #[allow(dead_code)]
    create_backups: bool,
}

impl ConfigurationUpdater {
    pub fn new() -> Self {
        Self {
            preserve_formatting: true,
            create_backups: true,
        }
    }

    /// Update repository version in configuration
    pub async fn update_repository(
        &self,
        config_path: &Path,
        repo_url: &str,
        new_version: &RepoVersion,
    ) -> Result<UpdateResult> {
        debug!(
            "Updating repository {} to version {}",
            repo_url, new_version.revision
        );

        // Read current configuration
        let content = tokio::fs::read_to_string(config_path)
            .await
            .map_err(SnpError::Io)?;
        let config: Config = serde_yaml::from_str(&content).map_err(|e| {
            SnpError::Config(Box::new(crate::error::ConfigError::InvalidYaml {
                message: e.to_string(),
                file_path: Some(config_path.to_path_buf()),
                line: e.location().map(|l| l.line() as u32),
                column: e.location().map(|l| l.column() as u32),
            }))
        })?;

        // Find the repository to update
        let mut found_repo = None;
        for repo in &config.repos {
            if repo.repo == repo_url {
                found_repo = Some(repo);
                break;
            }
        }

        let old_repo = found_repo.ok_or_else(|| {
            SnpError::Config(Box::new(crate::error::ConfigError::ValidationFailed {
                message: format!("Repository {repo_url} not found in configuration"),
                file_path: Some(config_path.to_path_buf()),
                errors: vec![format!(
                    "Repository {} is not in the configuration file",
                    repo_url
                )],
            }))
        })?;

        let old_version = RepoVersion::from_string(old_repo.rev.as_deref().unwrap_or(""));

        // TODO: Implement YAML formatting preservation
        // This should use regex-based replacement like the Python implementation
        // to preserve comments and formatting

        // For now, use a simple approach that will be enhanced later
        let updated_content =
            self.update_yaml_content(&content, repo_url, &new_version.revision)?;

        // Write back to file
        tokio::fs::write(config_path, updated_content)
            .await
            .map_err(SnpError::Io)?;

        info!(
            "Updated {} from {} to {}",
            repo_url, old_version.revision, new_version.revision
        );

        Ok(UpdateResult {
            repository_url: repo_url.to_string(),
            old_version,
            new_version: new_version.clone(),
            success: true,
            error: None,
        })
    }

    /// Update YAML content preserving formatting
    fn update_yaml_content(&self, content: &str, repo_url: &str, new_rev: &str) -> Result<String> {
        // This is a simplified implementation
        // TODO: Implement proper YAML formatting preservation using regex like Python pre-commit

        let lines: Vec<&str> = content.lines().collect();
        let mut updated_lines = Vec::new();
        let mut in_target_repo = false;

        for line in lines {
            if line.trim_start().starts_with("- repo:") && line.contains(repo_url) {
                in_target_repo = true;
                updated_lines.push(line.to_string());
            } else if in_target_repo && line.trim_start().starts_with("rev:") {
                // Replace the revision while preserving indentation and formatting
                let indent = line.len() - line.trim_start().len();
                let spaces = " ".repeat(indent);
                updated_lines.push(format!("{spaces}rev: {new_rev}"));
                in_target_repo = false;
            } else if in_target_repo && line.trim_start().starts_with("- repo:") {
                // We've moved to the next repository
                in_target_repo = false;
                updated_lines.push(line.to_string());
            } else {
                updated_lines.push(line.to_string());
            }
        }

        Ok(updated_lines.join("\n"))
    }

    /// Update all repositories in configuration
    pub async fn update_all_repositories(
        &self,
        config_path: &Path,
        updates: &[UpdateResult],
    ) -> Result<BatchUpdateResult> {
        debug!("Performing batch update of {} repositories", updates.len());

        let mut successful_updates = 0;
        let mut failed_updates = 0;
        let mut errors = Vec::new();

        // Read current content
        let mut content = tokio::fs::read_to_string(config_path)
            .await
            .map_err(SnpError::Io)?;

        // Apply all updates
        for update in updates {
            match self.update_yaml_content(
                &content,
                &update.repository_url,
                &update.new_version.revision,
            ) {
                Ok(updated_content) => {
                    content = updated_content;
                    successful_updates += 1;
                }
                Err(e) => {
                    failed_updates += 1;
                    errors.push(format!("Failed to update {}: {}", update.repository_url, e));
                }
            }
        }

        // Write back to file
        tokio::fs::write(config_path, content)
            .await
            .map_err(SnpError::Io)?;

        Ok(BatchUpdateResult {
            successful_updates,
            failed_updates,
            errors,
            total_processed: updates.len(),
        })
    }
}

/// Result of batch repository updates
#[derive(Debug)]
pub struct BatchUpdateResult {
    pub successful_updates: usize,
    pub failed_updates: usize,
    pub errors: Vec<String>,
    pub total_processed: usize,
}

/// Execute autoupdate command
pub async fn execute_autoupdate_command(
    config_path: &Path,
    autoupdate_config: &AutoupdateConfig,
) -> Result<AutoupdateResult> {
    info!("Starting autoupdate process for {}", config_path.display());

    // Read and parse configuration
    let content = tokio::fs::read_to_string(config_path)
        .await
        .map_err(SnpError::Io)?;
    let config: Config = serde_yaml::from_str(&content).map_err(|e| {
        SnpError::Config(Box::new(crate::error::ConfigError::InvalidYaml {
            message: e.to_string(),
            file_path: Some(config_path.to_path_buf()),
            line: e.location().map(|l| l.line() as u32),
            column: e.location().map(|l| l.column() as u32),
        }))
    })?;

    // Filter repositories based on specific_repos if provided
    let repos_to_update: Vec<&Repository> = if autoupdate_config.specific_repos.is_empty() {
        config.repos.iter().collect()
    } else {
        config
            .repos
            .iter()
            .filter(|repo| autoupdate_config.specific_repos.contains(&repo.repo))
            .collect()
    };

    // Skip local and meta repositories
    let updatable_repos: Vec<&Repository> = repos_to_update
        .iter()
        .filter(|repo| repo.repo != "local" && repo.repo != "meta")
        .cloned()
        .collect();

    info!("Found {} updatable repositories", updatable_repos.len());

    // Create resolver and updater
    let mut resolver = RepositoryVersionResolver::new();
    let updater = ConfigurationUpdater::new();

    // Check for updates
    let mut update_results = Vec::new();
    let mut dry_run_results = Vec::new();

    let repo_count = updatable_repos.len();

    for repo in &updatable_repos {
        let current_rev = repo.rev.as_deref().unwrap_or("");

        match resolver
            .check_update_available(&repo.repo, current_rev)
            .await
        {
            Ok(update_info) => {
                if update_info.update_available {
                    let update_result = UpdateResult {
                        repository_url: repo.repo.clone(),
                        old_version: update_info.current_version,
                        new_version: update_info.latest_version,
                        success: true,
                        error: None,
                    };

                    if autoupdate_config.dry_run {
                        dry_run_results.push(update_result);
                    } else {
                        // Perform actual update
                        match updater
                            .update_repository(config_path, &repo.repo, &update_result.new_version)
                            .await
                        {
                            Ok(result) => {
                                println!(
                                    "[{}] updating {} -> {}",
                                    repo.repo,
                                    result.old_version.revision,
                                    result.new_version.revision
                                );
                                update_results.push(result);
                            }
                            Err(e) => {
                                warn!("Failed to update {}: {}", repo.repo, e);
                                let failed_result = UpdateResult {
                                    repository_url: repo.repo.clone(),
                                    old_version: update_result.old_version,
                                    new_version: update_result.new_version,
                                    success: false,
                                    error: Some(e.to_string()),
                                };
                                update_results.push(failed_result);
                            }
                        }
                    }
                } else {
                    println!("[{}] already up to date!", repo.repo);
                }
            }
            Err(e) => {
                warn!("Failed to check updates for {}: {}", repo.repo, e);
            }
        }
    }

    let successful_updates = update_results.iter().filter(|r| r.success).count();
    let failed_updates = update_results.iter().filter(|r| !r.success).count();

    Ok(AutoupdateResult {
        repositories_processed: repo_count,
        updates_available: update_results.len() + dry_run_results.len(),
        successful_updates,
        failed_updates,
        update_results,
        dry_run_results,
    })
}

/// Result of autoupdate command execution
#[derive(Debug)]
pub struct AutoupdateResult {
    pub repositories_processed: usize,
    pub updates_available: usize,
    pub successful_updates: usize,
    pub failed_updates: usize,
    pub update_results: Vec<UpdateResult>,
    pub dry_run_results: Vec<UpdateResult>,
}

impl Default for RepositoryVersionResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ConfigurationUpdater {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repo_version_creation() {
        let version = RepoVersion::new("v1.2.3".to_string(), Some("v1.2.3".to_string()));
        assert_eq!(version.revision, "v1.2.3");
        assert_eq!(version.tag, Some("v1.2.3".to_string()));
        assert!(version.version.is_some());
        assert!(!version.is_prerelease);
    }

    #[test]
    fn test_repo_version_from_string() {
        let tag_version = RepoVersion::from_string("v1.2.3");
        assert_eq!(tag_version.revision, "v1.2.3");
        assert_eq!(tag_version.tag, Some("v1.2.3".to_string()));

        let commit_version = RepoVersion::from_string("abcdef123456");
        assert_eq!(commit_version.revision, "abcdef123456");
        assert!(commit_version.tag.is_none()); // Should be None for commit hashes
    }

    #[test]
    fn test_autoupdate_config_default() {
        let config = AutoupdateConfig::default();
        assert!(!config.bleeding_edge);
        assert!(!config.freeze);
        assert!(config.specific_repos.is_empty());
        assert_eq!(config.jobs, 1);
        assert!(!config.dry_run);
    }

    #[test]
    fn test_update_policy_default() {
        let policy = UpdatePolicy::default();
        assert!(matches!(policy.strategy, UpdateStrategy::LatestTag));
        assert!(!policy.allow_prereleases);
        assert!(policy.version_pattern.is_none());
        assert!(policy.blacklisted_versions.is_empty());
        assert!(policy.minimum_version.is_none());
    }

    #[tokio::test]
    async fn test_repository_version_resolver_creation() {
        let resolver = RepositoryVersionResolver::new();
        assert!(resolver.git_ops.is_none());
        assert!(resolver.cache.is_empty());
        assert_eq!(resolver.cache_ttl, Duration::from_secs(300));
    }

    #[tokio::test]
    async fn test_configuration_updater_creation() {
        let updater = ConfigurationUpdater::new();
        assert!(updater.preserve_formatting);
        assert!(updater.create_backups);
    }

    #[test]
    fn test_yaml_content_update() {
        let updater = ConfigurationUpdater::new();
        let content = r#"repos:
- repo: https://github.com/psf/black
  rev: 22.3.0
  hooks:
  - id: black"#;

        let updated = updater
            .update_yaml_content(content, "https://github.com/psf/black", "23.1.0")
            .unwrap();
        assert!(updated.contains("rev: 23.1.0"));
        assert!(!updated.contains("rev: 22.3.0"));
    }
}
