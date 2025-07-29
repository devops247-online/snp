// Autoupdate command implementation for SNP
// Updates repository references in .pre-commit-config.yaml to their latest available versions

#![allow(clippy::uninlined_format_args)]

use crate::config::{Config, Repository};
use crate::error::{Result, SnpError};
use crate::git::GitRepository;
use regex::Regex;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tracing::{debug, info, warn};
use url::Url;

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

/// GitHub API response structures
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitHubTag {
    name: String,
    commit: GitHubCommit,
    tarball_url: String,
    zipball_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitHubCommit {
    sha: String,
    url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    published_at: String,
    prerelease: bool,
    target_commitish: String,
}

/// GitLab API response structures
#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitLabTag {
    name: String,
    commit: GitLabCommit,
    release: Option<GitLabRelease>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitLabCommit {
    id: String,
    short_id: String,
    created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GitLabRelease {
    tag_name: String,
    description: String,
    created_at: String,
}

/// Repository service type
#[derive(Debug, Clone, PartialEq)]
enum RepositoryService {
    GitHub { owner: String, repo: String },
    GitLab { owner: String, repo: String },
    Other(String),
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
    http_client: Client,
}

impl RepositoryVersionResolver {
    pub fn new() -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .user_agent("SNP/0.1.0")
            .build()
            .expect("Failed to create HTTP client");

        Self {
            git_ops: None,
            cache: HashMap::new(),
            cache_ttl: Duration::from_secs(300),     // 5 minutes
            semaphore: Arc::new(Semaphore::new(10)), // Max 10 concurrent requests
            http_client,
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

        // Parse repository URL to determine service
        let service = self.parse_repository_service(repo_url)?;

        match service {
            RepositoryService::GitHub { owner, repo } => {
                self.resolve_github_version(&owner, &repo, strategy).await
            }
            RepositoryService::GitLab { owner, repo } => {
                self.resolve_gitlab_version(&owner, &repo, strategy).await
            }
            RepositoryService::Other(_) => self.resolve_git_version(repo_url, strategy).await,
        }
    }

    /// List all available versions for a repository
    pub async fn list_versions(&mut self, repo_url: &str) -> Result<Vec<RepoVersion>> {
        debug!("Listing versions for {}", repo_url);

        let service = self.parse_repository_service(repo_url)?;

        match service {
            RepositoryService::GitHub { owner, repo } => {
                self.list_github_versions(&owner, &repo).await
            }
            RepositoryService::GitLab { owner, repo } => {
                self.list_gitlab_versions(&owner, &repo).await
            }
            RepositoryService::Other(_) => self.list_git_versions(repo_url).await,
        }
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

    /// Parse repository URL to determine service type
    fn parse_repository_service(&self, repo_url: &str) -> Result<RepositoryService> {
        let url = Url::parse(repo_url).map_err(|e| {
            SnpError::Config(Box::new(crate::error::ConfigError::ValidationFailed {
                message: format!("Invalid repository URL: {}", e),
                file_path: None,
                errors: vec![format!("Failed to parse URL: {}", repo_url)],
            }))
        })?;

        match url.host_str() {
            Some("github.com") => {
                let path_segments: Vec<&str> = url
                    .path_segments()
                    .ok_or_else(|| {
                        SnpError::Config(Box::new(crate::error::ConfigError::ValidationFailed {
                            message: "Invalid GitHub URL path".to_string(),
                            file_path: None,
                            errors: vec!["GitHub URL must have owner/repo format".to_string()],
                        }))
                    })?
                    .collect();

                if path_segments.len() >= 2 {
                    let owner = path_segments[0].to_string();
                    let repo = path_segments[1].trim_end_matches(".git").to_string();
                    Ok(RepositoryService::GitHub { owner, repo })
                } else {
                    Err(SnpError::Config(Box::new(
                        crate::error::ConfigError::ValidationFailed {
                            message: "Invalid GitHub URL format".to_string(),
                            file_path: None,
                            errors: vec![
                                "Expected format: https://github.com/owner/repo".to_string()
                            ],
                        },
                    )))
                }
            }
            Some("gitlab.com") => {
                let path_segments: Vec<&str> = url
                    .path_segments()
                    .ok_or_else(|| {
                        SnpError::Config(Box::new(crate::error::ConfigError::ValidationFailed {
                            message: "Invalid GitLab URL path".to_string(),
                            file_path: None,
                            errors: vec!["GitLab URL must have owner/repo format".to_string()],
                        }))
                    })?
                    .collect();

                if path_segments.len() >= 2 {
                    let owner = path_segments[0].to_string();
                    let repo = path_segments[1].trim_end_matches(".git").to_string();
                    Ok(RepositoryService::GitLab { owner, repo })
                } else {
                    Err(SnpError::Config(Box::new(
                        crate::error::ConfigError::ValidationFailed {
                            message: "Invalid GitLab URL format".to_string(),
                            file_path: None,
                            errors: vec![
                                "Expected format: https://gitlab.com/owner/repo".to_string()
                            ],
                        },
                    )))
                }
            }
            _ => Ok(RepositoryService::Other(repo_url.to_string())),
        }
    }

    /// Resolve version from GitHub repository
    async fn resolve_github_version(
        &self,
        owner: &str,
        repo: &str,
        strategy: &UpdateStrategy,
    ) -> Result<RepoVersion> {
        debug!(
            "Resolving GitHub version for {}/{} with strategy {:?}",
            owner, repo, strategy
        );

        match strategy {
            UpdateStrategy::LatestTag | UpdateStrategy::LatestStable => {
                self.get_github_latest_tag(owner, repo).await
            }
            UpdateStrategy::LatestCommit => self.get_github_latest_commit(owner, repo).await,
            UpdateStrategy::MajorVersion(major) => {
                self.get_github_version_by_major(owner, repo, *major).await
            }
            UpdateStrategy::MinorVersion(major, minor) => {
                self.get_github_version_by_minor(owner, repo, *major, *minor)
                    .await
            }
        }
    }

    /// Get latest tag from GitHub
    async fn get_github_latest_tag(&self, owner: &str, repo: &str) -> Result<RepoVersion> {
        let url = format!("https://api.github.com/repos/{}/{}/tags", owner, repo);

        let response = self
            .http_client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .send()
            .await
            .map_err(|e| {
                SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("HTTP request failed: {}", e),
                    working_dir: None,
                }))
            })?;

        if !response.status().is_success() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: Some(response.status().as_u16() as i32),
                    stdout: String::new(),
                    stderr: format!("GitHub API returned status: {}", response.status()),
                    working_dir: None,
                },
            )));
        }

        let tags: Vec<GitHubTag> = response.json().await.map_err(|e| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: "parse JSON".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to parse GitHub API response: {}", e),
                working_dir: None,
            }))
        })?;

        if tags.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "get tags".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "No tags found in repository".to_string(),
                    working_dir: None,
                },
            )));
        }

        // Get the first (latest) tag
        let latest_tag = &tags[0];
        let mut version = RepoVersion::new(latest_tag.name.clone(), Some(latest_tag.name.clone()));
        version.commit_hash = Some(latest_tag.commit.sha.clone());
        version.commit_date = chrono::Utc::now(); // TODO: Get actual commit date

        Ok(version)
    }

    /// Get latest commit from GitHub
    async fn get_github_latest_commit(&self, owner: &str, repo: &str) -> Result<RepoVersion> {
        let url = format!("https://api.github.com/repos/{}/{}/commits", owner, repo);

        let response = self
            .http_client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .query(&[("per_page", "1")])
            .send()
            .await
            .map_err(|e| {
                SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("HTTP request failed: {}", e),
                    working_dir: None,
                }))
            })?;

        if !response.status().is_success() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: Some(response.status().as_u16() as i32),
                    stdout: String::new(),
                    stderr: format!("GitHub API returned status: {}", response.status()),
                    working_dir: None,
                },
            )));
        }

        let commits: Vec<serde_json::Value> = response.json().await.map_err(|e| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: "parse JSON".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to parse GitHub API response: {}", e),
                working_dir: None,
            }))
        })?;

        if commits.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "get commits".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "No commits found in repository".to_string(),
                    working_dir: None,
                },
            )));
        }

        let latest_commit = &commits[0];
        let sha = latest_commit["sha"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let mut version = RepoVersion::new(sha.clone(), None);
        version.commit_hash = Some(sha);
        version.commit_date = chrono::Utc::now(); // TODO: Parse actual commit date

        Ok(version)
    }

    /// Get version by major version from GitHub
    async fn get_github_version_by_major(
        &self,
        owner: &str,
        repo: &str,
        major: u32,
    ) -> Result<RepoVersion> {
        let tags = self.list_github_versions(owner, repo).await?;

        // Find the latest version with matching major version
        let matching_versions: Vec<_> = tags
            .iter()
            .filter(|v| {
                if let Some(version) = &v.version {
                    version.major == major as u64
                } else {
                    false
                }
            })
            .collect();

        if matching_versions.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "find version".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("No versions found with major version {}", major),
                    working_dir: None,
                },
            )));
        }

        // Return the latest (first) matching version
        Ok(matching_versions[0].clone())
    }

    /// Get version by major.minor from GitHub
    async fn get_github_version_by_minor(
        &self,
        owner: &str,
        repo: &str,
        major: u32,
        minor: u32,
    ) -> Result<RepoVersion> {
        let tags = self.list_github_versions(owner, repo).await?;

        // Find the latest version with matching major.minor version
        let matching_versions: Vec<_> = tags
            .iter()
            .filter(|v| {
                if let Some(version) = &v.version {
                    version.major == major as u64 && version.minor == minor as u64
                } else {
                    false
                }
            })
            .collect();

        if matching_versions.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "find version".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("No versions found with version {}.{}", major, minor),
                    working_dir: None,
                },
            )));
        }

        // Return the latest (first) matching version
        Ok(matching_versions[0].clone())
    }

    /// List all versions from GitHub
    async fn list_github_versions(&self, owner: &str, repo: &str) -> Result<Vec<RepoVersion>> {
        let url = format!("https://api.github.com/repos/{}/{}/tags", owner, repo);

        let response = self
            .http_client
            .get(&url)
            .header("Accept", "application/vnd.github.v3+json")
            .query(&[("per_page", "100")]) // Get up to 100 tags
            .send()
            .await
            .map_err(|e| {
                SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("HTTP request failed: {}", e),
                    working_dir: None,
                }))
            })?;

        if !response.status().is_success() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: Some(response.status().as_u16() as i32),
                    stdout: String::new(),
                    stderr: format!("GitHub API returned status: {}", response.status()),
                    working_dir: None,
                },
            )));
        }

        let tags: Vec<GitHubTag> = response.json().await.map_err(|e| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: "parse JSON".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to parse GitHub API response: {}", e),
                working_dir: None,
            }))
        })?;

        let mut versions = Vec::new();
        for tag in tags {
            let mut version = RepoVersion::new(tag.name.clone(), Some(tag.name.clone()));
            version.commit_hash = Some(tag.commit.sha.clone());
            version.commit_date = chrono::Utc::now(); // TODO: Get actual commit date
            versions.push(version);
        }

        Ok(versions)
    }

    /// Resolve version from GitLab repository
    async fn resolve_gitlab_version(
        &self,
        owner: &str,
        repo: &str,
        strategy: &UpdateStrategy,
    ) -> Result<RepoVersion> {
        debug!(
            "Resolving GitLab version for {}/{} with strategy {:?}",
            owner, repo, strategy
        );

        match strategy {
            UpdateStrategy::LatestTag | UpdateStrategy::LatestStable => {
                self.get_gitlab_latest_tag(owner, repo).await
            }
            UpdateStrategy::LatestCommit => self.get_gitlab_latest_commit(owner, repo).await,
            UpdateStrategy::MajorVersion(major) => {
                self.get_gitlab_version_by_major(owner, repo, *major).await
            }
            UpdateStrategy::MinorVersion(major, minor) => {
                self.get_gitlab_version_by_minor(owner, repo, *major, *minor)
                    .await
            }
        }
    }

    /// Get latest tag from GitLab
    async fn get_gitlab_latest_tag(&self, owner: &str, repo: &str) -> Result<RepoVersion> {
        let project_path = format!("{}/{}", owner, repo);
        let encoded_path = urlencoding::encode(&project_path);
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/repository/tags",
            encoded_path
        );

        let response = self.http_client.get(&url).send().await.map_err(|e| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: format!("GET {}", url),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("HTTP request failed: {}", e),
                working_dir: None,
            }))
        })?;

        if !response.status().is_success() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: Some(response.status().as_u16() as i32),
                    stdout: String::new(),
                    stderr: format!("GitLab API returned status: {}", response.status()),
                    working_dir: None,
                },
            )));
        }

        let tags: Vec<GitLabTag> = response.json().await.map_err(|e| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: "parse JSON".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to parse GitLab API response: {}", e),
                working_dir: None,
            }))
        })?;

        if tags.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "get tags".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "No tags found in repository".to_string(),
                    working_dir: None,
                },
            )));
        }

        // Get the first (latest) tag
        let latest_tag = &tags[0];
        let mut version = RepoVersion::new(latest_tag.name.clone(), Some(latest_tag.name.clone()));
        version.commit_hash = Some(latest_tag.commit.id.clone());
        // TODO: Parse created_at for actual date
        version.commit_date = chrono::Utc::now();

        Ok(version)
    }

    /// Get latest commit from GitLab
    async fn get_gitlab_latest_commit(&self, owner: &str, repo: &str) -> Result<RepoVersion> {
        let project_path = format!("{}/{}", owner, repo);
        let encoded_path = urlencoding::encode(&project_path);
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/repository/commits",
            encoded_path
        );

        let response = self
            .http_client
            .get(&url)
            .query(&[("per_page", "1")])
            .send()
            .await
            .map_err(|e| {
                SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("HTTP request failed: {}", e),
                    working_dir: None,
                }))
            })?;

        if !response.status().is_success() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: Some(response.status().as_u16() as i32),
                    stdout: String::new(),
                    stderr: format!("GitLab API returned status: {}", response.status()),
                    working_dir: None,
                },
            )));
        }

        let commits: Vec<serde_json::Value> = response.json().await.map_err(|e| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: "parse JSON".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to parse GitLab API response: {}", e),
                working_dir: None,
            }))
        })?;

        if commits.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "get commits".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "No commits found in repository".to_string(),
                    working_dir: None,
                },
            )));
        }

        let latest_commit = &commits[0];
        let sha = latest_commit["id"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        let mut version = RepoVersion::new(sha.clone(), None);
        version.commit_hash = Some(sha);
        version.commit_date = chrono::Utc::now(); // TODO: Parse actual commit date

        Ok(version)
    }

    /// Get version by major version from GitLab
    async fn get_gitlab_version_by_major(
        &self,
        owner: &str,
        repo: &str,
        major: u32,
    ) -> Result<RepoVersion> {
        let tags = self.list_gitlab_versions(owner, repo).await?;

        // Find the latest version with matching major version
        let matching_versions: Vec<_> = tags
            .iter()
            .filter(|v| {
                if let Some(version) = &v.version {
                    version.major == major as u64
                } else {
                    false
                }
            })
            .collect();

        if matching_versions.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "find version".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("No versions found with major version {}", major),
                    working_dir: None,
                },
            )));
        }

        // Return the latest (first) matching version
        Ok(matching_versions[0].clone())
    }

    /// Get version by major.minor from GitLab
    async fn get_gitlab_version_by_minor(
        &self,
        owner: &str,
        repo: &str,
        major: u32,
        minor: u32,
    ) -> Result<RepoVersion> {
        let tags = self.list_gitlab_versions(owner, repo).await?;

        // Find the latest version with matching major.minor version
        let matching_versions: Vec<_> = tags
            .iter()
            .filter(|v| {
                if let Some(version) = &v.version {
                    version.major == major as u64 && version.minor == minor as u64
                } else {
                    false
                }
            })
            .collect();

        if matching_versions.is_empty() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: "find version".to_string(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("No versions found with version {}.{}", major, minor),
                    working_dir: None,
                },
            )));
        }

        // Return the latest (first) matching version
        Ok(matching_versions[0].clone())
    }

    /// List all versions from GitLab
    async fn list_gitlab_versions(&self, owner: &str, repo: &str) -> Result<Vec<RepoVersion>> {
        let project_path = format!("{}/{}", owner, repo);
        let encoded_path = urlencoding::encode(&project_path);
        let url = format!(
            "https://gitlab.com/api/v4/projects/{}/repository/tags",
            encoded_path
        );

        let response = self
            .http_client
            .get(&url)
            .query(&[("per_page", "100")]) // Get up to 100 tags
            .send()
            .await
            .map_err(|e| {
                SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("HTTP request failed: {}", e),
                    working_dir: None,
                }))
            })?;

        if !response.status().is_success() {
            return Err(SnpError::Git(Box::new(
                crate::error::GitError::CommandFailed {
                    command: format!("GET {}", url),
                    exit_code: Some(response.status().as_u16() as i32),
                    stdout: String::new(),
                    stderr: format!("GitLab API returned status: {}", response.status()),
                    working_dir: None,
                },
            )));
        }

        let tags: Vec<GitLabTag> = response.json().await.map_err(|e| {
            SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
                command: "parse JSON".to_string(),
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to parse GitLab API response: {}", e),
                working_dir: None,
            }))
        })?;

        let mut versions = Vec::new();
        for tag in tags {
            let mut version = RepoVersion::new(tag.name.clone(), Some(tag.name.clone()));
            version.commit_hash = Some(tag.commit.id.clone());
            // TODO: Parse created_at for actual date
            version.commit_date = chrono::Utc::now();
            versions.push(version);
        }

        Ok(versions)
    }

    /// Resolve version from Git repository (fallback for non-GitHub/GitLab repos)
    async fn resolve_git_version(
        &self,
        repo_url: &str,
        strategy: &UpdateStrategy,
    ) -> Result<RepoVersion> {
        debug!(
            "Resolving Git version for {} with strategy {:?}",
            repo_url, strategy
        );

        // This would use git2 to clone and inspect the repository
        // For now, return an error indicating this needs Git integration
        Err(SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
            command: "git operations".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: "Direct Git repository access not implemented yet. Use GitHub or GitLab repositories.".to_string(),
            working_dir: None,
        })))
    }

    /// List versions from Git repository (fallback for non-GitHub/GitLab repos)
    async fn list_git_versions(&self, repo_url: &str) -> Result<Vec<RepoVersion>> {
        debug!("Listing Git versions for {}", repo_url);

        // This would use git2 to clone and list tags
        // For now, return an error indicating this needs Git integration
        Err(SnpError::Git(Box::new(crate::error::GitError::CommandFailed {
            command: "git operations".to_string(),
            exit_code: None,
            stdout: String::new(),
            stderr: "Direct Git repository access not implemented yet. Use GitHub or GitLab repositories.".to_string(),
            working_dir: None,
        })))
    }
}

/// Configuration updater for YAML files
pub struct ConfigurationUpdater {
    preserve_formatting: bool,
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

        // Create backup if enabled
        if self.create_backups {
            self.create_backup(config_path).await?;
        }

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

    /// Update YAML content with optional formatting preservation
    fn update_yaml_content(&self, content: &str, repo_url: &str, new_rev: &str) -> Result<String> {
        if self.preserve_formatting {
            // Preserve original formatting by updating line-by-line
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
        } else {
            // Parse and rewrite YAML, which may change formatting
            let mut config: serde_yaml::Value = serde_yaml::from_str(content).map_err(|e| {
                SnpError::Config(Box::new(crate::error::ConfigError::InvalidYaml {
                    message: e.to_string(),
                    file_path: None,
                    line: e.location().map(|l| l.line() as u32),
                    column: e.location().map(|l| l.column() as u32),
                }))
            })?;

            // Update the revision in the parsed YAML
            if let Some(repos) = config.get_mut("repos").and_then(|r| r.as_sequence_mut()) {
                for repo in repos {
                    if let Some(repo_map) = repo.as_mapping_mut() {
                        let repo_key = serde_yaml::Value::String("repo".to_string());
                        if let Some(repo_value) = repo_map.get(&repo_key) {
                            if repo_value.as_str() == Some(repo_url) {
                                repo_map.insert(
                                    serde_yaml::Value::String("rev".to_string()),
                                    serde_yaml::Value::String(new_rev.to_string()),
                                );
                                break;
                            }
                        }
                    }
                }
            }

            // Serialize back to YAML
            serde_yaml::to_string(&config).map_err(|e| {
                SnpError::Config(Box::new(crate::error::ConfigError::InvalidYaml {
                    message: format!("Failed to serialize updated config: {e}"),
                    file_path: None,
                    line: None,
                    column: None,
                }))
            })
        }
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

    /// Create a backup of the configuration file
    async fn create_backup(&self, config_path: &Path) -> Result<()> {
        let backup_path = config_path.with_extension("yaml.backup");
        tokio::fs::copy(config_path, &backup_path)
            .await
            .map_err(SnpError::Io)?;
        debug!("Created backup at {}", backup_path.display());
        Ok(())
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
